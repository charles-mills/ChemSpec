//! Algorithmic reaction solving.
//!
//! Predicts products for the classroom reaction families that follow
//! deterministically from reactant structure: acid-base (oxides,
//! hydroxides, carbonates, bicarbonates), acid + metal, binary synthesis,
//! combustion, oxide hydration and slaking, metal + water, single, double,
//! and halogen displacement with solubility rules, heat/light/electricity
//! decomposition, and confident no-reactions (noble gases, metal pairs,
//! light-stable and inert pairings). The output is an ordinary
//! [`ReactionClaim`]; downstream exact balancing and structural validation
//! gate it exactly like a model claim. Anything outside these families
//! returns None so the caller can fall back to the model.

use std::collections::BTreeMap;

use chem_domain::{
    ElementInventory, ElementSymbol, RepresentationKind, SpeciesRegistry, StructureDefinition,
    StructureId, classify_bronsted_acid, generate_structure,
};

use crate::{
    ClaimDisposition, ClaimIdentityHint, ClaimIdentityHintKind, ClaimObservation,
    ClaimObservationPredicate, ClaimPhase, ClaimProduct, NoReactionReason, OutcomeSpecies,
    ReactionBuildRequest, ReactionClaim, RequestIdentityResolution,
    claim::REACTION_CLAIM_SCHEMA_VERSION, organic, resolve_request_identities,
};

/// Donor elements that make a proton-donor site an acid site in practice;
/// carbon acids stay out so alkanes never neutralize.
const ACIDIC_DONORS: [&str; 6] = ["O", "F", "Cl", "Br", "I", "S"];

/// Attempts to solve the request without a model. Returns a fully formed
/// reaction claim, or None when no deterministic family applies.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn solve_reaction_claim(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Option<ReactionClaim> {
    if !(1..=2).contains(&request.reactants.len()) {
        return None;
    }
    let Ok(RequestIdentityResolution::Resolved(species)) =
        resolve_request_identities(request, identities)
    else {
        return None;
    };
    // Element identity is always known, even when structure generation
    // falls short; families that only need it still get verdicts.
    let compositions = species
        .iter()
        .map(species_composition)
        .collect::<Option<Vec<_>>>()?;
    let optional_structures = species
        .iter()
        .map(|entry| match entry {
            OutcomeSpecies::Resolved(resolved) => resolved.structure.as_ref(),
            OutcomeSpecies::FormulaOnly { .. } => None,
        })
        .collect::<Vec<_>>();
    let pair_context = request
        .selected_context
        .as_deref()
        .map(|context| context.trim().to_lowercase());
    let verdict = if species.len() == 1 {
        solve_decomposition(optional_structures[0]?, pair_context.as_deref()?)?
    } else {
        let identity_verdict = solve_trivial_no_reaction(
            (&compositions[0], optional_structures[0]),
            (&compositions[1], optional_structures[1]),
        );
        let structures = move || Some((optional_structures[0]?, optional_structures[1]?));
        identity_verdict.or_else(|| {
            let (first, second) = structures()?;
            let structures = [first, second];
            solve_acid_base(structures[0], structures[1])
                .or_else(|| solve_acid_base(structures[1], structures[0]))
                .or_else(|| solve_acid_metal(structures[0], structures[1]))
                .or_else(|| solve_acid_metal(structures[1], structures[0]))
                .or_else(|| {
                    solve_alcohol_oxidation(structures[0], structures[1], pair_context.as_deref())
                })
                .or_else(|| {
                    solve_alcohol_oxidation(structures[1], structures[0], pair_context.as_deref())
                })
                .or_else(|| {
                    solve_incomplete_combustion(
                        structures[0],
                        structures[1],
                        pair_context.as_deref(),
                    )
                })
                .or_else(|| {
                    solve_incomplete_combustion(
                        structures[1],
                        structures[0],
                        pair_context.as_deref(),
                    )
                })
                .or_else(|| solve_combustion(structures[0], structures[1]))
                .or_else(|| solve_combustion(structures[1], structures[0]))
                .or_else(|| {
                    solve_ester_hydrolysis(structures[0], structures[1], pair_context.as_deref())
                })
                .or_else(|| {
                    solve_ester_hydrolysis(structures[1], structures[0], pair_context.as_deref())
                })
                .or_else(|| solve_hydrohalogenation(structures[0], structures[1]))
                .or_else(|| solve_hydrohalogenation(structures[1], structures[0]))
                .or_else(|| solve_alkene_addition(structures[0], structures[1]))
                .or_else(|| solve_alkene_addition(structures[1], structures[0]))
                .or_else(|| {
                    solve_substitution(structures[0], structures[1], pair_context.as_deref())
                })
                .or_else(|| {
                    solve_substitution(structures[1], structures[0], pair_context.as_deref())
                })
                .or_else(|| solve_esterification(structures[0], structures[1]))
                .or_else(|| solve_esterification(structures[1], structures[0]))
                .or_else(|| solve_oxide_water(structures[0], structures[1]))
                .or_else(|| solve_oxide_water(structures[1], structures[0]))
                .or_else(|| solve_metal_water(structures[0], structures[1]))
                .or_else(|| solve_metal_water(structures[1], structures[0]))
                .or_else(|| solve_single_displacement(structures[0], structures[1]))
                .or_else(|| solve_single_displacement(structures[1], structures[0]))
                .or_else(|| solve_halogen_displacement(structures[0], structures[1]))
                .or_else(|| solve_halogen_displacement(structures[1], structures[0]))
                .or_else(|| solve_double_displacement(structures[0], structures[1]))
                .or_else(|| {
                    solve_synthesis(structures[0], structures[1]).map(|products| Verdict {
                        reason: None,
                        products,
                        observations: Vec::new(),
                    })
                })
        })?
    };
    let disposition = if verdict.products.is_empty() {
        ClaimDisposition::NoReaction
    } else {
        ClaimDisposition::Reaction
    };
    Some(ReactionClaim {
        schema_version: REACTION_CLAIM_SCHEMA_VERSION,
        disposition,
        products: verdict.products,
        required_context: request.selected_context.clone().unwrap_or_default(),
        observations: verdict.observations,
        sources: Vec::new(),
        ambiguity: None,
        no_reaction_reason: verdict.reason,
    })
}

/// A solved outcome: an empty product list is a confident "no reaction",
/// and every such verdict names its physical cause for the learner.
struct Verdict {
    products: Vec<ClaimProduct>,
    observations: Vec<ClaimObservation>,
    reason: Option<NoReactionReason>,
}

/// One element symbol as its lowercase display name ("Cu" → "copper").
fn display_name(symbol: &str) -> String {
    chem_domain::element_name(symbol).map_or_else(|| symbol.to_owned(), str::to_owned)
}

/// Reason for an alkali carbonate or hydroxide shrugging off heat.
fn heat_stable_reason(cation: &str, anion_kind: BaseAnion) -> NoReactionReason {
    let anion = if anion_kind == BaseAnion::Carbonate {
        "carbonate"
    } else {
        "hydroxide"
    };
    NoReactionReason::HeatStable {
        compound: format!("{} {anion}", display_name(cation)),
    }
}

/// Acid + ionic base (oxide, hydroxide, carbonate, or bicarbonate) → salt
/// + water, with carbon dioxide evolution for the carbonates.
fn solve_acid_base(acid: &StructureDefinition, base: &StructureDefinition) -> Option<Verdict> {
    let donors = acid_donor_count(acid)?;
    let (cation, cation_charge, anion_kind) = ionic_base(base)?;
    let salt = conjugate_salt(acid, donors, &cation, cation_charge)?;
    let mut products = vec![
        ClaimProduct {
            name: "Water".to_owned(),
            formula: "H2O".to_owned(),
            phase: ClaimPhase::Liquid,
            identity_hints: Vec::new(),
        },
        product_from_counts(&salt, Some((&cation, cation_charge))),
    ];
    let mut observations = Vec::new();
    if matches!(anion_kind, BaseAnion::Carbonate | BaseAnion::Bicarbonate) {
        products.push(ClaimProduct {
            name: "carbon dioxide".to_owned(),
            formula: "CO2".to_owned(),
            phase: ClaimPhase::Gas,
            identity_hints: Vec::new(),
        });
        observations.push(ClaimObservation {
            predicate: ClaimObservationPredicate::Evolves,
            subject: "carbon dioxide gas".to_owned(),
            value: None,
        });
    }
    Some(Verdict {
        reason: None,
        products,
        observations,
    })
}

