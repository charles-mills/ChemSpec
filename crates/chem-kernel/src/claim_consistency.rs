use std::collections::{BTreeMap, BTreeSet};

use chem_catalogue::{ObservationPredicate, ValidatedCatalogueBundle};
use num_bigint::BigInt;

use crate::{
    ExpandedInstance, ExpandedStructuralReaction, ReactionSideKind, ResolvedStructureBinding,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimConsistencyFailure {
    Equation,
    StructureMetadata,
    Declaration,
    Observation,
}

impl ClaimConsistencyFailure {
    pub(crate) const fn kernel_code(self) -> &'static str {
        match self {
            Self::Equation => "CHEMS-K003",
            Self::StructureMetadata => "CHEMS-K004",
            Self::Declaration => "CHEMS-K005",
            Self::Observation => "CHEMS-K006",
        }
    }

    pub(crate) const fn expansion_code(self) -> &'static str {
        match self {
            Self::Equation => "CHEMS-X037",
            Self::StructureMetadata => "CHEMS-X038",
            Self::Declaration => "CHEMS-X039",
            Self::Observation => "CHEMS-X040",
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Equation => "equation metadata disagrees with resolved structure bindings",
            Self::StructureMetadata => {
                "claim structure metadata disagrees with catalogue-expanded instances"
            }
            Self::Declaration => "reaction declaration disagrees with resolved claim metadata",
            Self::Observation => {
                "observation metadata disagrees with the resolved claim or provenance"
            }
        }
    }
}

