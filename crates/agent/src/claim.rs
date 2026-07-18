use chem_catalogue::{
    AtomRecord, BinaryElectronStateRecord, BondDelocalizationRecord, BondOrderRecord, BondRecord,
    ComponentRecord, ElectronContributionRecord, ElectronStateRecord, ElementValenceRecord,
    GroupRecord, IonicAssociationRecord, MetallicDomainRecord, MetallicElectronStateRecord,
    MetallicJoinAllocationRecord, MetallicReleaseAllocationRecord, MetallicValenceStateRecord,
    TransferElectronStateRecord, ValenceStateRecord,
};
use serde::{Deserialize, Serialize};

use crate::{AgentError, AgentErrorKind};

pub const REACTION_CLAIM_SCHEMA_VERSION: u32 = 1;
pub const MECHANISM_ESCALATION_SCHEMA_VERSION: u32 = 1;
pub const STRUCTURE_PROPOSAL_SCHEMA_VERSION: u32 = 1;
pub const MAX_REACTION_CLAIM_BYTES: usize = 64 * 1024;
pub const MAX_MECHANISM_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_STRUCTURE_RESPONSE_BYTES: usize = 128 * 1024;
pub const MAX_CLAIM_SOURCES: usize = 4;

const MAX_CLAIM_PRODUCTS: usize = 16;
const MAX_CLAIM_OBSERVATIONS: usize = 16;
const MAX_IDENTITY_HINTS: usize = 12;
const MAX_SOURCE_SUPPORTS: usize = 4;
const MAX_AMBIGUITY_ALTERNATIVES: usize = 8;
const MAX_PRODUCT_NAME_CHARS: usize = 300;
const MAX_FORMULA_CHARS: usize = 200;
const MAX_CONTEXT_CHARS: usize = 1_000;
const MAX_IDENTITY_HINT_CHARS: usize = 500;
const MAX_OBSERVATION_CHARS: usize = 300;
const MAX_SOURCE_ID_CHARS: usize = 40;
const MAX_SOURCE_TITLE_CHARS: usize = 500;
const MAX_SOURCE_PUBLISHER_CHARS: usize = 300;
const MAX_SOURCE_URL_CHARS: usize = 2_000;
const MAX_SOURCE_EXCERPT_CHARS: usize = 1_200;
const MAX_WIRE_IDENTIFIER_CHARS: usize = 120;
const MAX_MECHANISM_LABEL_CHARS: usize = 160;

/// Fixed factual-claim policy retained in cache bindings.
///
/// The application exposes one low-latency path; this enum remains serialized
/// so existing Fast cache entries retain a stable contract marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimMode {
    #[default]
    Fast,
}

/// Closed factual outcome returned by the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionClaim {
    pub schema_version: u32,
    pub disposition: ClaimDisposition,
    pub products: Vec<ClaimProduct>,
    pub required_context: String,
    pub observations: Vec<ClaimObservation>,
    pub sources: Vec<ClaimSource>,
    pub ambiguity: Option<ClaimAmbiguity>,
    #[serde(skip)]
    origin: ClaimProvenance,
    #[serde(skip)]
    solver_reason: Option<NoReactionReason>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClaimProvenance {
    #[default]
    Provider,
    Solver,
}

/// A bounded factual claim decoded from an untrusted provider boundary.
/// This capability cannot express solver-authored explanation copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProviderClaim(ReactionClaim);

impl<'de> Deserialize<'de> for ProviderClaim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let claim = ReactionClaim::deserialize(deserializer)?;
        claim.validate_wire().map_err(serde::de::Error::custom)?;
        Ok(Self(claim))
    }
}

impl std::ops::Deref for ProviderClaim {
    type Target = ReactionClaim;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ProviderClaim {
    /// Decodes one bounded provider claim. Solver-only fields are not part of
    /// this wire type and fail as unknown input.
    ///
    /// # Errors
    ///
    /// Returns a typed provider-output error for malformed or out-of-contract
    /// bytes.
    pub fn from_json(bytes: &[u8], _mode: ClaimMode) -> Result<Self, AgentError> {
        if bytes.len() > MAX_REACTION_CLAIM_BYTES {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "reaction claim",
                format!("claim exceeds the {MAX_REACTION_CLAIM_BYTES}-byte contract limit"),
            ));
        }
        let claim: ReactionClaim = serde_json::from_slice(bytes).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "reaction claim",
                error,
            )
        })?;
        claim.validate_wire()?;
        Ok(Self(claim))
    }

    #[must_use]
    pub fn into_claim(self) -> ReactionClaim {
        self.0
    }

    pub(crate) fn from_compiled(claim: ReactionClaim) -> Option<Self> {
        (!claim.is_solver_authored()).then_some(Self(claim))
    }
}

/// A deterministic solver conclusion. Only this capability may carry a
/// physical no-reaction reason for learner-facing explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolvedClaim(ReactionClaim);

impl std::ops::Deref for SolvedClaim {
    type Target = ReactionClaim;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Closed provenance-bearing input accepted by the claim compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimInput {
    Provider(ProviderClaim),
    Solved(SolvedClaim),
}

impl From<ProviderClaim> for ClaimInput {
    fn from(claim: ProviderClaim) -> Self {
        Self::Provider(claim)
    }
}

impl From<SolvedClaim> for ClaimInput {
    fn from(claim: SolvedClaim) -> Self {
        Self::Solved(claim)
    }
}

impl ClaimInput {
    pub(crate) fn into_claim(self) -> ReactionClaim {
        match self {
            Self::Provider(claim) => claim.0,
            Self::Solved(claim) => claim.0,
        }
    }
}

/// Why a pairing confidently does nothing. Variants carry lowercase
/// element or compound display names ("copper"), not symbols.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoReactionReason {
    /// The metal cannot displace hydrogen from acids or water.
    BelowHydrogen { metal: String },
    /// The metal is less active than the dissolved cation it would displace.
    LessActiveMetal { metal: String, displaced: String },
    /// The halogen is no more reactive than the one already in the salt.
    LessReactiveHalogen { incoming: String, resident: String },
    /// Partner exchange would only produce two more soluble salts.
    AllProductsSoluble,
    /// A light noble gas with a full outer shell.
    NobleGas { element: String },
    /// Two elemental metals: alloys are mixtures, not reactions.
    TwoMetals,
    /// Both portions are the same closed-shell substance.
    SameSubstance,
    /// An alkali-metal carbonate or hydroxide that shrugs off heating.
    HeatStable { compound: String },
    /// A recognised metal oxide that neither dissolves nor slakes.
    OxideInertInWater { metal: String },
}

