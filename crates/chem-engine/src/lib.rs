#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt;

use chem_catalogue::{
    AtomElectronState, AtomState, CatalogueBundle, CovalentBond, IonicAssociation, MetallicDomain,
    Phase, PresentationProfile, ProductMembership, ReviewedEquation, RuleObservation,
    StructuralOperation, StructuralState,
};
use chem_domain::{ContentDigest, StructuralRuleId, SubstanceId};
use chems_lang::{ChemicalSyntaxKind, Diagnostic, SourceNode, SourceNodeKind, parse_source};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpandedStructuralCertificate {
    source_digest: ContentDigest,
    catalogue_digest: ContentDigest,
    structural_rule_id: StructuralRuleId,
    event_model: String,
    sequence_model: String,
    states: Vec<StructuralState>,
    operations: Vec<StructuralOperation>,
    reactants: Vec<String>,
    products: Vec<String>,
    equation: ReviewedEquation,
    observations: Vec<RuleObservation>,
    safety_notices: Vec<String>,
    presentation: PresentationProfile,
}

impl ExpandedStructuralCertificate {
    #[must_use]
    pub const fn source_digest(&self) -> ContentDigest {
        self.source_digest
    }

    #[must_use]
    pub const fn catalogue_digest(&self) -> ContentDigest {
        self.catalogue_digest
    }

    #[must_use]
    pub const fn structural_rule_id(&self) -> &StructuralRuleId {
        &self.structural_rule_id
    }

    #[must_use]
    pub fn event_model(&self) -> &str {
        &self.event_model
    }

    #[must_use]
    pub fn sequence_model(&self) -> &str {
        &self.sequence_model
    }

    #[must_use]
    pub fn states(&self) -> &[StructuralState] {
        &self.states
    }

    #[must_use]
    pub fn operations(&self) -> &[StructuralOperation] {
        &self.operations
    }

    #[must_use]
    pub fn observations(&self) -> &[RuleObservation] {
        &self.observations
    }

    #[must_use]
    pub fn reactants(&self) -> &[String] {
        &self.reactants
    }

    #[must_use]
    pub fn products(&self) -> &[String] {
        &self.products
    }

    #[must_use]
    pub const fn equation(&self) -> &ReviewedEquation {
        &self.equation
    }

    #[must_use]
    pub const fn presentation(&self) -> &PresentationProfile {
        &self.presentation
    }

    #[must_use]
    pub fn safety_notices(&self) -> &[String] {
        &self.safety_notices
    }
}

