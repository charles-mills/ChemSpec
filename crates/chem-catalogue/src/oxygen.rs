//! Validated, closed-world screening for reactions with elemental oxygen.
//!
//! This catalogue selects a reviewed outcome class. It never constructs a
//! structural reaction or authorizes simulation frames.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{CatalogueError, CatalogueErrorCode, ValidatedCatalogueBundle};

pub const OXYGEN_SCREENING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OxygenScreeningDocument {
    pub schema_version: u32,
    pub evidence: Vec<OxygenEvidence>,
    pub element_outcomes: Vec<ElementOxygenRecord>,
    pub default_element_outcome: OxygenOutcome,
    pub compound_outcomes: Vec<CompoundOxygenRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OxygenEvidence {
    pub id: String,
    pub url: String,
    pub claim: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementOxygenRecord {
    pub atomic_number: u8,
    pub outcome: OxygenOutcome,
    pub evidence_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompoundOxygenRecord {
    pub formula: String,
    pub outcome: OxygenOutcome,
    pub evidence_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OxygenOutcome {
    Representative {
        reactant_formula: String,
        product_formula: String,
        equation: String,
        product_oxygen_atoms: u8,
        structural_support: StructuralSupport,
    },
    NoDirectReaction {
        reason: String,
    },
    Ambiguous {
        reason: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralSupport {
    PendingReviewedModel,
    UnsupportedBondModel,
}

#[derive(Debug, Clone)]
pub struct ValidatedOxygenScreening {
    known_atomic_numbers: BTreeSet<u8>,
    element_outcomes: BTreeMap<u8, ElementOxygenRecord>,
    default_element_outcome: OxygenOutcome,
    compound_outcomes: BTreeMap<String, CompoundOxygenRecord>,
}

impl ValidatedOxygenScreening {
    /// Parses and validates oxygen screening data against the reference element
    /// and structure catalogue.
    ///
    /// # Errors
    ///
    /// Returns a typed catalogue error for malformed data, unknown element or
    /// compound references, unresolved evidence, or incomplete outcomes.
    pub fn from_json(
        bytes: &[u8],
        catalogue: &ValidatedCatalogueBundle,
    ) -> Result<Self, CatalogueError> {
        let document: OxygenScreeningDocument = serde_json::from_slice(bytes).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidJson, error.to_string())
        })?;
        if document.schema_version != OXYGEN_SCREENING_SCHEMA_VERSION {
            return Err(invalid("unsupported oxygen screening schema version"));
        }
        if matches!(
            document.default_element_outcome,
            OxygenOutcome::Representative { .. }
        ) {
            return Err(invalid(
                "the default oxygen outcome cannot assert a product",
            ));
        }

        let evidence_ids = document
            .evidence
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        if evidence_ids.len() != document.evidence.len()
            || document.evidence.iter().any(|item| {
                item.id.trim().is_empty()
                    || item.url.trim().is_empty()
                    || item.claim.trim().is_empty()
            })
        {
            return Err(invalid("oxygen evidence must be unique and complete"));
        }

        let known_atomic_numbers = catalogue
            .document()
            .elements
            .iter()
            .filter_map(|element| u8::try_from(element.atomic_number).ok())
            .collect::<BTreeSet<_>>();
        let mut element_outcomes = BTreeMap::new();
        for record in document.element_outcomes {
            validate_outcome(&record.outcome)?;
            validate_evidence(&record.evidence_ids, &evidence_ids)?;
            if !known_atomic_numbers.contains(&record.atomic_number) {
                return Err(invalid("oxygen screening references an unknown element"));
            }
            if element_outcomes
                .insert(record.atomic_number, record)
                .is_some()
            {
                return Err(invalid("duplicate element oxygen outcome"));
            }
        }

        let known_compounds = catalogue
            .document()
            .structures
            .iter()
            .map(|structure| match structure {
                crate::StructureRecord::Molecular { formula, .. }
                | crate::StructureRecord::Ion { formula, .. }
                | crate::StructureRecord::Ionic { formula, .. }
                | crate::StructureRecord::Metallic { formula, .. } => formula.as_str(),
            })
            .chain(
                catalogue
                    .document()
                    .structure_applications
                    .iter()
                    .map(|application| application.formula.as_str()),
            )
            .collect::<BTreeSet<_>>();
        let mut compound_outcomes = BTreeMap::new();
        for record in document.compound_outcomes {
            validate_outcome(&record.outcome)?;
            validate_evidence(&record.evidence_ids, &evidence_ids)?;
            if record.formula.trim().is_empty()
                || !known_compounds.contains(record.formula.as_str())
                || compound_outcomes
                    .insert(record.formula.clone(), record)
                    .is_some()
            {
                return Err(invalid(
                    "compound oxygen formulae must be non-empty and unique",
                ));
            }
        }

        Ok(Self {
            known_atomic_numbers,
            element_outcomes,
            default_element_outcome: document.default_element_outcome,
            compound_outcomes,
        })
    }

    #[must_use]
    pub fn element(&self, atomic_number: u8) -> Option<&OxygenOutcome> {
        self.known_atomic_numbers.contains(&atomic_number).then(|| {
            self.element_outcomes
                .get(&atomic_number)
                .map_or(&self.default_element_outcome, |record| &record.outcome)
        })
    }

    #[must_use]
    pub fn compound(&self, formula: &str) -> Option<&OxygenOutcome> {
        self.compound_outcomes
            .get(formula)
            .map(|record| &record.outcome)
    }
}

fn validate_evidence(
    used: &BTreeSet<String>,
    known: &BTreeSet<&str>,
) -> Result<(), CatalogueError> {
    if used.is_empty() || used.iter().any(|id| !known.contains(id.as_str())) {
        return Err(invalid("oxygen outcomes require known evidence"));
    }
    Ok(())
}

fn validate_outcome(outcome: &OxygenOutcome) -> Result<(), CatalogueError> {
    match outcome {
        OxygenOutcome::Representative {
            reactant_formula,
            product_formula,
            equation,
            product_oxygen_atoms,
            ..
        } => {
            if *product_oxygen_atoms == 0
                || reactant_formula.trim().is_empty()
                || product_formula.trim().is_empty()
                || equation.trim().is_empty()
            {
                return Err(invalid("representative oxygen outcomes must be complete"));
            }
        }
        OxygenOutcome::NoDirectReaction { reason }
        | OxygenOutcome::Ambiguous { reason }
        | OxygenOutcome::Unsupported { reason } => {
            if reason.trim().is_empty() {
                return Err(invalid("oxygen outcome reasons cannot be empty"));
            }
        }
    }
    Ok(())
}

fn invalid(message: &str) -> CatalogueError {
    CatalogueError::new(CatalogueErrorCode::InvalidMetadata, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOGUE: &[u8] =
        include_bytes!("../../../catalogue/reference/core-chemistry/catalogue.json");
    const OXYGEN: &[u8] = include_bytes!("../../../catalogue/oxygen-screening/oxygen.json");

    fn screening() -> ValidatedOxygenScreening {
        let catalogue =
            ValidatedCatalogueBundle::from_json(CATALOGUE).expect("catalogue validates");
        ValidatedOxygenScreening::from_json(OXYGEN, &catalogue).expect("screening validates")
    }

    #[test]
    fn every_catalogued_element_has_an_outcome() {
        let screening = screening();
        assert!((1..=118).all(|number| screening.element(number).is_some()));
        assert!(screening.element(119).is_none());
    }

    #[test]
    fn oxygen_count_is_case_data_not_a_group_guess() {
        let screening = screening();
        let count = |number| match screening.element(number) {
            Some(OxygenOutcome::Representative {
                product_oxygen_atoms,
                ..
            }) => *product_oxygen_atoms,
            other => panic!("expected representative outcome, got {other:?}"),
        };
        assert_eq!(count(3), 1);
        assert_eq!(count(11), 2);
        assert_eq!(count(13), 3);
    }

    #[test]
    fn compounds_are_closed_to_catalogued_formulae() {
        let screening = screening();
        assert!(screening.compound("H2O").is_some());
        assert!(screening.compound("CO").is_none());
    }
}