impl NoReactionReason {
    /// Classroom-voice explanation of why nothing happens.
    #[must_use]
    pub fn learner_explanation(&self) -> String {
        let capitalize = |word: &str| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        };
        match self {
            Self::BelowHydrogen { metal } => format!(
                "{} sits below hydrogen in the activity series, so it cannot displace hydrogen — dilute acids and water leave it untouched.",
                capitalize(metal)
            ),
            Self::LessActiveMetal { metal, displaced } => format!(
                "{} is less active than {displaced}, so it cannot push {displaced} out of its dissolved salt.",
                capitalize(metal)
            ),
            Self::LessReactiveHalogen { incoming, resident } if incoming == resident => format!(
                "The salt already contains {resident} — there is nothing new for {incoming} to displace."
            ),
            Self::LessReactiveHalogen { incoming, resident } => format!(
                "{} is less reactive than {resident}, so it cannot displace {resident} from its salt.",
                capitalize(incoming)
            ),
            Self::AllProductsSoluble => "Swapping partners would only make two more soluble salts — every ion stays dissolved, so nothing new forms.".to_owned(),
            Self::NobleGas { element } => format!(
                "{} already has a full outer shell of electrons, so it has no drive to share, gain, or lose any.",
                capitalize(element)
            ),
            Self::TwoMetals => "Two metals can melt together into an alloy, but that is a mixture — no bonds form or break, so it is not a reaction.".to_owned(),
            Self::SameSubstance => "Both portions are the same substance, so there is nothing new for them to form.".to_owned(),
            Self::HeatStable { compound } => format!(
                "{} is heat-stable: the alkali-metal carbonates and hydroxides stay intact at Bunsen temperatures.",
                capitalize(compound)
            ),
            Self::OxideInertInWater { metal } => format!(
                "{} oxide does not react with water — it simply settles as an insoluble solid.",
                capitalize(metal)
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDisposition {
    Reaction,
    NoReaction,
    Ambiguous,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimProduct {
    pub name: String,
    pub formula: String,
    pub phase: ClaimPhase,
    pub identity_hints: Vec<ClaimIdentityHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimPhase {
    Aqueous,
    Solid,
    Liquid,
    Gas,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimIdentityHint {
    pub kind: ClaimIdentityHintKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimIdentityHintKind {
    Inchi,
    InchiKey,
    CanonicalSmiles,
    IsomericSmiles,
    PubChemCid,
    RegistryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimObservation {
    pub predicate: ClaimObservationPredicate,
    pub subject: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimObservationPredicate {
    Evolves,
    Disappears,
    Forms,
    Colour,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimSource {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub supporting_excerpt: String,
    pub supports: Vec<ClaimField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimField {
    Products,
    RequiredContext,
    Observations,
    NoReaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimAmbiguity {
    pub kind: ClaimAmbiguityKind,
    pub summary: String,
    pub alternatives: Vec<ClaimAlternative>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimAmbiguityKind {
    Conditions,
    ReactantIdentity,
    MultipleOutcomes,
    ConflictingEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimAlternative {
    pub label: String,
    pub products: Vec<ClaimProduct>,
    pub required_context: String,
}

impl ReactionClaim {
    pub(crate) fn solved(
        disposition: ClaimDisposition,
        products: Vec<ClaimProduct>,
        required_context: String,
        observations: Vec<ClaimObservation>,
        solver_reason: Option<NoReactionReason>,
    ) -> SolvedClaim {
        SolvedClaim(Self {
            schema_version: REACTION_CLAIM_SCHEMA_VERSION,
            disposition,
            products,
            required_context,
            observations,
            sources: Vec::new(),
            ambiguity: None,
            origin: ClaimProvenance::Solver,
            solver_reason,
        })
    }

    #[must_use]
    pub const fn no_reaction_reason(&self) -> Option<&NoReactionReason> {
        self.solver_reason.as_ref()
    }

    #[must_use]
    pub const fn provenance(&self) -> ClaimProvenance {
        self.origin
    }

    pub(crate) const fn is_solver_authored(&self) -> bool {
        matches!(self.origin, ClaimProvenance::Solver)
    }

    pub(crate) fn validate_wire(&self) -> Result<(), AgentError> {
        require_serialized_size(self, MAX_REACTION_CLAIM_BYTES, "claim", "reaction claim")?;
        if self.schema_version != REACTION_CLAIM_SCHEMA_VERSION {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "reaction claim",
                format!("unsupported claim schema {}", self.schema_version),
            ));
        }
        if self.solver_reason.is_some() && self.disposition != ClaimDisposition::NoReaction {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "reaction claim",
                "only a no-reaction claim may carry a no-reaction reason",
            ));
        }
        match self.disposition {
            ClaimDisposition::Reaction => {
                if self.products.is_empty() || self.ambiguity.is_some() {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "reaction claim",
                        "a reaction requires products and cannot carry ambiguity",
                    ));
                }
            }
            ClaimDisposition::NoReaction | ClaimDisposition::Unsupported => {
                if !self.products.is_empty()
                    || !self.observations.is_empty()
                    || self.ambiguity.is_some()
                {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "reaction claim",
                        "no-reaction and unsupported claims cannot carry products, observations, or ambiguity",
                    ));
                }
            }
            ClaimDisposition::Ambiguous => {
                let ambiguity = self.ambiguity.as_ref().ok_or_else(|| {
                    AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "reaction claim",
                        "an ambiguous claim requires ambiguity details",
                    )
                })?;
                if !self.products.is_empty()
                    || !self.observations.is_empty()
                    || ambiguity.alternatives.len() < 2
                {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "reaction claim",
                        "an ambiguous claim requires at least two alternatives and no selected outcome",
                    ));
                }
            }
        }
        self.validate_fields()
    }

    fn validate_fields(&self) -> Result<(), AgentError> {
        require_max_len(
            self.products.len(),
            MAX_CLAIM_PRODUCTS,
            "products",
            "reaction claim",
        )?;
        require_max_len(
            self.observations.len(),
            MAX_CLAIM_OBSERVATIONS,
            "observations",
            "reaction claim",
        )?;
        if self.sources.len() > MAX_CLAIM_SOURCES {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "reaction claim",
                format!("a claim may cite at most {MAX_CLAIM_SOURCES} direct sources"),
            ));
        }
        require_text(
            &self.required_context,
            "required context",
            MAX_CONTEXT_CHARS,
        )?;
        let mut text = vec![self.required_context.as_str()];
        self.validate_products(&mut text)?;
        self.validate_observations(&mut text)?;
        self.validate_sources(&mut text)?;
        self.validate_ambiguity(&mut text)?;
        reject_procedural_content(&text)
    }

    fn validate_products<'a>(&'a self, text: &mut Vec<&'a str>) -> Result<(), AgentError> {
        for product in &self.products {
            validate_claim_product(product, "product")?;
            text.push(&product.name);
            text.push(&product.formula);
            for hint in &product.identity_hints {
                text.push(&hint.value);
            }
        }
        Ok(())
    }

