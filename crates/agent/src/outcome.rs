use std::{collections::BTreeMap, str::FromStr};

use chem_domain::{
    Charge, ContentDigest, ExternalIdentifier, FormulaComposition, Phase, ReactionDeclaration,
    RepresentationKind, ResolvedSpecies, SpeciesAmbiguity, SpeciesId, SpeciesQuery,
    SpeciesRegistry, SpeciesResolution, UnbalancedReactionTerm,
};
use num_bigint::BigUint;

use crate::{
    AgentError, ClaimDisposition, ClaimIdentityHint, ClaimIdentityHintKind, ClaimPhase,
    ClaimProduct, ReactionBuildRequest, ReactionClaim,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Reviewed,
    EvidenceBacked,
    ModelAsserted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeSpecies {
    Resolved(Box<ResolvedSpecies>),
    FormulaOnly {
        id: SpeciesId,
        display_name: String,
        formula: String,
        phase: Phase,
    },
}

impl OutcomeSpecies {
    #[must_use]
    pub const fn has_structure(&self) -> bool {
        matches!(self, Self::Resolved(species) if species.structure.is_some())
    }

    #[must_use]
    pub fn id(&self) -> &SpeciesId {
        match self {
            Self::Resolved(species) => &species.id,
            Self::FormulaOnly { id, .. } => id,
        }
    }
}

/// Structurally checked static capability. It deliberately exposes no frame
/// construction or playback API.
#[derive(Debug, Clone)]
pub struct ValidatedStaticOutcome {
    declaration: ReactionDeclaration,
    reactants: Vec<ResolvedSpecies>,
    products: Vec<OutcomeSpecies>,
    claim: ReactionClaim,
    trust_tier: TrustTier,
    equation: String,
}

impl ValidatedStaticOutcome {
    #[must_use]
    pub const fn declaration(&self) -> &ReactionDeclaration {
        &self.declaration
    }

    #[must_use]
    pub fn reactants(&self) -> &[ResolvedSpecies] {
        &self.reactants
    }

    #[must_use]
    pub fn products(&self) -> &[OutcomeSpecies] {
        &self.products
    }

    #[must_use]
    pub const fn claim(&self) -> &ReactionClaim {
        &self.claim
    }

    #[must_use]
    pub const fn trust_tier(&self) -> TrustTier {
        self.trust_tier
    }

    #[must_use]
    pub fn equation(&self) -> &str {
        &self.equation
    }

    #[must_use]
    pub fn products_without_structure(&self) -> Vec<String> {
        self.products
            .iter()
            .filter(|product| !product.has_structure())
            .map(|product| match product {
                OutcomeSpecies::Resolved(species) => {
                    format!("{} ({})", species.display_name, species.formula_text)
                }
                OutcomeSpecies::FormulaOnly {
                    display_name,
                    formula,
                    ..
                } => format!("{display_name} ({formula})"),
            })
            .collect()
    }

    pub(crate) fn mark_evidence_backed(mut self) -> Self {
        self.trust_tier = TrustTier::EvidenceBacked;
        self
    }

    /// Replaces products with structurally adopted equivalents. Species ids
    /// and order must stay identical so the balanced declaration remains
    /// valid unchanged.
    pub(crate) fn with_adopted_products(
        mut self,
        products: Vec<OutcomeSpecies>,
    ) -> Result<Self, AgentError> {
        if products.len() != self.products.len()
            || products
                .iter()
                .zip(&self.products)
                .any(|(adopted, existing)| adopted.id() != existing.id())
        {
            return Err(AgentError::new(
                "structure adoption",
                "adopted products must preserve species identity and order",
            ));
        }
        self.products = products;
        Ok(self)
    }

    pub(crate) fn mark_reviewed(mut self) -> Self {
        self.trust_tier = TrustTier::Reviewed;
        self
    }
}

#[derive(Debug, Clone)]
pub enum CompiledClaimOutcome {
    Static(ValidatedStaticOutcome),
    NoReaction(ReactionClaim),
    Ambiguous(ReactionClaim),
    Unsupported(ReactionClaim),
}

#[derive(Debug, Clone)]
pub struct ReactantIdentityAmbiguity {
    pub reactant_index: usize,
    pub query: SpeciesQuery,
    pub alternatives: Vec<ResolvedSpecies>,
}

#[derive(Debug, Clone)]
pub enum RequestIdentityResolution {
    Resolved(Vec<ResolvedSpecies>),
    Ambiguous(ReactantIdentityAmbiguity),
}

/// Resolves request and claimed product identities, proves request binding,
/// balances exactly, and constructs the private static capability.
///
/// # Errors
///
/// Returns a typed error for unresolved/ambiguous reactants, request identity
/// mismatch, invalid product formulae, or any exact balance failure.
pub fn compile_claim_outcome(
    request: &ReactionBuildRequest,
    claim: ReactionClaim,
    identities: &SpeciesRegistry,
) -> Result<CompiledClaimOutcome, AgentError> {
    match claim.disposition {
        ClaimDisposition::NoReaction => return Ok(CompiledClaimOutcome::NoReaction(claim)),
        ClaimDisposition::Ambiguous => return Ok(CompiledClaimOutcome::Ambiguous(claim)),
        ClaimDisposition::Unsupported => return Ok(CompiledClaimOutcome::Unsupported(claim)),
        ClaimDisposition::Reaction => {}
    }
    let reactants = resolve_request_species(request, identities)?;
    let products = claim
        .products
        .iter()
        .map(|product| {
            if let Some(species) = resolve_claim_product(product, identities) {
                Ok(OutcomeSpecies::Resolved(Box::new(species.clone())))
            } else {
                let id_material = format!("{}\0{}", product.name, product.formula);
                let digest = ContentDigest::sha256(id_material.as_bytes()).to_hex();
                let id = SpeciesId::from_str(&format!("dynamic.s{}", &digest[..24]))
                    .map_err(|error| AgentError::new("outcome identity", error.to_string()))?;
                FormulaComposition::parse(&product.formula)
                    .map_err(|error| AgentError::new("outcome formula", error.to_string()))?;
                Ok(OutcomeSpecies::FormulaOnly {
                    id,
                    display_name: product.name.clone(),
                    formula: product.formula.clone(),
                    phase: claim_phase(product.phase),
                })
            }
        })
        .collect::<Result<Vec<_>, AgentError>>()?;
    let declaration = ReactionDeclaration::balance(
        reactants
            .iter()
            .map(resolved_term)
            .collect::<Result<Vec<_>, AgentError>>()?,
        products
            .iter()
            .map(outcome_term)
            .collect::<Result<Vec<_>, AgentError>>()?,
        claim.required_context.clone(),
    )
    .map_err(|error| AgentError::new("outcome balance", error.to_string()))?;
    // Identity resolution and exact balancing establish structure and meaning,
    // not source support. EvidenceBacked is an explicit later upgrade after
    // fetched bytes cover each claim field.
    let trust_tier = TrustTier::ModelAsserted;
    let equation = format_equation(&declaration);
    Ok(CompiledClaimOutcome::Static(ValidatedStaticOutcome {
        declaration,
        reactants,
        products,
        claim,
        trust_tier,
        equation,
    }))
}

fn resolve_claim_product<'a>(
    product: &ClaimProduct,
    identities: &'a SpeciesRegistry,
) -> Option<&'a ResolvedSpecies> {
    let exact = SpeciesQuery {
        name: Some(product.name.clone()),
        formula: Some(product.formula.clone()),
        charge: None,
        phase: None,
        external_identifier: None,
    };
    if let SpeciesResolution::Resolved(species) = identities.resolve(&exact) {
        return Some(species);
    }

    // Model names are descriptive, not identity-bearing. A unique reviewed
    // formula or a formula-bound external hint may recover a local structure;
    // ambiguous formulae still fail closed to FormulaOnly.
    for hint in &product.identity_hints {
        let hinted = SpeciesQuery {
            name: None,
            formula: Some(product.formula.clone()),
            charge: None,
            phase: None,
            external_identifier: Some(external_identifier(hint)),
        };
        if let SpeciesResolution::Resolved(species) = identities.resolve(&hinted) {
            return Some(species);
        }
    }
    let formula_only = SpeciesQuery {
        name: None,
        formula: Some(product.formula.clone()),
        charge: None,
        phase: None,
        external_identifier: None,
    };
    match identities.resolve(&formula_only) {
        SpeciesResolution::Resolved(species) => Some(species),
        SpeciesResolution::Ambiguous(_) | SpeciesResolution::NotFound => None,
    }
}

