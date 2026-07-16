//! Structure escalation: adopting model-proposed structural graphs for
//! claimed products absent from the reviewed structure library.
//!
//! A proposal never touches the trusted catalogue. It is compiled into an
//! isolated `Working` catalogue bundle that contains the complete trusted
//! document plus the proposed structures under one provisional premise, then
//! crosses the identical catalogue validation (graph integrity, formula
//! agreement, supported valence states) every reviewed structure crossed.

use std::{collections::BTreeMap, collections::BTreeSet, str::FromStr};

use chem_catalogue::{
    CatalogueEnvelope, PremiseRecord, PublicationKind, ReviewMetadata, ReviewStatus,
    StructureRecord, ValidatedCatalogueBundle,
};
use chem_domain::{ContentDigest, FormulaComposition, Phase, PremiseId, SpeciesId, StructureId};

use crate::{
    AgentError, LabelledStructure, OutcomeSpecies, StructureProposalRequest,
    StructureProposalResponse, StructureProposalSpecies, ValidatedStaticOutcome,
    claim::STRUCTURE_PROPOSAL_SCHEMA_VERSION, identity::model_proposed_species,
};

pub(crate) const DYNAMIC_STRUCTURE_PREMISE: &str = "premise.dynamic.structure";

/// An outcome whose formula-only products acquired validated model-proposed
/// structures, together with the isolated working bundle those structures
/// live in. The bundle is required for kernel expansion and never replaces
/// the trusted catalogue.
#[derive(Debug, Clone)]
pub struct AdoptedProposedStructures {
    pub outcome: ValidatedStaticOutcome,
    pub bundle: ValidatedCatalogueBundle,
}

#[derive(Debug, Clone)]
struct MissingProduct {
    index: usize,
    species: SpeciesId,
    name: String,
    formula: String,
    phase: Phase,
}

fn missing_products(outcome: &ValidatedStaticOutcome) -> Vec<MissingProduct> {
    outcome
        .products()
        .iter()
        .enumerate()
        .filter_map(|(index, product)| match product {
            OutcomeSpecies::Resolved(species) if species.structure.is_none() => {
                Some(MissingProduct {
                    index,
                    species: species.id.clone(),
                    name: species.display_name.clone(),
                    formula: species.formula_text.clone(),
                    phase: species.phase,
                })
            }
            OutcomeSpecies::Resolved(_) => None,
            OutcomeSpecies::FormulaOnly {
                id,
                display_name,
                formula,
                phase,
            } => Some(MissingProduct {
                index,
                species: id.clone(),
                name: display_name.clone(),
                formula: formula.clone(),
                phase: *phase,
            }),
        })
        .collect()
}

/// Builds the fixed structure-escalation request for an outcome, or `None`
/// when every product already has a reviewed structure.
#[must_use]
pub fn structure_proposal_request(
    outcome: &ValidatedStaticOutcome,
) -> Option<StructureProposalRequest> {
    let missing = missing_products(outcome);
    if missing.is_empty() {
        return None;
    }
    Some(StructureProposalRequest {
        schema_version: STRUCTURE_PROPOSAL_SCHEMA_VERSION,
        species: missing
            .iter()
            .enumerate()
            .map(|(ordinal, product)| StructureProposalSpecies {
                id: format!("DynamicStructure{}", ordinal + 1),
                name: product.name.clone(),
                formula: product.formula.clone(),
            })
            .collect(),
    })
}