    fn validate_observations<'a>(&'a self, text: &mut Vec<&'a str>) -> Result<(), AgentError> {
        for observation in &self.observations {
            require_text(
                &observation.subject,
                "observation subject",
                MAX_OBSERVATION_CHARS,
            )?;
            let needs_value = observation.predicate == ClaimObservationPredicate::Colour;
            if needs_value != observation.value.is_some() {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "reaction claim",
                    "only colour observations require a value",
                ));
            }
            text.push(&observation.subject);
            if let Some(value) = &observation.value {
                require_max_chars(
                    value,
                    "observation value",
                    MAX_OBSERVATION_CHARS,
                    "reaction claim",
                )?;
                text.push(value);
            }
        }
        Ok(())
    }

    fn validate_sources<'a>(&'a self, text: &mut Vec<&'a str>) -> Result<(), AgentError> {
        let mut source_ids = std::collections::BTreeSet::new();
        for source in &self.sources {
            if !source_ids.insert(source.id.as_str())
                || source.supports.is_empty()
                || !source.url.starts_with("https://")
            {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidProviderOutput,
                    "reaction claim",
                    "sources require unique IDs, HTTPS URLs, and claim-level coverage",
                ));
            }
            require_max_len(
                source.supports.len(),
                MAX_SOURCE_SUPPORTS,
                "source supports",
                "reaction claim",
            )?;
            require_text(&source.id, "source ID", MAX_SOURCE_ID_CHARS)?;
            require_text(&source.title, "source title", MAX_SOURCE_TITLE_CHARS)?;
            require_text(
                &source.publisher,
                "source publisher",
                MAX_SOURCE_PUBLISHER_CHARS,
            )?;
            require_text(&source.url, "source URL", MAX_SOURCE_URL_CHARS)?;
            require_text(
                &source.supporting_excerpt,
                "source supporting excerpt",
                MAX_SOURCE_EXCERPT_CHARS,
            )?;
            text.extend([
                source.id.as_str(),
                source.title.as_str(),
                source.publisher.as_str(),
                source.url.as_str(),
                source.supporting_excerpt.as_str(),
            ]);
        }
        Ok(())
    }

    fn validate_ambiguity<'a>(&'a self, text: &mut Vec<&'a str>) -> Result<(), AgentError> {
        if let Some(ambiguity) = &self.ambiguity {
            require_text(&ambiguity.summary, "ambiguity summary", MAX_CONTEXT_CHARS)?;
            require_max_len(
                ambiguity.alternatives.len(),
                MAX_AMBIGUITY_ALTERNATIVES,
                "ambiguity alternatives",
                "reaction claim",
            )?;
            text.push(&ambiguity.summary);
            for alternative in &ambiguity.alternatives {
                require_text(
                    &alternative.label,
                    "ambiguity label",
                    MAX_PRODUCT_NAME_CHARS,
                )?;
                require_text(
                    &alternative.required_context,
                    "alternative context",
                    MAX_CONTEXT_CHARS,
                )?;
                require_max_len(
                    alternative.products.len(),
                    MAX_CLAIM_PRODUCTS,
                    "alternative products",
                    "reaction claim",
                )?;
                text.push(&alternative.label);
                text.push(&alternative.required_context);
                for product in &alternative.products {
                    validate_claim_product(product, "alternative product")?;
                    text.push(&product.name);
                    text.push(&product.formula);
                    text.extend(
                        product
                            .identity_hints
                            .iter()
                            .map(|hint| hint.value.as_str()),
                    );
                }
            }
        }
        Ok(())
    }
}

fn validate_claim_product(product: &ClaimProduct, label: &str) -> Result<(), AgentError> {
    require_text(
        &product.name,
        &format!("{label} name"),
        MAX_PRODUCT_NAME_CHARS,
    )?;
    require_text(
        &product.formula,
        &format!("{label} formula"),
        MAX_FORMULA_CHARS,
    )?;
    require_max_len(
        product.identity_hints.len(),
        MAX_IDENTITY_HINTS,
        &format!("{label} identity hints"),
        "reaction claim",
    )?;
    for hint in &product.identity_hints {
        require_text(
            &hint.value,
            &format!("{label} identity hint"),
            MAX_IDENTITY_HINT_CHARS,
        )?;
    }
    Ok(())
}

fn require_text(value: &str, label: &str, max_chars: usize) -> Result<(), AgentError> {
    if value.trim().is_empty() {
        Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "reaction claim",
            format!("{label} cannot be empty"),
        ))
    } else {
        require_max_chars(value, label, max_chars, "reaction claim")
    }
}

fn require_max_chars(
    value: &str,
    label: &str,
    max_chars: usize,
    context: &'static str,
) -> Result<(), AgentError> {
    if value.chars().count() > max_chars {
        Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            context,
            format!("{label} exceeds the {max_chars}-character contract limit"),
        ))
    } else {
        Ok(())
    }
}

fn require_max_len(
    len: usize,
    max: usize,
    label: &str,
    context: &'static str,
) -> Result<(), AgentError> {
    if len > max {
        Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            context,
            format!("{label} exceeds the {max}-item contract limit"),
        ))
    } else {
        Ok(())
    }
}

fn require_non_empty_len(
    len: usize,
    max: usize,
    label: &str,
    context: &'static str,
) -> Result<(), AgentError> {
    if len == 0 {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            context,
            format!("{label} must be non-empty"),
        ));
    }
    require_max_len(len, max, label, context)
}

fn reject_procedural_content(values: &[&str]) -> Result<(), AgentError> {
    const BLOCKED: [&str; 18] = [
        "apparatus",
        "procedure",
        "milliliter",
        "millilitre",
        " grams",
        "temperature",
        "heat to",
        "stir for",
        "add slowly",
        "concentration",
        "purif",
        "collect the gas",
        "reaction vessel",
        "protective equipment",
        "safety goggles",
        "wear gloves",
        "ventilation",
        "yield percent",
    ];
    if values.iter().any(|value| {
        let normalized = value.to_ascii_lowercase();
        BLOCKED.iter().any(|blocked| normalized.contains(blocked))
    }) {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "reaction claim",
            "provider output contains disallowed procedural or hazard-control content",
        ));
    }
    Ok(())
}

/// Fully labelled, locally resolved structures supplied to mechanism
/// escalation. The provider cannot add or replace any of these structures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismEscalationRequest {
    pub schema_version: u32,
    pub reaction_id: String,
    pub reactants: Vec<MechanismSpecies>,
    pub products: Vec<MechanismSpecies>,
    /// Every reactant atom path across all coefficient instances. The mapping
    /// must use each exactly once; listing them removes instance-indexing
    /// ambiguity for the provider.
    pub reactant_atom_paths: Vec<String>,
    /// Every product atom path across all coefficient instances.
    pub product_atom_paths: Vec<String>,
    /// Reviewed neutral-valence axioms used by `ChemSpec` to derive, rather
    /// than accept, any provisional operation state.
    pub neutral_valence: Vec<ElementValenceRecord>,
    /// Every reviewed covalent electron state an involved element may pass
    /// through. Operations whose before/after states leave this set fail
    /// kernel validation, so the closed vocabulary is stated up front.
    pub supported_states: Vec<ValenceStateRecord>,
    /// Every reviewed metallic site state for the involved elements.
    pub metallic_states: Vec<MetallicValenceStateRecord>,
    /// Signals that the model may propose an electron state outside the
    /// reviewed list; `ChemSpec` still derives and validates its valence record.
    pub provisional_states_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismSpecies {
    pub role: String,
    pub coefficient: u32,
    pub structure: LabelledStructure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "representation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LabelledStructure {
    Molecular {
        id: String,
        formula: String,
        atoms: Vec<AtomRecord>,
        bonds: Vec<BondRecord>,
        groups: Vec<GroupRecord>,
    },
    Ion {
        id: String,
        formula: String,
        atoms: Vec<AtomRecord>,
        bonds: Vec<BondRecord>,
        groups: Vec<GroupRecord>,
    },
    Ionic {
        id: String,
        formula: String,
        components: Vec<ComponentRecord>,
        associations: Vec<IonicAssociationRecord>,
    },
    Metallic {
        id: String,
        formula: String,
        sites: Vec<AtomRecord>,
        domains: Vec<MetallicDomainRecord>,
    },
}

/// Request for structural graphs of claimed products absent from the reviewed
/// structure library. `ChemSpec` names the species and the exact formulas;
/// the model may only fill in one graph per requested species.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalRequest {
    pub schema_version: u32,
    pub species: Vec<StructureProposalSpecies>,
    pub neutral_valence: Vec<ElementValenceRecord>,
    pub supported_states: Vec<ValenceStateRecord>,
    pub metallic_states: Vec<MetallicValenceStateRecord>,
    pub provisional_states_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalSpecies {
    pub id: String,
    pub name: String,
    pub formula: String,
}

/// The only model-authored content accepted during structure escalation. Every
/// proposed structure is untrusted until it crosses the full catalogue
/// validation boundary inside an isolated working bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalResponse {
    pub schema_version: u32,
    pub structures: Vec<LabelledStructure>,
}