fn external_identifier(hint: &ClaimIdentityHint) -> ExternalIdentifier {
    match hint.kind {
        ClaimIdentityHintKind::Inchi => ExternalIdentifier::Inchi(hint.value.clone()),
        ClaimIdentityHintKind::InchiKey => ExternalIdentifier::InchiKey(hint.value.clone()),
        ClaimIdentityHintKind::CanonicalSmiles => {
            ExternalIdentifier::CanonicalSmiles(hint.value.clone())
        }
        ClaimIdentityHintKind::IsomericSmiles => {
            ExternalIdentifier::IsomericSmiles(hint.value.clone())
        }
        ClaimIdentityHintKind::PubChemCid => ExternalIdentifier::PubChemCid(hint.value.clone()),
        ClaimIdentityHintKind::RegistryId => ExternalIdentifier::RegistryId(hint.value.clone()),
    }
}

/// Resolves and binds the two authored reactants to stable checked identities.
///
/// # Errors
///
/// Returns an error for a missing or chemically ambiguous identity, or when
/// the selected identity disagrees with the authored atom inventory.
pub fn resolve_request_species(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Result<Vec<ResolvedSpecies>, AgentError> {
    match resolve_request_identities(request, identities)? {
        RequestIdentityResolution::Resolved(species) => Ok(species),
        RequestIdentityResolution::Ambiguous(ambiguity) => Err(AgentError::new(
            "request identity",
            format!(
                "reactant `{}` resolves to multiple identities",
                request.reactants[ambiguity.reactant_index].display
            ),
        )),
    }
}

/// Resolves the two request reactants without guessing between chemically
/// distinct alternatives.
///
/// # Errors
///
/// Returns an error for an absent selected identity or authored atom mismatch.
pub fn resolve_request_identities(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Result<RequestIdentityResolution, AgentError> {
    let mut selections = Vec::with_capacity(request.reactants.len());
    for (reactant_index, input) in request.reactants.iter().enumerate() {
        let lookup = if let Some(species_id) = &input.species_id {
            IdentityLookup::Resolved(identities.get(species_id).ok_or_else(|| {
                AgentError::new(
                    "request identity",
                    format!("selected species `{species_id}` is not in the current snapshot"),
                )
            })?)
        } else {
            resolve_name_or_formula(&input.display, identities)?
        };
        match lookup {
            IdentityLookup::Resolved(species) => {
                validate_atomic_numbers(&input.atomic_numbers, species)?;
                selections.push(species.clone());
            }
            IdentityLookup::Ambiguous(ambiguity) => {
                let alternatives = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .filter(|species| {
                        validate_atomic_numbers(&input.atomic_numbers, species).is_ok()
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if alternatives.len() == 1 {
                    selections.push(alternatives[0].clone());
                } else if alternatives.len() >= 2 {
                    return Ok(RequestIdentityResolution::Ambiguous(
                        ReactantIdentityAmbiguity {
                            reactant_index,
                            query: ambiguity.query,
                            alternatives,
                        },
                    ));
                } else {
                    return Err(AgentError::new(
                        "request binding",
                        format!(
                            "no identity alternative for `{}` matches its composed atoms",
                            input.display
                        ),
                    ));
                }
            }
        }
    }
    Ok(RequestIdentityResolution::Resolved(selections))
}

enum IdentityLookup<'a> {
    Resolved(&'a ResolvedSpecies),
    Ambiguous(SpeciesAmbiguity),
}

fn resolve_name_or_formula<'a>(
    value: &str,
    identities: &'a SpeciesRegistry,
) -> Result<IdentityLookup<'a>, AgentError> {
    let formula_key = ascii_formula_key(value);
    for query in [
        SpeciesQuery {
            name: Some(value.to_owned()),
            formula: None,
            charge: None,
            phase: None,
            external_identifier: None,
        },
        SpeciesQuery {
            name: None,
            formula: Some(formula_key.clone()),
            charge: None,
            phase: None,
            external_identifier: None,
        },
    ] {
        match identities.resolve(&query) {
            SpeciesResolution::Resolved(species) => return Ok(IdentityLookup::Resolved(species)),
            SpeciesResolution::Ambiguous(ambiguity) => {
                let alternatives = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .collect::<Vec<_>>();
                if let Some(first) = alternatives.first()
                    && alternatives.iter().all(|candidate| {
                        candidate.formula == first.formula
                            && candidate.charge == first.charge
                            && candidate.phase == first.phase
                            && equivalent_structure(candidate, first)
                    })
                {
                    return alternatives
                        .into_iter()
                        .min_by(|left, right| left.id.cmp(&right.id))
                        .map(IdentityLookup::Resolved)
                        .ok_or_else(|| {
                            AgentError::new(
                                "request identity",
                                format!("reactant `{value}` has no stable equivalent identity"),
                            )
                        });
                }
                let metallic = ambiguity
                    .alternatives
                    .iter()
                    .filter_map(|id| identities.get(id))
                    .filter(|species| {
                        species.structure.as_ref().is_some_and(|structure| {
                            structure.representation() == RepresentationKind::Metallic
                        })
                    })
                    .collect::<Vec<_>>();
                if let Some(first) = metallic.first()
                    && metallic.iter().all(|candidate| {
                        candidate.formula == first.formula && candidate.charge == first.charge
                    })
                {
                    return metallic
                        .into_iter()
                        .min_by(|left, right| left.id.cmp(&right.id))
                        .map(IdentityLookup::Resolved)
                        .ok_or_else(|| {
                            AgentError::new(
                                "request identity",
                                format!("reactant `{value}` has no stable metallic identity"),
                            )
                        });
                }
                return Ok(IdentityLookup::Ambiguous(ambiguity));
            }
            SpeciesResolution::NotFound => {}
        }
    }
    Err(AgentError::new(
        "request identity",
        format!("reactant `{value}` is not in the current identity capability"),
    ))
}

fn equivalent_structure(left: &ResolvedSpecies, right: &ResolvedSpecies) -> bool {
    match (&left.structure, &right.structure) {
        (Some(left), Some(right)) => {
            left.representation() == right.representation() && left.graph() == right.graph()
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn ascii_formula_key(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '₀' => '0',
            '₁' => '1',
            '₂' => '2',
            '₃' => '3',
            '₄' => '4',
            '₅' => '5',
            '₆' => '6',
            '₇' => '7',
            '₈' => '8',
            '₉' => '9',
            other => other,
        })
        .collect()
}

fn validate_atomic_numbers(authored: &[u8], resolved: &ResolvedSpecies) -> Result<(), AgentError> {
    let mut expected = BTreeMap::<u16, BigUint>::new();
    for (element, count) in resolved.formula.composition() {
        expected.insert(element.atomic_number(), count.clone());
    }
    let mut actual = BTreeMap::<u16, BigUint>::new();
    for atomic_number in authored {
        *actual.entry(u16::from(*atomic_number)).or_default() += BigUint::from(1_u8);
    }
    if actual != expected {
        return Err(AgentError::new(
            "request binding",
            format!(
                "reactant `{}` identity disagrees with its composed atoms",
                resolved.display_name
            ),
        ));
    }
    Ok(())
}

fn resolved_term(species: &ResolvedSpecies) -> Result<UnbalancedReactionTerm, AgentError> {
    Ok(UnbalancedReactionTerm {
        species: species.id.clone(),
        display_name: species.display_name.clone(),
        formula_text: species.formula_text.clone(),
        formula: FormulaComposition::parse(&species.formula_text)
            .map_err(|error| AgentError::new("outcome formula", error.to_string()))?,
        charge: species.charge.clone(),
        phase: species.phase,
    })
}

fn outcome_term(species: &OutcomeSpecies) -> Result<UnbalancedReactionTerm, AgentError> {
    match species {
        OutcomeSpecies::Resolved(species) => resolved_term(species),
        OutcomeSpecies::FormulaOnly {
            id,
            display_name,
            formula,
            phase,
        } => Ok(UnbalancedReactionTerm {
            species: id.clone(),
            display_name: display_name.clone(),
            formula_text: formula.clone(),
            formula: FormulaComposition::parse(formula)
                .map_err(|error| AgentError::new("outcome formula", error.to_string()))?,
            charge: Charge::neutral(),
            phase: *phase,
        }),
    }
}

const fn claim_phase(phase: ClaimPhase) -> Phase {
    match phase {
        ClaimPhase::Aqueous => Phase::Aqueous,
        ClaimPhase::Solid => Phase::Solid,
        ClaimPhase::Liquid => Phase::Liquid,
        ClaimPhase::Gas => Phase::Gas,
        ClaimPhase::Unknown => Phase::Unknown,
    }
}

fn format_equation(declaration: &ReactionDeclaration) -> String {
    let side = |terms: &[chem_domain::ReactionTerm]| {
        terms
            .iter()
            .map(|term| {
                if term.coefficient() == 1 {
                    term.formula_text().to_owned()
                } else {
                    format!("{} {}", term.coefficient(), term.formula_text())
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    format!(
        "{} → {}",
        side(declaration.reactants()),
        side(declaration.products())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_catalogue::{CatalogueEnvelope, ValidatedCatalogueBundle};
    use serde_json::json;

    use crate::{ClaimMode, ReactantInput, ReactionClaim, reviewed_species_registry};

    fn registry() -> SpeciesRegistry {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue");
        let mut envelope: CatalogueEnvelope = serde_json::from_slice(&bytes).expect("envelope");
        envelope.digest = envelope.computed_digest().expect("digest");
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");
        reviewed_species_registry(&catalogue).expect("identities")
    }

    #[test]
    fn compact_claim_compiles_to_exact_private_static_outcome() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[{"predicate":"evolves","subject":"hydrogen","value":null}],
            "sources":[],
            "ambiguity":null
        });
        let claim = ReactionClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let outcome = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![3],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ],
                selected_context: None,
            },
            claim,
            &registry(),
        )
        .expect("compile claim");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("expected static outcome")
        };
        assert_eq!(outcome.equation(), "2 Li + 2 H2O → H2 + 2 LiOH");
        assert_eq!(outcome.trust_tier(), TrustTier::ModelAsserted);
        assert!(outcome.products().iter().all(OutcomeSpecies::has_structure));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue");
        let mut envelope: CatalogueEnvelope = serde_json::from_slice(&bytes).expect("envelope");
        envelope.digest = envelope.computed_digest().expect("digest");
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");
        let source =
            std::fs::read_to_string(root.join("conformance/end-to-end/alkali-water-li-001.chems"))
                .expect("source");
        let evidence =
            std::fs::read(root.join("conformance/observations/alkali-water-li-001.evidence.json"))
                .expect("evidence");
        let parsed = chem_kernel::expand_review_candidate(
            "alkali-water-li-001.chems",
            &source,
            &catalogue,
            &evidence,
        )
        .expect("parsed declaration");
        assert_eq!(outcome.declaration(), &parsed.claim.declaration);
    }

    #[test]
    fn unique_reviewed_product_formula_recovers_structure_when_model_name_drifts() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"aqueous hydroxide product","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen gas product","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"ordinary contact",
            "observations":[], "sources":[], "ambiguity":null
        });
        let claim = ReactionClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim bytes"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![3],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ],
                selected_context: None,
            },
            claim,
            &registry(),
        )
        .expect("compiled");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("expected static outcome")
        };
        assert!(outcome.products().iter().all(OutcomeSpecies::has_structure));
        assert!(outcome.products_without_structure().is_empty());
    }

    #[test]
    fn request_binding_rejects_stale_composed_atoms() {
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"ordinary contact","observations":[],"sources":[],"ambiguity":null
        });
        let parsed =
            ReactionClaim::from_json(&serde_json::to_vec(&claim).unwrap(), ClaimMode::Fast)
                .unwrap();
        let error = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "Li".into(),
                        atomic_numbers: vec![37],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ],
                selected_context: None,
            },
            parsed,
            &registry(),
        )
        .expect_err("stale atoms must fail request binding");
        assert_eq!(error.stage(), "request binding");
    }

    #[test]
    fn composer_subscript_formula_resolves_without_changing_display_text() {
        let identities = registry();
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "Li".into(),
                    atomic_numbers: vec![3],
                    species_id: None,
                },
                ReactantInput {
                    display: "H₂O".into(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let RequestIdentityResolution::Resolved(resolved) =
            resolve_request_identities(&request, &identities).expect("identity resolution")
        else {
            panic!("request should resolve")
        };
        assert_eq!(resolved[1].formula_text, "H2O");
        assert_eq!(request.reactants[1].display, "H₂O");
    }
}