/// Validates a structure proposal inside an isolated working bundle and
/// upgrades the outcome's formula-only products to resolved species carrying
/// the validated graphs.
///
/// # Errors
///
/// Returns a typed diagnostic when the response does not answer the request
/// exactly, or when any proposed structure fails catalogue validation,
/// element-inventory agreement, or neutral-charge binding. The diagnostic is
/// suitable for one bounded proposal repair.
#[allow(clippy::too_many_lines)]
pub fn adopt_proposed_structures(
    outcome: &ValidatedStaticOutcome,
    request: &StructureProposalRequest,
    response: &StructureProposalResponse,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<AdoptedProposedStructures, AgentError> {
    let missing = missing_products(outcome);
    if missing.len() != request.species.len() {
        return Err(AgentError::new(
            "structure adoption",
            "request does not describe this outcome's missing products",
        ));
    }
    let premise_id = PremiseId::from_str(DYNAMIC_STRUCTURE_PREMISE)
        .map_err(|error| AgentError::new("structure adoption", error.to_string()))?;
    let mut proposals = BTreeMap::new();
    for structure in &response.structures {
        let (id, _) = labelled_id_formula(structure);
        if proposals.insert(id.to_owned(), structure).is_some() {
            return Err(AgentError::new(
                "structure adoption",
                format!("duplicate proposed structure id `{id}`"),
            ));
        }
    }
    if proposals.len() != request.species.len() {
        return Err(AgentError::new(
            "structure adoption",
            "the proposal must contain exactly one structure per requested species",
        ));
    }
    let mut records = Vec::new();
    for species in &request.species {
        let proposal = *proposals.get(&species.id).ok_or_else(|| {
            AgentError::new(
                "structure adoption",
                format!(
                    "no proposed structure uses the requested id `{}`",
                    species.id
                ),
            )
        })?;
        let (_, formula) = labelled_id_formula(proposal);
        if *formula != species.formula {
            return Err(AgentError::new(
                "structure adoption",
                format!(
                    "proposed structure `{}` must keep the requested formula `{}`",
                    species.id, species.formula
                ),
            ));
        }
        records.push(structure_record(proposal, &premise_id)?);
    }

    let mut document = catalogue.document().clone();
    document.publication = PublicationKind::Working;
    // The premise must reference resolvable evidence. The trusted document's
    // internal explanatory-model source is the honest anchor: a proposed
    // structure is a modeling assumption, not an empirical claim.
    let evidence = document
        .evidence
        .first()
        .map(|source| source.id.clone())
        .into_iter()
        .collect::<BTreeSet<_>>();
    document.premises.push(PremiseRecord {
        id: premise_id,
        statement: "Model-proposed dynamic structure awaiting independent review".to_owned(),
        evidence,
        review: ReviewMetadata {
            status: ReviewStatus::Provisional,
            reviewers: Vec::new(),
        },
        rule_version: "1".to_owned(),
    });
    document.structures.extend(records);
    let mut envelope = CatalogueEnvelope {
        digest: ContentDigest::sha256(b"uncomputed dynamic working bundle"),
        bundle: document,
    };
    envelope.digest = envelope
        .computed_digest()
        .map_err(|error| AgentError::new("structure adoption", error.to_string()))?;
    let bundle = ValidatedCatalogueBundle::validate(envelope).map_err(|error| {
        AgentError::new(
            "structure validation",
            format!("proposed structure failed catalogue validation: {error}"),
        )
    })?;

    let mut products = outcome.products().to_vec();
    for (product, species_request) in missing.iter().zip(&request.species) {
        let structure_id = StructureId::from_str(&species_request.id)
            .map_err(|error| AgentError::new("structure adoption", error.to_string()))?;
        let structure = bundle.structures().get(&structure_id).ok_or_else(|| {
            AgentError::new(
                "structure adoption",
                "validated working bundle lost a proposed structure",
            )
        })?;
        if structure.graph().system_net_charge() != 0 {
            return Err(AgentError::new(
                "structure validation",
                format!(
                    "proposed structure `{}` must be net neutral to bind the balanced product",
                    species_request.id
                ),
            ));
        }
        let claimed = FormulaComposition::parse(&species_request.formula)
            .map_err(|error| AgentError::new("structure validation", error.to_string()))?;
        if claimed.elements() != structure.formula().elements() {
            return Err(AgentError::new(
                "structure validation",
                format!(
                    "proposed structure `{}` does not contain exactly the atoms of `{}`",
                    species_request.id, species_request.formula
                ),
            ));
        }
        let resolved = model_proposed_species(
            &product.species,
            &product.name,
            &product.formula,
            product.phase,
            structure,
            &bundle,
        )?;
        products[product.index] = OutcomeSpecies::Resolved(Box::new(resolved));
    }
    let outcome = outcome.clone().with_adopted_products(products)?;
    Ok(AdoptedProposedStructures { outcome, bundle })
}

const fn labelled_id_formula(structure: &LabelledStructure) -> (&String, &String) {
    match structure {
        LabelledStructure::Molecular { id, formula, .. }
        | LabelledStructure::Ion { id, formula, .. }
        | LabelledStructure::Ionic { id, formula, .. }
        | LabelledStructure::Metallic { id, formula, .. } => (id, formula),
    }
}

fn structure_record(
    proposal: &LabelledStructure,
    premise_id: &PremiseId,
) -> Result<StructureRecord, AgentError> {
    let structure_id = |id: &str| {
        StructureId::from_str(id)
            .map_err(|error| AgentError::new("structure adoption", error.to_string()))
    };
    Ok(match proposal.clone() {
        LabelledStructure::Molecular {
            id,
            formula,
            atoms,
            bonds,
            groups,
        } => StructureRecord::Molecular {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            atoms,
            bonds,
            groups,
            traits: Vec::new(),
        },
        LabelledStructure::Ion {
            id,
            formula,
            atoms,
            bonds,
            groups,
        } => StructureRecord::Ion {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            atoms,
            bonds,
            groups,
            traits: Vec::new(),
        },
        LabelledStructure::Ionic {
            id,
            formula,
            components,
            associations,
        } => StructureRecord::Ionic {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            components,
            associations,
            traits: Vec::new(),
        },
        LabelledStructure::Metallic {
            id,
            formula,
            sites,
            domains,
        } => StructureRecord::Metallic {
            id: structure_id(&id)?,
            premise_id: premise_id.clone(),
            formula,
            sites,
            domains,
            traits: Vec::new(),
        },
    })
}