pub(crate) fn validate(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<(), ClaimConsistencyFailure> {
    validate_equation(expanded)?;
    validate_structure_metadata(expanded, catalogue)?;
    validate_declaration(expanded)?;
    validate_observations(expanded, catalogue)
}

fn validate_declaration(
    expanded: &ExpandedStructuralReaction,
) -> Result<(), ClaimConsistencyFailure> {
    fn terms_match(
        terms: &[chem_domain::ReactionTerm],
        bindings: &BTreeMap<String, ResolvedStructureBinding>,
    ) -> bool {
        terms.len() == bindings.len()
            && bindings.values().all(|binding| {
                terms
                    .iter()
                    .find(|term| term.species() == &binding.declaration.species)
                    .is_some_and(|term| {
                        term.display_name() == binding.declaration.display_name
                            && term.formula_text() == binding.declaration.formula_text
                            && term
                                .formula()
                                .elements()
                                .iter()
                                .map(|(element, count)| (element.to_string(), *count))
                                .collect::<BTreeMap<_, _>>()
                                == binding.formula
                            && term.charge() == &binding.declaration.charge
                            && term.phase() == binding.declaration.phase
                            && term.coefficient() == binding.coefficient
                    })
            })
    }

    let claim = &expanded.claim;
    if claim.declaration.required_context() == claim.rule.applicability.required_context
        && terms_match(claim.declaration.reactants(), &claim.reactants)
        && terms_match(claim.declaration.products(), &claim.products)
    {
        Ok(())
    } else {
        Err(ClaimConsistencyFailure::Declaration)
    }
}

fn validate_observations(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<(), ClaimConsistencyFailure> {
    let claim = &expanded.claim;
    let rule = catalogue.rule(&claim.rule.rule);
    let mut claim_ids = BTreeSet::new();
    let consistent = claim.evidence.observations.iter().all(|observation| {
        let expected_bindings = match observation.predicate {
            ObservationPredicate::Disappears => &claim.reactants,
            ObservationPredicate::Evolves
            | ObservationPredicate::Forms
            | ObservationPredicate::Colour => &claim.products,
        };
        let value_matches = matches!(observation.predicate, ObservationPredicate::Colour)
            == observation.value.is_some();
        let evidence_matches = observation.provenance.evidence.iter().any(|origin| {
            origin.packet == claim.evidence.packet.qualified()
                && origin.packet_digest == claim.evidence.digest
                && origin.claim == observation.claim
                && !origin.sources.is_empty()
        });
        let catalogue_matches = !observation.provenance.catalogue.is_empty()
            && observation
                .provenance
                .catalogue
                .iter()
                .all(|origin| origin.catalogue_digest == claim.catalogue.digest);
        let retained_compatibility_matches = observation.predicate
            == observation.compatibility.predicate
            && observation.subject_binding == observation.compatibility.subject_binding
            && observation.value == observation.compatibility.value
            && observation.evidence_subject == observation.compatibility.evidence_subject
            && observation
                .provenance
                .catalogue
                .iter()
                .any(|origin| origin.premises.contains(&observation.compatibility.premise));
        let catalogue_rule_matches =
            rule.map_or_else(
                || claim.rule.generalized.is_some(),
                |rule| {
                    rule.record()
                        .observation_compatibility
                        .iter()
                        .any(|compatibility| {
                            compatibility.predicate == observation.predicate
                                && compatibility.value == observation.value
                                && compatibility.evidence_subject == observation.evidence_subject
                                && claim
                                    .rule
                                    .bindings
                                    .get(&compatibility.subject_role)
                                    .is_some_and(|binding| {
                                        binding.binding == observation.subject_binding
                                    })
                                && observation.provenance.catalogue.iter().any(|origin| {
                                    origin.premises.contains(&compatibility.premise_id)
                                })
                        })
                },
            );
        claim_ids.insert(&observation.claim)
            && expected_bindings.contains_key(&observation.subject_binding)
            && value_matches
            && evidence_matches
            && catalogue_matches
            && retained_compatibility_matches
            && catalogue_rule_matches
    });
    if consistent {
        Ok(())
    } else {
        Err(ClaimConsistencyFailure::Observation)
    }
}

fn validate_structure_metadata(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<(), ClaimConsistencyFailure> {
    let side_consistent = |bindings: &BTreeMap<String, ResolvedStructureBinding>,
                           instances: &BTreeMap<String, ExpandedInstance>,
                           side: ReactionSideKind| {
        let expected_count = bindings
            .values()
            .map(|binding| usize::try_from(binding.coefficient).unwrap_or(usize::MAX))
            .sum::<usize>();
        instances.len() == expected_count
            && bindings.iter().all(|(name, binding)| {
                let catalogue_structure = catalogue.structure(&binding.structure);
                let catalogue_matches = catalogue_structure.is_some_and(|structure| {
                    let expected_charge = structure.graph().system_net_charge();
                    binding.name == *name
                        && binding.side == side
                        && binding.representation == structure.representation()
                        && binding.declaration.charge.value() == &BigInt::from(expected_charge)
                        && chem_domain::FormulaComposition::parse(&binding.declaration.formula_text)
                            .is_ok_and(|formula| {
                                formula
                                    .elements()
                                    .iter()
                                    .map(|(element, count)| (element.to_string(), *count))
                                    .collect::<BTreeMap<_, _>>()
                                    == binding.formula
                            })
                        && binding.formula
                            == structure
                                .formula()
                                .elements()
                                .iter()
                                .map(|(symbol, count)| (symbol.to_string(), *count))
                                .collect()
                });
                catalogue_matches
                    && (1..=binding.coefficient).all(|ordinal| {
                        let key = format!("{name}[{ordinal}]");
                        instances.get(&key).is_some_and(|expanded_instance| {
                            expanded_instance.binding == *name
                                && expanded_instance.ordinal == ordinal
                                && expanded_instance.instance.id().as_str() == key
                                && expanded_instance.instance.structure() == &binding.structure
                                && expanded_instance
                                    .instance
                                    .graph()
                                    .element_inventory()
                                    .elements()
                                    .iter()
                                    .map(|(symbol, count)| (symbol.to_string(), *count))
                                    .collect::<BTreeMap<_, _>>()
                                    == binding.formula
                        })
                    })
            })
    };
    if side_consistent(
        &expanded.claim.reactants,
        &expanded.reactant_instances,
        ReactionSideKind::Reactant,
    ) && side_consistent(
        &expanded.claim.products,
        &expanded.product_instances,
        ReactionSideKind::Product,
    ) {
        Ok(())
    } else {
        Err(ClaimConsistencyFailure::StructureMetadata)
    }
}

fn validate_equation(expanded: &ExpandedStructuralReaction) -> Result<(), ClaimConsistencyFailure> {
    let claim = &expanded.claim;
    let expected_len = claim.reactants.len() + claim.products.len();
    let mut seen = BTreeSet::new();
    let consistent = claim.equation.len() == expected_len
        && claim.equation.iter().all(|term| {
            let bindings = match term.side {
                ReactionSideKind::Reactant => &claim.reactants,
                ReactionSideKind::Product => &claim.products,
            };
            bindings.get(&term.binding).is_some_and(|binding| {
                seen.insert((term.side, term.binding.as_str()))
                    && binding.side == term.side
                    && binding.coefficient == term.coefficient
                    && binding.formula == term.formula
                    && binding.representation == term.representation
            })
        });
    if consistent && seen.len() == expected_len {
        Ok(())
    } else {
        Err(ClaimConsistencyFailure::Equation)
    }
}