/// Acid + elemental metal: salt + hydrogen above the hydrogen pivot in the
/// activity series, a confident no-reaction below it. Dilute acids are
/// mild oxidants: variable-charge metals only reach their mild aqueous
/// state (Fe + 2HCl → `FeCl2`, never `FeCl3`).
fn solve_acid_metal(acid: &StructureDefinition, metal: &StructureDefinition) -> Option<Verdict> {
    let donors = acid_donor_count(acid)?;
    if metal.representation() != RepresentationKind::Metallic {
        return None;
    }
    let (element, _) = single_element(metal)?;
    let charge = chem_domain::aqueous_cation_charge(element.as_str())?;
    if !chem_domain::displaces_hydrogen_from_acids(element.as_str())? {
        return Some(Verdict {
            reason: Some(NoReactionReason::BelowHydrogen {
                metal: display_name(element.as_str()),
            }),
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    let cation = element.as_str().to_owned();
    let salt = conjugate_salt(acid, donors, &cation, u64::try_from(charge).ok()?)?;
    Some(Verdict {
        reason: None,
        products: vec![
            product_from_counts(&salt, Some((&cation, u64::try_from(charge).ok()?))),
            ClaimProduct {
                name: "Hydrogen".to_owned(),
                formula: "H2".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
        ],
        observations: vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Evolves,
            subject: "hydrogen gas".to_owned(),
            value: None,
        }],
    })
}

/// Organic fuel + oxygen → complete combustion to carbon dioxide and
/// water. Deterministic for any C/H(/O) fuel; the balancer finds the
/// coefficients.
/// A molecular C/H(/O) fuel facing elemental oxygen.
fn combustible_pair(fuel: &StructureDefinition, oxidizer: &StructureDefinition) -> bool {
    let Some((oxidizer_element, _)) = single_element(oxidizer) else {
        return false;
    };
    if oxidizer_element.as_str() != "O" || fuel.representation() != RepresentationKind::Molecular {
        return false;
    }
    let elements = fuel.formula().elements();
    elements
        .keys()
        .all(|symbol| matches!(symbol.as_str(), "C" | "H" | "O"))
        && elements.keys().any(|symbol| symbol.as_str() == "C")
        && elements.keys().any(|symbol| symbol.as_str() == "H")
}

/// Combustion in restricted air: the carbon only reaches carbon monoxide,
/// with the sooty flame the classroom associates with it.
fn solve_incomplete_combustion(
    fuel: &StructureDefinition,
    oxidizer: &StructureDefinition,
    context: Option<&str>,
) -> Option<Verdict> {
    if context != Some("limited oxygen") || !combustible_pair(fuel, oxidizer) {
        return None;
    }
    Some(Verdict {
        reason: None,
        products: vec![
            ClaimProduct {
                name: "carbon monoxide".to_owned(),
                formula: "CO".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
            ClaimProduct {
                name: "Water".to_owned(),
                formula: "H2O".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
        ],
        observations: vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Colour,
            subject: "flame".to_owned(),
            value: Some("sooty and yellow".to_owned()),
        }],
    })
}

/// Catalytic partial oxidation of an alcohol by oxygen: primary alcohols
/// give the aldehyde, secondary the ketone, plus water.
fn solve_alcohol_oxidation(
    alcohol: &StructureDefinition,
    oxidizer: &StructureDefinition,
    context: Option<&str>,
) -> Option<Verdict> {
    if context != Some("catalyst") {
        return None;
    }
    let (oxidizer_element, _) = single_element(oxidizer)?;
    if oxidizer_element.as_str() != "O" {
        return None;
    }
    let molecule = organic::Editable::from_structure(alcohol)?;
    let product = organic::oxidise_alcohol(&molecule)?;
    Some(Verdict {
        reason: None,
        products: vec![
            organic_product(&product, ClaimPhase::Liquid)?,
            water_product(ClaimPhase::Liquid),
        ],
        observations: Vec::new(),
    })
}

/// Ester + water under heat: hydrolysis back to the acid and alcohol.
fn solve_ester_hydrolysis(
    ester: &StructureDefinition,
    water: &StructureDefinition,
    context: Option<&str>,
) -> Option<Verdict> {
    if context != Some("heat") || !is_water(water.formula()) {
        return None;
    }
    let molecule = organic::Editable::from_structure(ester)?;
    let (acid, alcohol) = organic::hydrolyse_ester(&molecule)?;
    Some(Verdict {
        reason: None,
        products: vec![
            organic_product(&acid, ClaimPhase::Liquid)?,
            organic_product(&alcohol, ClaimPhase::Liquid)?,
        ],
        observations: Vec::new(),
    })
}

/// Markovnikov addition of a hydrogen halide across the alkene.
fn solve_hydrohalogenation(
    candidate: &StructureDefinition,
    reagent: &StructureDefinition,
) -> Option<Verdict> {
    if reagent.representation() != RepresentationKind::Molecular {
        return None;
    }
    let elements = reagent.formula().elements();
    let mut entries = elements.iter();
    let halogen = match (entries.next(), entries.next(), entries.next()) {
        (Some((first, 1)), Some((second, 1)), None) => {
            let (hydrogen, halogen) = if first.as_str() == "H" {
                (first, second)
            } else {
                (second, first)
            };
            if hydrogen.as_str() != "H" || !matches!(halogen.as_str(), "Cl" | "Br" | "I") {
                return None;
            }
            halogen.as_str().to_owned()
        }
        _ => return None,
    };
    let molecule = organic::Editable::from_structure(candidate)?;
    let product = organic::hydrohalogenate(&molecule, &halogen)?;
    Some(Verdict {
        reason: None,
        products: vec![organic_product(&product, ClaimPhase::Liquid)?],
        observations: Vec::new(),
    })
}

fn solve_combustion(fuel: &StructureDefinition, oxidizer: &StructureDefinition) -> Option<Verdict> {
    if !combustible_pair(fuel, oxidizer) {
        return None;
    }
    Some(Verdict {
        reason: None,
        products: vec![
            ClaimProduct {
                name: "carbon dioxide".to_owned(),
                formula: "CO2".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
            ClaimProduct {
                name: "Water".to_owned(),
                formula: "H2O".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
        ],
        observations: vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Forms,
            subject: "carbon dioxide and water vapour".to_owned(),
            value: None,
        }],
    })
}

/// One anhydride entry: gcd-reduced composition, oxoacid formula, name.
type Anhydride = (&'static [(&'static str, u64)], &'static str, &'static str);

/// Acid anhydrides: gcd-reduced nonmetal oxide compositions and the
/// oxoacid each one hydrates into.
const ACID_ANHYDRIDES: [Anhydride; 6] = [
    (&[("C", 1), ("O", 2)], "H2CO3", "carbonic acid"),
    (&[("O", 2), ("S", 1)], "H2SO3", "sulfurous acid"),
    (&[("O", 3), ("S", 1)], "H2SO4", "sulfuric acid"),
    (&[("N", 2), ("O", 3)], "HNO2", "nitrous acid"),
    (&[("N", 2), ("O", 5)], "HNO3", "nitric acid"),
    (&[("O", 5), ("P", 2)], "H3PO4", "phosphoric acid"),
];

/// Metals reactive enough that both they and their oxides turn water into
/// the hydroxide (the alkali and heavy alkaline-earth metals).
const WATER_REACTIVE_METALS: [&str; 10] =
    ["Li", "Na", "K", "Rb", "Cs", "Fr", "Ca", "Sr", "Ba", "Ra"];

/// The hydroxide of one cation, conventionally formatted and phased by
/// solubility.
fn hydroxide_salt(cation: &str, charge: u64) -> ClaimProduct {
    let hydroxide = Salt {
        cation: String::new(),
        cation_charge: 0,
        anion: [("H".to_owned(), 1), ("O".to_owned(), 1)].into(),
        anion_charge: 1,
    };
    exchanged_salt(
        cation,
        charge,
        &hydroxide,
        salt_solubility(cation, &hydroxide.anion),
    )
}

/// Oxide + water. Acid anhydrides hydrate to their oxoacid, reactive metal
/// oxides slake to hydroxides, and other known metal oxides confidently do
/// nothing. Borderline oxides (`MgO` reacts slowly, amphoterics passivate,
/// `NO2` disproportionates) fall to the model.
fn solve_oxide_water(oxide: &StructureDefinition, water: &StructureDefinition) -> Option<Verdict> {
    if !is_water(water.formula()) || is_water(oxide.formula()) {
        return None;
    }
    if oxide.representation() == RepresentationKind::Molecular {
        let elements = oxide.formula().elements();
        let shared = elements.values().fold(0, |acc, count| gcd(acc, *count));
        let reduced = elements
            .iter()
            .map(|(symbol, count)| (symbol.as_str(), count / shared))
            .collect::<Vec<_>>();
        let (_, formula, name) = ACID_ANHYDRIDES
            .iter()
            .find(|(anhydride, _, _)| *anhydride == reduced.as_slice())?;
        return Some(Verdict {
            reason: None,
            products: vec![ClaimProduct {
                name: (*name).to_owned(),
                formula: (*formula).to_owned(),
                phase: ClaimPhase::Aqueous,
                identity_hints: Vec::new(),
            }],
            observations: vec![ClaimObservation {
                predicate: ClaimObservationPredicate::Forms,
                subject: "an acidic solution".to_owned(),
                value: None,
            }],
        });
    }
    let salt = ionic_salt(oxide)?;
    if !(salt.anion.len() == 1 && salt.anion.get("O") == Some(&1)) {
        return None;
    }
    if WATER_REACTIVE_METALS.contains(&salt.cation.as_str()) {
        return Some(Verdict {
            reason: None,
            products: vec![hydroxide_salt(&salt.cation, salt.cation_charge)],
            observations: vec![ClaimObservation {
                predicate: ClaimObservationPredicate::Forms,
                subject: "an alkaline solution".to_owned(),
                value: None,
            }],
        });
    }
    if matches!(
        salt.cation.as_str(),
        "Mg" | "Be" | "Al" | "Zn" | "Pb" | "Sn"
    ) {
        return None;
    }
    // Any other recognised metal oxide just sits in water.
    chem_domain::common_cation_charge(&salt.cation)?;
    Some(Verdict {
        reason: Some(NoReactionReason::OxideInertInWater {
            metal: display_name(&salt.cation),
        }),
        products: Vec::new(),
        observations: Vec::new(),
    })
}

/// Element counts of one request species, from its formula text. Available
/// even when no structure could be generated.
fn species_composition(entry: &OutcomeSpecies) -> Option<BTreeMap<String, u64>> {
    let formula = match entry {
        OutcomeSpecies::Resolved(resolved) => resolved.formula_text.as_str(),
        OutcomeSpecies::FormulaOnly { formula, .. } => formula.as_str(),
    };
    let composition = chem_domain::FormulaComposition::parse(formula).ok()?;
    Some(
        composition
            .elements()
            .iter()
            .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
            .collect(),
    )
}

/// Pairings that confidently do nothing, decided from element identity
/// alone where possible: a light noble gas with anything (Kr and Xe form
/// real fluorides, so no verdict there), two elemental metals (alloys are
/// mixtures, not reactions), or two portions of the same closed-shell
/// substance (open-shell twins like NO2 dimerize, so those need their
/// structures checked).
fn solve_trivial_no_reaction(
    left: (&BTreeMap<String, u64>, Option<&StructureDefinition>),
    right: (&BTreeMap<String, u64>, Option<&StructureDefinition>),
) -> Option<Verdict> {
    let lone_element = |composition: &BTreeMap<String, u64>| {
        (composition.len() == 1)
            .then(|| composition.keys().next().cloned())
            .flatten()
    };
    let inert = |composition: &BTreeMap<String, u64>| {
        lone_element(composition)
            .is_some_and(|element| matches!(element.as_str(), "He" | "Ne" | "Ar"))
    };
    let metal = |composition: &BTreeMap<String, u64>| {
        lone_element(composition).is_some_and(|element| chem_domain::is_metal(&element))
    };
    let closed_shell = |structure: Option<&StructureDefinition>| {
        structure.is_some_and(|structure| {
            structure
                .graph()
                .atoms()
                .values()
                .all(|atom| atom.electrons().unpaired_electrons() == 0)
        })
    };
    let same_substance = left.0 == right.0 && closed_shell(left.1) && closed_shell(right.1);
    let noble = [left.0, right.0]
        .into_iter()
        .find(|composition| inert(composition))
        .and_then(lone_element);
    let reason = if let Some(element) = noble {
        NoReactionReason::NobleGas {
            element: display_name(&element),
        }
    } else if metal(left.0) && metal(right.0) {
        NoReactionReason::TwoMetals
    } else if same_substance {
        NoReactionReason::SameSubstance
    } else {
        return None;
    };
    Some(Verdict {
        reason: Some(reason),
        products: Vec::new(),
        observations: Vec::new(),
    })
}

/// Halogens in decreasing reactivity.
const HALOGENS: [&str; 4] = ["F", "Cl", "Br", "I"];

/// Solution colours of the displaced halogens.
fn halogen_colour(symbol: &str) -> Option<&'static str> {
    match symbol {
        "Br" => Some("orange"),
        "I" => Some("brown"),
        _ => None,
    }
}