impl StructureProposalResponse {
    /// Strictly decodes a structure proposal and enforces every wire-level
    /// string, collection, numeric, and document-size bound. Cross-reference,
    /// formula, charge-balance, and valence validation happens later inside an
    /// isolated working catalogue bundle.
    ///
    /// # Errors
    ///
    /// Returns an error for a wire-bound violation, oversized JSON, unknown
    /// fields or variants, or an unsupported schema version.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AgentError> {
        if bytes.len() > MAX_STRUCTURE_RESPONSE_BYTES {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure proposal",
                format!("response exceeds the {MAX_STRUCTURE_RESPONSE_BYTES}-byte contract limit"),
            ));
        }
        let response: Self = serde_json::from_slice(bytes).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "structure proposal",
                error,
            )
        })?;
        response.validate_wire()?;
        Ok(response)
    }

    pub(crate) fn validate_wire(&self) -> Result<(), AgentError> {
        require_serialized_size(
            self,
            MAX_STRUCTURE_RESPONSE_BYTES,
            "response",
            "structure proposal",
        )?;
        if self.schema_version != STRUCTURE_PROPOSAL_SCHEMA_VERSION {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "structure proposal",
                format!("unsupported structure schema {}", self.schema_version),
            ));
        }
        require_non_empty_len(
            self.structures.len(),
            16,
            "structures",
            "structure proposal",
        )?;
        for structure in &self.structures {
            validate_labelled_structure(structure)?;
        }
        Ok(())
    }
}

fn validate_labelled_structure(structure: &LabelledStructure) -> Result<(), AgentError> {
    match structure {
        LabelledStructure::Molecular {
            id,
            formula,
            atoms,
            bonds,
            groups,
        }
        | LabelledStructure::Ion {
            id,
            formula,
            atoms,
            bonds,
            groups,
        } => validate_molecular_structure(id, formula, atoms, bonds, groups),
        LabelledStructure::Ionic {
            id,
            formula,
            components,
            associations,
        } => validate_ionic_structure(id, formula, components, associations),
        LabelledStructure::Metallic {
            id,
            formula,
            sites,
            domains,
        } => validate_metallic_structure(id, formula, sites, domains),
    }
}

fn validate_ionic_structure(
    id: &str,
    formula: &str,
    components: &[ComponentRecord],
    associations: &[IonicAssociationRecord],
) -> Result<(), AgentError> {
    validate_structure_header(id, formula)?;
    require_non_empty_len(
        components.len(),
        64,
        "ionic components",
        "structure proposal",
    )?;
    require_non_empty_len(
        associations.len(),
        64,
        "ionic associations",
        "structure proposal",
    )?;
    for component in components {
        validate_identifier(&component.label, "component label")?;
        require_non_empty_len(
            component.atoms.len(),
            64,
            "component atoms",
            "structure proposal",
        )?;
        require_max_len(
            component.bonds.len(),
            128,
            "component bonds",
            "structure proposal",
        )?;
        require_max_len(
            component.groups.len(),
            32,
            "component groups",
            "structure proposal",
        )?;
        validate_atoms(&component.atoms)?;
        validate_bonds(&component.bonds)?;
        validate_groups(&component.groups)?;
    }
    for association in associations {
        validate_identifier(&association.label, "association label")?;
        require_non_empty_len(
            association.components.len(),
            64,
            "association components",
            "structure proposal",
        )?;
        for component in &association.components {
            validate_identifier(component, "association component")?;
        }
    }
    Ok(())
}

fn validate_metallic_structure(
    id: &str,
    formula: &str,
    sites: &[AtomRecord],
    domains: &[MetallicDomainRecord],
) -> Result<(), AgentError> {
    validate_structure_header(id, formula)?;
    require_non_empty_len(sites.len(), 64, "metallic sites", "structure proposal")?;
    require_non_empty_len(domains.len(), 16, "metallic domains", "structure proposal")?;
    validate_atoms(sites)?;
    for domain in domains {
        validate_identifier(&domain.label, "metallic domain label")?;
        require_non_empty_len(
            domain.sites.len(),
            64,
            "metallic domain sites",
            "structure proposal",
        )?;
        for site in &domain.sites {
            validate_identifier(site, "metallic domain site")?;
        }
        if domain.delocalized_electrons > 4_096 {
            return Err(wire_bound_error(
                "structure proposal",
                "metallic domain electrons exceed the 4096 contract limit",
            ));
        }
    }
    Ok(())
}

fn validate_molecular_structure(
    id: &str,
    formula: &str,
    atoms: &[AtomRecord],
    bonds: &[BondRecord],
    groups: &[GroupRecord],
) -> Result<(), AgentError> {
    validate_structure_header(id, formula)?;
    require_non_empty_len(atoms.len(), 128, "atoms", "structure proposal")?;
    require_max_len(bonds.len(), 256, "bonds", "structure proposal")?;
    require_max_len(groups.len(), 32, "groups", "structure proposal")?;
    validate_atoms(atoms)?;
    validate_bonds(bonds)?;
    validate_groups(groups)
}

fn validate_structure_header(id: &str, formula: &str) -> Result<(), AgentError> {
    validate_identifier(id, "structure ID")?;
    require_text_for_context(
        formula,
        "structure formula",
        MAX_FORMULA_CHARS,
        "structure proposal",
    )
}

fn validate_identifier(value: &str, label: &str) -> Result<(), AgentError> {
    require_text_for_context(
        value,
        label,
        MAX_WIRE_IDENTIFIER_CHARS,
        "structure proposal",
    )
}

fn validate_atoms(atoms: &[AtomRecord]) -> Result<(), AgentError> {
    for atom in atoms {
        validate_identifier(&atom.label, "atom label")?;
        require_text_for_context(&atom.element, "atom element", 3, "structure proposal")?;
        if !(-8..=8).contains(&atom.formal_charge) {
            return Err(wire_bound_error(
                "structure proposal",
                "atom formal charge lies outside -8..=8",
            ));
        }
        if atom.non_bonding_electrons > 64 || atom.unpaired_electrons > 16 {
            return Err(wire_bound_error(
                "structure proposal",
                "atom electron count exceeds its schema limit",
            ));
        }
    }
    Ok(())
}

fn validate_bonds(bonds: &[BondRecord]) -> Result<(), AgentError> {
    for bond in bonds {
        validate_identifier(&bond.left, "bond endpoint")?;
        validate_identifier(&bond.right, "bond endpoint")?;
    }
    Ok(())
}

fn validate_groups(groups: &[GroupRecord]) -> Result<(), AgentError> {
    for group in groups {
        validate_identifier(&group.label, "group label")?;
        require_max_len(group.atoms.len(), 64, "group atoms", "structure proposal")?;
        for atom in &group.atoms {
            validate_identifier(atom, "group atom")?;
        }
    }
    Ok(())
}

fn require_text_for_context(
    value: &str,
    label: &str,
    max_chars: usize,
    context: &'static str,
) -> Result<(), AgentError> {
    if value.is_empty() {
        return Err(wire_bound_error(
            context,
            format!("{label} cannot be empty"),
        ));
    }
    require_max_chars(value, label, max_chars, context)
}

fn wire_bound_error(context: &'static str, message: impl Into<String>) -> AgentError {
    AgentError::new(AgentErrorKind::InvalidProviderOutput, context, message)
}

