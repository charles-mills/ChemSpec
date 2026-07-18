use std::collections::{BTreeMap, BTreeSet};

use chem_catalogue::{
    ElaboratedGeneralizedRule, GeneralizedRoleInput, RepresentationRecord, RuleSideRecord,
    TrustedCatalogue,
};
use chem_domain::{ElementSymbol, RepresentationKind, SpeciesId, StructureId};
use chem_kernel::{
    CurrentArtifactIdentity, SimulationFrames, expand_reviewed_declaration, generate_frames,
    validate_trusted,
};

use crate::{AgentError, AgentErrorKind, ValidatedStaticOutcome};

#[derive(Debug, Clone)]
pub struct ReviewedFamilyMatch {
    selected: ElaboratedGeneralizedRule,
    role_species: BTreeMap<String, SpeciesId>,
}

impl ReviewedFamilyMatch {
    #[must_use]
    pub fn rule_id(&self) -> &chem_domain::ReactionRuleId {
        &self.selected.rule.id
    }

    #[cfg(test)]
    pub(crate) const fn selected(&self) -> &ElaboratedGeneralizedRule {
        &self.selected
    }

    #[cfg(test)]
    pub(crate) const fn role_species(&self) -> &BTreeMap<String, SpeciesId> {
        &self.role_species
    }
}

#[derive(Debug, Clone)]
pub enum FamilyMatchOutcome {
    Matched(Box<ReviewedFamilyMatch>),
    NoMatch,
    Ambiguous(Vec<chem_domain::ReactionRuleId>),
}

#[derive(Debug, Clone)]
pub struct ReviewedAnimationOutcome {
    static_outcome: ValidatedStaticOutcome,
    frames: SimulationFrames,
    family_rule: chem_domain::ReactionRuleId,
}

impl ReviewedAnimationOutcome {
    #[must_use]
    pub const fn static_outcome(&self) -> &ValidatedStaticOutcome {
        &self.static_outcome
    }

    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }

    #[must_use]
    pub const fn family_rule(&self) -> &chem_domain::ReactionRuleId {
        &self.family_rule
    }
}

#[derive(Debug, Clone)]
struct FamilyTerm {
    species: SpeciesId,
    structure: StructureId,
    coefficient: u32,
    side: RuleSideRecord,
    representation: RepresentationRecord,
}