/// Elemental halogen + dissolved halide salt: a more reactive halogen
/// displaces a less reactive one (Cl2 + 2KBr → 2KCl + Br2); the reverse
/// confidently does nothing. Elemental fluorine attacks the water itself,
/// so it gets no solution verdict.
fn solve_halogen_displacement(
    halogen: &StructureDefinition,
    salt: &StructureDefinition,
) -> Option<Verdict> {
    if halogen.representation() != RepresentationKind::Molecular {
        return None;
    }
    let (element, _) = single_element(halogen)?;
    let incoming = HALOGENS
        .iter()
        .position(|candidate| *candidate == element.as_str())?;
    let salt = ionic_salt(salt)?;
    let resident_symbol = if salt.anion.len() == 1 {
        salt.anion.keys().next()?.clone()
    } else {
        return None;
    };
    let resident = HALOGENS
        .iter()
        .position(|candidate| *candidate == resident_symbol)?;
    if salt_solubility(&salt.cation, &salt.anion) != Some(true) {
        return None;
    }
    if incoming == 0 {
        return None;
    }
    if incoming >= resident {
        return Some(Verdict {
            reason: Some(NoReactionReason::LessReactiveHalogen {
                incoming: display_name(element.as_str()),
                resident: display_name(&resident_symbol),
            }),
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    let displaced_name = chem_domain::element_name(&resident_symbol)?.to_owned();
    let incoming_anion = Salt {
        cation: String::new(),
        cation_charge: 0,
        anion: [(element.as_str().to_owned(), 1)].into(),
        anion_charge: 1,
    };
    let mut observations = vec![ClaimObservation {
        predicate: ClaimObservationPredicate::Forms,
        subject: format!("displaced {displaced_name}"),
        value: None,
    }];
    if let Some(colour) = halogen_colour(&resident_symbol) {
        observations.push(ClaimObservation {
            predicate: ClaimObservationPredicate::Colour,
            subject: "solution".to_owned(),
            value: Some(colour.to_owned()),
        });
    }
    Some(Verdict {
        reason: None,
        products: vec![
            exchanged_salt(
                &salt.cation,
                salt.cation_charge,
                &incoming_anion,
                salt_solubility(&salt.cation, &incoming_anion.anion),
            ),
            ClaimProduct {
                name: displaced_name,
                formula: format!("{resident_symbol}2"),
                phase: ClaimPhase::Aqueous,
                identity_hints: Vec::new(),
            },
        ],
        observations,
    })
}

/// Elemental metal + water: the very reactive metals form their hydroxide
/// and hydrogen; metals below hydrogen in the activity series confidently
/// do nothing. The steam-only band (Mg through Pb) falls to the model.
fn solve_metal_water(metal: &StructureDefinition, water: &StructureDefinition) -> Option<Verdict> {
    if metal.representation() != RepresentationKind::Metallic || !is_water(water.formula()) {
        return None;
    }
    let (element, _) = single_element(metal)?;
    if WATER_REACTIVE_METALS.contains(&element.as_str()) {
        let charge = u64::try_from(chem_domain::common_cation_charge(element.as_str())?).ok()?;
        return Some(Verdict {
            reason: None,
            products: vec![
                hydroxide_salt(element.as_str(), charge),
                ClaimProduct {
                    name: "Hydrogen".to_owned(),
                    formula: "H2".to_owned(),
                    phase: ClaimPhase::Gas,
                    identity_hints: Vec::new(),
                },
            ],
            observations: vec![
                ClaimObservation {
                    predicate: ClaimObservationPredicate::Evolves,
                    subject: "hydrogen gas".to_owned(),
                    value: None,
                },
                ClaimObservation {
                    predicate: ClaimObservationPredicate::Forms,
                    subject: "an alkaline solution".to_owned(),
                    value: None,
                },
            ],
        });
    }
    if !chem_domain::displaces_hydrogen_from_acids(element.as_str())? {
        // Below the hydrogen pivot nothing happens, even to steam.
        return Some(Verdict {
            reason: Some(NoReactionReason::BelowHydrogen {
                metal: display_name(element.as_str()),
            }),
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    None
}

/// Cations whose carbonates and hydroxides shrug off a Bunsen flame.
const HEAT_STABLE_CATIONS: [&str; 4] = ["Na", "K", "Rb", "Cs"];

/// Single reactant + energy context. Heat decomposes carbonates,
/// bicarbonates, and hydroxides (except the heat-stable alkali ones, a
/// confident no-reaction); electricity electrolyses water.
fn solve_decomposition(reactant: &StructureDefinition, context: &str) -> Option<Verdict> {
    match context {
        "electricity" if is_water(reactant.formula()) => Some(Verdict {
            reason: None,
            products: vec![
                ClaimProduct {
                    name: "Hydrogen".to_owned(),
                    formula: "H2".to_owned(),
                    phase: ClaimPhase::Gas,
                    identity_hints: Vec::new(),
                },
                ClaimProduct {
                    name: "Oxygen".to_owned(),
                    formula: "O2".to_owned(),
                    phase: ClaimPhase::Gas,
                    identity_hints: Vec::new(),
                },
            ],
            observations: vec![ClaimObservation {
                predicate: ClaimObservationPredicate::Evolves,
                subject: "hydrogen and oxygen gases".to_owned(),
                value: None,
            }],
        }),
        "light" => photolysis(reactant),
        "heat" => {
            if let Some(verdict) = wohler_urea_synthesis(reactant) {
                return Some(verdict);
            }
            if let Some(verdict) = solve_dehydration(reactant) {
                return Some(verdict);
            }
            if let Some(verdict) = nitrate_decomposition(reactant) {
                return Some(verdict);
            }
            let (cation, charge, anion_kind) = ionic_base(reactant)?;
            let stable = HEAT_STABLE_CATIONS.contains(&cation.as_str());
            let oxide = |cation: &str, charge: u64| {
                let shared = gcd(charge, 2);
                let mut counts = BTreeMap::new();
                counts.insert(cation.to_owned(), 2 / shared);
                *counts.entry("O".to_owned()).or_insert(0) += charge / shared;
                counts
            };
            let carbon_dioxide = ClaimProduct {
                name: "carbon dioxide".to_owned(),
                formula: "CO2".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            };
            let water = ClaimProduct {
                name: "Water".to_owned(),
                formula: "H2O".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            };
            let evolves_carbon_dioxide = ClaimObservation {
                predicate: ClaimObservationPredicate::Evolves,
                subject: "carbon dioxide gas".to_owned(),
                value: None,
            };
            match anion_kind {
                // Most metal oxides shrug off heat, but HgO and Ag2O
                // genuinely decompose; no confident verdict either way.
                BaseAnion::Oxide => None,
                BaseAnion::Carbonate | BaseAnion::Hydroxide if stable => Some(Verdict {
                    reason: Some(heat_stable_reason(&cation, anion_kind)),
                    products: Vec::new(),
                    observations: Vec::new(),
                }),
                BaseAnion::Carbonate => Some(Verdict {
                    reason: None,
                    products: vec![
                        product_from_counts(&oxide(&cation, charge), Some((&cation, charge))),
                        carbon_dioxide,
                    ],
                    observations: vec![evolves_carbon_dioxide],
                }),
                BaseAnion::Hydroxide => Some(Verdict {
                    reason: None,
                    products: vec![
                        product_from_counts(&oxide(&cation, charge), Some((&cation, charge))),
                        water,
                    ],
                    observations: Vec::new(),
                }),
                BaseAnion::Bicarbonate => {
                    // 2 MHCO3 -> M2CO3 + H2O + CO2 (charge-balanced).
                    let shared = gcd(charge, 2);
                    let mut carbonate = BTreeMap::new();
                    carbonate.insert(cation.clone(), 2 / shared);
                    *carbonate.entry("C".to_owned()).or_insert(0) += charge / shared;
                    *carbonate.entry("O".to_owned()).or_insert(0) += 3 * (charge / shared);
                    Some(Verdict {
                        reason: None,
                        products: vec![
                            product_from_counts(&carbonate, Some((&cation, charge))),
                            water,
                            carbon_dioxide,
                        ],
                        observations: vec![evolves_carbon_dioxide],
                    })
                }
            }
        }
        _ => None,
    }
}

/// A claim product for an editable organic molecule: named when the graph
/// matches a known molecule (a wrong name is worse than none, so the
/// formula stands in otherwise), with its subset SMILES attached so the
/// outcome compiler rebuilds the exact isomer.
fn organic_product(molecule: &organic::Editable, phase: ClaimPhase) -> Option<ClaimProduct> {
    let smiles = molecule.to_smiles()?;
    let formula = molecule.formula_text();
    let name = organic::recognised_name(molecule).unwrap_or_else(|| formula.clone());
    Some(ClaimProduct {
        name,
        formula,
        phase,
        identity_hints: vec![ClaimIdentityHint {
            kind: ClaimIdentityHintKind::CanonicalSmiles,
            value: smiles,
        }],
    })
}

fn water_product(phase: ClaimPhase) -> ClaimProduct {
    ClaimProduct {
        name: "Water".to_owned(),
        formula: "H2O".to_owned(),
        phase,
        identity_hints: Vec::new(),
    }
}

/// The element of a molecular diatomic reagent (H2, Cl2, Br2), when it is
/// one the addition family knows.
fn addition_reagent(structure: &StructureDefinition) -> Option<&str> {
    if structure.representation() != RepresentationKind::Molecular {
        return None;
    }
    let elements = structure.formula().elements();
    let mut entries = elements.iter();
    match (entries.next(), entries.next()) {
        (Some((symbol, 2)), None) if matches!(symbol.as_str(), "H" | "Cl" | "Br") => {
            Some(match symbol.as_str() {
                "H" => "H",
                "Cl" => "Cl",
                _ => "Br",
            })
        }
        _ => None,
    }
}

/// Addition across a lone C=C bond: hydrogenation to the alkane, or
/// halogenation to the 1,2-dihalide (the bromine-water decolourisation
/// test).
fn solve_alkene_addition(
    candidate: &StructureDefinition,
    reagent: &StructureDefinition,
) -> Option<Verdict> {
    let molecule = organic::Editable::from_structure(candidate)?;
    organic::single_alkene(&molecule)?;
    let element = addition_reagent(reagent)?;
    let product = organic::saturate_alkene(&molecule, element)?;
    let phase = if element == "H" {
        ClaimPhase::Gas
    } else {
        ClaimPhase::Liquid
    };
    let observations = match element {
        "Br" => vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Colour,
            subject: "bromine".to_owned(),
            value: Some("the orange colour fades".to_owned()),
        }],
        _ => Vec::new(),
    };
    Some(Verdict {
        reason: None,
        products: vec![organic_product(&product, phase)?],
        observations,
    })
}

/// Free-radical substitution under light: alkane + halogen → haloalkane +
/// hydrogen halide, only where every substitutable position is equivalent
/// (methane, ethane, neopentane) so the product is unambiguous.
fn solve_substitution(
    candidate: &StructureDefinition,
    reagent: &StructureDefinition,
    context: Option<&str>,
) -> Option<Verdict> {
    if context != Some("light") {
        return None;
    }
    let element = addition_reagent(reagent)?;
    if element == "H" {
        return None;
    }
    let molecule = organic::Editable::from_structure(candidate)?;
    let product = organic::unique_monosubstitution(&molecule, element)?;
    let halide_root = if element == "Cl" {
        "chloride"
    } else {
        "bromide"
    };
    Some(Verdict {
        reason: None,
        products: vec![
            organic_product(&product, ClaimPhase::Gas)?,
            ClaimProduct {
                name: format!("hydrogen {halide_root}"),
                formula: format!("H{element}"),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
        ],
        observations: Vec::new(),
    })
}

/// Fischer esterification: carboxylic acid + alcohol → ester + water.
fn solve_esterification(
    acid: &StructureDefinition,
    alcohol: &StructureDefinition,
) -> Option<Verdict> {
    let acid = organic::Editable::from_structure(acid)?;
    let alcohol = organic::Editable::from_structure(alcohol)?;
    let ester = organic::esterify(&acid, &alcohol)?;
    Some(Verdict {
        reason: None,
        products: vec![
            organic_product(&ester, ClaimPhase::Liquid)?,
            water_product(ClaimPhase::Liquid),
        ],
        observations: vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Forms,
            subject: "a sweet-smelling ester".to_owned(),
            value: None,
        }],
    })
}

/// Dehydration of a simple alcohol under heat: the hydroxyl leaves with an
/// adjacent hydrogen as water, leaving the alkene.
fn solve_dehydration(reactant: &StructureDefinition) -> Option<Verdict> {
    let molecule = organic::Editable::from_structure(reactant)?;
    let alkene = organic::dehydrate(&molecule)?;
    Some(Verdict {
        reason: None,
        products: vec![
            organic_product(&alkene, ClaimPhase::Gas)?,
            water_product(ClaimPhase::Gas),
        ],
        observations: Vec::new(),
    })
}

/// Wöhler's 1828 synthesis: ammonium cyanate rearranges to urea on gentle
/// heating, the founding reaction of organic chemistry. An ionic CH4N2O
/// reactant can only be ammonium cyanate — the molecular representation of
/// the same inventory is urea itself.
fn wohler_urea_synthesis(reactant: &StructureDefinition) -> Option<Verdict> {
    if reactant.representation() != RepresentationKind::Ionic {
        return None;
    }
    let counts = reactant
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect::<Vec<_>>();
    if counts != [("C", 1), ("H", 4), ("N", 2), ("O", 1)] {
        return None;
    }
    Some(Verdict {
        reason: None,
        products: vec![ClaimProduct {
            name: "urea".to_owned(),
            formula: "CO(NH2)2".to_owned(),
            phase: ClaimPhase::Solid,
            identity_hints: Vec::new(),
        }],
        observations: vec![ClaimObservation {
            predicate: ClaimObservationPredicate::Forms,
            subject: "white crystalline urea".to_owned(),
            value: None,
        }],
    })
}

/// Metal nitrates under heat: sodium-tier alkali nitrates melt to the
/// nitrite and oxygen; lithium (the classic exception) and every heavier
/// metal decompose further, to the oxide, brown nitrogen dioxide, and
/// oxygen.
fn nitrate_decomposition(reactant: &StructureDefinition) -> Option<Verdict> {
    let salt = ionic_salt(reactant)?;
    let nitrate_unit: BTreeMap<String, u64> = [("N".to_owned(), 1), ("O".to_owned(), 3)].into();
    if salt.anion != nitrate_unit {
        return None;
    }
    let oxygen = ClaimProduct {
        name: "Oxygen".to_owned(),
        formula: "O2".to_owned(),
        phase: ClaimPhase::Gas,
        identity_hints: Vec::new(),
    };
    let evolves = |subject: &str| ClaimObservation {
        predicate: ClaimObservationPredicate::Evolves,
        subject: subject.to_owned(),
        value: None,
    };
    if matches!(salt.cation.as_str(), "Na" | "K" | "Rb" | "Cs") {
        let nitrite = Salt {
            cation: String::new(),
            cation_charge: 0,
            anion: [("N".to_owned(), 1), ("O".to_owned(), 2)].into(),
            anion_charge: 1,
        };
        return Some(Verdict {
            reason: None,
            products: vec![
                exchanged_salt(&salt.cation, salt.cation_charge, &nitrite, None),
                oxygen,
            ],
            observations: vec![evolves("oxygen gas")],
        });
    }
    chem_domain::common_cation_charge(&salt.cation)?;
    let oxide = Salt {
        cation: String::new(),
        cation_charge: 0,
        anion: [("O".to_owned(), 1)].into(),
        anion_charge: 2,
    };
    Some(Verdict {
        reason: None,
        products: vec![
            exchanged_salt(&salt.cation, salt.cation_charge, &oxide, None),
            ClaimProduct {
                name: "nitrogen dioxide".to_owned(),
                formula: "NO2".to_owned(),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
            oxygen,
        ],
        observations: vec![evolves("brown nitrogen dioxide gas")],
    })
}

/// Silver chloride and bromide under light: photolysis to metallic silver
/// and the halogen, the reaction behind photographic film. Iodide is too
/// sluggish for a confident verdict.
fn photolysis(reactant: &StructureDefinition) -> Option<Verdict> {
    let salt = ionic_salt(reactant)?;
    if salt.cation != "Ag" || salt.anion.len() != 1 {
        return None;
    }
    let halide = salt.anion.keys().next()?.clone();
    if !matches!(halide.as_str(), "Cl" | "Br") {
        return None;
    }
    Some(Verdict {
        reason: None,
        products: vec![
            ClaimProduct {
                name: "silver".to_owned(),
                formula: "Ag".to_owned(),
                phase: ClaimPhase::Solid,
                identity_hints: Vec::new(),
            },
            ClaimProduct {
                name: chem_domain::element_name(&halide)?.to_owned(),
                formula: format!("{halide}2"),
                phase: ClaimPhase::Gas,
                identity_hints: Vec::new(),
            },
        ],
        observations: vec![
            ClaimObservation {
                predicate: ClaimObservationPredicate::Forms,
                subject: "metallic silver".to_owned(),
                value: None,
            },
            ClaimObservation {
                predicate: ClaimObservationPredicate::Colour,
                subject: "the solid".to_owned(),
                value: Some("darkening grey".to_owned()),
            },
        ],
    })
}

/// Qualifying proton-donor count for a molecular acid (never water).
fn acid_donor_count(acid: &StructureDefinition) -> Option<u64> {
    if acid.representation() != RepresentationKind::Molecular || is_water(acid.formula()) {
        return None;
    }
    let donors = classify_bronsted_acid(acid)
        .proton_donor_sites()
        .iter()
        .filter(|site| ACIDIC_DONORS.contains(&site.donor_element().as_str()))
        .count();
    let donors = u64::try_from(donors).ok()?;
    (donors > 0).then_some(donors)
}

/// The charge-balanced salt of an acid's conjugate anion with one cation.
fn conjugate_salt(
    acid: &StructureDefinition,
    donors: u64,
    cation: &str,
    cation_charge: u64,
) -> Option<BTreeMap<String, u64>> {
    let mut anion = acid
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect::<BTreeMap<_, _>>();
    let hydrogens = anion.get_mut("H")?;
    *hydrogens = hydrogens.checked_sub(donors)?;
    if *hydrogens == 0 {
        anion.remove("H");
    }
    if anion.is_empty() {
        return None;
    }
    let shared = gcd(cation_charge, donors);
    let mut salt = BTreeMap::new();
    salt.insert(cation.to_owned(), donors / shared);
    for (symbol, count) in &anion {
        *salt.entry(symbol.clone()).or_insert(0) += count * (cation_charge / shared);
    }
    Some(salt)
}

/// Element + element → binary compound: stoichiometry from charge/valence
/// balance where that is deterministic (metal + nonmetal, hydrides), and
/// from unique structural generability otherwise. Chemically ambiguous
/// pairs (multiple stable oxides, ...) are left to the model.
fn solve_synthesis(
    left: &StructureDefinition,
    right: &StructureDefinition,
) -> Option<Vec<ClaimProduct>> {
    let (left_element, _) = single_element(left)?;
    let (right_element, _) = single_element(right)?;
    if left_element == right_element {
        return None;
    }
    let generable = |first: (&ElementSymbol, u64), second: (&ElementSymbol, u64)| {
        let inventory =
            ElementInventory::new([(first.0.clone(), first.1), (second.0.clone(), second.1)])
                .ok()?;
        let id = StructureId::new("generated.synthesis").ok()?;
        generate_structure(id, &inventory)
    };
    // The oxidant decides how far a variable-charge metal goes: fluorine,
    // oxygen, chlorine, and bromine reach the highest common state
    // (Fe + Cl2 → FeCl3), while the milder chalcogens and iodine only
    // reach the lowest (Fe + S → FeS, Fe + I2 → FeI2 — iodide cannot
    // coexist with the higher cation).
    let cation_of = |metal: &ElementSymbol, oxidant: &ElementSymbol| match oxidant.as_str() {
        "S" | "Se" | "Te" | "I" => chem_domain::lowest_cation_charge(metal.as_str()),
        _ => chem_domain::common_cation_charge(metal.as_str()),
    };
    let anion_of = |symbol: &ElementSymbol| chem_domain::anion_valence_charge(symbol.as_str());

    // Metal + nonmetal, or hydride: exact charge balance.
    let charge_pair = match (
        cation_of(&left_element, &right_element),
        cation_of(&right_element, &left_element),
    ) {
        (Some(charge), None) => Some((left_element.clone(), charge, right_element.clone())),
        (None, Some(charge)) => Some((right_element.clone(), charge, left_element.clone())),
        (Some(_), Some(_)) => return None,
        (None, None) if left_element.as_str() == "H" => {
            Some((left_element.clone(), 1, right_element.clone()))
        }
        (None, None) if right_element.as_str() == "H" => {
            Some((right_element.clone(), 1, left_element.clone()))
        }
        (None, None) => None,
    };
    if let Some((positive, positive_charge, negative)) = charge_pair {
        let anion = i16::from(anion_of(&negative)?);
        let shared = gcd(
            u64::try_from(positive_charge).ok()?,
            u64::try_from(anion).ok()?,
        );
        let positive_count = u64::try_from(anion).ok()? / shared;
        let negative_count = u64::try_from(positive_charge).ok()? / shared;
        let structure = generable((&positive, positive_count), (&negative, negative_count))?;
        let counts = [
            (positive.as_str().to_owned(), positive_count),
            (negative.as_str().to_owned(), negative_count),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let cation = (structure.representation() == RepresentationKind::Ionic).then(|| {
            (
                positive.as_str().to_owned(),
                u64::try_from(positive_charge).unwrap_or(1),
            )
        });
        return Some(vec![product_from_counts(
            &counts,
            cation
                .as_ref()
                .map(|(symbol, charge)| (symbol.as_str(), *charge)),
        )]);
    }

    // Two nonmetals: solved only when exactly one small compound generates.
    let mut candidates = Vec::new();
    for (i, j) in [(1, 1), (1, 2), (1, 3), (2, 1), (3, 1), (2, 3), (3, 2)] {
        if generable((&left_element, i), (&right_element, j)).is_some() {
            candidates.push((i, j));
        }
    }
    let [(i, j)] = candidates.as_slice() else {
        return None;
    };
    let counts = [
        (left_element.as_str().to_owned(), *i),
        (right_element.as_str().to_owned(), *j),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    Some(vec![product_from_counts(&counts, None)])
}

fn single_element(structure: &StructureDefinition) -> Option<(ElementSymbol, u64)> {
    let elements = structure.formula().elements();
    if elements.len() != 1 {
        return None;
    }
    elements
        .iter()
        .next()
        .map(|(symbol, count)| (symbol.clone(), *count))
}

fn is_water(inventory: &ElementInventory) -> bool {
    let elements = inventory.elements();
    elements.len() == 2
        && elements
            .iter()
            .all(|(symbol, count)| matches!((symbol.as_str(), count), ("H", 2) | ("O", 1)))
}

/// One ionic hydroxide base: every group is either a single-atom cation of
/// one element or an O-H hydroxide unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseAnion {
    Oxide,
    Hydroxide,
    Carbonate,
    Bicarbonate,
}

fn base_anion_kind(sorted_elements: &[&str]) -> Option<BaseAnion> {
    match sorted_elements {
        ["O"] => Some(BaseAnion::Oxide),
        ["H", "O"] => Some(BaseAnion::Hydroxide),
        ["C", "O", "O", "O"] => Some(BaseAnion::Carbonate),
        ["C", "H", "O", "O", "O"] => Some(BaseAnion::Bicarbonate),
        _ => None,
    }
}

/// One ionic base: every group is either a single-atom cation of one
/// element, or one consistent basic anion (hydroxide/carbonate/bicarbonate).
fn ionic_base(base: &StructureDefinition) -> Option<(String, u64, BaseAnion)> {
    let salt = ionic_salt(base)?;
    let sorted = salt
        .anion
        .iter()
        .flat_map(|(symbol, count)| {
            std::iter::repeat_n(symbol.as_str(), usize::try_from(*count).unwrap_or(0))
        })
        .collect::<Vec<_>>();
    let kind = base_anion_kind(&sorted)?;
    Some((salt.cation, salt.cation_charge, kind))
}

/// A simple ionic salt: one kind of single-atom cation, one kind of anion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Salt {
    pub(crate) cation: String,
    pub(crate) cation_charge: u64,
    /// Element counts of one anion unit.
    pub(crate) anion: BTreeMap<String, u64>,
    pub(crate) anion_charge: u64,
}

pub(crate) fn ionic_salt(structure: &StructureDefinition) -> Option<Salt> {
    if structure.representation() != RepresentationKind::Ionic {
        return None;
    }
    let graph = structure.graph();
    let mut adjacency: BTreeMap<&chem_domain::AtomId, Vec<&chem_domain::AtomId>> = BTreeMap::new();
    for bond in graph.covalent_bonds().values() {
        adjacency.entry(bond.left()).or_default().push(bond.right());
        adjacency.entry(bond.right()).or_default().push(bond.left());
    }
    let mut cation: Option<(String, u64)> = None;
    let mut anion: Option<(BTreeMap<String, u64>, u64)> = None;
    for group in graph.groups().values() {
        // One group may pack several ions of the same kind (a catalogue
        // Na2CO3 keeps both Na+ in a single group); covalent connectivity
        // splits it back into the actual ion units.
        let atoms = group.atoms();
        let mut visited = std::collections::BTreeSet::new();
        for start in atoms {
            if !visited.insert(start) {
                continue;
            }
            let mut component = vec![start];
            let mut queue = vec![start];
            while let Some(current) = queue.pop() {
                for neighbour in adjacency.get(current).into_iter().flatten() {
                    if atoms.contains(*neighbour) && visited.insert(*neighbour) {
                        component.push(*neighbour);
                        queue.push(*neighbour);
                    }
                }
            }
            classify_ion_unit(graph, &component, &mut cation, &mut anion)?;
        }
    }
    let (cation, cation_charge) = cation?;
    let (anion, anion_charge) = anion?;
    Some(Salt {
        cation,
        cation_charge,
        anion,
        anion_charge,
    })
}

/// Folds one bonded ion unit into the running cation/anion consensus;
/// None on a mixed, neutral, or inconsistent unit.
fn classify_ion_unit(
    graph: &chem_domain::StructuralGraph,
    component: &[&chem_domain::AtomId],
    cation: &mut Option<(String, u64)>,
    anion: &mut Option<(BTreeMap<String, u64>, u64)>,
) -> Option<()> {
    let members = component
        .iter()
        .map(|id| &graph.atoms()[*id])
        .collect::<Vec<_>>();
    let charge = members
        .iter()
        .map(|atom| i64::from(atom.electrons().formal_charge()))
        .sum::<i64>();
    if members.len() == 1 && charge > 0 {
        let found = (
            members[0].element().as_str().to_owned(),
            charge.unsigned_abs(),
        );
        if cation.get_or_insert_with(|| found.clone()) != &found {
            return None;
        }
    } else if charge < 0 {
        let mut counts = BTreeMap::new();
        for atom in &members {
            *counts
                .entry(atom.element().as_str().to_owned())
                .or_insert(0) += 1;
        }
        let found = (counts, charge.unsigned_abs());
        if anion.get_or_insert_with(|| found.clone()) != &found {
            return None;
        }
    } else {
        return None;
    }
    Some(())
}

/// Classroom solubility rules for salts in water: Some(true) dissolves,
/// Some(false) precipitates, None is outside the table (borderline cases
/// like `CaSO4` and `Ca(OH)2` stay with the model).
fn salt_solubility(cation: &str, anion: &BTreeMap<String, u64>) -> Option<bool> {
    if ["Li", "Na", "K", "Rb", "Cs", "Fr"].contains(&cation) {
        return Some(true);
    }
    let key = if anion.len() == 1 && anion.values().all(|count| *count == 1) {
        anion.keys().next()?.clone()
    } else {
        anion
            .iter()
            .map(|(symbol, count)| format!("{symbol}{count}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    match key.as_str() {
        // Nitrates and acetates always dissolve.
        "N1 O3" | "C2 H3 O2" => Some(true),
        "Cl" | "Br" | "I" => Some(!matches!(cation, "Ag" | "Pb" | "Hg")),
        "O4 S1" => match cation {
            "Ba" | "Sr" | "Pb" => Some(false),
            "Ca" | "Ag" => None,
            _ => Some(true),
        },
        // Alkali metals returned above; everyone else precipitates.
        "C1 O3" | "O4 P1" | "O3 S1" => Some(false),
        "H1 O1" => match cation {
            "Ba" | "Sr" => Some(true),
            "Ca" => None,
            _ => Some(false),
        },
        "S" => Some(matches!(cation, "Be" | "Mg" | "Ca" | "Sr" | "Ba")),
        _ => None,
    }
}

/// Two soluble ionic salts exchange partners when a product precipitates;
/// both products soluble is a confident no-reaction. Any solubility outside
/// the table, or a redox-prone ion pairing, falls to the model.
fn solve_double_displacement(
    left: &StructureDefinition,
    right: &StructureDefinition,
) -> Option<Verdict> {
    let first = ionic_salt(left)?;
    let second = ionic_salt(right)?;
    if first.cation == second.cation || first.anion == second.anion {
        return None;
    }
    // Oxidising cations turn iodide exchanges into redox (2Fe³⁺ + 2I⁻ →
    // 2Fe²⁺ + I₂); those never follow the exchange rule.
    let oxidising = |salt: &Salt| {
        matches!(
            (salt.cation.as_str(), salt.cation_charge),
            ("Fe", 3) | ("Cu", 2)
        )
    };
    let iodide = |salt: &Salt| salt.anion.len() == 1 && salt.anion.contains_key("I");
    if (oxidising(&first) && iodide(&second)) || (oxidising(&second) && iodide(&first)) {
        return None;
    }
    // Both reactants must dissolve for their ions to meet at all.
    if !(salt_solubility(&first.cation, &first.anion)?
        && salt_solubility(&second.cation, &second.anion)?)
    {
        return None;
    }
    let first_soluble = salt_solubility(&first.cation, &second.anion)?;
    let second_soluble = salt_solubility(&second.cation, &first.anion)?;
    if first_soluble && second_soluble {
        return Some(Verdict {
            reason: Some(NoReactionReason::AllProductsSoluble),
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    let products = vec![
        exchanged_salt(
            &first.cation,
            first.cation_charge,
            &second,
            Some(first_soluble),
        ),
        exchanged_salt(
            &second.cation,
            second.cation_charge,
            &first,
            Some(second_soluble),
        ),
    ];
    let observations = products
        .iter()
        .filter(|product| product.phase == ClaimPhase::Solid)
        .map(|product| ClaimObservation {
            predicate: ClaimObservationPredicate::Forms,
            subject: format!("a precipitate of {}", product.name),
            value: None,
        })
        .collect();
    Some(Verdict {
        reason: None,
        products,
        observations,
    })
}

/// Charge-balanced salt of a cation with another salt's anion.
fn exchanged_salt(
    cation: &str,
    cation_charge: u64,
    anion: &Salt,
    soluble: Option<bool>,
) -> ClaimProduct {
    let shared = gcd(cation_charge, anion.anion_charge);
    let cation_count = anion.anion_charge / shared;
    let anion_multiplicity = cation_charge / shared;
    let mut counts = BTreeMap::new();
    counts.insert(cation.to_owned(), cation_count);
    for (symbol, count) in &anion.anion {
        *counts.entry(symbol.clone()).or_insert(0) += count * anion_multiplicity;
    }
    let mut product = product_from_counts(&counts, Some((cation, cation_charge)));
    if anion_multiplicity > 1 && anion.anion.values().sum::<u64>() > 1 {
        // Repeated polyatomic units read conventionally: Mg(NO3)2, Cu(OH)2.
        let mut unit = String::new();
        let mut append = |symbol: &str, count: u64| {
            unit.push_str(symbol);
            if count > 1 {
                unit.push_str(&count.to_string());
            }
        };
        for (symbol, count) in &anion.anion {
            if symbol != "O" && symbol != "H" {
                append(symbol, *count);
            }
        }
        for tail in ["O", "H"] {
            if let Some(count) = anion.anion.get(tail) {
                append(tail, *count);
            }
        }
        let mut formula = cation.to_owned();
        if cation_count > 1 {
            formula.push_str(&cation_count.to_string());
        }
        product.formula = format!("{formula}({unit}){anion_multiplicity}");
    }
    product.phase = match soluble {
        Some(true) => ClaimPhase::Aqueous,
        Some(false) => ClaimPhase::Solid,
        None => ClaimPhase::Unknown,
    };
    product
}

/// Elemental metal + dissolved salt of a less active metal: the activity
/// series decides. A less active metal is a confident no-reaction unless
/// the cation sits above its lowest common charge (Cu + `FeCl3` etches
/// copper despite the series), where redox falls to the model.
fn solve_single_displacement(
    metal: &StructureDefinition,
    salt: &StructureDefinition,
) -> Option<Verdict> {
    if metal.representation() != RepresentationKind::Metallic {
        return None;
    }
    let (element, _) = single_element(metal)?;
    let salt = ionic_salt(salt)?;
    if element.as_str() == salt.cation {
        return None;
    }
    let metal_rank = chem_domain::activity_rank(element.as_str())?;
    let cation_rank = chem_domain::activity_rank(&salt.cation)?;
    // Displacement chemistry happens in solution.
    if salt_solubility(&salt.cation, &salt.anion) != Some(true) {
        return None;
    }
    if metal_rank > cation_rank {
        let lowest = chem_domain::lowest_cation_charge(&salt.cation)?;
        if i16::try_from(salt.cation_charge).ok()? != lowest {
            return None;
        }
        return Some(Verdict {
            reason: Some(NoReactionReason::LessActiveMetal {
                metal: display_name(element.as_str()),
                displaced: display_name(&salt.cation),
            }),
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    // A dissolved cation is a mild oxidant: the incoming metal only
    // reaches its mild aqueous state (Fe + CuSO4 → FeSO4, never Fe2(SO4)3).
    let charge = u64::try_from(chem_domain::aqueous_cation_charge(element.as_str())?).ok()?;
    let displaced = ClaimProduct {
        name: chem_domain::element_name(&salt.cation)?.to_owned(),
        formula: salt.cation.clone(),
        phase: ClaimPhase::Solid,
        identity_hints: Vec::new(),
    };
    let observations = vec![ClaimObservation {
        predicate: ClaimObservationPredicate::Forms,
        subject: format!("solid {}", displaced.name),
        value: None,
    }];
    Some(Verdict {
        reason: None,
        products: vec![
            exchanged_salt(
                element.as_str(),
                charge,
                &salt,
                salt_solubility(element.as_str(), &salt.anion),
            ),
            displaced,
        ],
        observations,
    })
}

/// Salt-style formula text: cation first, then non-O/H elements, then O,
/// then H. Molecular formulas without a cation follow the same tail order
/// with H promoted to the front (`H2O`, `HCl`, `H2S`).
fn product_from_counts(
    counts: &BTreeMap<String, u64>,
    cation: Option<(&str, u64)>,
) -> ClaimProduct {
    let mut formula = String::new();
    let mut append = |symbol: &str, count: u64| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };
    let cation_symbol = cation.map(|(symbol, _)| symbol);
    if let Some(symbol) = cation_symbol
        && let Some(count) = counts.get(symbol)
    {
        append(symbol, *count);
    }
    if cation.is_none()
        && let Some(count) = counts.get("H")
    {
        append("H", *count);
    }
    for (symbol, count) in counts {
        if Some(symbol.as_str()) == cation_symbol || symbol == "O" || symbol == "H" {
            continue;
        }
        append(symbol, *count);
    }
    if let Some(count) = counts.get("O") {
        append("O", *count);
    }
    if cation.is_some()
        && let Some(count) = counts.get("H")
    {
        append("H", *count);
    }
    let name = crate::naming::compound_name(counts, cation).unwrap_or_else(|| formula.clone());
    ClaimProduct {
        name,
        formula,
        phase: ClaimPhase::Unknown,
        identity_hints: Vec::new(),
    }
}

pub(crate) const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let swap = left % right;
        left = right;
        right = swap;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompiledClaimOutcome, ReactantInput, compile_claim_outcome};

    fn request(reactants: &[(&str, &[u8])]) -> ReactionBuildRequest {
        ReactionBuildRequest {
            reactants: reactants
                .iter()
                .map(|(display, atoms)| ReactantInput {
                    display: (*display).to_owned(),
                    atomic_numbers: atoms.to_vec(),
                    species_id: None,
                })
                .collect(),
            selected_context: None,
        }
    }

    fn contextual(display: &str, atoms: &[u8], context: &str) -> ReactionBuildRequest {
        ReactionBuildRequest {
            reactants: vec![ReactantInput {
                display: display.to_owned(),
                atomic_numbers: atoms.to_vec(),
                species_id: None,
            }],
            selected_context: Some(context.to_owned()),
        }
    }

    #[test]
    fn sulfuric_acid_and_sodium_hydroxide_solve_without_a_model() {
        let request = request(&[("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8]), ("NaOH", &[11, 8, 1])]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "Na2SO4"]);
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(
            outcome.equation().contains("Na2SO4"),
            "{}",
            outcome.equation()
        );
        assert!(
            outcome.species_without_structure().is_empty(),
            "every species should carry a generated structure: {:?}",
            outcome.species_without_structure()
        );
    }

    #[test]
    fn hydrochloric_acid_neutralization_solves() {
        let request = request(&[("HCl", &[1, 17]), ("NaOH", &[11, 8, 1])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "NaCl"]);
    }

    #[test]
    fn hydrogen_and_chlorine_synthesize_hydrogen_chloride() {
        let request = request(&[("H₂", &[1, 1]), ("Cl₂", &[17, 17])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].formula, "HCl");
    }

    #[test]
    fn acid_and_carbonate_solve_with_carbon_dioxide_evolution() {
        let request = request(&[("HCl", &[1, 17]), ("Na₂CO₃", &[11, 11, 6, 8, 8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "NaCl", "CO2"]);
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("carbon dioxide"))
        );
    }

    #[test]
    fn acid_and_bicarbonate_solve_the_same_family() {
        let request = request(&[
            ("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8]),
            ("NaHCO₃", &[11, 1, 6, 8, 8, 8]),
        ]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "Na2SO4", "CO2"]);
    }

    #[test]
    fn reactive_metal_and_acid_evolve_hydrogen() {
        let request = request(&[("Zn", &[30]), ("HCl", &[1, 17])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["ZnCl2", "H2"]);
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("hydrogen"))
        );
    }

    #[test]
    fn variable_charge_metals_stay_low_against_mild_oxidants() {
        // Dilute acid: iron(II), never iron(III).
        let acid = request(&[("Fe", &[26]), ("HCl", &[1, 17])]);
        let claim = solve_reaction_claim(&acid, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "FeCl2");
        assert_eq!(claim.products[0].name, "iron(II) chloride");

        // Displacement from a dissolved salt: iron(II) sulfate.
        let displacement = request(&[("Fe", &[26]), ("CuSO₄", &[29, 16, 8, 8, 8, 8])]);
        let claim =
            solve_reaction_claim(&displacement, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "FeSO4");
        assert_eq!(claim.products[0].name, "iron(II) sulfate");
        assert_eq!(claim.products[1].formula, "Cu");

        // Sulfur and iodine are mild; chlorine reaches iron(III).
        let sulfide = request(&[("Fe", &[26]), ("S₈", &[16; 8])]);
        let claim = solve_reaction_claim(&sulfide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "FeS");
        assert_eq!(claim.products[0].name, "iron(II) sulfide");

        let iodide = request(&[("Fe", &[26]), ("I₂", &[53, 53])]);
        let claim = solve_reaction_claim(&iodide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "FeI2");

        let chloride = request(&[("Fe", &[26]), ("Cl₂", &[17, 17])]);
        let claim = solve_reaction_claim(&chloride, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "FeCl3");

        // Copper is the aqueous exception (Cu+ disproportionates in water),
        // but dry synthesis genuinely reaches copper(I).
        let copper_sulfide = request(&[("Cu", &[29]), ("S₈", &[16; 8])]);
        let claim =
            solve_reaction_claim(&copper_sulfide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "Cu2S");
        assert_eq!(claim.products[0].name, "copper(I) sulfide");

        let copper_iodide = request(&[("Cu", &[29]), ("I₂", &[53, 53])]);
        let claim =
            solve_reaction_claim(&copper_iodide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "CuI");
    }

    #[test]
    fn noble_metal_and_acid_is_a_confident_no_reaction() {
        let request = request(&[("Cu", &[29]), ("HCl", &[1, 17])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
        assert!(claim.products.is_empty());
        assert_eq!(
            claim.no_reaction_reason,
            Some(NoReactionReason::BelowHydrogen {
                metal: "copper".to_owned()
            })
        );
    }

    #[test]
    fn methane_combustion_solves() {
        let request = request(&[("CH₄", &[6, 1, 1, 1, 1]), ("O₂", &[8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CO2", "H2O"]);
    }

    #[test]
    fn ethanol_combustion_solves() {
        let request = request(&[("C₂H₆O", &[6, 6, 8, 1, 1, 1, 1, 1, 1]), ("O₂", &[8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CO2", "H2O"]);
    }

    #[test]
    fn benzene_combustion_solves() {
        let request = request(&[
            ("C₆H₆", &[6, 6, 6, 6, 6, 6, 1, 1, 1, 1, 1, 1]),
            ("O₂", &[8, 8]),
        ]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CO2", "H2O"]);
    }

    #[test]
    fn methane_and_chlorine_substitute_under_light() {
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "methane".to_owned(),
                    atomic_numbers: vec![6, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "chlorine".to_owned(),
                    atomic_numbers: vec![17, 17],
                    species_id: None,
                },
            ],
            selected_context: Some("light".to_owned()),
        };
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        let names: Vec<&str> = claim
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["chloromethane", "hydrogen chloride"]);
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
        assert!(matches!(outcome, CompiledClaimOutcome::Static(_)));

        // Without light nothing fires; with an ambiguous position (propane)
        // nothing fires either.
        let dark = ReactionBuildRequest {
            selected_context: None,
            ..request
        };
        assert!(solve_reaction_claim(&dark, &registry).is_none());
        let propane = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "propane".to_owned(),
                    atomic_numbers: vec![6, 6, 6, 1, 1, 1, 1, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "chlorine".to_owned(),
                    atomic_numbers: vec![17, 17],
                    species_id: None,
                },
            ],
            selected_context: Some("light".to_owned()),
        };
        assert!(
            solve_reaction_claim(&propane, &registry).is_none(),
            "positional ambiguity fails closed"
        );
    }

    #[test]
    fn ethene_decolourises_bromine_to_the_dibromide() {
        let request = request(&[("ethene", &[6, 6, 1, 1, 1, 1]), ("bromine", &[35, 35])]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].name, "1,2-dibromoethane");
        assert_eq!(claim.products[0].formula, "C2H4Br2");
        assert!(
            claim.products[0]
                .identity_hints
                .iter()
                .any(|hint| hint.kind == ClaimIdentityHintKind::CanonicalSmiles)
        );
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject == "bromine")
        );
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(outcome.species_without_structure().is_empty());
    }

    #[test]
    fn stereo_alkenes_hydrogenate_like_their_plain_isomer() {
        let request = request(&[
            ("trans-but-2-ene", &[6, 6, 6, 6, 1, 1, 1, 1, 1, 1, 1, 1]),
            ("hydrogen", &[1, 1]),
        ]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.products[0].name, "butane");
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
        assert!(matches!(outcome, CompiledClaimOutcome::Static(_)));
    }

    #[test]
    fn ethene_hydrogenates_to_ethane() {
        let request = request(&[("hydrogen", &[1, 1]), ("ethene", &[6, 6, 1, 1, 1, 1])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].name, "ethane");
        assert_eq!(claim.products[0].formula, "C2H6");
    }

    #[test]
    fn ethanoic_acid_and_ethanol_esterify() {
        let acid: &[u8] = &[6, 6, 8, 8, 1, 1, 1, 1];
        let alcohol: &[u8] = &[6, 6, 8, 1, 1, 1, 1, 1, 1];
        for reactants in [
            [("ethanoic acid", acid), ("ethanol", alcohol)],
            [("ethanol", alcohol), ("ethanoic acid", acid)],
        ] {
            let request = request(&reactants);
            let registry = SpeciesRegistry::default();
            let claim = solve_reaction_claim(&request, &registry).expect("solved");
            let names: Vec<&str> = claim
                .products
                .iter()
                .map(|product| product.name.as_str())
                .collect();
            assert_eq!(names, ["ethyl ethanoate", "Water"]);
            let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
            let CompiledClaimOutcome::Static(outcome) = outcome else {
                panic!("expected static outcome");
            };
            assert!(
                outcome.species_without_structure().is_empty(),
                "{:?}",
                outcome.species_without_structure()
            );
        }
    }

    #[test]
    fn ethanol_dehydrates_to_ethene_under_heat() {
        let request = contextual("ethanol", &[6, 6, 8, 1, 1, 1, 1, 1, 1], "heat");
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let names: Vec<&str> = claim
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["ethene", "Water"]);
    }

    #[test]
    fn sketched_smiles_displays_compile_and_balance() {
        // A sketched draft's display is its SMILES; "CCO" must not be
        // mistaken for the formula C2O.
        let request = contextual("CCO", &[6, 6, 8, 1, 1, 1, 1, 1, 1], "heat");
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(
            outcome.equation().contains("C2H6O"),
            "{}",
            outcome.equation()
        );
    }

    #[test]
    fn limited_oxygen_combustion_stops_at_carbon_monoxide() {
        let methane: &[u8] = &[6, 1, 1, 1, 1];
        let oxygen: &[u8] = &[8, 8];
        let limited = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "methane".to_owned(),
                    atomic_numbers: methane.to_vec(),
                    species_id: None,
                },
                ReactantInput {
                    display: "oxygen".to_owned(),
                    atomic_numbers: oxygen.to_vec(),
                    species_id: None,
                },
            ],
            selected_context: Some("limited oxygen".to_owned()),
        };
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&limited, &registry).expect("solved");
        let formulas: Vec<&str> = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect();
        assert_eq!(formulas, ["CO", "H2O"]);
        let outcome = compile_claim_outcome(&limited, claim, &registry).expect("balanced");
        assert!(matches!(outcome, CompiledClaimOutcome::Static(_)));
        // The unconditioned pair still burns completely.
        let plain = ReactionBuildRequest {
            selected_context: None,
            ..limited
        };
        let claim = solve_reaction_claim(&plain, &registry).expect("solved");
        assert_eq!(claim.products[0].formula, "CO2");
    }

    #[test]
    fn esters_hydrolyse_back_to_acid_and_alcohol_under_heat() {
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "ethyl ethanoate".to_owned(),
                    atomic_numbers: vec![6, 6, 6, 6, 8, 8, 1, 1, 1, 1, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "water".to_owned(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ],
            selected_context: Some("heat".to_owned()),
        };
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        let names: Vec<&str> = claim
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["ethanoic acid", "ethanol"]);
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced");
        assert!(matches!(outcome, CompiledClaimOutcome::Static(_)));
    }

    #[test]
    fn alcohols_oxidise_with_a_catalyst_instead_of_burning() {
        let ethanol: &[u8] = &[6, 6, 8, 1, 1, 1, 1, 1, 1];
        let oxygen: &[u8] = &[8, 8];
        let request = |context: Option<&str>| ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "ethanol".to_owned(),
                    atomic_numbers: ethanol.to_vec(),
                    species_id: None,
                },
                ReactantInput {
                    display: "oxygen".to_owned(),
                    atomic_numbers: oxygen.to_vec(),
                    species_id: None,
                },
            ],
            selected_context: context.map(str::to_owned),
        };
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request(Some("catalyst")), &registry).expect("solved");
        let names: Vec<&str> = claim
            .products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        assert_eq!(names, ["ethanal", "Water"]);
        // A secondary alcohol gives the ketone.
        let isopropanol = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "propan-2-ol".to_owned(),
                    atomic_numbers: vec![6, 6, 6, 8, 1, 1, 1, 1, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "oxygen".to_owned(),
                    atomic_numbers: oxygen.to_vec(),
                    species_id: None,
                },
            ],
            selected_context: Some("catalyst".to_owned()),
        };
        let claim = solve_reaction_claim(&isopropanol, &registry).expect("solved");
        assert_eq!(claim.products[0].name, "propanone");
        // Without the catalyst the alcohol just burns.
        let claim = solve_reaction_claim(&request(None), &registry).expect("solved");
        assert_eq!(claim.products[0].formula, "CO2");
    }

    #[test]
    fn hydrogen_halides_add_by_markovnikov() {
        let registry = SpeciesRegistry::default();
        // Propene + HBr: the bromine lands on the middle carbon.
        let request = request(&[("propene", &[6, 6, 6, 1, 1, 1, 1, 1, 1]), ("HBr", &[1, 35])]);
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.products[0].name, "2-bromopropane");
        // Symmetric alkene: unambiguous without the rule.
        let symmetric = request_pair(
            ("but-2-ene", &[6, 6, 6, 6, 1, 1, 1, 1, 1, 1, 1, 1]),
            ("HBr", &[1, 35]),
        );
        let claim = solve_reaction_claim(&symmetric, &registry).expect("solved");
        assert_eq!(claim.products[0].name, "2-bromobutane");
    }

    fn request_pair(first: (&str, &[u8]), second: (&str, &[u8])) -> ReactionBuildRequest {
        request(&[first, second])
    }

    #[test]
    fn ethers_and_acids_do_not_dehydrate() {
        // Dimethyl ether has no hydroxyl; ethanoic acid's is a carboxyl.
        for (name, atoms) in [
            ("dimethyl ether", &[6, 6, 8, 1, 1, 1, 1, 1, 1][..]),
            ("ethanoic acid", &[6, 6, 8, 8, 1, 1, 1, 1][..]),
        ] {
            let request = contextual(name, atoms, "heat");
            assert!(
                solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none(),
                "{name} must not dehydrate"
            );
        }
    }

    #[test]
    fn ammonium_cyanate_heated_isomerizes_to_urea() {
        let request = contextual("ammonium cyanate", &[7, 1, 1, 1, 1, 7, 6, 8], "heat");
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].name, "urea");
        assert_eq!(claim.products[0].formula, "CO(NH2)2");
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(
            outcome.species_without_structure().is_empty(),
            "reactant and product both carry generated structures: {:?}",
            outcome.species_without_structure()
        );
        // The isomer pair stays structurally distinct end to end: ionic
        // ammonium cyanate in, molecular urea out.
        let representation = |species: &crate::OutcomeSpecies| match species {
            crate::OutcomeSpecies::Resolved(resolved) => resolved
                .structure
                .as_ref()
                .map(chem_domain::StructureDefinition::representation),
            crate::OutcomeSpecies::FormulaOnly { .. } => None,
        };
        assert_eq!(
            representation(&outcome.reactants()[0]),
            Some(RepresentationKind::Ionic)
        );
        assert_eq!(
            representation(&outcome.products()[0]),
            Some(RepresentationKind::Molecular)
        );
    }

    #[test]
    fn heating_urea_itself_stays_unsolved() {
        // Molecular CH4N2O is urea; the Wöhler arm only fires for the ionic
        // salt, so urea + heat falls through to the model.
        let request = contextual("urea", &[6, 1, 1, 1, 1, 7, 7, 8], "heat");
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn carbonate_decomposition_solves_under_heat() {
        let request = contextual("CaCO₃", &[20, 6, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.required_context, "heat");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CaO", "CO2"]);
    }

    #[test]
    fn bicarbonate_decomposition_solves_under_heat() {
        let request = contextual("NaHCO₃", &[11, 1, 6, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["Na2CO3", "H2O", "CO2"]);
    }

    #[test]
    fn hydroxide_decomposition_and_alkali_stability() {
        let calcium = contextual("Ca(OH)₂", &[20, 8, 8, 1, 1], "heat");
        let claim = solve_reaction_claim(&calcium, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CaO", "H2O"]);

        let sodium = contextual("NaOH", &[11, 8, 1], "heat");
        let claim = solve_reaction_claim(&sodium, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);

        let sodium_carbonate = contextual("Na₂CO₃", &[11, 11, 6, 8, 8, 8], "heat");
        let claim =
            solve_reaction_claim(&sodium_carbonate, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
    }

    #[test]
    fn nitrates_decompose_by_cation_tier_under_heat() {
        // Sodium-tier alkali: nitrite + oxygen.
        let saltpetre = contextual("KNO₃", &[19, 7, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&saltpetre, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["KNO2", "O2"]);
        assert_eq!(claim.products[0].name, "potassium nitrite");

        // Lithium is the classic exception: oxide + brown gas + oxygen.
        let lithium = contextual("LiNO₃", &[3, 7, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&lithium, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["Li2O", "NO2", "O2"]);

        let copper = contextual("Cu(NO₃)₂", &[29, 7, 8, 8, 8, 7, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&copper, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CuO", "NO2", "O2"]);
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("brown"))
        );
    }

    #[test]
    fn silver_halides_photolyse_under_light() {
        let chloride = contextual("AgCl", &[47, 17], "light");
        let claim = solve_reaction_claim(&chloride, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.required_context, "light");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["Ag", "Cl2"]);
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("silver"))
        );

        let bromide = contextual("AgBr", &[47, 35], "light");
        let claim = solve_reaction_claim(&bromide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[1].formula, "Br2");

        // Iodide is sluggish and sodium chloride is light-stable: no verdicts.
        let iodide = contextual("AgI", &[47, 53], "light");
        assert!(solve_reaction_claim(&iodide, &SpeciesRegistry::default()).is_none());
        let table_salt = contextual("NaCl", &[11, 17], "light");
        assert!(solve_reaction_claim(&table_salt, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn water_electrolysis_solves_under_electricity() {
        let request = contextual("H₂O", &[1, 1, 8], "electricity");
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.required_context, "electricity");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2", "O2"]);

        // Heat alone does not electrolyse water.
        let heated = contextual("H₂O", &[1, 1, 8], "heat");
        assert!(solve_reaction_claim(&heated, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn solver_products_carry_systematic_names() {
        let neutralization = request(&[("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8]), ("NaOH", &[11, 8, 1])]);
        let claim =
            solve_reaction_claim(&neutralization, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[1].name, "sodium sulfate");

        let displacement = request(&[("Zn", &[30]), ("HCl", &[1, 17])]);
        let claim =
            solve_reaction_claim(&displacement, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].name, "zinc chloride");

        let iron = request(&[("Fe", &[26]), ("Cl₂", &[17, 17])]);
        let claim = solve_reaction_claim(&iron, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].name, "iron(III) chloride");

        let lime = contextual("CaCO₃", &[20, 6, 8, 8, 8], "heat");
        let claim = solve_reaction_claim(&lime, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].name, "calcium oxide");

        let nitric = request(&[("HNO₃", &[1, 7, 8, 8, 8]), ("NaOH", &[11, 8, 1])]);
        let claim = solve_reaction_claim(&nitric, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[1].name, "sodium nitrate");

        let hydride = request(&[("H₂", &[1, 1]), ("Cl₂", &[17, 17])]);
        let claim = solve_reaction_claim(&hydride, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].name, "hydrogen chloride");
    }

    #[test]
    fn silver_nitrate_and_sodium_chloride_precipitate_silver_chloride() {
        let request = request(&[("AgNO₃", &[47, 7, 8, 8, 8]), ("NaCl", &[11, 17])]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        let products = claim
            .products
            .iter()
            .map(|product| (product.formula.as_str(), product.phase))
            .collect::<Vec<_>>();
        assert_eq!(
            products,
            [("AgCl", ClaimPhase::Solid), ("NaNO3", ClaimPhase::Aqueous)]
        );
        assert_eq!(claim.products[0].name, "silver chloride");
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("precipitate"))
        );
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(
            outcome.species_without_structure().is_empty(),
            "every species should carry a generated structure: {:?}",
            outcome.species_without_structure()
        );
    }

    #[test]
    fn barium_chloride_and_sodium_sulfate_precipitate_barium_sulfate() {
        let request = request(&[
            ("BaCl₂", &[56, 17, 17]),
            ("Na₂SO₄", &[11, 11, 16, 8, 8, 8, 8]),
        ]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let products = claim
            .products
            .iter()
            .map(|product| (product.formula.as_str(), product.phase))
            .collect::<Vec<_>>();
        assert_eq!(
            products,
            [("BaSO4", ClaimPhase::Solid), ("NaCl", ClaimPhase::Aqueous)]
        );
        assert_eq!(claim.products[0].name, "barium sulfate");
    }

    #[test]
    fn copper_sulfate_and_sodium_hydroxide_precipitate_the_hydroxide() {
        let request = request(&[("CuSO₄", &[29, 16, 8, 8, 8, 8]), ("NaOH", &[11, 8, 1])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let precipitate = &claim.products[0];
        assert_eq!(precipitate.formula, "Cu(OH)2");
        assert_eq!(precipitate.name, "copper(II) hydroxide");
        assert_eq!(precipitate.phase, ClaimPhase::Solid);
        assert_eq!(claim.products[1].formula, "Na2SO4");
    }

    #[test]
    fn lead_nitrate_and_potassium_iodide_precipitate_lead_iodide() {
        let request = request(&[
            ("Pb(NO₃)₂", &[82, 7, 8, 8, 8, 7, 8, 8, 8]),
            ("KI", &[19, 53]),
        ]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let precipitate = &claim.products[0];
        assert_eq!(precipitate.formula, "PbI2");
        assert_eq!(precipitate.name, "lead(II) iodide");
        assert_eq!(precipitate.phase, ClaimPhase::Solid);
        assert_eq!(claim.products[1].formula, "KNO3");
    }

    #[test]
    fn zinc_displaces_copper_from_its_sulfate() {
        let request = request(&[("Zn", &[30]), ("CuSO₄", &[29, 16, 8, 8, 8, 8])]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        let products = claim
            .products
            .iter()
            .map(|product| {
                (
                    product.formula.as_str(),
                    product.name.as_str(),
                    product.phase,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            products,
            [
                ("ZnSO4", "zinc sulfate", ClaimPhase::Aqueous),
                ("Cu", "copper", ClaimPhase::Solid),
            ]
        );
        let outcome = compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(
            outcome.species_without_structure().is_empty(),
            "every species should carry a generated structure: {:?}",
            outcome.species_without_structure()
        );
    }

    #[test]
    fn copper_displaces_silver_from_its_nitrate() {
        let request = request(&[("Cu", &[29]), ("AgNO₃", &[47, 7, 8, 8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "Cu(NO3)2");
        assert_eq!(claim.products[0].name, "copper(II) nitrate");
        assert_eq!(claim.products[1].formula, "Ag");
        assert_eq!(claim.products[1].phase, ClaimPhase::Solid);
    }

    #[test]
    fn less_active_metal_and_salt_is_a_confident_no_reaction() {
        let request = request(&[("Cu", &[29]), ("ZnSO₄", &[30, 16, 8, 8, 8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
        assert!(claim.products.is_empty());
        assert_eq!(
            claim.no_reaction_reason,
            Some(NoReactionReason::LessActiveMetal {
                metal: "copper".to_owned(),
                displaced: "zinc".to_owned()
            })
        );
    }

    #[test]
    fn copper_and_iron_iii_chloride_falls_to_the_model() {
        // The series says no reaction, but Fe3+ etches copper anyway.
        let request = request(&[("Cu", &[29]), ("FeCl₃", &[26, 17, 17, 17])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn fully_soluble_exchange_is_a_confident_no_reaction() {
        let request = request(&[("NaCl", &[11, 17]), ("KNO₃", &[19, 7, 8, 8, 8])]);
        let claim = solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
        assert!(claim.products.is_empty());
        assert_eq!(
            claim.no_reaction_reason,
            Some(NoReactionReason::AllProductsSoluble)
        );
    }

    #[test]
    fn redox_prone_and_borderline_exchanges_fall_to_the_model() {
        // Fe³⁺ oxidises iodide instead of exchanging.
        let redox = request(&[("FeCl₃", &[26, 17, 17, 17]), ("KI", &[19, 53])]);
        assert!(solve_reaction_claim(&redox, &SpeciesRegistry::default()).is_none());

        // CaSO4 is borderline soluble; no confident verdict either way.
        let borderline = request(&[
            ("CaCl₂", &[20, 17, 17]),
            ("Na₂SO₄", &[11, 11, 16, 8, 8, 8, 8]),
        ]);
        assert!(solve_reaction_claim(&borderline, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn metal_oxides_neutralize_acids() {
        let copper = request(&[("CuO", &[29, 8]), ("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8])]);
        let claim = solve_reaction_claim(&copper, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "CuSO4"]);
        assert_eq!(claim.products[1].name, "copper(II) sulfate");

        let soda = request(&[("HCl", &[1, 17]), ("Na₂O", &[11, 11, 8])]);
        let claim = solve_reaction_claim(&soda, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "NaCl"]);
    }

    #[test]
    fn acid_anhydrides_hydrate_to_oxoacids() {
        let sulfur_trioxide = request(&[("SO₃", &[16, 8, 8, 8]), ("H₂O", &[1, 1, 8])]);
        let claim =
            solve_reaction_claim(&sulfur_trioxide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].formula, "H2SO4");
        assert_eq!(claim.products[0].name, "sulfuric acid");

        let carbon_dioxide = request(&[("H₂O", &[1, 1, 8]), ("CO₂", &[6, 8, 8])]);
        let claim =
            solve_reaction_claim(&carbon_dioxide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "H2CO3");
        assert_eq!(claim.products[0].name, "carbonic acid");
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("acidic"))
        );
    }

    #[test]
    fn reactive_metal_oxides_slake_and_noble_ones_sit_still() {
        let quicklime = request(&[("CaO", &[20, 8]), ("H₂O", &[1, 1, 8])]);
        let claim = solve_reaction_claim(&quicklime, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].formula, "Ca(OH)2");
        assert_eq!(claim.products[0].name, "calcium hydroxide");

        let soda = request(&[("Na₂O", &[11, 11, 8]), ("H₂O", &[1, 1, 8])]);
        let claim = solve_reaction_claim(&soda, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "NaOH");

        let copper = request(&[("CuO", &[29, 8]), ("H₂O", &[1, 1, 8])]);
        let claim = solve_reaction_claim(&copper, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);

        // MgO reacts slowly and NO2 disproportionates: no confident verdict.
        let magnesia = request(&[("MgO", &[12, 8]), ("H₂O", &[1, 1, 8])]);
        assert!(solve_reaction_claim(&magnesia, &SpeciesRegistry::default()).is_none());
        let dioxide = request(&[("NO₂", &[7, 8, 8]), ("H₂O", &[1, 1, 8])]);
        assert!(solve_reaction_claim(&dioxide, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn reactive_metals_turn_water_into_hydroxide_and_hydrogen() {
        let sodium = request(&[("Na", &[11]), ("H₂O", &[1, 1, 8])]);
        let claim = solve_reaction_claim(&sodium, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["NaOH", "H2"]);
        assert_eq!(claim.products[0].name, "sodium hydroxide");
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.subject.contains("hydrogen"))
        );

        // Order-independent, and Ca(OH)2 formats conventionally.
        let calcium = request(&[("H₂O", &[1, 1, 8]), ("Ca", &[20])]);
        let claim = solve_reaction_claim(&calcium, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "Ca(OH)2");

        let copper = request(&[("Cu", &[29]), ("H₂O", &[1, 1, 8])]);
        let claim = solve_reaction_claim(&copper, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);

        // The steam-only band has no cold-water verdict.
        let magnesium = request(&[("Mg", &[12]), ("H₂O", &[1, 1, 8])]);
        assert!(solve_reaction_claim(&magnesium, &SpeciesRegistry::default()).is_none());
        let iron = request(&[("Fe", &[26]), ("H₂O", &[1, 1, 8])]);
        assert!(solve_reaction_claim(&iron, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn metal_oxides_do_not_claim_heat_decomposition() {
        let heated = contextual("CuO", &[29, 8], "heat");
        assert!(solve_reaction_claim(&heated, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn methane_is_not_treated_as_an_acid() {
        let request = request(&[("CH₄", &[6, 1, 1, 1, 1]), ("NaOH", &[11, 8, 1])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn multi_oxide_synthesis_is_left_to_the_model() {
        // SO, SO2, and SO3 are all structurally valid; which one forms is
        // empirical, so the claim falls to the model (its structures and
        // mechanism still derive algorithmically afterwards).
        let request = request(&[("S₈", &[16; 8]), ("O₂", &[8, 8])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn charge_balanced_synthesis_solves_metal_salts_and_hydrides() {
        let magnesium = request(&[("Mg", &[12]), ("N₂", &[7, 7])]);
        let claim = solve_reaction_claim(&magnesium, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "Mg3N2");

        let hydrogen_sulfide = request(&[("H₂", &[1, 1]), ("S₈", &[16; 8])]);
        let claim =
            solve_reaction_claim(&hydrogen_sulfide, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "H2S");
    }

    #[test]
    fn trivially_inert_pairings_are_confident_no_reactions() {
        let no_reaction = |reactants: &[(&str, &[u8])]| {
            let claim = solve_reaction_claim(&request(reactants), &SpeciesRegistry::default())
                .expect("verdict");
            assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
            assert!(claim.products.is_empty());
            assert!(
                claim.no_reaction_reason.is_some(),
                "reason for {reactants:?}"
            );
        };
        // Light noble gases with anything.
        no_reaction(&[("He", &[2]), ("O₂", &[8, 8])]);
        no_reaction(&[("NaCl", &[11, 17]), ("Ar", &[18])]);
        // Two metals only alloy.
        no_reaction(&[("Cu", &[29]), ("Zn", &[30])]);
        // A substance and itself.
        no_reaction(&[("O₂", &[8, 8]), ("O₂", &[8, 8])]);
        no_reaction(&[("H₂O", &[1, 1, 8]), ("H₂O", &[1, 1, 8])]);

        // Xenon genuinely fluorinates; no verdict.
        let xenon = request(&[("Xe", &[54]), ("F₂", &[9, 9])]);
        assert!(solve_reaction_claim(&xenon, &SpeciesRegistry::default()).is_none());
    }

    #[test]
    fn identity_verdicts_survive_missing_structures() {
        // Tungsten has no generator facts, so no structure generates — but
        // element identity still settles these pairings.
        no_reaction_claim(&[("W", &[74]), ("He", &[2])]);
        no_reaction_claim(&[("W", &[74]), ("Cu", &[29])]);
    }

    fn no_reaction_claim(reactants: &[(&str, &[u8])]) {
        let claim = solve_reaction_claim(&request(reactants), &SpeciesRegistry::default())
            .expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
        assert!(claim.products.is_empty());
        assert!(
            claim.no_reaction_reason.is_some(),
            "reason for {reactants:?}"
        );
    }

    #[test]
    fn reactive_halogens_displace_less_reactive_halides() {
        let displacement = request(&[("Cl₂", &[17, 17]), ("KBr", &[19, 35])]);
        let claim =
            solve_reaction_claim(&displacement, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["KCl", "Br2"]);
        assert_eq!(claim.products[1].name, "bromine");
        assert!(
            claim
                .observations
                .iter()
                .any(|observation| observation.value.as_deref() == Some("orange"))
        );

        // The reverse is a confident no-reaction.
        let reverse = request(&[("I₂", &[53, 53]), ("NaCl", &[11, 17])]);
        let claim = solve_reaction_claim(&reverse, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);

        // Fluorine attacks water first; silver bromide never dissolves.
        let fluorine = request(&[("F₂", &[9, 9]), ("NaCl", &[11, 17])]);
        assert!(solve_reaction_claim(&fluorine, &SpeciesRegistry::default()).is_none());
        let silver = request(&[("Cl₂", &[17, 17]), ("AgBr", &[47, 35])]);
        assert!(solve_reaction_claim(&silver, &SpeciesRegistry::default()).is_none());
    }
}
