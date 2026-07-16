//! Algorithmic reaction solving.
//!
//! Predicts products for reaction families that follow deterministically
//! from reactant structure: acid-base neutralization and binary synthesis.
//! The output is an ordinary [`ReactionClaim`]; downstream exact balancing
//! and structural validation gate it exactly like a model claim. Anything
//! outside these families returns None so the caller can fall back to the
//! model.

use std::collections::BTreeMap;

use chem_domain::{
    ElementInventory, ElementSymbol, RepresentationKind, SpeciesRegistry, StructureDefinition,
    StructureId, classify_bronsted_acid, generate_structure,
};

use crate::{
    ClaimDisposition, ClaimObservation, ClaimObservationPredicate, ClaimPhase, ClaimProduct,
    OutcomeSpecies, ReactionBuildRequest, ReactionClaim, RequestIdentityResolution,
    claim::REACTION_CLAIM_SCHEMA_VERSION, resolve_request_identities,
};

/// Donor elements that make a proton-donor site an acid site in practice;
/// carbon acids stay out so alkanes never neutralize.
const ACIDIC_DONORS: [&str; 6] = ["O", "F", "Cl", "Br", "I", "S"];

/// Attempts to solve the request without a model. Returns a fully formed
/// reaction claim, or None when no deterministic family applies.
#[must_use]
pub fn solve_reaction_claim(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Option<ReactionClaim> {
    if request.reactants.len() != 2 {
        return None;
    }
    let Ok(RequestIdentityResolution::Resolved(species)) =
        resolve_request_identities(request, identities)
    else {
        return None;
    };
    let structures = species
        .iter()
        .map(|entry| match entry {
            OutcomeSpecies::Resolved(resolved) => resolved.structure.as_ref(),
            OutcomeSpecies::FormulaOnly { .. } => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let verdict = solve_acid_base(structures[0], structures[1])
        .or_else(|| solve_acid_base(structures[1], structures[0]))
        .or_else(|| solve_acid_metal(structures[0], structures[1]))
        .or_else(|| solve_acid_metal(structures[1], structures[0]))
        .or_else(|| solve_combustion(structures[0], structures[1]))
        .or_else(|| solve_combustion(structures[1], structures[0]))
        .or_else(|| {
            solve_synthesis(structures[0], structures[1]).map(|products| Verdict {
                products,
                observations: Vec::new(),
            })
        })?;
    let disposition = if verdict.products.is_empty() {
        ClaimDisposition::NoReaction
    } else {
        ClaimDisposition::Reaction
    };
    Some(ReactionClaim {
        schema_version: REACTION_CLAIM_SCHEMA_VERSION,
        disposition,
        products: verdict.products,
        required_context: String::new(),
        observations: verdict.observations,
        sources: Vec::new(),
        ambiguity: None,
    })
}

/// A solved outcome: an empty product list is a confident "no reaction".
struct Verdict {
    products: Vec<ClaimProduct>,
    observations: Vec<ClaimObservation>,
}

/// Acid + ionic base (hydroxide, carbonate, or bicarbonate) → salt + water,
/// with carbon dioxide evolution for the carbonates.
fn solve_acid_base(
    acid: &StructureDefinition,
    base: &StructureDefinition,
) -> Option<Verdict> {
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
        product_from_counts(&salt, Some(&cation)),
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
        products,
        observations,
    })
}

/// Acid + elemental metal: salt + hydrogen above the hydrogen pivot in the
/// activity series, a confident no-reaction below it.
fn solve_acid_metal(
    acid: &StructureDefinition,
    metal: &StructureDefinition,
) -> Option<Verdict> {
    let donors = acid_donor_count(acid)?;
    if metal.representation() != RepresentationKind::Metallic {
        return None;
    }
    let (element, _) = single_element(metal)?;
    let charge = chem_domain::common_cation_charge(element.as_str())?;
    if !chem_domain::displaces_hydrogen_from_acids(element.as_str())? {
        return Some(Verdict {
            products: Vec::new(),
            observations: Vec::new(),
        });
    }
    let cation = element.as_str().to_owned();
    let salt = conjugate_salt(acid, donors, &cation, u64::try_from(charge).ok()?)?;
    Some(Verdict {
        products: vec![
            product_from_counts(&salt, Some(&cation)),
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
fn solve_combustion(
    fuel: &StructureDefinition,
    oxidizer: &StructureDefinition,
) -> Option<Verdict> {
    let (oxidizer_element, _) = single_element(oxidizer)?;
    if oxidizer_element.as_str() != "O" {
        return None;
    }
    if fuel.representation() != RepresentationKind::Molecular {
        return None;
    }
    let elements = fuel.formula().elements();
    let allowed = elements
        .keys()
        .all(|symbol| matches!(symbol.as_str(), "C" | "H" | "O"));
    let has_carbon = elements.keys().any(|symbol| symbol.as_str() == "C");
    let has_hydrogen = elements.keys().any(|symbol| symbol.as_str() == "H");
    if !(allowed && has_carbon && has_hydrogen) {
        return None;
    }
    Some(Verdict {
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
    let cation_of = |symbol: &ElementSymbol| chem_domain::common_cation_charge(symbol.as_str());
    let anion_of = |symbol: &ElementSymbol| chem_domain::anion_valence_charge(symbol.as_str());

    // Metal + nonmetal, or hydride: exact charge balance.
    let charge_pair = match (cation_of(&left_element), cation_of(&right_element)) {
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
        let cation = (structure.representation() == RepresentationKind::Ionic)
            .then(|| positive.as_str().to_owned());
        return Some(vec![product_from_counts(&counts, cation.as_ref())]);
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
    Hydroxide,
    Carbonate,
    Bicarbonate,
}

fn base_anion_kind(sorted_elements: &[&str]) -> Option<BaseAnion> {
    match sorted_elements {
        ["H", "O"] => Some(BaseAnion::Hydroxide),
        ["C", "O", "O", "O"] => Some(BaseAnion::Carbonate),
        ["C", "H", "O", "O", "O"] => Some(BaseAnion::Bicarbonate),
        _ => None,
    }
}

/// One ionic base: every group is either a single-atom cation of one
/// element, or one consistent basic anion (hydroxide/carbonate/bicarbonate).
fn ionic_base(base: &StructureDefinition) -> Option<(String, u64, BaseAnion)> {
    if base.representation() != RepresentationKind::Ionic {
        return None;
    }
    let graph = base.graph();
    let mut cation: Option<(String, u64)> = None;
    let mut anion_kind: Option<BaseAnion> = None;
    for group in graph.groups().values() {
        let members = group
            .atoms()
            .iter()
            .map(|id| &graph.atoms()[id])
            .collect::<Vec<_>>();
        let elements = members
            .iter()
            .map(|atom| atom.element().as_str())
            .collect::<Vec<_>>();
        match elements.as_slice() {
            [only] if members[0].electrons().formal_charge() > 0 => {
                let found = (
                    (*only).to_owned(),
                    u64::from(members[0].electrons().formal_charge().unsigned_abs()),
                );
                if cation.get_or_insert_with(|| found.clone()) != &found {
                    return None;
                }
            }
            _ => {
                let mut sorted = elements.clone();
                sorted.sort_unstable();
                let kind = base_anion_kind(&sorted)?;
                if *anion_kind.get_or_insert(kind) != kind {
                    return None;
                }
            }
        }
    }
    let (cation, charge) = cation?;
    Some((cation, charge, anion_kind?))
}

/// Salt-style formula text: cation first, then non-O/H elements, then O,
/// then H. Molecular formulas without a cation follow the same tail order
/// with H promoted to the front (`H2O`, `HCl`, `H2S`).
fn product_from_counts(counts: &BTreeMap<String, u64>, cation: Option<&String>) -> ClaimProduct {
    let mut formula = String::new();
    let mut append = |symbol: &str, count: u64| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };
    if let Some(cation) = cation
        && let Some(count) = counts.get(cation)
    {
        append(cation, *count);
    }
    if cation.is_none()
        && let Some(count) = counts.get("H")
    {
        append("H", *count);
    }
    for (symbol, count) in counts {
        if Some(symbol) == cation || symbol == "O" || symbol == "H" {
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
    ClaimProduct {
        name: formula.clone(),
        formula,
        phase: ClaimPhase::Unknown,
        identity_hints: Vec::new(),
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
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

    #[test]
    fn sulfuric_acid_and_sodium_hydroxide_solve_without_a_model() {
        let request = request(&[
            ("H₂SO₄", &[1, 1, 16, 8, 8, 8, 8]),
            ("NaOH", &[11, 8, 1]),
        ]);
        let registry = SpeciesRegistry::default();
        let claim = solve_reaction_claim(&request, &registry).expect("solved");
        assert_eq!(claim.disposition, ClaimDisposition::Reaction);
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["H2O", "Na2SO4"]);
        let outcome =
            compile_claim_outcome(&request, claim, &registry).expect("balanced outcome");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome");
        };
        assert!(outcome.equation().contains("Na2SO4"), "{}", outcome.equation());
        assert!(
            outcome.species_without_structure().is_empty(),
            "every species should carry a generated structure: {:?}",
            outcome.species_without_structure()
        );
    }

    #[test]
    fn hydrochloric_acid_neutralization_solves() {
        let request = request(&[("HCl", &[1, 17]), ("NaOH", &[11, 8, 1])]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
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
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products.len(), 1);
        assert_eq!(claim.products[0].formula, "HCl");
    }

    #[test]
    fn acid_and_carbonate_solve_with_carbon_dioxide_evolution() {
        let request = request(&[
            ("HCl", &[1, 17]),
            ("Na₂CO₃", &[11, 11, 6, 8, 8, 8]),
        ]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
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
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
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
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
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
    fn noble_metal_and_acid_is_a_confident_no_reaction() {
        let request = request(&[("Cu", &[29]), ("HCl", &[1, 17])]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("verdict");
        assert_eq!(claim.disposition, ClaimDisposition::NoReaction);
        assert!(claim.products.is_empty());
    }

    #[test]
    fn methane_combustion_solves() {
        let request = request(&[("CH₄", &[6, 1, 1, 1, 1]), ("O₂", &[8, 8])]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CO2", "H2O"]);
    }

    #[test]
    fn ethanol_combustion_solves() {
        let request = request(&[
            ("C₂H₆O", &[6, 6, 8, 1, 1, 1, 1, 1, 1]),
            ("O₂", &[8, 8]),
        ]);
        let claim =
            solve_reaction_claim(&request, &SpeciesRegistry::default()).expect("solved");
        let formulas = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(formulas, ["CO2", "H2O"]);
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
        let claim =
            solve_reaction_claim(&magnesium, &SpeciesRegistry::default()).expect("solved");
        assert_eq!(claim.products[0].formula, "Mg3N2");

        let hydrogen_sulfide = request(&[("H₂", &[1, 1]), ("S₈", &[16; 8])]);
        let claim = solve_reaction_claim(&hydrogen_sulfide, &SpeciesRegistry::default())
            .expect("solved");
        assert_eq!(claim.products[0].formula, "H2S");
    }

    #[test]
    fn noble_gas_synthesis_is_declined() {
        let request = request(&[("He", &[2]), ("O₂", &[8, 8])]);
        assert!(solve_reaction_claim(&request, &SpeciesRegistry::default()).is_none());
    }
}