/// Matches a checked outcome against every registered reviewed generalized
/// family. Only the *reactant* structures are required up front: the family's
/// own reviewed selectors derive the product structures, which are then
/// compared against the balanced declaration by exact element inventory and
/// coefficient. Formula-only products therefore never block a family match.
/// Provider hints are not accepted by this API.
///
/// # Errors
///
/// Returns a system error only if the trusted catalogue becomes internally
/// inconsistent during elaboration.
#[allow(clippy::too_many_lines)]
pub fn match_reviewed_family(
    outcome: &ValidatedStaticOutcome,
    catalogue: &TrustedCatalogue,
) -> Result<FamilyMatchOutcome, AgentError> {
    let Some(terms) = reactant_family_terms(outcome) else {
        return Ok(FamilyMatchOutcome::NoMatch);
    };
    // A resolved reactant may be one of several graph-identical reviewed
    // aliases (for example a metal reviewed under two structure templates).
    // Family selectors are alias-specific, so matching must consider every
    // equivalent reviewed structure, not only the resolver's stable pick.
    let alternatives = terms
        .iter()
        .map(|term| structure_alternatives(&term.structure, catalogue))
        .collect::<Vec<_>>();
    let declared_products = outcome
        .declaration()
        .products()
        .iter()
        .map(|term| {
            (
                term.species().clone(),
                term.formula().elements().clone(),
                term.coefficient(),
            )
        })
        .collect::<Vec<_>>();
    let mut matches = Vec::new();
    for rule in &catalogue.document().generalized_rules {
        let reactant_roles = rule
            .roles
            .iter()
            .filter(|(_, schema)| schema.side == RuleSideRecord::Reactant)
            .map(|(role, _)| role.clone())
            .collect::<Vec<_>>();
        if reactant_roles.len() != terms.len()
            || rule.roles.len() - reactant_roles.len() != declared_products.len()
        {
            continue;
        }
        let mut assignments = Vec::new();
        enumerate_assignments(
            &reactant_roles,
            &rule.roles,
            &terms,
            0,
            &mut BTreeSet::new(),
            &mut BTreeMap::new(),
            &mut assignments,
        );
        for assignment in assignments {
            let ordered = assignment.iter().collect::<Vec<_>>();
            let mut combo = vec![0_usize; ordered.len()];
            loop {
                let reactant_inputs = ordered
                    .iter()
                    .zip(&combo)
                    .map(|((role, index), choice)| {
                        let term = &terms[**index];
                        GeneralizedRoleInput {
                            role: (*role).clone(),
                            structure: alternatives[**index][*choice].clone(),
                            coefficient: term.coefficient,
                            side: term.side,
                            representation: term.representation,
                        }
                    })
                    .collect::<Vec<_>>();
                if let Ok(derived) = catalogue
                    .derive_generalized_products(&rule.id, &reactant_inputs)
                    .map_err(|error| {
                        AgentError::from_source(
                            AgentErrorKind::CompilationFailure,
                            "family match",
                            error,
                        )
                    })?
                    && let Some(product_species) =
                        bind_declared_products(&derived, &declared_products, catalogue)
                {
                    let mut inputs = reactant_inputs;
                    inputs.extend(derived);
                    if let Ok(selected) = catalogue
                        .elaborate_generalized_rule(&rule.id, &inputs)
                        .map_err(|error| {
                            AgentError::from_source(
                                AgentErrorKind::CompilationFailure,
                                "family match",
                                error,
                            )
                        })?
                    {
                        let mut role_species = ordered
                            .iter()
                            .map(|(role, index)| ((*role).clone(), terms[**index].species.clone()))
                            .collect::<BTreeMap<_, _>>();
                        role_species.extend(product_species);
                        matches.push(ReviewedFamilyMatch {
                            selected,
                            role_species,
                        });
                    }
                }
                let mut position = 0;
                loop {
                    if position == combo.len() {
                        break;
                    }
                    combo[position] += 1;
                    if combo[position] < alternatives[*ordered[position].1].len() {
                        break;
                    }
                    combo[position] = 0;
                    position += 1;
                }
                if position == combo.len() {
                    break;
                }
            }
        }
    }
    matches.sort_by(|left, right| left.rule_id().cmp(right.rule_id()));
    matches.dedup_by(|left, right| {
        left.rule_id() == right.rule_id() && left.role_species == right.role_species
    });
    match matches.len() {
        0 => Ok(FamilyMatchOutcome::NoMatch),
        1 => {
            let Some(family) = matches.pop() else {
                return Ok(FamilyMatchOutcome::NoMatch);
            };
            Ok(FamilyMatchOutcome::Matched(Box::new(family)))
        }
        _ => Ok(FamilyMatchOutcome::Ambiguous(
            matches
                .into_iter()
                .map(|family| family.selected.rule.id)
                .collect(),
        )),
    }
}

/// Expands and validates a local reviewed-family match through the trusted
/// kernel and frame boundary.
///
/// # Errors
///
/// Returns a typed family, kernel, or frame error. No provider-authored
/// operation can enter this path.
pub fn compile_reviewed_animation(
    outcome: ValidatedStaticOutcome,
    family: ReviewedFamilyMatch,
    catalogue: &TrustedCatalogue,
) -> Result<ReviewedAnimationOutcome, AgentError> {
    let reaction_name = format!(
        "Dynamic.r{}",
        &outcome.declaration().digest().to_hex()[..24]
    );
    let expanded = expand_reviewed_declaration(
        &reaction_name,
        outcome.declaration(),
        &family.role_species,
        &family.selected,
        catalogue,
    )
    .map_err(|error| {
        AgentError::from_source(AgentErrorKind::KernelRejection, "family expansion", error)
    })?;
    let identity = CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| {
        AgentError::from_source(AgentErrorKind::KernelRejection, "family expansion", error)
    })?;
    let validated = validate_trusted(&expanded, catalogue).map_err(|error| {
        AgentError::from_source(AgentErrorKind::KernelRejection, "family validation", error)
    })?;
    let frames = generate_frames(&validated, identity).map_err(|error| {
        AgentError::from_source(AgentErrorKind::KernelRejection, "family frames", error)
    })?;
    Ok(ReviewedAnimationOutcome {
        static_outcome: outcome.mark_reviewed(),
        frames,
        family_rule: family.selected.rule.id,
    })
}

