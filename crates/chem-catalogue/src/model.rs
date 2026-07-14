use std::collections::{BTreeMap, BTreeSet};

use chem_domain::{ContentDigest, EvidenceSourceId, PremiseId, ReactionRuleId, StructureId};
use serde::{Deserialize, Serialize};

/// Supported on-disk schema version for structural catalogue envelopes.
pub const CATALOGUE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueEnvelope {
    pub digest: ContentDigest,
    pub bundle: CatalogueDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueDocument {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub publication: PublicationKind,
    pub created: CreationMetadata,
    pub evidence: Vec<EvidenceSource>,
    pub premises: Vec<PremiseRecord>,
    pub valence_premises: Vec<ValencePremiseRecord>,
    pub structures: Vec<StructureRecord>,
    pub rules: Vec<ReactionRuleRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PublicationKind {
    Working,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationMetadata {
    pub created_on: String,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub id: EvidenceSourceId,
    pub title: String,
    pub publisher: String,
    pub locator: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_date: Option<String>,
    pub retrieved_on: String,
    pub usage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PremiseRecord {
    pub id: PremiseId,
    pub statement: String,
    pub evidence: BTreeSet<EvidenceSourceId>,
    pub review: ReviewMetadata,
    pub rule_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewMetadata {
    pub status: ReviewStatus,
    #[serde(default)]
    pub reviewers: Vec<ReviewerRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewStatus {
    Reviewed,
    Provisional,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerRecord {
    pub reviewer: String,
    pub reviewed_on: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValencePremiseRecord {
    pub premise_id: PremiseId,
    pub neutral_valence: Vec<ElementValenceRecord>,
    pub supported_states: Vec<ValenceStateRecord>,
    #[serde(default)]
    pub metallic_domain_states: Vec<MetallicValenceStateRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementValenceRecord {
    pub element: String,
    pub neutral_valence_electrons: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValenceStateRecord {
    pub element: String,
    pub formal_charge: i16,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
    pub covalent_bond_order_sum: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetallicValenceStateRecord {
    pub element: String,
    pub site_formal_charge: i16,
    pub site_local_electrons: u8,
    pub delocalized_electrons_per_site: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "representation", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructureRecord {
    Molecular {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        atoms: Vec<AtomRecord>,
        #[serde(default)]
        bonds: Vec<BondRecord>,
        #[serde(default)]
        groups: Vec<GroupRecord>,
    },
    Ion {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        atoms: Vec<AtomRecord>,
        #[serde(default)]
        bonds: Vec<BondRecord>,
        #[serde(default)]
        groups: Vec<GroupRecord>,
    },
    Ionic {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        components: Vec<ComponentRecord>,
        associations: Vec<IonicAssociationRecord>,
    },
    Metallic {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        sites: Vec<AtomRecord>,
        domains: Vec<MetallicDomainRecord>,
    },
}

impl StructureRecord {
    #[must_use]
    pub const fn id(&self) -> &StructureId {
        match self {
            Self::Molecular { id, .. }
            | Self::Ion { id, .. }
            | Self::Ionic { id, .. }
            | Self::Metallic { id, .. } => id,
        }
    }

    #[must_use]
    pub const fn premise_id(&self) -> &PremiseId {
        match self {
            Self::Molecular { premise_id, .. }
            | Self::Ion { premise_id, .. }
            | Self::Ionic { premise_id, .. }
            | Self::Metallic { premise_id, .. } => premise_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomRecord {
    pub label: String,
    pub element: String,
    pub formal_charge: i16,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondRecord {
    pub left: String,
    pub right: String,
    pub order: BondOrderRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondOrderRecord {
    Single,
    Double,
    Triple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRecord {
    pub label: String,
    pub atoms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRecord {
    pub label: String,
    pub atoms: Vec<AtomRecord>,
    #[serde(default)]
    pub bonds: Vec<BondRecord>,
    #[serde(default)]
    pub groups: Vec<GroupRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IonicAssociationRecord {
    pub label: String,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetallicDomainRecord {
    pub label: String,
    pub sites: Vec<String>,
    pub delocalized_electrons: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionRuleRecord {
    pub id: ReactionRuleId,
    pub premise_ids: BTreeSet<PremiseId>,
    pub roles: BTreeMap<String, RoleSchemaRecord>,
    pub reactant_pattern: Vec<PatternTermRecord>,
    pub product_pattern: Vec<PatternTermRecord>,
    pub applicability: ApplicabilityRecord,
    pub mapping_template: Vec<MappingPairRecord>,
    pub operation_template: Vec<OperationTemplateRecord>,
    pub model_assumptions: ModelAssumptionsRecord,
    pub observation_compatibility: Vec<ObservationCompatibilityRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSchemaRecord {
    pub side: RuleSideRecord,
    pub representation: RepresentationRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSideRecord {
    Reactant,
    Product,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepresentationRecord {
    Molecular,
    Ion,
    Ionic,
    Metallic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternTermRecord {
    pub role: String,
    pub structure_id: StructureId,
    pub coefficient: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicabilityRecord {
    pub premise_id: PremiseId,
    pub request_relation: RequestRelation,
    pub reactant_structure_ids: BTreeSet<StructureId>,
    pub required_context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestRelation {
    Contact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPairRecord {
    pub reactant: String,
    pub product: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAssumptionsRecord {
    pub event: EventModel,
    pub sequence: SequenceModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventModel {
    Representative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceModel {
    Explanatory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationCompatibilityRecord {
    pub subject_role: String,
    pub predicate: ObservationPredicate,
    pub evidence_subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub premise_id: PremiseId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationPredicate {
    Evolves,
    Disappears,
    Forms,
    Colour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectronStateRecord(pub i16, pub u8, pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryElectronStateRecord {
    pub left: ElectronStateRecord,
    pub right: ElectronStateRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferElectronStateRecord {
    pub donor: ElectronStateRecord,
    pub acceptor: ElectronStateRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetallicElectronStateRecord {
    pub site: ElectronStateRecord,
    pub domain_electrons: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationTemplateRecord {
    CleaveCovalent {
        edge: (String, String, BondOrderRecord),
        allocation: CleavageAllocationRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormCovalent {
        edge: (String, String, BondOrderRecord),
        electron_contribution: ElectronContributionRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalent {
        edge: (String, String),
        old_order: BondOrderRecord,
        new_order: BondOrderRecord,
        allocation: CleavageAllocationRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    AssociateIonic {
        label: String,
        components: Vec<Vec<String>>,
        component_charges: Vec<i16>,
    },
    DissociateIonic {
        association: String,
    },
    ReleaseMetallic {
        site: String,
        domain: String,
        allocation: MetallicReleaseAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    JoinMetallic {
        site: String,
        domain: String,
        allocation: MetallicJoinAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    TransferElectron {
        count: u8,
        donor: String,
        acceptor: String,
        before: TransferElectronStateRecord,
        after: TransferElectronStateRecord,
    },
    AssignProduct {
        atoms: Vec<String>,
        product: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CleavageAllocationRecord {
    Homolytic(String),
    Heterolytic { heterolytic_to: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetallicReleaseAllocationRecord {
    RetainElectron,
    LeaveElectron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetallicJoinAllocationRecord {
    DonateElectron,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElectronContributionRecord {
    pub left: u8,
    pub right: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueReviewAttestation {
    pub schema_version: u32,
    pub id: String,
    pub catalogue_digest: ContentDigest,
    pub reviewer: String,
    pub reviewed_on: String,
    pub scope: String,
    pub method: String,
    pub sources: BTreeSet<EvidenceSourceId>,
    pub premises: BTreeSet<PremiseId>,
    pub coverage_conclusion: String,
    pub limitation: String,
}
