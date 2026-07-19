use std::collections::{BTreeMap, BTreeSet};

use chem_domain::{
    ContentDigest, DeclaredId, ElementSymbol, EvidenceSourceId, IdKind, Phase, PremiseId,
    ReactionRuleId, StructureId,
};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// Supported on-disk schema version for structural catalogue envelopes.
pub const CATALOGUE_SCHEMA_VERSION: u32 = 1;
pub const CATALOGUE_REVIEW_SCHEMA_VERSION: u32 = 1;

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<ElementRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub element_categories: Vec<ElementCategoryRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structural_traits: Vec<StructuralTraitDefinitionRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structure_templates: Vec<StructureTemplateRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structure_applications: Vec<StructureTemplateApplicationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_patterns: Vec<GraphPatternRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generalized_rules: Vec<GeneralizedReactionRuleRecord>,
    /// Reviewed macroscopic state facts used by presentation. These facts are
    /// optional so schema-1 catalogues produced before the visual layer remain
    /// byte-for-byte compatible after canonical normalization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macroscopic_materials: Vec<MacroscopicMaterialRecord>,
}

/// External review artifact accepted by a host trust policy.
///
/// This artifact deliberately lives outside [`CatalogueEnvelope`]: candidate
/// content cannot assert its own promotion. Its semantic digest is pinned by
/// the host before it can label a [`crate::ReferenceCatalogue`] as reviewed.
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
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub sources: BTreeSet<EvidenceSourceId>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premises: BTreeSet<PremiseId>,
    pub coverage_conclusion: String,
    pub limitation: String,
}

/// Context in which a reviewed material phase applies.
///
/// A pure substance's ambient phase is not always its phase in a reaction
/// medium. Rule-role overrides therefore take precedence over `Standard` when
/// the presentation adapter resolves a validated rule application.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MacroscopicMaterialContextRecord {
    Standard,
    ReactionRole { rule: ReactionRuleId, role: String },
}

/// One evidence-backed macroscopic phase fact for a catalogue structure.
///
/// This record describes presentation state only. It cannot create a product,
/// authorize a reaction, or alter a validated structural derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroscopicMaterialRecord {
    pub structure: StructureId,
    pub context: MacroscopicMaterialContextRecord,
    pub phase: Phase,
    /// Optional evidence-backed sRGB bulk colour. Opacity remains a renderer
    /// concern and omission preserves the conservative phase-only default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<[u8; 3]>,
    /// Optional reviewed physical behaviour when this exact material contacts
    /// liquid water. It is a presentation capability only: it neither
    /// predicts products nor changes structural validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_contact: Option<WaterContactBehaviourRecord>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

/// A reviewed, bounded physical behaviour for an exact material in contact
/// with water. This is intentionally separate from phase and colour records:
/// phase alone is not authority for a high-energy authored assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WaterContactBehaviourRecord {
    Explosive {
        variant: ExplosiveWaterContactVariantRecord,
    },
}

/// Authored-layout choice carried by a reviewed water-contact fact. The
/// renderer receives this typed value only after structural validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplosiveWaterContactVariantRecord {
    Rubidium,
    Caesium,
    Francium,
}