/// Resolves one `.chems 1` model binding and deterministically expands the
/// selected reviewed catalogue certificate. This result is not yet trusted.
///
/// # Errors
///
/// Returns a typed boundary error for malformed source, catalogue mismatch,
/// missing model disclosure, invalid rule identity, or unknown reviewed rule.
pub fn expand_structural_rule(
    source: &str,
    catalogue: &CatalogueBundle,
) -> Result<ExpandedStructuralCertificate, ExpansionError> {
    let parsed = parse_source(source);
    if !parsed.is_complete() {
        return Err(ExpansionError::Source(parsed.diagnostics));
    }
    let source_catalogue = parsed
        .ast
        .catalogue
        .as_ref()
        .ok_or(ExpansionError::MissingCatalogue)?;
    if source_catalogue.name != catalogue.identity().as_str()
        || source_catalogue.version.as_deref() != Some(catalogue.version())
    {
        return Err(ExpansionError::CatalogueMismatch {
            source: format!(
                "{}@{}",
                source_catalogue.name,
                source_catalogue.version.as_deref().unwrap_or("?")
            ),
            loaded: format!("{}@{}", catalogue.identity(), catalogue.version()),
        });
    }
    let experiment = parsed
        .ast
        .experiment
        .as_ref()
        .ok_or(ExpansionError::MissingModel)?;
    let model = experiment
        .model
        .as_ref()
        .ok_or(ExpansionError::MissingModel)?;
    let structural_rule_id = StructuralRuleId::new(model.structural_rule.clone())
        .map_err(|error| ExpansionError::InvalidRuleId(error.to_string()))?;
    let rule = catalogue
        .structural_rule(&structural_rule_id)
        .ok_or_else(|| ExpansionError::UnknownRule(model.structural_rule.clone()))?;
    let authored_reactants = source_species(&experiment.materials)
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let rule_reactants = rule
        .reactants
        .iter()
        .map(|id| catalogue_species_formula(catalogue, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let authored_expectations = experiment
        .expectations
        .iter()
        .flat_map(|expectation| expectation.claims.iter())
        .flat_map(species_in)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let rule_products = rule
        .products
        .iter()
        .map(|id| catalogue_species_formula(catalogue, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if authored_reactants != rule_reactants || !rule_products.is_subset(&authored_expectations) {
        return Err(ExpansionError::RuleInapplicable {
            rule: model.structural_rule.clone(),
        });
    }

    Ok(ExpandedStructuralCertificate {
        source_digest: ContentDigest::sha256(source.as_bytes()),
        catalogue_digest: catalogue.digest(),
        structural_rule_id,
        event_model: model.event.clone(),
        sequence_model: model.sequence.clone(),
        states: rule.states.clone(),
        operations: rule.operations.clone(),
        reactants: rule.reactants.clone(),
        products: rule.products.clone(),
        equation: rule.equation.clone(),
        observations: rule.observations.clone(),
        safety_notices: rule.safety_notices.clone(),
        presentation: rule.presentation.clone(),
    })
}

fn catalogue_species_formula(
    catalogue: &CatalogueBundle,
    id: &str,
) -> Result<String, ExpansionError> {
    let id = SubstanceId::new(id.to_owned())
        .map_err(|_| ExpansionError::MalformedCatalogueReference(id.to_owned()))?;
    catalogue
        .species(&id)
        .map(|species| {
            let phase = match species.phase {
                Phase::Aqueous => "(aq)",
                Phase::Solid => "(s)",
                Phase::Liquid => "(l)",
                Phase::Gas => "(g)",
            };
            format!("{}{phase}", species.formula)
        })
        .ok_or_else(|| ExpansionError::MalformedCatalogueReference(id.to_string()))
}

fn source_species(nodes: &[SourceNode]) -> BTreeSet<&str> {
    nodes.iter().flat_map(species_in).collect()
}

fn species_in(node: &SourceNode) -> Vec<&str> {
    let mut values = Vec::new();
    if matches!(
        node.kind,
        SourceNodeKind::Chemical {
            form: ChemicalSyntaxKind::Species
        }
    ) && let Some(lexeme) = node.lexeme.as_deref()
    {
        values.push(lexeme);
        return values;
    }
    for child in &node.children {
        values.extend(species_in(child));
    }
    values
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationDisposition {
    Validated,
    ValidatedWithAssumptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedStructuralReaction {
    certificate: ExpandedStructuralCertificate,
    digest: ContentDigest,
    disposition: ValidationDisposition,
}

impl ValidatedStructuralReaction {
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub const fn disposition(&self) -> ValidationDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn source_digest(&self) -> ContentDigest {
        self.certificate.source_digest
    }

    #[must_use]
    pub const fn catalogue_digest(&self) -> ContentDigest {
        self.certificate.catalogue_digest
    }

    #[must_use]
    pub const fn structural_rule_id(&self) -> &StructuralRuleId {
        &self.certificate.structural_rule_id
    }

    #[must_use]
    pub fn reactants(&self) -> &[String] {
        &self.certificate.reactants
    }

    #[must_use]
    pub fn products(&self) -> &[String] {
        &self.certificate.products
    }

    #[must_use]
    pub const fn equation(&self) -> &ReviewedEquation {
        &self.certificate.equation
    }

    #[must_use]
    pub fn observations(&self) -> &[RuleObservation] {
        &self.certificate.observations
    }

    #[must_use]
    pub const fn presentation(&self) -> &PresentationProfile {
        &self.certificate.presentation
    }

    #[must_use]
    pub fn safety_notices(&self) -> &[String] {
        &self.certificate.safety_notices
    }
}

/// Checks the expanded certificate and privately constructs the trusted
/// structural reaction artifact.
///
/// # Errors
///
/// Rejects any atom, charge, relationship, operation, product, observation, or
/// disclosure inconsistency.
pub fn validate_structural_reaction(
    certificate: ExpandedStructuralCertificate,
) -> Result<ValidatedStructuralReaction, StructuralValidationError> {
    if certificate.event_model != "representative" || certificate.sequence_model != "explanatory" {
        return Err(StructuralValidationError::InvalidDisclosure);
    }
    if certificate.states.len() != certificate.operations.len() + 1 || certificate.states.is_empty()
    {
        return Err(StructuralValidationError::InvalidTimeline);
    }
    let initial_atoms = atom_identity(&certificate.states[0]);
    let initial_charge = total_formal_charge(&certificate.states[0]);
    let initial_electrons = represented_electrons(&certificate.states[0]);
    for state in &certificate.states {
        if atom_identity(state) != initial_atoms
            || total_formal_charge(state) != initial_charge
            || represented_electrons(state) != initial_electrons
        {
            return Err(StructuralValidationError::Conservation);
        }
    }
    for (index, operation) in certificate.operations.iter().enumerate() {
        validate_transition(
            &certificate.states[index],
            &certificate.states[index + 1],
            operation,
        )?;
    }
    validate_final_products(&certificate)?;
    for observation in &certificate.observations {
        if usize::from(observation.trigger_ordinal) >= certificate.states.len() {
            return Err(StructuralValidationError::InvalidObservation);
        }
    }
    let value =
        serde_json::to_value(&certificate).map_err(|_| StructuralValidationError::Serialization)?;
    let digest =
        ContentDigest::of_json(&value).map_err(|_| StructuralValidationError::Serialization)?;
    Ok(ValidatedStructuralReaction {
        certificate,
        digest,
        disposition: ValidationDisposition::ValidatedWithAssumptions,
    })
}

fn validate_transition(
    before: &StructuralState,
    after: &StructuralState,
    operation: &StructuralOperation,
) -> Result<(), StructuralValidationError> {
    match operation {
        StructuralOperation::AssociateIonic { left, right } => {
            if before.atoms != after.atoms
                || before.covalent_bonds != after.covalent_bonds
                || before.metallic_domains != after.metallic_domains
                || before.product_memberships != after.product_memberships
            {
                return Err(StructuralValidationError::OperationMismatch);
            }
            let mut expected = before.ionic_associations.clone();
            expected.push(IonicAssociation {
                left: left.clone(),
                right: right.clone(),
            });
            if normalized_associations(&expected)
                != normalized_associations(&after.ionic_associations)
            {
                return Err(StructuralValidationError::OperationMismatch);
            }
        }
        StructuralOperation::AssignProduct { product, atoms } => {
            if before.atoms != after.atoms
                || before.covalent_bonds != after.covalent_bonds
                || before.metallic_domains != after.metallic_domains
                || before.ionic_associations != after.ionic_associations
            {
                return Err(StructuralValidationError::OperationMismatch);
            }
            let mut expected = before.product_memberships.clone();
            expected.push(ProductMembership {
                product: product.clone(),
                atoms: atoms.clone(),
            });
            if normalized_memberships(&expected)
                != normalized_memberships(&after.product_memberships)
            {
                return Err(StructuralValidationError::OperationMismatch);
            }
        }
        StructuralOperation::TransferMetallicElectron {
            domain,
            donor_site,
            acceptor,
            count,
            acceptor_after,
        } => validate_metallic_transfer(
            before,
            after,
            domain,
            donor_site,
            acceptor,
            *count,
            acceptor_after,
        )?,
        StructuralOperation::CleaveCovalent {
            left,
            right,
            order,
            left_after,
            right_after,
        } => {
            validate_bond_change(
                before,
                after,
                left,
                right,
                *order,
                left_after,
                right_after,
                false,
            )?;
        }
        StructuralOperation::FormCovalent {
            left,
            right,
            order,
            left_after,
            right_after,
        } => {
            validate_bond_change(
                before,
                after,
                left,
                right,
                *order,
                left_after,
                right_after,
                true,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_metallic_transfer(
    before: &StructuralState,
    after: &StructuralState,
    domain: &str,
    donor_site: &str,
    acceptor: &str,
    count: u8,
    acceptor_after: &AtomElectronState,
) -> Result<(), StructuralValidationError> {
    if before.covalent_bonds != after.covalent_bonds
        || before.ionic_associations != after.ionic_associations
        || before.product_memberships != after.product_memberships
    {
        return Err(StructuralValidationError::OperationMismatch);
    }
    let mut expected_atoms = before.atoms.clone();
    replace_electron_state(&mut expected_atoms, acceptor, acceptor_after)?;
    let mut expected_domains = before.metallic_domains.clone();
    let Some(index) = expected_domains.iter().position(|item| item.id == domain) else {
        return Err(StructuralValidationError::OperationMismatch);
    };
    let metallic = &mut expected_domains[index];
    if !metallic.sites.iter().any(|site| site == donor_site)
        || metallic.delocalized_electrons < count
    {
        return Err(StructuralValidationError::OperationMismatch);
    }
    metallic.delocalized_electrons -= count;
    metallic.sites.retain(|site| site != donor_site);
    if metallic.delocalized_electrons == 0 {
        expected_domains.remove(index);
    }
    if expected_atoms != after.atoms || expected_domains != after.metallic_domains {
        return Err(StructuralValidationError::OperationMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_bond_change(
    before: &StructuralState,
    after: &StructuralState,
    left: &str,
    right: &str,
    order: u8,
    left_after: &AtomElectronState,
    right_after: &AtomElectronState,
    forming: bool,
) -> Result<(), StructuralValidationError> {
    if before.ionic_associations != after.ionic_associations
        || before.metallic_domains != after.metallic_domains
        || before.product_memberships != after.product_memberships
    {
        return Err(StructuralValidationError::OperationMismatch);
    }
    let mut expected_atoms = before.atoms.clone();
    replace_electron_state(&mut expected_atoms, left, left_after)?;
    replace_electron_state(&mut expected_atoms, right, right_after)?;
    let mut expected_bonds = before.covalent_bonds.clone();
    if forming {
        expected_bonds.push(CovalentBond {
            left: left.to_owned(),
            right: right.to_owned(),
            order,
            dative_origin: None,
        });
    } else {
        let Some(index) = expected_bonds.iter().position(|bond| {
            bond.order == order
                && ((bond.left == left && bond.right == right)
                    || (bond.left == right && bond.right == left))
        }) else {
            return Err(StructuralValidationError::OperationMismatch);
        };
        expected_bonds.remove(index);
    }
    if expected_atoms != after.atoms || expected_bonds != after.covalent_bonds {
        return Err(StructuralValidationError::OperationMismatch);
    }
    Ok(())
}

fn replace_electron_state(
    atoms: &mut [AtomState],
    id: &str,
    state: &AtomElectronState,
) -> Result<(), StructuralValidationError> {
    let Some(atom) = atoms.iter_mut().find(|atom| atom.id == id) else {
        return Err(StructuralValidationError::OperationMismatch);
    };
    atom.formal_charge = state.formal_charge;
    atom.non_bonding_electrons = state.non_bonding_electrons;
    atom.unpaired_electrons = state.unpaired_electrons;
    Ok(())
}

fn validate_final_products(
    certificate: &ExpandedStructuralCertificate,
) -> Result<(), StructuralValidationError> {
    let final_state = certificate
        .states
        .last()
        .ok_or(StructuralValidationError::InvalidTimeline)?;
    let products = final_state
        .product_memberships
        .iter()
        .map(|membership| membership.product.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = certificate
        .products
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if products != expected {
        return Err(StructuralValidationError::ProductMismatch);
    }
    let assigned = final_state
        .product_memberships
        .iter()
        .flat_map(|membership| membership.atoms.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let unique = assigned
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if assigned.len() != unique.len() || unique != atom_identity(final_state) {
        return Err(StructuralValidationError::ProductMismatch);
    }
    Ok(())
}

fn atom_identity(state: &StructuralState) -> std::collections::BTreeSet<&str> {
    state.atoms.iter().map(|atom| atom.id.as_str()).collect()
}

fn total_formal_charge(state: &StructuralState) -> i32 {
    let atomic = state
        .atoms
        .iter()
        .map(|atom| i32::from(atom.formal_charge))
        .sum::<i32>();
    let delocalized = state
        .metallic_domains
        .iter()
        .map(|domain| i32::from(domain.delocalized_electrons))
        .sum::<i32>();
    atomic - delocalized
}

fn represented_electrons(state: &StructuralState) -> u32 {
    let non_bonding = state
        .atoms
        .iter()
        .map(|atom| u32::from(atom.non_bonding_electrons))
        .sum::<u32>();
    let covalent = state
        .covalent_bonds
        .iter()
        .map(|bond| u32::from(bond.order) * 2)
        .sum::<u32>();
    let metallic = state
        .metallic_domains
        .iter()
        .map(|domain| u32::from(domain.delocalized_electrons))
        .sum::<u32>();
    non_bonding + covalent + metallic
}

fn normalized_associations(values: &[IonicAssociation]) -> Vec<(&str, &str)> {
    let mut normalized = values
        .iter()
        .map(|value| (value.left.as_str(), value.right.as_str()))
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized
}

fn normalized_memberships(values: &[ProductMembership]) -> Vec<(&str, Vec<&str>)> {
    let mut normalized = values
        .iter()
        .map(|value| {
            let mut atoms = value.atoms.iter().map(String::as_str).collect::<Vec<_>>();
            atoms.sort_unstable();
            (value.product.as_str(), atoms)
        })
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralValidationError {
    InvalidDisclosure,
    InvalidTimeline,
    Conservation,
    OperationMismatch,
    ProductMismatch,
    InvalidObservation,
    Serialization,
}

impl fmt::Display for StructuralValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "structural validation failed: {self:?}")
    }
}

impl std::error::Error for StructuralValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservationStage {
    Pending,
    Active,
    Established,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameObservation {
    pub observation: RuleObservation,
    pub stage: ObservationStage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralFrame {
    pub id: ContentDigest,
    pub ordinal: u16,
    pub atoms: Vec<AtomState>,
    pub covalent_bonds: Vec<CovalentBond>,
    pub ionic_associations: Vec<IonicAssociation>,
    pub metallic_domains: Vec<MetallicDomain>,
    pub product_memberships: Vec<ProductMembership>,
    pub active_operation: Option<StructuralOperation>,
    pub observations: Vec<FrameObservation>,
    pub event_model: String,
    pub sequence_model: String,
}

/// Generates renderer-independent frames from a trusted structural reaction.
/// No corresponding function accepts an expanded or unvalidated certificate.
///
/// # Errors
///
/// Returns an error only if canonical frame identity serialization fails.
pub fn structural_frames(
    validated: &ValidatedStructuralReaction,
) -> Result<Vec<StructuralFrame>, StructuralValidationError> {
    validated
        .certificate
        .states
        .iter()
        .enumerate()
        .map(|(index, state)| {
            let observations = validated
                .certificate
                .observations
                .iter()
                .cloned()
                .map(|observation| {
                    let stage = match index.cmp(&usize::from(observation.trigger_ordinal)) {
                        std::cmp::Ordering::Less => ObservationStage::Pending,
                        std::cmp::Ordering::Equal => ObservationStage::Active,
                        std::cmp::Ordering::Greater => ObservationStage::Established,
                    };
                    FrameObservation { observation, stage }
                })
                .collect::<Vec<_>>();
            let mut frame = StructuralFrame {
                id: ContentDigest::sha256(&[]),
                ordinal: state.ordinal,
                atoms: state.atoms.clone(),
                covalent_bonds: state.covalent_bonds.clone(),
                ionic_associations: state.ionic_associations.clone(),
                metallic_domains: state.metallic_domains.clone(),
                product_memberships: state.product_memberships.clone(),
                active_operation: index
                    .checked_sub(1)
                    .and_then(|operation| validated.certificate.operations.get(operation))
                    .cloned(),
                observations,
                event_model: validated.certificate.event_model.clone(),
                sequence_model: validated.certificate.sequence_model.clone(),
            };
            let value = serde_json::to_value(&frame)
                .map_err(|_| StructuralValidationError::Serialization)?;
            frame.id = ContentDigest::of_json(&value)
                .map_err(|_| StructuralValidationError::Serialization)?;
            Ok(frame)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpansionError {
    Source(Vec<Diagnostic>),
    MissingCatalogue,
    CatalogueMismatch { source: String, loaded: String },
    MissingModel,
    InvalidRuleId(String),
    UnknownRule(String),
    RuleInapplicable { rule: String },
    MalformedCatalogueReference(String),
}

impl fmt::Display for ExpansionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(diagnostics) => {
                write!(formatter, "source has {} diagnostic(s)", diagnostics.len())
            }
            Self::MissingCatalogue => formatter.write_str("source has no catalogue selection"),
            Self::CatalogueMismatch { source, loaded } => {
                write!(
                    formatter,
                    "source selects `{source}` but `{loaded}` is loaded"
                )
            }
            Self::MissingModel => formatter.write_str("source has no model disclosure"),
            Self::InvalidRuleId(error) => write!(formatter, "invalid structural rule id: {error}"),
            Self::UnknownRule(rule) => write!(formatter, "unknown structural rule `{rule}`"),
            Self::RuleInapplicable { rule } => {
                write!(
                    formatter,
                    "structural rule `{rule}` does not match the authored reaction"
                )
            }
            Self::MalformedCatalogueReference(id) => {
                write!(formatter, "structural rule contains unknown species `{id}`")
            }
        }
    }
}

impl std::error::Error for ExpansionError {}

#[cfg(test)]
mod tests {
    use chem_catalogue::{CatalogueBundle, CatalogueInput};

    use super::{
        ExpansionError, ObservationStage, StructuralValidationError, ValidationDisposition,
        expand_structural_rule, structural_frames, validate_structural_reaction,
    };

    const SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
    const CATALOGUE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/silver-chloride.catalogue.json");
    const LITHIUM_SOURCE: &str = include_str!("../../../fixtures/lithium-water.chems");
    const LITHIUM_CATALOGUE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/lithium-water.catalogue.json");

    #[test]
    fn canonical_source_resolves_one_reviewed_rule_deterministically() {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let first = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        let second = expand_structural_rule(SOURCE, &catalogue).expect("rule expands again");
        assert_eq!(first, second);
        assert_eq!(first.event_model(), "representative");
        assert_eq!(first.sequence_model(), "explanatory");
        assert_eq!(first.states().len(), first.operations().len() + 1);
        assert_eq!(first.catalogue_digest(), catalogue.digest());
    }

    #[test]
    fn unknown_rule_and_catalogue_digest_changes_cannot_reuse_expansion() {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let unknown = SOURCE.replace(
            "ChemSpec.Structural.Precipitation.SilverChloride",
            "ChemSpec.Structural.Unknown",
        );
        assert!(matches!(
            expand_structural_rule(&unknown, &catalogue),
            Err(ExpansionError::UnknownRule(_))
        ));

        let original = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        let mut changed: CatalogueInput =
            serde_json::from_slice(CATALOGUE).expect("catalogue decodes");
        changed.species[0].aliases.push("changedAlias".to_owned());
        let changed = CatalogueBundle::load(changed).expect("changed catalogue loads");
        let refreshed = expand_structural_rule(SOURCE, &changed).expect("rule re-expands");
        assert_ne!(original.catalogue_digest(), refreshed.catalogue_digest());
    }

    #[test]
    fn reviewed_rule_must_match_authored_reactants_and_products() {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let mismatched = SOURCE.replace("AgNO3(aq)", "H2O(l)");

        assert!(matches!(
            expand_structural_rule(&mismatched, &catalogue),
            Err(ExpansionError::RuleInapplicable { .. })
        ));
    }

    #[test]
    fn trusted_validation_is_required_before_shared_frames_exist() {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let expanded = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        let validated = validate_structural_reaction(expanded).expect("certificate validates");
        assert_eq!(
            validated.disposition(),
            ValidationDisposition::ValidatedWithAssumptions
        );
        let frames = structural_frames(&validated).expect("frames generate");
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].atoms, frames[3].atoms);
        assert!(frames[0].active_operation.is_none());
        assert_eq!(
            frames[1].active_operation.as_ref(),
            validated.certificate.operations.first()
        );
        assert_eq!(frames[1].observations[0].stage, ObservationStage::Pending);
        assert_eq!(frames[2].observations[0].stage, ObservationStage::Active);
        assert_eq!(
            frames[3].observations[0].stage,
            ObservationStage::Established
        );
        assert_ne!(frames[0].id, frames[1].id);
    }

    #[test]
    fn kernel_rejects_charge_and_operation_mutations() {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let mut charge = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        charge.states[2].atoms[0].formal_charge = 0;
        assert_eq!(
            validate_structural_reaction(charge),
            Err(StructuralValidationError::Conservation)
        );

        let mut operation = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        operation.states[1].ionic_associations.clear();
        assert_eq!(
            validate_structural_reaction(operation),
            Err(StructuralValidationError::OperationMismatch)
        );
    }

    #[test]
    fn lithium_water_validates_electron_bond_and_metallic_domain_changes() {
        let catalogue = CatalogueBundle::load_json(LITHIUM_CATALOGUE).expect("catalogue loads");
        let expanded = expand_structural_rule(LITHIUM_SOURCE, &catalogue).expect("rule expands");
        let validated = validate_structural_reaction(expanded).expect("rule validates");
        let frames = structural_frames(&validated).expect("frames generate");
        assert_eq!(frames.len(), 11);
        assert_eq!(frames[0].metallic_domains[0].delocalized_electrons, 2);
        assert!(frames[2].metallic_domains.is_empty());
        assert_eq!(frames[5].covalent_bonds.len(), 3);
        assert_eq!(frames[10].product_memberships.len(), 3);
        assert_eq!(validated.safety_notices().len(), 1);
    }
}
