#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::{fmt, str::FromStr};

use chem_domain::{
    AtomId, CatalogueId, ContentDigest, EvidenceSourceId, FactId, SourceDecimal, StructuralRuleId,
    SubstanceId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueInput {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub production: bool,
    pub elements: Vec<ElementRecord>,
    pub species: Vec<SpeciesRecord>,
    pub evidence: Vec<EvidenceSource>,
    pub facts: Vec<FactRecord>,
    pub structural_rules: Vec<StructuralRuleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRecord {
    pub atomic_number: u8,
    pub symbol: String,
    pub name: String,
    pub abridged_atomic_weight: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesRecord {
    pub id: String,
    pub formula: String,
    pub charge: i8,
    pub phase: Phase,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Aqueous,
    Solid,
    Liquid,
    Gas,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub locator: String,
    pub url: String,
    pub retrieved: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    Reviewed,
    Provisional,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactRecord {
    pub id: String,
    pub proposition: FactProposition,
    pub evidence: Vec<String>,
    pub review: ReviewState,
    pub reviewed_by: String,
    pub reviewed_on: String,
    pub rule_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FactProposition {
    Dissociates {
        substance: String,
        products: Vec<String>,
    },
    Insoluble {
        species: String,
        medium: String,
    },
    HasColour {
        species: String,
        colour: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralRuleRecord {
    pub id: String,
    pub review: ReviewState,
    pub reviewed_by: String,
    pub reviewed_on: String,
    pub evidence: Vec<String>,
    pub reactants: Vec<String>,
    pub products: Vec<String>,
    pub equation: ReviewedEquation,
    pub states: Vec<StructuralState>,
    pub operations: Vec<StructuralOperation>,
    pub observations: Vec<RuleObservation>,
    #[serde(default)]
    pub safety_notices: Vec<String>,
    pub presentation: PresentationProfile,
}

/// A reviewed, display-ready balanced equation. Coefficients and formulae are
/// catalogue data; animation code must not derive them from atom layouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedEquation {
    pub reactants: Vec<StoichiometricTerm>,
    pub products: Vec<StoichiometricTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoichiometricTerm {
    pub species: String,
    pub coefficient: u16,
    pub formula: String,
}

/// Reviewed macroscopic presentation data. These values authorize reusable
/// visual building blocks; they do not add chemistry or laboratory procedure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationProfile {
    pub id: String,
    pub environment: AssetProfile,
    pub objects: Vec<PresentationObject>,
    pub effects: Vec<PresentationEffect>,
    pub camera: Vec<CameraCue>,
    pub disclosure: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetProfile {
    LaboratoryBench,
    DarkPresentationPlatform,
    Beaker,
    TestTube,
    ConicalFlask,
    MeasuringCylinder,
    MetalChunk,
    MetalStrip,
    CrystalCluster,
    PowderPile,
    LiquidVolume,
    PrecipitateCloud,
    GasCloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SceneRole {
    Environment,
    Vessel,
    Reactant,
    Product,
    Contents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppearanceProfile {
    LaboratoryNeutral,
    ClearGlass,
    Water,
    AqueousColourless,
    WhitePrecipitate,
    AlkaliMetal,
    MetalSilver,
}

/// Integer transform components are milli-scene-units and milli-turns. The
/// renderer may convert them to floats after planning; catalogue hashing and
/// selection remain exact and deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationTransform {
    pub translation: [i16; 3],
    pub rotation: [i16; 3],
    pub scale: [u16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationObject {
    pub id: String,
    pub asset: AssetProfile,
    pub semantic_identity: String,
    pub appearance: AppearanceProfile,
    pub role: SceneRole,
    pub transform: PresentationTransform,
    pub visible_from_ordinal: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectProfile {
    BubbleEmitter,
    GasRelease,
    SurfaceDisturbance,
    ObjectShrinkage,
    PrecipitateFormation,
    Clouding,
    ColourTransition,
    SplashEmitter,
    HeatDistortion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectIntensity {
    Subtle,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationEffect {
    pub effect: EffectProfile,
    pub trigger_observation: String,
    pub intensity: EffectIntensity,
    pub start_ordinal: u16,
    pub end_ordinal: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraBehaviour {
    WideEstablishingShot,
    SlowPushIn,
    ReactionFocus,
    ObservationCloseUp,
    SlowPullBack,
    FinalHeroShot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraCue {
    pub behaviour: CameraBehaviour,
    pub start_ordinal: u16,
    pub end_ordinal: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleObservation {
    pub id: String,
    pub trigger_ordinal: u16,
    pub claim: ObservationClaim,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ObservationClaim {
    ProductForms { species: String },
    ProductHasColour { species: String, colour: String },
    GasEvolves { species: String },
    ReactantConsumed { species: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralState {
    pub ordinal: u16,
    pub atoms: Vec<AtomState>,
    #[serde(default)]
    pub covalent_bonds: Vec<CovalentBond>,
    #[serde(default)]
    pub ionic_associations: Vec<IonicAssociation>,
    #[serde(default)]
    pub metallic_domains: Vec<MetallicDomain>,
    #[serde(default)]
    pub product_memberships: Vec<ProductMembership>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetallicDomain {
    pub id: String,
    pub sites: Vec<String>,
    pub delocalized_electrons: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomState {
    pub id: String,
    pub element: String,
    pub formal_charge: i8,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CovalentBond {
    pub left: String,
    pub right: String,
    pub order: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dative_origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IonicAssociation {
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductMembership {
    pub product: String,
    pub atoms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StructuralOperation {
    AssociateIonic {
        left: String,
        right: String,
    },
    AssignProduct {
        product: String,
        atoms: Vec<String>,
    },
    TransferMetallicElectron {
        domain: String,
        donor_site: String,
        acceptor: String,
        count: u8,
        acceptor_after: AtomElectronState,
    },
    CleaveCovalent {
        left: String,
        right: String,
        order: u8,
        left_after: AtomElectronState,
        right_after: AtomElectronState,
    },
    FormCovalent {
        left: String,
        right: String,
        order: u8,
        left_after: AtomElectronState,
        right_after: AtomElectronState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomElectronState {
    pub formal_charge: i8,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
}

#[derive(Debug)]
pub struct CatalogueBundle {
    identity: CatalogueId,
    version: String,
    digest: ContentDigest,
    input: CatalogueInput,
    species: BTreeMap<SubstanceId, usize>,
    facts: BTreeMap<FactId, usize>,
    evidence: BTreeMap<EvidenceSourceId, usize>,
    structural_rules: BTreeMap<StructuralRuleId, usize>,
}

impl CatalogueBundle {
    /// Loads and validates one immutable catalogue bundle.
    ///
    /// # Errors
    ///
    /// Returns a typed system error when schema, identity, evidence, review, or
    /// structural consistency checks fail.
    pub fn load_json(bytes: &[u8]) -> Result<Self, CatalogueError> {
        let input: CatalogueInput = serde_json::from_slice(bytes)
            .map_err(|error| CatalogueError::Malformed(error.to_string()))?;
        Self::load(input)
    }

    /// Validates a decoded bundle and binds its canonical semantic digest.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogueError`] for any invalid production catalogue.
    pub fn load(input: CatalogueInput) -> Result<Self, CatalogueError> {
        validate_input(&input)?;
        let canonical = serde_json::to_value(&input)
            .map_err(|error| CatalogueError::Malformed(error.to_string()))?;
        let digest = ContentDigest::of_json(&canonical)
            .map_err(|error| CatalogueError::Malformed(error.to_string()))?;
        let identity = CatalogueId::new(input.name.clone())?;
        let species = declared_index(&input.species, |record| record.id.as_str())?;
        let facts = declared_index(&input.facts, |record| record.id.as_str())?;
        let evidence = declared_index(&input.evidence, |record| record.id.as_str())?;
        let structural_rules =
            declared_index(&input.structural_rules, |record| record.id.as_str())?;
        Ok(Self {
            identity,
            version: input.version.clone(),
            digest,
            input,
            species,
            facts,
            evidence,
            structural_rules,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> &CatalogueId {
        &self.identity
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub fn structural_rule(&self, id: &StructuralRuleId) -> Option<&StructuralRuleRecord> {
        self.structural_rules
            .get(id)
            .and_then(|index| self.input.structural_rules.get(*index))
    }

    #[must_use]
    pub fn species(&self, id: &SubstanceId) -> Option<&SpeciesRecord> {
        self.species
            .get(id)
            .and_then(|index| self.input.species.get(*index))
    }

    #[must_use]
    pub fn fact(&self, id: &FactId) -> Option<&FactRecord> {
        self.facts
            .get(id)
            .and_then(|index| self.input.facts.get(*index))
    }

    #[must_use]
    pub fn evidence(&self, id: &EvidenceSourceId) -> Option<&EvidenceSource> {
        self.evidence
            .get(id)
            .and_then(|index| self.input.evidence.get(*index))
    }
}

fn declared_index<T, K>(
    records: &[T],
    id: impl Fn(&T) -> &str,
) -> Result<BTreeMap<K, usize>, CatalogueError>
where
    K: Ord + FromStr<Err = chem_domain::IdError>,
{
    let mut index = BTreeMap::new();
    for (offset, record) in records.iter().enumerate() {
        let typed = K::from_str(id(record))?;
        if index.insert(typed, offset).is_some() {
            return Err(CatalogueError::DuplicateId(id(record).to_owned()));
        }
    }
    Ok(index)
}

fn validate_input(input: &CatalogueInput) -> Result<(), CatalogueError> {
    if input.schema_version != 1 {
        return Err(CatalogueError::UnsupportedSchema(input.schema_version));
    }
    if input.version.is_empty() {
        return Err(CatalogueError::Missing("catalogue version"));
    }
    let evidence_ids = unique_strings(input.evidence.iter().map(|record| record.id.as_str()))?;
    let species_ids = unique_strings(input.species.iter().map(|record| record.id.as_str()))?;
    let species_formulae = input
        .species
        .iter()
        .map(|record| (record.id.as_str(), record.formula.as_str()))
        .collect::<BTreeMap<_, _>>();
    unique_strings(input.facts.iter().map(|record| record.id.as_str()))?;
    unique_strings(
        input
            .structural_rules
            .iter()
            .map(|record| record.id.as_str()),
    )?;

    let mut atomic_numbers = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    for element in &input.elements {
        if element.atomic_number == 0 || !atomic_numbers.insert(element.atomic_number) {
            return Err(CatalogueError::InvalidElement(element.symbol.clone()));
        }
        if !symbols.insert(element.symbol.as_str())
            || element
                .abridged_atomic_weight
                .parse::<SourceDecimal>()
                .is_err()
        {
            return Err(CatalogueError::InvalidElement(element.symbol.clone()));
        }
        validate_evidence(&element.evidence, &evidence_ids)?;
    }
    for species in &input.species {
        SubstanceId::new(species.id.clone())?;
        if species.formula.is_empty() {
            return Err(CatalogueError::Missing("species formula"));
        }
    }
    for fact in &input.facts {
        FactId::new(fact.id.clone())?;
        validate_review(fact.review, input.production, &fact.id)?;
        validate_evidence(&fact.evidence, &evidence_ids)?;
        for species in fact_species(&fact.proposition) {
            if !species_ids.contains(species) {
                return Err(CatalogueError::UnknownReference(species.to_owned()));
            }
        }
    }
    for rule in &input.structural_rules {
        StructuralRuleId::new(rule.id.clone())?;
        validate_review(rule.review, input.production, &rule.id)?;
        validate_evidence(&rule.evidence, &evidence_ids)?;
        validate_structural_rule(rule, &species_ids, &species_formulae, &symbols)?;
    }
    Ok(())
}

fn unique_strings<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<&'a str>, CatalogueError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value) {
            return Err(CatalogueError::DuplicateId(value.to_owned()));
        }
    }
    Ok(unique)
}

fn validate_evidence(
    references: &[String],
    evidence: &BTreeSet<&str>,
) -> Result<(), CatalogueError> {
    if references.is_empty() {
        return Err(CatalogueError::Missing("review evidence"));
    }
    for reference in references {
        if !evidence.contains(reference.as_str()) {
            return Err(CatalogueError::UnknownReference(reference.clone()));
        }
    }
    Ok(())
}

fn validate_review(state: ReviewState, production: bool, id: &str) -> Result<(), CatalogueError> {
    if production && state != ReviewState::Reviewed {
        return Err(CatalogueError::UnreviewedProductionRecord(id.to_owned()));
    }
    Ok(())
}

fn fact_species(proposition: &FactProposition) -> Vec<&str> {
    match proposition {
        FactProposition::Dissociates {
            substance,
            products,
        } => std::iter::once(substance.as_str())
            .chain(products.iter().map(String::as_str))
            .collect(),
        FactProposition::Insoluble { species, .. } | FactProposition::HasColour { species, .. } => {
            vec![species]
        }
    }
}

fn validate_structural_rule(
    rule: &StructuralRuleRecord,
    species: &BTreeSet<&str>,
    species_formulae: &BTreeMap<&str, &str>,
    elements: &BTreeSet<&str>,
) -> Result<(), CatalogueError> {
    if rule.states.len() != rule.operations.len().saturating_add(1) || rule.states.is_empty() {
        return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
    }
    for reference in rule.reactants.iter().chain(&rule.products) {
        if !species.contains(reference.as_str()) {
            return Err(CatalogueError::UnknownReference(reference.clone()));
        }
    }
    validate_equation_side(
        &rule.equation.reactants,
        &rule.reactants,
        species_formulae,
        &rule.id,
    )?;
    validate_equation_side(
        &rule.equation.products,
        &rule.products,
        species_formulae,
        &rule.id,
    )?;
    let initial_ids = atom_ids(&rule.states[0], elements)?;
    for (ordinal, state) in rule.states.iter().enumerate() {
        if usize::from(state.ordinal) != ordinal || atom_ids(state, elements)? != initial_ids {
            return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
        }
        validate_relationships(state, &initial_ids, species)?;
    }
    for operation in &rule.operations {
        match operation {
            StructuralOperation::AssociateIonic { left, right } => {
                validate_atom_reference(left, &initial_ids)?;
                validate_atom_reference(right, &initial_ids)?;
            }
            StructuralOperation::AssignProduct { product, atoms } => {
                if !species.contains(product.as_str()) || atoms.is_empty() {
                    return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
                }
                for atom in atoms {
                    validate_atom_reference(atom, &initial_ids)?;
                }
            }
            StructuralOperation::TransferMetallicElectron {
                domain,
                donor_site,
                acceptor,
                count,
                ..
            } => {
                if domain.is_empty() || *count == 0 {
                    return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
                }
                validate_atom_reference(donor_site, &initial_ids)?;
                validate_atom_reference(acceptor, &initial_ids)?;
            }
            StructuralOperation::CleaveCovalent {
                left, right, order, ..
            }
            | StructuralOperation::FormCovalent {
                left, right, order, ..
            } => {
                if !(1..=3).contains(order) {
                    return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
                }
                validate_atom_reference(left, &initial_ids)?;
                validate_atom_reference(right, &initial_ids)?;
            }
        }
    }
    for observation in &rule.observations {
        if observation.trigger_ordinal as usize >= rule.states.len() {
            return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
        }
        let species_id = match &observation.claim {
            ObservationClaim::ProductForms { species }
            | ObservationClaim::ProductHasColour { species, .. }
            | ObservationClaim::GasEvolves { species }
            | ObservationClaim::ReactantConsumed { species } => species,
        };
        if !species.contains(species_id.as_str()) {
            return Err(CatalogueError::UnknownReference(species_id.clone()));
        }
    }
    validate_presentation(rule)?;
    Ok(())
}

fn validate_equation_side(
    terms: &[StoichiometricTerm],
    expected_species: &[String],
    species_formulae: &BTreeMap<&str, &str>,
    rule_id: &str,
) -> Result<(), CatalogueError> {
    if terms.is_empty()
        || terms.iter().any(|term| {
            term.coefficient == 0
                || species_formulae.get(term.species.as_str()).copied()
                    != Some(term.formula.as_str())
        })
    {
        return Err(CatalogueError::InvalidStructuralRule(rule_id.to_owned()));
    }
    let actual = terms
        .iter()
        .map(|term| term.species.as_str())
        .collect::<BTreeSet<_>>();
    let expected = expected_species.iter().map(String::as_str).collect();
    if actual != expected {
        return Err(CatalogueError::InvalidStructuralRule(rule_id.to_owned()));
    }
    Ok(())
}

fn validate_presentation(rule: &StructuralRuleRecord) -> Result<(), CatalogueError> {
    let presentation = &rule.presentation;
    if presentation.id.is_empty()
        || presentation.disclosure.is_empty()
        || presentation.objects.is_empty()
        || presentation.camera.is_empty()
    {
        return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
    }
    let object_ids = unique_strings(presentation.objects.iter().map(|object| object.id.as_str()))?;
    if object_ids.len() != presentation.objects.len() {
        return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
    }
    let last_ordinal = u16::try_from(rule.states.len().saturating_sub(1)).unwrap_or(u16::MAX);
    for object in &presentation.objects {
        if object.semantic_identity.is_empty()
            || object.visible_from_ordinal > last_ordinal
            || object.transform.scale.contains(&0)
        {
            return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
        }
    }
    let observation_ids = rule
        .observations
        .iter()
        .map(|observation| observation.id.as_str())
        .collect::<BTreeSet<_>>();
    for effect in &presentation.effects {
        if !observation_ids.contains(effect.trigger_observation.as_str())
            || effect.start_ordinal > effect.end_ordinal
            || effect.end_ordinal > last_ordinal
        {
            return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
        }
    }
    for cue in &presentation.camera {
        if cue.start_ordinal > cue.end_ordinal || cue.end_ordinal > last_ordinal {
            return Err(CatalogueError::InvalidStructuralRule(rule.id.clone()));
        }
    }
    Ok(())
}

fn atom_ids<'a>(
    state: &'a StructuralState,
    elements: &BTreeSet<&str>,
) -> Result<BTreeSet<&'a str>, CatalogueError> {
    let mut ids = BTreeSet::new();
    for atom in &state.atoms {
        AtomId::new(atom.id.clone())?;
        if !elements.contains(atom.element.as_str()) || !ids.insert(atom.id.as_str()) {
            return Err(CatalogueError::InvalidStructuralRule(format!(
                "state {}",
                state.ordinal
            )));
        }
    }
    Ok(ids)
}

fn validate_relationships(
    state: &StructuralState,
    atoms: &BTreeSet<&str>,
    species: &BTreeSet<&str>,
) -> Result<(), CatalogueError> {
    for bond in &state.covalent_bonds {
        if !(1..=3).contains(&bond.order) {
            return Err(CatalogueError::InvalidStructuralRule(
                "invalid covalent order".to_owned(),
            ));
        }
        validate_atom_reference(&bond.left, atoms)?;
        validate_atom_reference(&bond.right, atoms)?;
    }
    for association in &state.ionic_associations {
        validate_atom_reference(&association.left, atoms)?;
        validate_atom_reference(&association.right, atoms)?;
    }
    for domain in &state.metallic_domains {
        if domain.id.is_empty() || domain.sites.is_empty() || domain.delocalized_electrons == 0 {
            return Err(CatalogueError::InvalidStructuralRule(
                "invalid metallic domain".to_owned(),
            ));
        }
        for site in &domain.sites {
            validate_atom_reference(site, atoms)?;
        }
    }
    for membership in &state.product_memberships {
        if !species.contains(membership.product.as_str()) {
            return Err(CatalogueError::UnknownReference(membership.product.clone()));
        }
        for atom in &membership.atoms {
            validate_atom_reference(atom, atoms)?;
        }
    }
    Ok(())
}

fn validate_atom_reference(atom: &str, atoms: &BTreeSet<&str>) -> Result<(), CatalogueError> {
    if atoms.contains(atom) {
        Ok(())
    } else {
        Err(CatalogueError::UnknownReference(atom.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogueError {
    Malformed(String),
    UnsupportedSchema(u32),
    DuplicateId(String),
    Missing(&'static str),
    InvalidElement(String),
    UnknownReference(String),
    UnreviewedProductionRecord(String),
    InvalidStructuralRule(String),
    InvalidId(String),
}

impl From<chem_domain::IdError> for CatalogueError {
    fn from(error: chem_domain::IdError) -> Self {
        Self::InvalidId(error.to_string())
    }
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => write!(formatter, "malformed catalogue: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported catalogue schema {version}")
            }
            Self::DuplicateId(id) => write!(formatter, "duplicate catalogue id `{id}`"),
            Self::Missing(field) => write!(formatter, "missing {field}"),
            Self::InvalidElement(element) => write!(formatter, "invalid element `{element}`"),
            Self::UnknownReference(reference) => {
                write!(formatter, "unknown catalogue reference `{reference}`")
            }
            Self::UnreviewedProductionRecord(id) => {
                write!(formatter, "production record `{id}` is not reviewed")
            }
            Self::InvalidStructuralRule(id) => {
                write!(formatter, "invalid structural rule `{id}`")
            }
            Self::InvalidId(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for CatalogueError {}

#[cfg(test)]
mod tests {
    use chem_domain::StructuralRuleId;

    use super::{CatalogueBundle, CatalogueError, CatalogueInput, ReviewState};

    const SILVER_CHLORIDE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/silver-chloride.catalogue.json");
    const LITHIUM_WATER: &[u8] =
        include_bytes!("../../../fixtures/catalogue/lithium-water.catalogue.json");

    #[test]
    fn reviewed_silver_chloride_bundle_loads_with_stable_digest_and_rule() {
        let first = CatalogueBundle::load_json(SILVER_CHLORIDE).expect("catalogue loads");
        let second = CatalogueBundle::load_json(SILVER_CHLORIDE).expect("catalogue reloads");
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.identity().as_str(), "ChemSpec.Aqueous");
        assert_eq!(first.version(), "1.0");
        let id = StructuralRuleId::new("ChemSpec.Structural.Precipitation.SilverChloride")
            .expect("rule id");
        let rule = first.structural_rule(&id).expect("reviewed rule resolves");
        assert_eq!(rule.states.len(), 4);
        assert_eq!(rule.operations.len(), 3);
        assert_eq!(rule.states[0].atoms, rule.states[3].atoms);
    }

    #[test]
    fn lithium_water_uses_reviewed_generic_structural_and_presentation_profiles() {
        let bundle = CatalogueBundle::load_json(LITHIUM_WATER).expect("catalogue loads");
        let id = StructuralRuleId::new("ChemSpec.Structural.Redox.LithiumWater").expect("rule id");
        let rule = bundle.structural_rule(&id).expect("reviewed rule resolves");
        assert_eq!(rule.states.len(), rule.operations.len() + 1);
        assert_eq!(rule.presentation.id, "presentation.reactive-metal-on-water");
        assert!(rule.operations.iter().any(|operation| matches!(
            operation,
            super::StructuralOperation::TransferMetallicElectron { .. }
        )));
    }

    #[test]
    fn every_semantic_mutation_changes_the_bundle_digest() {
        let original: CatalogueInput =
            serde_json::from_slice(SILVER_CHLORIDE).expect("fixture decodes");
        let original_bundle = CatalogueBundle::load(original.clone()).expect("fixture loads");
        let mut changed = original;
        changed.species[0].aliases.push("changedAlias".to_owned());
        let changed_bundle = CatalogueBundle::load(changed).expect("mutation remains valid");
        assert_ne!(original_bundle.digest(), changed_bundle.digest());
    }

    #[test]
    fn production_bundle_rejects_unreviewed_records_and_broken_atom_identity() {
        let original: CatalogueInput =
            serde_json::from_slice(SILVER_CHLORIDE).expect("fixture decodes");
        let mut provisional = original.clone();
        provisional.facts[0].review = ReviewState::Provisional;
        assert!(matches!(
            CatalogueBundle::load(provisional),
            Err(CatalogueError::UnreviewedProductionRecord(_))
        ));

        let mut broken = original;
        broken.structural_rules[0].states[1].atoms.pop();
        assert!(matches!(
            CatalogueBundle::load(broken),
            Err(CatalogueError::InvalidStructuralRule(_))
        ));
    }
}