#[derive(Debug)]
pub enum ElementCategoryIdKind {}
impl IdKind for ElementCategoryIdKind {
    const NAME: &'static str = "ElementCategoryId";
}
pub type ElementCategoryId = DeclaredId<ElementCategoryIdKind>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRecord {
    pub symbol: ElementSymbol,
    pub name: String,
    pub atomic_number: u16,
    pub period: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u8>,
    pub block: ElementBlockRecord,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementBlockRecord {
    S,
    P,
    D,
    F,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementCategoryRecord {
    pub id: ElementCategoryId,
    pub subject: ElementCategorySubjectRecord,
    pub membership: ElementCategoryMembershipRecord,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementCategorySubjectRecord {
    Element,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementCategoryMembershipRecord {
    Predicate {
        predicate: ElementPredicateRecord,
        #[serde(
            default,
            skip_serializing_if = "BTreeSet::is_empty",
            deserialize_with = "deserialize_unique_set"
        )]
        include: BTreeSet<ElementSymbol>,
        #[serde(
            default,
            skip_serializing_if = "BTreeSet::is_empty",
            deserialize_with = "deserialize_unique_set"
        )]
        exclude: BTreeSet<ElementSymbol>,
    },
    Explicit {
        #[serde(deserialize_with = "deserialize_unique_set")]
        members: BTreeSet<ElementSymbol>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementPredicateRecord {
    All {
        predicates: Vec<ElementPredicateRecord>,
    },
    Any {
        predicates: Vec<ElementPredicateRecord>,
    },
    Not {
        predicate: Box<ElementPredicateRecord>,
    },
    Equals {
        field: ElementFieldRecord,
        value: ElementScalarRecord,
    },
    Range {
        field: ElementFieldRecord,
        min: i64,
        max: i64,
    },
    InSet {
        field: ElementFieldRecord,
        values: Vec<ElementScalarRecord>,
    },
    Present {
        field: ElementFieldRecord,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementFieldRecord {
    Symbol,
    Name,
    AtomicNumber,
    Period,
    Group,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElementScalarRecord {
    String(String),
    Integer(i64),
}

#[derive(Debug)]
pub enum StructuralTraitIdKind {}
impl IdKind for StructuralTraitIdKind {
    const NAME: &'static str = "StructuralTraitId";
}
pub type StructuralTraitId = DeclaredId<StructuralTraitIdKind>;

#[derive(Debug)]
pub enum StructureTemplateIdKind {}
impl IdKind for StructureTemplateIdKind {
    const NAME: &'static str = "StructureTemplateId";
}
pub type StructureTemplateId = DeclaredId<StructureTemplateIdKind>;

#[derive(Debug)]
pub enum GraphPatternIdKind {}
impl IdKind for GraphPatternIdKind {
    const NAME: &'static str = "GraphPatternId";
}
pub type GraphPatternId = DeclaredId<GraphPatternIdKind>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphPatternRecord {
    pub id: GraphPatternId,
    pub variables: BTreeMap<String, PatternVariableRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<GraphPatternRelationshipRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<GraphPatternTraitRecord>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternVariableRecord {
    pub atom: PatternAtomConstraintRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PatternAtomConstraintRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<PatternElementRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_charge: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_bonding_electrons: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unpaired_electrons: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_order_sum: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatternElementRecord {
    Literal(ElementSymbol),
    Parameter(TemplateParameterReferenceRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphPatternRelationshipRecord {
    Covalent {
        bond: String,
        left: String,
        right: String,
        order: BondOrderRecord,
        #[serde(default, skip_serializing_if = "BondElectronOriginRecord::is_shared")]
        electron_origin: BondElectronOriginRecord,
    },
    GroupMembership {
        group: String,
        #[serde(deserialize_with = "deserialize_unique_set")]
        atoms: BTreeSet<String>,
    },
    IonicAssociation {
        association: String,
        #[serde(deserialize_with = "deserialize_unique_set")]
        groups: BTreeSet<String>,
    },
    MetallicDomain {
        domain: String,
        #[serde(deserialize_with = "deserialize_unique_set")]
        sites: BTreeSet<String>,
        delocalized_electrons: u32,
    },
}

impl GraphPatternRelationshipRecord {
    #[must_use]
    pub const fn binding_name(&self) -> &String {
        match self {
            Self::Covalent { bond, .. } => bond,
            Self::GroupMembership { group, .. } => group,
            Self::IonicAssociation { association, .. } => association,
            Self::MetallicDomain { domain, .. } => domain,
        }
    }

    #[must_use]
    pub const fn binding_kind(&self) -> StructuralTraitSiteKindRecord {
        match self {
            Self::Covalent { .. } => StructuralTraitSiteKindRecord::CovalentBond,
            Self::GroupMembership { .. } => StructuralTraitSiteKindRecord::Group,
            Self::IonicAssociation { .. } => StructuralTraitSiteKindRecord::IonicAssociation,
            Self::MetallicDomain { .. } => StructuralTraitSiteKindRecord::MetallicDomain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphPatternTraitRecord {
    #[serde(rename = "trait")]
    pub trait_id: StructuralTraitId,
    pub sites: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizedReactionRuleRecord {
    pub id: ReactionRuleId,
    pub parameters: BTreeMap<String, GeneralizedParameterRecord>,
    pub roles: BTreeMap<String, GeneralizedRoleSchemaRecord>,
    pub reactants: BTreeMap<String, GeneralizedStructureSelectorRecord>,
    pub cases: Vec<GeneralizedReactionCaseRecord>,
    pub applicability: GeneralizedApplicabilityRecord,
    pub model_assumptions: ModelAssumptionsRecord,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneralizedParameterRecord {
    Element {
        category: ElementCategoryId,
    },
    Structure {
        #[serde(rename = "trait")]
        trait_id: StructuralTraitId,
    },
    Enum {
        #[serde(deserialize_with = "deserialize_unique_set")]
        values: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizedRoleSchemaRecord {
    pub side: RuleSideRecord,
    pub representation: RepresentationRecord,
    pub coefficient: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeneralizedArgumentRecord {
    Literal(String),
    Parameter(TemplateParameterReferenceRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneralizedStructureSelectorRecord {
    Exact {
        structure: StructureId,
    },
    Template {
        template: StructureTemplateId,
        arguments: BTreeMap<String, GeneralizedArgumentRecord>,
    },
    Trait {
        #[serde(rename = "trait")]
        trait_id: StructuralTraitId,
    },
    StructureParameter {
        parameter: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneralizedReactionCaseRecord {
    Supported {
        id: String,
        when: GeneralizedCasePredicateRecord,
        products: BTreeMap<String, GeneralizedStructureSelectorRecord>,
        patterns: BTreeMap<String, GraphPatternId>,
        correspondence: Vec<MappingPairRecord>,
        rewrite: Vec<OperationTemplateRecord>,
        #[serde(default)]
        observation_compatibility: Vec<ObservationCompatibilityRecord>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
    Unsupported {
        id: String,
        when: GeneralizedCasePredicateRecord,
        required_feature: String,
        explanation: String,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
}

impl GeneralizedReactionCaseRecord {
    #[must_use]
    pub const fn id(&self) -> &String {
        match self {
            Self::Supported { id, .. } | Self::Unsupported { id, .. } => id,
        }
    }

    #[must_use]
    pub const fn when(&self) -> &GeneralizedCasePredicateRecord {
        match self {
            Self::Supported { when, .. } | Self::Unsupported { when, .. } => when,
        }
    }

    #[must_use]
    pub const fn premise_ids(&self) -> &BTreeSet<PremiseId> {
        match self {
            Self::Supported { premise_ids, .. } | Self::Unsupported { premise_ids, .. } => {
                premise_ids
            }
        }
    }

    #[must_use]
    pub const fn when_mut(&mut self) -> &mut GeneralizedCasePredicateRecord {
        match self {
            Self::Supported { when, .. } | Self::Unsupported { when, .. } => when,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneralizedCasePredicateRecord {
    Always,
    All {
        predicates: Vec<GeneralizedCasePredicateRecord>,
    },
    Any {
        predicates: Vec<GeneralizedCasePredicateRecord>,
    },
    Not {
        predicate: Box<GeneralizedCasePredicateRecord>,
    },
    ParameterEquals {
        parameter: String,
        value: String,
    },
    ParameterInSet {
        parameter: String,
        #[serde(deserialize_with = "deserialize_unique_set")]
        values: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizedApplicabilityRecord {
    pub premise_id: PremiseId,
    pub request_relation: RequestRelation,
    pub required_context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralTraitDefinitionRecord {
    pub id: StructuralTraitId,
    pub sites: BTreeMap<String, StructuralTraitSiteKindRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, StructuralTraitValueProjectionRecord>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralTraitSiteKindRecord {
    Atom,
    CovalentBond,
    Group,
    IonicAssociation,
    MetallicDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructuralTraitValueProjectionRecord {
    AtomElement {
        site: String,
    },
    AtomFormalCharge {
        site: String,
    },
    AtomNonBondingElectrons {
        site: String,
    },
    AtomUnpairedElectrons {
        site: String,
    },
    AtomBondOrderSum {
        site: String,
    },
    CovalentBondOrder {
        left_site: String,
        right_site: String,
    },
    CovalentElectronOrigin {
        left_site: String,
        right_site: String,
    },
    GroupAtomCount {
        site: String,
    },
    IonicComponentCount {
        site: String,
    },
    MetallicSiteCount {
        site: String,
    },
    MetallicDelocalizedElectrons {
        site: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StructuralTraitScalarRecord {
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralTraitAssertionRecord {
    #[serde(rename = "trait")]
    pub trait_id: StructuralTraitId,
    pub sites: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, StructuralTraitScalarRecord>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructureTemplateParameterRecord {
    Element {
        category: ElementCategoryId,
    },
    Structure {
        #[serde(deserialize_with = "deserialize_unique_set")]
        traits: BTreeSet<StructuralTraitId>,
    },
    Enum {
        #[serde(deserialize_with = "deserialize_unique_set")]
        values: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateElementRecord {
    Literal(ElementSymbol),
    Parameter(TemplateParameterReferenceRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateParameterReferenceRecord {
    pub parameter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateBondOrderRecord {
    Literal(BondOrderRecord),
    Parameter(TemplateParameterReferenceRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateAtomRecord {
    pub label: String,
    pub element: TemplateElementRecord,
    pub formal_charge: i16,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateBondRecord {
    pub left: String,
    pub right: String,
    pub order: TemplateBondOrderRecord,
    #[serde(default, skip_serializing_if = "BondElectronOriginRecord::is_shared")]
    pub electron_origin: BondElectronOriginRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delocalization: Option<BondDelocalizationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateComponentRecord {
    pub label: String,
    pub atoms: Vec<TemplateAtomRecord>,
    #[serde(default)]
    pub bonds: Vec<TemplateBondRecord>,
    #[serde(default)]
    pub groups: Vec<GroupRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "representation", rename_all = "snake_case", deny_unknown_fields)]
pub enum StructureTemplateRecord {
    Molecular {
        id: StructureTemplateId,
        parameters: BTreeMap<String, StructureTemplateParameterRecord>,
        atoms: Vec<TemplateAtomRecord>,
        #[serde(default)]
        bonds: Vec<TemplateBondRecord>,
        #[serde(default)]
        groups: Vec<GroupRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
    Ion {
        id: StructureTemplateId,
        parameters: BTreeMap<String, StructureTemplateParameterRecord>,
        atoms: Vec<TemplateAtomRecord>,
        #[serde(default)]
        bonds: Vec<TemplateBondRecord>,
        #[serde(default)]
        groups: Vec<GroupRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
    Ionic {
        id: StructureTemplateId,
        parameters: BTreeMap<String, StructureTemplateParameterRecord>,
        components: Vec<TemplateComponentRecord>,
        associations: Vec<IonicAssociationRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
    Metallic {
        id: StructureTemplateId,
        parameters: BTreeMap<String, StructureTemplateParameterRecord>,
        sites: Vec<TemplateAtomRecord>,
        domains: Vec<MetallicDomainRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
        #[serde(deserialize_with = "deserialize_unique_set")]
        premise_ids: BTreeSet<PremiseId>,
    },
}

impl StructureTemplateRecord {
    #[must_use]
    pub const fn id(&self) -> &StructureTemplateId {
        match self {
            Self::Molecular { id, .. }
            | Self::Ion { id, .. }
            | Self::Ionic { id, .. }
            | Self::Metallic { id, .. } => id,
        }
    }

    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<String, StructureTemplateParameterRecord> {
        match self {
            Self::Molecular { parameters, .. }
            | Self::Ion { parameters, .. }
            | Self::Ionic { parameters, .. }
            | Self::Metallic { parameters, .. } => parameters,
        }
    }

    #[must_use]
    pub const fn traits(&self) -> &Vec<StructuralTraitAssertionRecord> {
        match self {
            Self::Molecular { traits, .. }
            | Self::Ion { traits, .. }
            | Self::Ionic { traits, .. }
            | Self::Metallic { traits, .. } => traits,
        }
    }

    #[must_use]
    pub const fn premise_ids(&self) -> &BTreeSet<PremiseId> {
        match self {
            Self::Molecular { premise_ids, .. }
            | Self::Ion { premise_ids, .. }
            | Self::Ionic { premise_ids, .. }
            | Self::Metallic { premise_ids, .. } => premise_ids,
        }
    }

    #[must_use]
    pub const fn traits_mut(&mut self) -> &mut Vec<StructuralTraitAssertionRecord> {
        match self {
            Self::Molecular { traits, .. }
            | Self::Ion { traits, .. }
            | Self::Ionic { traits, .. }
            | Self::Metallic { traits, .. } => traits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureTemplateApplicationRecord {
    pub id: StructureId,
    pub template: StructureTemplateId,
    pub arguments: BTreeMap<String, String>,
    pub formula: String,
    #[serde(
        default,
        skip_serializing_if = "BTreeSet::is_empty",
        deserialize_with = "deserialize_unique_set"
    )]
    pub aliases: BTreeSet<String>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub premise_ids: BTreeSet<PremiseId>,
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
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
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
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
    },
    Ionic {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        components: Vec<ComponentRecord>,
        associations: Vec<IonicAssociationRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
    },
    Metallic {
        id: StructureId,
        premise_id: PremiseId,
        formula: String,
        sites: Vec<AtomRecord>,
        domains: Vec<MetallicDomainRecord>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        traits: Vec<StructuralTraitAssertionRecord>,
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
    pub fn formula(&self) -> &str {
        match self {
            Self::Molecular { formula, .. }
            | Self::Ion { formula, .. }
            | Self::Ionic { formula, .. }
            | Self::Metallic { formula, .. } => formula,
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

    #[must_use]
    pub const fn traits(&self) -> &Vec<StructuralTraitAssertionRecord> {
        match self {
            Self::Molecular { traits, .. }
            | Self::Ion { traits, .. }
            | Self::Ionic { traits, .. }
            | Self::Metallic { traits, .. } => traits,
        }
    }

    #[must_use]
    pub const fn traits_mut(&mut self) -> &mut Vec<StructuralTraitAssertionRecord> {
        match self {
            Self::Molecular { traits, .. }
            | Self::Ion { traits, .. }
            | Self::Ionic { traits, .. }
            | Self::Metallic { traits, .. } => traits,
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
    #[serde(default, skip_serializing_if = "BondElectronOriginRecord::is_shared")]
    pub electron_origin: BondElectronOriginRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delocalization: Option<BondDelocalizationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondDelocalizationRecord {
    pub domain: String,
    pub effective_order: EffectiveBondOrderRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveBondOrderRecord {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondOrderRecord {
    Single,
    Double,
    Triple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BondElectronOriginRecord {
    #[default]
    Shared,
    Dative {
        donor: String,
        acceptor: String,
    },
}

impl BondElectronOriginRecord {
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }
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
    pub premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAssumptionsRecord {
    pub event: EventModel,
    pub sequence: SequenceModel,
    pub premise_ids: BTreeSet<PremiseId>,
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
    ReconfigureElectrons {
        premise_ids: BTreeSet<PremiseId>,
        atom: String,
        before: ElectronStateRecord,
        after: ElectronStateRecord,
    },
    CleaveCovalent {
        premise_ids: BTreeSet<PremiseId>,
        edge: (String, String, BondOrderRecord),
        allocation: CleavageAllocationRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormCovalent {
        premise_ids: BTreeSet<PremiseId>,
        edge: (String, String, BondOrderRecord),
        electron_contribution: ElectronContributionRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    CleaveDative {
        premise_ids: BTreeSet<PremiseId>,
        donor: String,
        acceptor: String,
        allocation: CleavageAllocationRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormDative {
        premise_ids: BTreeSet<PremiseId>,
        donor: String,
        acceptor: String,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalent {
        premise_ids: BTreeSet<PremiseId>,
        edge: (String, String),
        old_order: BondOrderRecord,
        new_order: BondOrderRecord,
        allocation: CleavageAllocationRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalentDelocalization {
        premise_ids: BTreeSet<PremiseId>,
        edge: (String, String),
        expected: Option<BondDelocalizationRecord>,
        replacement: Option<BondDelocalizationRecord>,
    },
    AssociateIonic {
        premise_ids: BTreeSet<PremiseId>,
        label: String,
        components: Vec<Vec<String>>,
        component_charges: Vec<i16>,
    },
    DissociateIonic {
        premise_ids: BTreeSet<PremiseId>,
        association: String,
    },
    ReleaseMetallic {
        premise_ids: BTreeSet<PremiseId>,
        site: String,
        domain: String,
        allocation: MetallicReleaseAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    JoinMetallic {
        premise_ids: BTreeSet<PremiseId>,
        site: String,
        domain: String,
        allocation: MetallicJoinAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    TransferElectron {
        premise_ids: BTreeSet<PremiseId>,
        count: u8,
        donor: String,
        acceptor: String,
        before: TransferElectronStateRecord,
        after: TransferElectronStateRecord,
    },
    AssignProduct {
        premise_ids: BTreeSet<PremiseId>,
        atoms: Vec<String>,
        product: String,
    },
}

impl OperationTemplateRecord {
    #[must_use]
    pub const fn premise_ids(&self) -> &BTreeSet<PremiseId> {
        match self {
            Self::ReconfigureElectrons { premise_ids, .. }
            | Self::CleaveCovalent { premise_ids, .. }
            | Self::FormCovalent { premise_ids, .. }
            | Self::CleaveDative { premise_ids, .. }
            | Self::FormDative { premise_ids, .. }
            | Self::ChangeCovalent { premise_ids, .. }
            | Self::ChangeCovalentDelocalization { premise_ids, .. }
            | Self::AssociateIonic { premise_ids, .. }
            | Self::DissociateIonic { premise_ids, .. }
            | Self::ReleaseMetallic { premise_ids, .. }
            | Self::JoinMetallic { premise_ids, .. }
            | Self::TransferElectron { premise_ids, .. }
            | Self::AssignProduct { premise_ids, .. } => premise_ids,
        }
    }
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

fn deserialize_unique_set<'de, D, T>(deserializer: D) -> Result<BTreeSet<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Ord,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    let value_count = values.len();
    let values = values.into_iter().collect::<BTreeSet<_>>();
    if values.len() != value_count {
        return Err(D::Error::custom("array entries must be unique"));
    }
    Ok(values)
}