fn require_serialized_size<T: Serialize>(
    value: &T,
    max_bytes: usize,
    label: &str,
    context: &'static str,
) -> Result<(), AgentError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidProviderOutput, context, error)
    })?;
    if bytes.len() > max_bytes {
        return Err(wire_bound_error(
            context,
            format!("{label} exceeds the {max_bytes}-byte contract limit"),
        ));
    }
    Ok(())
}

/// The only model-authored content accepted during mechanism escalation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismEscalationResponse {
    pub schema_version: u32,
    pub mapping: Vec<MechanismMapping>,
    pub operations: Vec<MechanismOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismMapping {
    pub reactant: String,
    pub product: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MechanismOperation {
    ReconfigureElectrons {
        atom: String,
        before: ElectronStateRecord,
        after: ElectronStateRecord,
    },
    CleaveCovalent {
        edge: (String, String, BondOrderRecord),
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormCovalent {
        edge: (String, String, BondOrderRecord),
        electron_contribution: ElectronContributionRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    CleaveDative {
        donor: String,
        acceptor: String,
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormDative {
        donor: String,
        acceptor: String,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalent {
        edge: (String, String),
        old_order: BondOrderRecord,
        new_order: BondOrderRecord,
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalentDelocalization {
        edge: (String, String),
        expected: Option<BondDelocalizationRecord>,
        replacement: Option<BondDelocalizationRecord>,
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
pub enum MechanismCleavageAllocation {
    Homolytic(MechanismHomolytic),
    Heterolytic { heterolytic_to: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanismHomolytic {
    Homolytic,
}

impl MechanismEscalationResponse {
    /// Strictly decodes a mechanism response and enforces every wire-level
    /// label, collection, numeric, and document-size bound. Label binding to
    /// the exact request and chemical transition validation happen later.
    ///
    /// # Errors
    ///
    /// Returns an error for a wire-bound violation, oversized JSON, unknown
    /// fields or variants, or an unsupported schema version.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AgentError> {
        if bytes.len() > MAX_MECHANISM_RESPONSE_BYTES {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "mechanism response",
                format!("response exceeds the {MAX_MECHANISM_RESPONSE_BYTES}-byte contract limit"),
            ));
        }
        let response: Self = serde_json::from_slice(bytes).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidProviderOutput,
                "mechanism response",
                error,
            )
        })?;
        response.validate_wire()?;
        Ok(response)
    }

    pub(crate) fn validate_wire(&self) -> Result<(), AgentError> {
        require_serialized_size(
            self,
            MAX_MECHANISM_RESPONSE_BYTES,
            "response",
            "mechanism response",
        )?;
        if self.schema_version != MECHANISM_ESCALATION_SCHEMA_VERSION {
            return Err(AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "mechanism response",
                format!("unsupported mechanism schema {}", self.schema_version),
            ));
        }
        require_non_empty_len(self.mapping.len(), 512, "mapping", "mechanism response")?;
        require_non_empty_len(
            self.operations.len(),
            512,
            "operations",
            "mechanism response",
        )?;
        for mapping in &self.mapping {
            validate_mechanism_label(&mapping.reactant, "reactant mapping label")?;
            validate_mechanism_label(&mapping.product, "product mapping label")?;
        }
        for operation in &self.operations {
            validate_mechanism_operation(operation)?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn validate_mechanism_operation(operation: &MechanismOperation) -> Result<(), AgentError> {
    match operation {
        MechanismOperation::ReconfigureElectrons {
            atom,
            before,
            after,
        } => {
            validate_mechanism_label(atom, "atom label")?;
            validate_electron_state(*before)?;
            validate_electron_state(*after)
        }
        MechanismOperation::CleaveCovalent {
            edge,
            allocation,
            before,
            after,
        } => {
            validate_mechanism_label(&edge.0, "edge label")?;
            validate_mechanism_label(&edge.1, "edge label")?;
            validate_cleavage_allocation(allocation)?;
            validate_binary_state(before)?;
            validate_binary_state(after)
        }
        MechanismOperation::FormCovalent {
            edge,
            electron_contribution,
            before,
            after,
        } => {
            validate_mechanism_label(&edge.0, "edge label")?;
            validate_mechanism_label(&edge.1, "edge label")?;
            if electron_contribution.left > 2 || electron_contribution.right > 2 {
                return Err(wire_bound_error(
                    "mechanism response",
                    "covalent electron contribution exceeds 2",
                ));
            }
            validate_binary_state(before)?;
            validate_binary_state(after)
        }
        MechanismOperation::CleaveDative {
            donor,
            acceptor,
            allocation,
            before,
            after,
        } => {
            validate_mechanism_label(donor, "dative donor")?;
            validate_mechanism_label(acceptor, "dative acceptor")?;
            validate_cleavage_allocation(allocation)?;
            validate_binary_state(before)?;
            validate_binary_state(after)
        }
        MechanismOperation::FormDative {
            donor,
            acceptor,
            before,
            after,
        } => {
            validate_mechanism_label(donor, "dative donor")?;
            validate_mechanism_label(acceptor, "dative acceptor")?;
            validate_binary_state(before)?;
            validate_binary_state(after)
        }
        MechanismOperation::ChangeCovalent {
            edge,
            allocation,
            before,
            after,
            ..
        } => {
            validate_mechanism_label(&edge.0, "edge label")?;
            validate_mechanism_label(&edge.1, "edge label")?;
            validate_cleavage_allocation(allocation)?;
            validate_binary_state(before)?;
            validate_binary_state(after)
        }
        MechanismOperation::ChangeCovalentDelocalization {
            edge,
            expected,
            replacement,
        } => {
            validate_mechanism_label(&edge.0, "edge label")?;
            validate_mechanism_label(&edge.1, "edge label")?;
            if let Some(delocalization) = expected {
                validate_delocalization(delocalization)?;
            }
            if let Some(delocalization) = replacement {
                validate_delocalization(delocalization)?;
            }
            Ok(())
        }
        MechanismOperation::AssociateIonic {
            label,
            components,
            component_charges,
        } => {
            validate_mechanism_label(label, "ionic association label")?;
            if components.is_empty() || components.iter().any(Vec::is_empty) {
                return Err(wire_bound_error(
                    "mechanism response",
                    "ionic components and their atom lists must be non-empty",
                ));
            }
            for component in components {
                for atom in component {
                    validate_mechanism_label(atom, "ionic component atom")?;
                }
            }
            if component_charges.is_empty()
                || component_charges
                    .iter()
                    .any(|charge| !(-32..=32).contains(charge))
            {
                return Err(wire_bound_error(
                    "mechanism response",
                    "ionic component charges must be non-empty and lie within -32..=32",
                ));
            }
            Ok(())
        }
        MechanismOperation::DissociateIonic { association } => {
            validate_mechanism_label(association, "ionic association label")
        }
        MechanismOperation::ReleaseMetallic {
            site,
            domain,
            before,
            after,
            ..
        }
        | MechanismOperation::JoinMetallic {
            site,
            domain,
            before,
            after,
            ..
        } => {
            validate_mechanism_label(site, "metallic site")?;
            validate_mechanism_label(domain, "metallic domain")?;
            validate_metallic_state(before)?;
            validate_metallic_state(after)
        }
        MechanismOperation::TransferElectron {
            count,
            donor,
            acceptor,
            before,
            after,
        } => {
            if !(1..=8).contains(count) {
                return Err(wire_bound_error(
                    "mechanism response",
                    "electron transfer count lies outside 1..=8",
                ));
            }
            validate_mechanism_label(donor, "electron donor")?;
            validate_mechanism_label(acceptor, "electron acceptor")?;
            validate_transfer_state(before)?;
            validate_transfer_state(after)
        }
        MechanismOperation::AssignProduct { atoms, product } => {
            if atoms.is_empty() {
                return Err(wire_bound_error(
                    "mechanism response",
                    "assigned product atoms must be non-empty",
                ));
            }
            for atom in atoms {
                validate_mechanism_label(atom, "assigned product atom")?;
            }
            validate_mechanism_label(product, "assigned product label")
        }
    }
}

fn validate_mechanism_label(value: &str, label: &str) -> Result<(), AgentError> {
    require_text_for_context(
        value,
        label,
        MAX_MECHANISM_LABEL_CHARS,
        "mechanism response",
    )
}

fn validate_cleavage_allocation(
    allocation: &MechanismCleavageAllocation,
) -> Result<(), AgentError> {
    if let MechanismCleavageAllocation::Heterolytic { heterolytic_to } = allocation {
        validate_mechanism_label(heterolytic_to, "heterolytic allocation label")?;
    }
    Ok(())
}

fn validate_electron_state(state: ElectronStateRecord) -> Result<(), AgentError> {
    if !(-64..=64).contains(&state.0) || state.1 > 64 || state.2 > 64 {
        Err(wire_bound_error(
            "mechanism response",
            "electron state lies outside -64..=64",
        ))
    } else {
        Ok(())
    }
}

fn validate_binary_state(state: &BinaryElectronStateRecord) -> Result<(), AgentError> {
    validate_electron_state(state.left)?;
    validate_electron_state(state.right)
}

fn validate_transfer_state(state: &TransferElectronStateRecord) -> Result<(), AgentError> {
    validate_electron_state(state.donor)?;
    validate_electron_state(state.acceptor)
}

fn validate_metallic_state(state: &MetallicElectronStateRecord) -> Result<(), AgentError> {
    validate_electron_state(state.site)
}

fn validate_delocalization(delocalization: &BondDelocalizationRecord) -> Result<(), AgentError> {
    validate_mechanism_label(&delocalization.domain, "delocalization domain")?;
    if delocalization.effective_order.numerator == 0
        || delocalization.effective_order.denominator == 0
    {
        return Err(wire_bound_error(
            "mechanism response",
            "effective bond order numerator and denominator must be non-zero",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rubidium_claim() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {
                    "name": "rubidium hydroxide",
                    "formula": "RbOH",
                    "phase": "aqueous",
                    "identity_hints": []
                },
                {
                    "name": "hydrogen",
                    "formula": "H2",
                    "phase": "gas",
                    "identity_hints": []
                }
            ],
            "required_context": "Representative reaction with liquid water",
            "observations": [{
                "predicate": "evolves",
                "subject": "hydrogen",
                "value": null
            }],
            "sources": [],
            "ambiguity": null
        })
    }

    #[test]
    fn rubidium_water_claim_has_no_structural_ledger() {
        let bytes = serde_json::to_vec(&rubidium_claim()).expect("claim JSON");
        let claim = ProviderClaim::from_json(&bytes, ClaimMode::Fast).expect("valid claim");
        assert_eq!(claim.products.len(), 2);
        let text = String::from_utf8(bytes).expect("UTF-8 JSON");
        for forbidden in [
            "coefficients",
            "structures",
            "valence",
            "mapping",
            "operations",
            "catalogue",
            ".chems",
        ] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn claim_contract_rejects_unknown_missing_and_procedural_content() {
        let mut unknown = rubidium_claim();
        unknown["procedure"] = json!("none");
        assert!(
            ProviderClaim::from_json(
                &serde_json::to_vec(&unknown).expect("unknown JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );

        let mut missing = rubidium_claim();
        missing.as_object_mut().expect("object").remove("products");
        assert!(
            ProviderClaim::from_json(
                &serde_json::to_vec(&missing).expect("missing JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );

        let mut procedural = rubidium_claim();
        procedural["required_context"] = json!("Heat to 80 C and stir for ten minutes");
        let error = ProviderClaim::from_json(
            &serde_json::to_vec(&procedural).expect("procedural JSON"),
            ClaimMode::Fast,
        )
        .expect_err("procedural content must fail closed");
        assert!(error.to_string().contains("procedural"));

        let mut empty_context = rubidium_claim();
        empty_context["required_context"] = json!("");
        assert!(
            ProviderClaim::from_json(
                &serde_json::to_vec(&empty_context).expect("empty context JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );
    }

    #[test]
    fn claim_text_limit_uses_json_schema_character_count() {
        let mut at_limit = rubidium_claim();
        at_limit["products"][0]["name"] = json!("é".repeat(300));
        ProviderClaim::from_json(
            &serde_json::to_vec(&at_limit).expect("bounded claim JSON"),
            ClaimMode::Fast,
        )
        .expect("300 Unicode characters are within the schema limit");

        let mut over_limit = rubidium_claim();
        over_limit["products"][0]["name"] = json!("é".repeat(301));
        let error = ProviderClaim::from_json(
            &serde_json::to_vec(&over_limit).expect("oversized claim JSON"),
            ClaimMode::Fast,
        )
        .expect_err("301 Unicode characters exceed the schema limit");
        assert!(error.to_string().contains("product name"));
    }

    fn assert_invalid_claim(value: &serde_json::Value, expected: &str) {
        let error = ProviderClaim::from_json(
            &serde_json::to_vec(value).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect_err("claim must exceed a wire bound");
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn claim_collection_and_nested_text_limits_match_schema_boundaries() {
        let product = rubidium_claim()["products"][0].clone();

        let mut products_at_limit = rubidium_claim();
        products_at_limit["products"] = json!(vec![product.clone(); 16]);
        ProviderClaim::from_json(
            &serde_json::to_vec(&products_at_limit).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("16 products");
        products_at_limit["products"] = json!(vec![product.clone(); 17]);
        assert_invalid_claim(&products_at_limit, "products");

        let mut context = rubidium_claim();
        context["required_context"] = json!("x".repeat(1_000));
        ProviderClaim::from_json(
            &serde_json::to_vec(&context).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("1,000-character context");
        context["required_context"] = json!("x".repeat(1_001));
        assert_invalid_claim(&context, "required context");

        let mut formula = rubidium_claim();
        formula["products"][0]["formula"] = json!("X".repeat(200));
        ProviderClaim::from_json(
            &serde_json::to_vec(&formula).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("200-character formula");
        formula["products"][0]["formula"] = json!("X".repeat(201));
        assert_invalid_claim(&formula, "product formula");

        let hint = json!({"kind":"registry_id", "value":"x".repeat(500)});
        let mut hints = rubidium_claim();
        hints["products"][0]["identity_hints"] = json!(vec![hint.clone(); 12]);
        ProviderClaim::from_json(
            &serde_json::to_vec(&hints).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("12 bounded identity hints");
        hints["products"][0]["identity_hints"] = json!(vec![hint.clone(); 13]);
        assert_invalid_claim(&hints, "identity hints");
        hints["products"][0]["identity_hints"] =
            json!([{"kind":"registry_id", "value":"x".repeat(501)}]);
        assert_invalid_claim(&hints, "identity hint");

        let observation = json!({"predicate":"evolves", "subject":"x".repeat(300), "value":null});
        let mut observations = rubidium_claim();
        observations["observations"] = json!(vec![observation; 16]);
        ProviderClaim::from_json(
            &serde_json::to_vec(&observations).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("16 bounded observations");
        observations["observations"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "predicate":"evolves", "subject":"x", "value":null
            }));
        assert_invalid_claim(&observations, "observations");
        let mut colour = rubidium_claim();
        colour["observations"] = json!([{
            "predicate":"colour", "subject":"solution", "value":"x".repeat(301)
        }]);
        assert_invalid_claim(&colour, "observation value");

        let source = |id: usize| {
            json!({
                "id": format!("source-{id}"),
                "title": "x".repeat(500),
                "publisher": "x".repeat(300),
                "url": format!("https://{}", "x".repeat(1_992)),
                "supporting_excerpt": "x".repeat(1_200),
                "supports": ["products", "required_context", "observations", "no_reaction"]
            })
        };
        let mut sources = rubidium_claim();
        sources["sources"] = json!((0..4).map(source).collect::<Vec<_>>());
        ProviderClaim::from_json(
            &serde_json::to_vec(&sources).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("four sources at every nested boundary");
        sources["sources"][0]["id"] = json!("x".repeat(41));
        assert_invalid_claim(&sources, "source ID");
        sources["sources"][0] = source(0);
        sources["sources"][0]["url"] = json!(format!("https://{}", "x".repeat(1_993)));
        assert_invalid_claim(&sources, "source URL");
        sources["sources"][0] = source(0);
        sources["sources"][0]["supporting_excerpt"] = json!("x".repeat(1_201));
        assert_invalid_claim(&sources, "supporting excerpt");
    }

    #[test]
    fn ambiguity_limits_apply_to_alternatives_and_their_nested_products() {
        let alternative = json!({
            "label":"x".repeat(300),
            "products":[],
            "required_context":"x".repeat(1_000)
        });
        let mut claim = json!({
            "schema_version":1, "disposition":"ambiguous", "products":[],
            "required_context":"context", "observations":[], "sources":[],
            "ambiguity":{
                "kind":"conditions", "summary":"x".repeat(1_000),
                "alternatives":vec![alternative.clone(); 8]
            }
        });
        ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("eight bounded alternatives");
        claim["ambiguity"]["alternatives"] = json!(vec![alternative.clone(); 9]);
        assert_invalid_claim(&claim, "ambiguity alternatives");
        claim["ambiguity"]["alternatives"] = json!(vec![alternative.clone(); 2]);
        claim["ambiguity"]["alternatives"][0]["label"] = json!("x".repeat(301));
        assert_invalid_claim(&claim, "ambiguity label");
        claim["ambiguity"]["alternatives"][0] = alternative;
        claim["ambiguity"]["alternatives"][0]["products"] =
            json!(vec![rubidium_claim()["products"][0].clone(); 17]);
        assert_invalid_claim(&claim, "alternative products");
    }

    fn molecular_structure() -> serde_json::Value {
        json!({
            "representation":"molecular", "id":"s1", "formula":"H2",
            "atoms":[
                {"label":"h1", "element":"H", "formal_charge":0,
                 "non_bonding_electrons":0, "unpaired_electrons":0}
            ],
            "bonds":[], "groups":[]
        })
    }

    fn assert_invalid_structure(
        structures: impl IntoIterator<Item = serde_json::Value>,
        expected: &str,
    ) {
        let structures = structures.into_iter().collect::<Vec<_>>();
        let value = json!({"schema_version":1, "structures":structures});
        let error = StructureProposalResponse::from_json(
            &serde_json::to_vec(&value).expect("structure JSON"),
        )
        .expect_err("structure must exceed a wire bound");
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn structure_scalar_and_collection_limits_match_schema_boundaries() {
        let mut bounded = molecular_structure();
        bounded["id"] = json!("é".repeat(120));
        bounded["formula"] = json!("X".repeat(200));
        bounded["atoms"][0]["label"] = json!("x".repeat(120));
        bounded["atoms"][0]["element"] = json!("XxX");
        bounded["atoms"][0]["formal_charge"] = json!(8);
        bounded["atoms"][0]["non_bonding_electrons"] = json!(64);
        bounded["atoms"][0]["unpaired_electrons"] = json!(16);
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({"schema_version":1, "structures":[bounded.clone()]}))
                .expect("structure JSON"),
        )
        .expect("structure at scalar bounds");

        let mut over = bounded.clone();
        over["id"] = json!("x".repeat(121));
        assert_invalid_structure(vec![over], "structure ID");
        let mut over = bounded.clone();
        over["formula"] = json!("X".repeat(201));
        assert_invalid_structure(vec![over], "structure formula");
        let mut over = bounded.clone();
        over["atoms"][0]["formal_charge"] = json!(9);
        assert_invalid_structure(vec![over], "formal charge");
        let mut over = bounded.clone();
        over["atoms"][0]["non_bonding_electrons"] = json!(65);
        assert_invalid_structure(vec![over], "electron count");

        let atom = molecular_structure()["atoms"][0].clone();
        let mut atoms = molecular_structure();
        atoms["atoms"] = json!(vec![atom.clone(); 128]);
        atoms["bonds"] = json!(vec![
            json!({"left":"h1", "right":"h1", "order":"single"});
            256
        ]);
        atoms["groups"] = json!(vec![json!({"label":"g", "atoms":vec!["h1"; 64]}); 32]);
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({"schema_version":1, "structures":[atoms.clone()]}))
                .expect("structure JSON"),
        )
        .expect("molecular structure at collection bounds");
        atoms["atoms"] = json!(vec![atom; 129]);
        assert_invalid_structure(vec![atoms], "atoms");

        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({
                "schema_version":1, "structures":vec![molecular_structure(); 16]
            }))
            .expect("structure JSON"),
        )
        .expect("16 structures");
        assert_invalid_structure(vec![molecular_structure(); 17], "structures");
    }

    #[test]
    fn ionic_and_metallic_nested_limits_match_schema_boundaries() {
        let atom = molecular_structure()["atoms"][0].clone();
        let component = json!({"label":"c", "atoms":[atom.clone()], "bonds":[], "groups":[]});
        let association = json!({"label":"a", "components":["c"]});
        let ionic = json!({
            "representation":"ionic", "id":"i", "formula":"HX",
            "components":vec![component.clone(); 64],
            "associations":vec![association.clone(); 64]
        });
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({"schema_version":1, "structures":[ionic]}))
                .expect("ionic JSON"),
        )
        .expect("ionic structure at collection bounds");

        let metallic = json!({
            "representation":"metallic", "id":"m", "formula":"M",
            "sites":vec![atom; 64],
            "domains":vec![json!({
                "label":"d", "sites":vec!["h1"; 64], "delocalized_electrons":4096
            }); 16]
        });
        StructureProposalResponse::from_json(
            &serde_json::to_vec(&json!({"schema_version":1, "structures":[metallic.clone()]}))
                .expect("metallic JSON"),
        )
        .expect("metallic structure at collection and scalar bounds");
        let mut over = metallic;
        over["domains"][0]["delocalized_electrons"] = json!(4097);
        assert_invalid_structure(vec![over], "domain electrons");

        let mut over_component = component;
        over_component["atoms"] = json!(vec![molecular_structure()["atoms"][0].clone(); 65]);
        assert_invalid_structure(
            vec![json!({
                "representation":"ionic", "id":"i", "formula":"HX",
                "components":[over_component], "associations":[association]
            })],
            "component atoms",
        );
    }

    #[test]
    fn closed_dispositions_represent_no_reaction_condition_and_conflict() {
        let no_reaction = json!({
            "schema_version": 1,
            "disposition": "no_reaction",
            "products": [],
            "required_context": "Ordinary contact",
            "observations": [],
            "sources": [],
            "ambiguity": null
        });
        ProviderClaim::from_json(
            &serde_json::to_vec(&no_reaction).expect("no reaction"),
            ClaimMode::Fast,
        )
        .expect("no-reaction claim");

        let ambiguous = json!({
            "schema_version": 1,
            "disposition": "ambiguous",
            "products": [],
            "required_context": "Outcome depends on unspecified conditions",
            "observations": [],
            "sources": [],
            "ambiguity": {
                "kind": "conflicting_evidence",
                "summary": "Direct sources disagree",
                "alternatives": [
                    {"label":"outcome A","products":[],"required_context":"condition A"},
                    {"label":"outcome B","products":[],"required_context":"condition B"}
                ]
            }
        });
        ProviderClaim::from_json(
            &serde_json::to_vec(&ambiguous).expect("ambiguous"),
            ClaimMode::Fast,
        )
        .expect("conflicting claim");
    }

    #[test]
    fn mechanism_contract_rejects_extra_output_authority() {
        let valid = json!({
            "schema_version": 1,
            "mapping": [{"reactant":"hydrogen[1].h1","product":"hydrogen[1].h1"}],
            "operations": [{
                "kind":"assign_product",
                "atoms":["hydrogen[1].h1"],
                "product":"hydrogen[1]"
            }]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&valid).expect("valid mechanism"),
        )
        .expect("closed mechanism");

        let mut invalid = valid;
        invalid["structures"] = json!([]);
        assert!(
            MechanismEscalationResponse::from_json(
                &serde_json::to_vec(&invalid).expect("invalid mechanism")
            )
            .is_err()
        );
    }

    fn mechanism_value(operation: serde_json::Value) -> serde_json::Value {
        let operations = vec![operation];
        json!({
            "schema_version":1,
            "mapping":[{"reactant":"r", "product":"p"}],
            "operations":operations
        })
    }

    fn assert_invalid_mechanism(value: &serde_json::Value, expected: &str) {
        let error = MechanismEscalationResponse::from_json(
            &serde_json::to_vec(value).expect("mechanism JSON"),
        )
        .expect_err("mechanism must exceed a wire bound");
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }

    #[test]
    fn mechanism_label_and_top_level_collection_limits_match_schema_boundaries() {
        let assignment = json!({"kind":"assign_product", "atoms":["a"], "product":"p"});
        let bounded_label = mechanism_value(assignment.clone());
        let mut bounded_label = bounded_label;
        bounded_label["mapping"][0] =
            json!({"reactant":"é".repeat(160), "product":"x".repeat(160)});
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&bounded_label).expect("mechanism JSON"),
        )
        .expect("160-character mechanism labels");

        let mapping = json!({"reactant":"r", "product":"p"});
        let value = json!({
            "schema_version":1,
            "mapping":vec![mapping.clone(); 512],
            "operations":vec![assignment.clone(); 512]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&value).expect("mechanism JSON"),
        )
        .expect("mechanism at label and collection bounds");

        let mut over_mapping = value.clone();
        over_mapping["mapping"] = json!(vec![mapping; 513]);
        assert_invalid_mechanism(&over_mapping, "mapping");
        let mut over_operations = value.clone();
        over_operations["operations"] = json!(vec![assignment; 513]);
        assert_invalid_mechanism(&over_operations, "operations");
        let mut over_label = bounded_label;
        over_label["mapping"][0]["reactant"] = json!("x".repeat(161));
        assert_invalid_mechanism(&over_label, "reactant mapping label");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn mechanism_nested_collection_and_numeric_limits_match_schema_boundaries() {
        let reconfigure = json!({
            "kind":"reconfigure_electrons", "atom":"a",
            "before":[-64, 64, 64], "after":[64, 64, 64]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&mechanism_value(reconfigure.clone())).expect("mechanism JSON"),
        )
        .expect("electron state at bounds");
        let mut invalid = mechanism_value(reconfigure);
        invalid["operations"][0]["after"] = json!([65, 0, 0]);
        assert_invalid_mechanism(&invalid, "electron state");

        let form = json!({
            "kind":"form_covalent", "edge":["a", "b", "single"],
            "electron_contribution":{"left":2, "right":2},
            "before":{"left":[0,0,0], "right":[0,0,0]},
            "after":{"left":[0,0,0], "right":[0,0,0]}
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&mechanism_value(form.clone())).expect("mechanism JSON"),
        )
        .expect("electron contribution at bound");
        let mut invalid = mechanism_value(form);
        invalid["operations"][0]["electron_contribution"]["left"] = json!(3);
        assert_invalid_mechanism(&invalid, "electron contribution");

        let association = json!({
            "kind":"associate_ionic", "label":"salt",
            "components":[["a"]], "component_charges":[-32, 32]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&mechanism_value(association.clone())).expect("mechanism JSON"),
        )
        .expect("ionic component charges at bounds");
        let mut invalid = mechanism_value(association.clone());
        invalid["operations"][0]["component_charges"] = json!([33]);
        assert_invalid_mechanism(&invalid, "component charges");
        let mut invalid = mechanism_value(association);
        invalid["operations"][0]["components"] = json!([]);
        assert_invalid_mechanism(&invalid, "ionic components");

        let transfer = json!({
            "kind":"transfer_electron", "count":8, "donor":"a", "acceptor":"b",
            "before":{"donor":[0,0,0], "acceptor":[0,0,0]},
            "after":{"donor":[0,0,0], "acceptor":[0,0,0]}
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&mechanism_value(transfer.clone())).expect("mechanism JSON"),
        )
        .expect("transfer count at bound");
        let mut invalid = mechanism_value(transfer);
        invalid["operations"][0]["count"] = json!(0);
        assert_invalid_mechanism(&invalid, "transfer count");

        let delocalization = json!({
            "kind":"change_covalent_delocalization", "edge":["a", "b"],
            "expected":null,
            "replacement":{"domain":"d", "effective_order":{"numerator":1, "denominator":255}}
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&mechanism_value(delocalization.clone())).expect("mechanism JSON"),
        )
        .expect("effective order at bounds");
        let mut invalid = mechanism_value(delocalization);
        invalid["operations"][0]["replacement"]["effective_order"]["numerator"] = json!(0);
        assert_invalid_mechanism(&invalid, "effective bond order");

        let empty_assignment = mechanism_value(json!({
            "kind":"assign_product", "atoms":[], "product":"p"
        }));
        assert_invalid_mechanism(&empty_assignment, "assigned product atoms");
    }

    #[test]
    fn provider_claim_rejects_solver_only_no_reaction_reason() {
        let no_reaction = json!({
            "schema_version": 1,
            "disposition": "no_reaction",
            "products": [],
            "required_context": "standard conditions",
            "observations": [], "sources": [], "ambiguity": null,
            "no_reaction_reason": {"below_hydrogen": {"metal": "copper"}}
        });
        let error = ProviderClaim::from_json(
            &serde_json::to_vec(&no_reaction).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect_err("a provider cannot author solver-only explanation copy");
        assert!(error.to_string().contains("no_reaction_reason"));
    }

    #[test]
    fn same_halogen_copy_explains_nothing_to_displace() {
        let same = NoReactionReason::LessReactiveHalogen {
            incoming: "chlorine".to_owned(),
            resident: "chlorine".to_owned(),
        };
        assert!(same.learner_explanation().contains("already contains"));
        let weaker = NoReactionReason::LessReactiveHalogen {
            incoming: "bromine".to_owned(),
            resident: "chlorine".to_owned(),
        };
        assert!(
            weaker
                .learner_explanation()
                .starts_with("Bromine is less reactive")
        );
    }
}