fn reactant_family_terms(outcome: &ValidatedStaticOutcome) -> Option<Vec<FamilyTerm>> {
    let coefficients = outcome
        .declaration()
        .reactants()
        .iter()
        .map(|term| (term.species().clone(), term.coefficient()))
        .collect::<BTreeMap<_, _>>();
    let mut terms = Vec::new();
    for species in outcome.reactants() {
        let crate::OutcomeSpecies::Resolved(species) = species else {
            return None;
        };
        let structure = species.structure.as_ref()?;
        terms.push(FamilyTerm {
            species: species.id.clone(),
            structure: structure.id().clone(),
            coefficient: coefficients[&species.id],
            side: RuleSideRecord::Reactant,
            representation: representation(structure.representation()),
        });
    }
    Some(terms)
}

/// Every reviewed structure exactly isomorphic to the given one, including
/// itself. Label-differing duplicates of one species are interchangeable
/// aliases; constitutional isomers are not isomorphic and stay distinct.
/// Failure or a hit work limit fails closed to the structure itself.
fn structure_alternatives(
    structure: &StructureId,
    catalogue: &TrustedCatalogue,
) -> Vec<StructureId> {
    let Some(own) = catalogue.structures().get(structure) else {
        return vec![structure.clone()];
    };
    catalogue
        .structures()
        .iter()
        .filter(|(id, candidate)| {
            *id == structure
                || (candidate.representation() == own.representation()
                    && candidate.formula().elements() == own.formula().elements()
                    && catalogue
                        .structures_isomorphic(structure, id)
                        .ok()
                        .flatten()
                        .unwrap_or(false))
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Binds each family-derived product to exactly one declaration product with
/// an identical element inventory and coefficient. Returns the role-to-species
/// binding, or `None` when the family's products are not the claimed products.
fn bind_declared_products(
    derived: &[GeneralizedRoleInput],
    declared: &[(SpeciesId, BTreeMap<ElementSymbol, u64>, u32)],
    catalogue: &TrustedCatalogue,
) -> Option<BTreeMap<String, SpeciesId>> {
    let mut used = vec![false; declared.len()];
    let mut role_species = BTreeMap::new();
    for input in derived {
        let structure = catalogue.structures().get(&input.structure)?;
        let elements = structure.formula().elements();
        let (index, (species, _, _)) =
            declared
                .iter()
                .enumerate()
                .find(|(index, (_, formula, coefficient))| {
                    !used[*index] && *coefficient == input.coefficient && formula == elements
                })?;
        used[index] = true;
        role_species.insert(input.role.clone(), species.clone());
    }
    Some(role_species)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_assignments(
    roles: &[String],
    schemas: &BTreeMap<String, chem_catalogue::GeneralizedRoleSchemaRecord>,
    terms: &[FamilyTerm],
    role_index: usize,
    used: &mut BTreeSet<usize>,
    current: &mut BTreeMap<String, usize>,
    output: &mut Vec<BTreeMap<String, usize>>,
) {
    if role_index == roles.len() {
        output.push(current.clone());
        return;
    }
    let role = &roles[role_index];
    let schema = &schemas[role];
    for (index, term) in terms.iter().enumerate() {
        if used.contains(&index)
            || term.side != schema.side
            || term.representation != schema.representation
            || term.coefficient != schema.coefficient
        {
            continue;
        }
        used.insert(index);
        current.insert(role.clone(), index);
        enumerate_assignments(roles, schemas, terms, role_index + 1, used, current, output);
        current.remove(role);
        used.remove(&index);
    }
}

const fn representation(value: RepresentationKind) -> RepresentationRecord {
    match value {
        RepresentationKind::Molecular => RepresentationRecord::Molecular,
        RepresentationKind::Ion => RepresentationRecord::Ion,
        RepresentationKind::Ionic => RepresentationRecord::Ionic,
        RepresentationKind::Metallic => RepresentationRecord::Metallic,
    }
}

#[cfg(test)]
mod tests {
    use chem_catalogue::TrustedCatalogue;
    use serde_json::json;

    use super::*;
    use crate::{
        ClaimMode, CompiledClaimOutcome, MechanismEscalationRequest, MechanismEscalationResponse,
        MechanismProvider, ReactantInput, ReactionBuildRequest, ReactionClaim,
        compile_claim_outcome, reviewed_species_registry,
    };

    struct UnexpectedMechanismProvider;

    impl MechanismProvider for UnexpectedMechanismProvider {
        fn propose(
            &mut self,
            _request: &MechanismEscalationRequest,
            _diagnostic: Option<&str>,
        ) -> Result<MechanismEscalationResponse, crate::AgentError> {
            panic!("reviewed family hits must not invoke mechanism escalation")
        }
    }

    fn trusted() -> TrustedCatalogue {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        TrustedCatalogue::from_canonical_json(
            &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
                .expect("catalogue"),
        )
        .expect("trusted catalogue")
    }

    #[test]
    fn formula_only_products_cannot_block_a_reviewed_family_match() {
        // The product formula `KHO` parses to the same element inventory as
        // the reviewed `KOH` structure but misses the string-keyed registry
        // lookup. The structure generator now fills that gap on the fly, and
        // the family must still match from the reactants alone.
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"a hydroxide the registry cannot name","formula":"KHO","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[],"sources":[],"ambiguity":null
        });
        let claim =
            ReactionClaim::from_json(&serde_json::to_vec(&claim).expect("claim"), ClaimMode::Fast)
                .expect("claim contract");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "K".into(),
                        atomic_numbers: vec![19],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            claim,
            &identities,
        )
        .expect("compiled outcome");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("static outcome")
        };
        assert!(
            outcome.products_without_structure().is_empty(),
            "the mis-keyed hydroxide product should gain a generated structure"
        );
        let matched = match_reviewed_family(&outcome, &trusted).expect("family match");
        let FamilyMatchOutcome::Matched(family) = matched else {
            panic!("expected a reviewed family despite the formula-only product: {matched:?}")
        };
        let animation =
            compile_reviewed_animation(outcome, *family, &trusted).expect("reviewed animation");
        assert!(!animation.frames().frames().is_empty());
        assert_eq!(
            animation.static_outcome().trust_tier(),
            crate::TrustTier::Reviewed
        );
    }

    #[test]
    fn reviewed_family_match_crosses_typed_kernel_and_frames() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let claim = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[],"sources":[],"ambiguity":null
        });
        let claim =
            ReactionClaim::from_json(&serde_json::to_vec(&claim).expect("claim"), ClaimMode::Fast)
                .expect("claim contract");
        let compiled = compile_claim_outcome(
            &ReactionBuildRequest {
                reactants: [
                    ReactantInput {
                        display: "LithiumMetal".into(),
                        atomic_numbers: vec![3],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "H2O".into(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ]
                .to_vec(),
                selected_context: None,
            },
            claim,
            &identities,
        )
        .expect("compiled outcome");
        let CompiledClaimOutcome::Static(outcome) = compiled else {
            panic!("static outcome")
        };
        let presentation = crate::enrich_static_outcome(
            outcome.clone(),
            &trusted,
            &mut UnexpectedMechanismProvider,
        )
        .expect("local-first presentation ladder");
        assert!(matches!(
            presentation,
            crate::DynamicPresentationOutcome::ReviewedFamily(_)
        ));
        let started = std::time::Instant::now();
        let matched = match_reviewed_family(&outcome, &trusted).expect("family match");
        let FamilyMatchOutcome::Matched(family) = matched else {
            panic!("reviewed family: {matched:?}")
        };
        let animation =
            compile_reviewed_animation(outcome, *family, &trusted).expect("reviewed animation");
        assert!(!animation.frames().frames().is_empty());
        assert_eq!(
            animation.frames().trust(),
            chem_kernel::DerivationTrust::Trusted
        );
        assert_eq!(
            animation.static_outcome().trust_tier(),
            crate::TrustTier::Reviewed
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "generalized-family hit exceeded the 250 ms local-hit budget"
        );
    }
}
