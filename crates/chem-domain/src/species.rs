use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    Charge, ContentDigest, ElementRegistry, NormalizedFormula, Phase, SpeciesId,
    StructureDefinition,
    material::{ResolvedSpecies, ResolvedSpeciesInput},
};

pub const IDENTITY_CACHE_SCHEMA_VERSION: u32 = 1;

/// User- or provider-supplied identity search terms. Names and formulae are
/// lookup keys only; neither becomes a species identity by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesQuery {
    pub name: Option<String>,
    pub formula: Option<String>,
    pub charge: Option<Charge>,
    pub phase: Option<Phase>,
    pub external_identifier: Option<ExternalIdentifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ExternalIdentifier {
    Inchi(String),
    InchiKey(String),
    CanonicalSmiles(String),
    IsomericSmiles(String),
    PubChemCid(String),
    RegistryId(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StereochemistryPolicy {
    NotApplicable,
    Unspecified,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TautomerPolicy {
    NotApplicable,
    ExactTautomer,
    CanonicalTautomer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtonationPolicy {
    ExactChargeState,
    ContextDependent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityConfidence {
    Reviewed,
    Corroborated,
    ExternalUnverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IdentityProvenance {
    HostPinned {
        catalogue_digest: ContentDigest,
    },
    DeviceCache {
        snapshot_digest: ContentDigest,
    },
    External {
        resolver: String,
        source_url: String,
        retrieved_at_unix_ms: u64,
        content_digest: ContentDigest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalSpeciesSerialization {
    pub media_type: String,
    pub value: String,
    pub digest: ContentDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesAmbiguity {
    pub query: SpeciesQuery,
    pub alternatives: Vec<SpeciesId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesResolution<'a> {
    Resolved(&'a ResolvedSpecies),
    Ambiguous(SpeciesAmbiguity),
    NotFound,
}

/// Deterministic host-pinned and device-cached identity index. External
/// services remain adapters outside this pure domain type.
#[derive(Debug, Clone, Default)]
pub struct SpeciesRegistry {
    records: BTreeMap<SpeciesId, ResolvedSpecies>,
    aliases: BTreeMap<String, BTreeSet<SpeciesId>>,
    formulae: BTreeMap<String, BTreeSet<SpeciesId>>,
    external: BTreeMap<ExternalIdentifier, BTreeSet<SpeciesId>>,
}

impl SpeciesRegistry {
    /// Adds one already-validated identity and indexes it deterministically.
    ///
    /// # Errors
    ///
    /// Rejects duplicate stable IDs or records with no searchable name or
    /// formula.
    pub fn insert(&mut self, species: ResolvedSpecies) -> Result<(), SpeciesIdentityError> {
        if self.records.contains_key(&species.id) {
            return Err(SpeciesIdentityError::DuplicateSpecies(species.id));
        }
        let mut names = species.normalized_aliases.clone();
        names.insert(normalize_name(&species.display_name));
        if names.iter().any(String::is_empty) || species.formula_text.trim().is_empty() {
            return Err(SpeciesIdentityError::EmptySearchKey);
        }
        for name in names {
            self.aliases
                .entry(name)
                .or_default()
                .insert(species.id.clone());
        }
        self.formulae
            .entry(normalize_formula(&species.formula_text))
            .or_default()
            .insert(species.id.clone());
        for identifier in &species.external_identifiers {
            self.external
                .entry(identifier.clone())
                .or_default()
                .insert(species.id.clone());
        }
        self.records.insert(species.id.clone(), species);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &SpeciesId) -> Option<&ResolvedSpecies> {
        self.records.get(id)
    }

    #[must_use]
    pub fn resolve(&self, query: &SpeciesQuery) -> SpeciesResolution<'_> {
        let mut candidates: Option<BTreeSet<SpeciesId>> = None;
        if let Some(identifier) = &query.external_identifier {
            intersect_candidates(&mut candidates, self.external.get(identifier));
        }
        if let Some(name) = &query.name {
            intersect_candidates(&mut candidates, self.aliases.get(&normalize_name(name)));
        }
        if let Some(formula) = &query.formula {
            intersect_candidates(
                &mut candidates,
                self.formulae.get(&normalize_formula(formula)),
            );
        }
        let mut candidates = candidates.unwrap_or_default();
        candidates.retain(|id| {
            let species = &self.records[id];
            query
                .charge
                .as_ref()
                .is_none_or(|charge| charge == &species.charge)
                && query.phase.is_none_or(|phase| phase == species.phase)
        });
        match candidates.len() {
            0 => SpeciesResolution::NotFound,
            1 => {
                let Some(id) = candidates.first() else {
                    return SpeciesResolution::NotFound;
                };
                let Some(species) = self.records.get(id) else {
                    return SpeciesResolution::NotFound;
                };
                SpeciesResolution::Resolved(species)
            }
            _ => SpeciesResolution::Ambiguous(SpeciesAmbiguity {
                query: query.clone(),
                alternatives: candidates.into_iter().collect(),
            }),
        }
    }

    #[must_use]
    pub const fn records(&self) -> &BTreeMap<SpeciesId, ResolvedSpecies> {
        &self.records
    }

    /// Returns a deterministic digest of every checked identity record.
    ///
    /// # Errors
    ///
    /// Returns an error only if canonical serialization of the checked records
    /// fails.
    pub fn snapshot_digest(&self) -> Result<ContentDigest, SpeciesIdentityError> {
        let value = serde_json::to_value(&self.records)
            .map_err(|error| SpeciesIdentityError::InvalidCache(error.to_string()))?;
        let bytes = crate::canonical_json(&value)
            .map_err(|error| SpeciesIdentityError::InvalidCache(error.to_string()))?;
        Ok(ContentDigest::sha256(&bytes))
    }
}

fn intersect_candidates(
    current: &mut Option<BTreeSet<SpeciesId>>,
    next: Option<&BTreeSet<SpeciesId>>,
) {
    let next = next.cloned().unwrap_or_default();
    if let Some(current) = current {
        current.retain(|id| next.contains(id));
    } else {
        *current = Some(next);
    }
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_formula(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Strict cache record. Canonical structure bytes remain untrusted and must be
/// reconstructed by a chemistry adapter before producing `ResolvedSpecies`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CachedIdentityRecord {
    pub species_id: SpeciesId,
    pub display_name: String,
    pub normalized_aliases: BTreeSet<String>,
    pub formula: String,
    pub charge: Charge,
    pub phase: Phase,
    pub canonical_structure_json: Option<String>,
    pub structure_digest: Option<ContentDigest>,
    pub external_identifiers: BTreeSet<ExternalIdentifier>,
    pub stereochemistry_policy: StereochemistryPolicy,
    pub tautomer_policy: TautomerPolicy,
    pub protonation_policy: ProtonationPolicy,
    pub provenance: Vec<IdentityProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCacheEnvelope {
    pub schema_version: u32,
    pub records: Vec<CachedIdentityRecord>,
}

impl IdentityCacheEnvelope {
    /// Constructs a deterministic, duplicate-free identity cache envelope.
    ///
    /// # Errors
    ///
    /// Rejects duplicate stable identities.
    pub fn new(mut records: Vec<CachedIdentityRecord>) -> Result<Self, SpeciesIdentityError> {
        records.sort_by(|left, right| left.species_id.cmp(&right.species_id));
        if records
            .windows(2)
            .any(|pair| pair[0].species_id == pair[1].species_id)
        {
            return Err(SpeciesIdentityError::DuplicateCacheIdentity);
        }
        Ok(Self {
            schema_version: IDENTITY_CACHE_SCHEMA_VERSION,
            records,
        })
    }

    /// Decodes a strict, sorted cache snapshot.
    ///
    /// # Errors
    ///
    /// Rejects schema drift, unknown fields, duplicates, or nondeterministic
    /// record ordering.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SpeciesIdentityError> {
        let envelope: Self = serde_json::from_slice(bytes)
            .map_err(|error| SpeciesIdentityError::InvalidCache(error.to_string()))?;
        if envelope.schema_version != IDENTITY_CACHE_SCHEMA_VERSION {
            return Err(SpeciesIdentityError::UnsupportedCacheSchema(
                envelope.schema_version,
            ));
        }
        let normalized = Self::new(envelope.records.clone())?;
        if normalized.records != envelope.records {
            return Err(SpeciesIdentityError::NondeterministicCacheOrder);
        }
        Ok(envelope)
    }

    /// Serializes with canonical JSON object-key ordering.
    ///
    /// # Errors
    ///
    /// Returns an error if canonical serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, SpeciesIdentityError> {
        let value = serde_json::to_value(self)
            .map_err(|error| SpeciesIdentityError::InvalidCache(error.to_string()))?;
        crate::canonical_json(&value)
            .map_err(|error| SpeciesIdentityError::InvalidCache(error.to_string()))
    }
}

impl ResolvedSpecies {
    /// Validates formula, structural graph, and charge agreement before an
    /// identity becomes resolvable.
    ///
    /// # Errors
    ///
    /// Rejects empty identity fields, non-normalized aliases, or a supplied
    /// graph whose element inventory or net charge disagrees with the record.
    pub fn validate_identity<R: ElementRegistry>(
        input: ResolvedSpeciesInput,
        elements: &R,
    ) -> Result<Self, SpeciesIdentityError> {
        if input.display_name.trim().is_empty() || input.formula_text.trim().is_empty() {
            return Err(SpeciesIdentityError::EmptySearchKey);
        }
        if input
            .normalized_aliases
            .iter()
            .any(|alias| alias.is_empty() || alias != &normalize_name(alias))
        {
            return Err(SpeciesIdentityError::NonCanonicalAlias);
        }
        if let Some(structure) = &input.structure {
            validate_formula_graph(&input.formula, structure, elements)?;
            if structure.graph().system_net_charge().to_string() != input.charge.value().to_string()
            {
                return Err(SpeciesIdentityError::ChargeGraphMismatch);
            }
        }
        Ok(Self {
            id: input.id,
            substance: input.substance,
            display_name: input.display_name,
            normalized_aliases: input.normalized_aliases,
            formula_text: input.formula_text,
            formula: input.formula,
            charge: input.charge,
            phase: input.phase,
            structure: input.structure,
            canonical_serialization: input.canonical_serialization,
            external_identifiers: input.external_identifiers,
            stereochemistry_policy: input.stereochemistry_policy,
            tautomer_policy: input.tautomer_policy,
            protonation_policy: input.protonation_policy,
            identity_confidence: input.identity_confidence,
            identity_provenance: input.identity_provenance,
            identity_premise: input.identity_premise,
        })
    }
}

fn validate_formula_graph<R: ElementRegistry>(
    formula: &NormalizedFormula,
    structure: &StructureDefinition,
    elements: &R,
) -> Result<(), SpeciesIdentityError> {
    let mut graph_composition = BTreeMap::new();
    for (symbol, count) in structure.formula().elements() {
        let element = elements
            .resolve(symbol)
            .ok_or_else(|| SpeciesIdentityError::UnknownGraphElement(symbol.to_string()))?;
        graph_composition.insert(element.id, BigUint::from(*count));
    }
    if formula.composition() != &graph_composition {
        return Err(SpeciesIdentityError::FormulaGraphMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesIdentityError {
    DuplicateSpecies(SpeciesId),
    DuplicateCacheIdentity,
    EmptySearchKey,
    NonCanonicalAlias,
    FormulaGraphMismatch,
    ChargeGraphMismatch,
    UnknownGraphElement(String),
    UnsupportedCacheSchema(u32),
    NondeterministicCacheOrder,
    InvalidCache(String),
}

impl fmt::Display for SpeciesIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateSpecies(id) => write!(formatter, "duplicate species identity `{id}`"),
            Self::DuplicateCacheIdentity => {
                formatter.write_str("duplicate cached species identity")
            }
            Self::EmptySearchKey => {
                formatter.write_str("species identity requires a name and formula")
            }
            Self::NonCanonicalAlias => formatter.write_str("species aliases must be normalized"),
            Self::FormulaGraphMismatch => {
                formatter.write_str("species formula and structural graph disagree")
            }
            Self::ChargeGraphMismatch => {
                formatter.write_str("species charge and structural graph disagree")
            }
            Self::UnknownGraphElement(symbol) => write!(
                formatter,
                "structural graph uses unknown element `{symbol}`"
            ),
            Self::UnsupportedCacheSchema(version) => {
                write!(formatter, "unsupported identity cache schema {version}")
            }
            Self::NondeterministicCacheOrder => {
                formatter.write_str("identity cache records are not canonically ordered")
            }
            Self::InvalidCache(message) => write!(formatter, "invalid identity cache: {message}"),
        }
    }
}

impl std::error::Error for SpeciesIdentityError {}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use num_bigint::BigUint;

    use super::*;
    use crate::{
        Count, Element, ElementId, ElementSymbol, FactId, FormulaPart, FormulaSegment,
        FormulaSyntax, StaticElementRegistry, SubstanceId,
    };

    fn elements() -> StaticElementRegistry {
        StaticElementRegistry::new([
            Element {
                id: ElementId::new(1).expect("H id"),
                symbol: ElementSymbol::new("H").expect("H"),
            },
            Element {
                id: ElementId::new(8).expect("O id"),
                symbol: ElementSymbol::new("O").expect("O"),
            },
        ])
        .expect("registry")
    }

    fn formula(parts: &[(&str, u32)]) -> NormalizedFormula {
        FormulaSyntax {
            segments: vec![FormulaSegment {
                coefficient: Count::one(),
                parts: parts
                    .iter()
                    .map(|(symbol, count)| FormulaPart::Element {
                        symbol: ElementSymbol::new(*symbol).expect("symbol"),
                        count: Count::new(BigUint::from(*count)).expect("count"),
                    })
                    .collect(),
            }],
        }
        .normalize(&elements())
        .expect("normalized formula")
    }

    fn record(id: &str, name: &str, aliases: &[&str], formula_text: &str) -> ResolvedSpecies {
        ResolvedSpecies::validate_identity(
            ResolvedSpeciesInput {
                id: SpeciesId::from_str(id).expect("species id"),
                substance: SubstanceId::from_str(id).expect("substance id"),
                display_name: name.to_owned(),
                normalized_aliases: aliases.iter().map(|value| normalize_name(value)).collect(),
                formula_text: formula_text.to_owned(),
                formula: formula(&[("H", 2), ("O", 1)]),
                charge: Charge::neutral(),
                phase: Phase::Liquid,
                structure: None,
                canonical_serialization: None,
                external_identifiers: BTreeSet::new(),
                stereochemistry_policy: StereochemistryPolicy::NotApplicable,
                tautomer_policy: TautomerPolicy::NotApplicable,
                protonation_policy: ProtonationPolicy::ExactChargeState,
                identity_confidence: IdentityConfidence::Reviewed,
                identity_provenance: Vec::new(),
                identity_premise: FactId::from_str("fact.identity.water").expect("fact id"),
            },
            &elements(),
        )
        .expect("resolved identity")
    }

    #[test]
    fn name_and_formula_synonyms_converge() {
        let mut registry = SpeciesRegistry::default();
        registry
            .insert(record(
                "species.water",
                "water",
                &["oxidane", "dihydrogen monoxide"],
                "H2O",
            ))
            .expect("insert water");
        for query in [
            SpeciesQuery {
                name: Some("Oxidane".into()),
                formula: None,
                charge: None,
                phase: None,
                external_identifier: None,
            },
            SpeciesQuery {
                name: None,
                formula: Some("H2O".into()),
                charge: None,
                phase: None,
                external_identifier: None,
            },
        ] {
            let SpeciesResolution::Resolved(species) = registry.resolve(&query) else {
                panic!("query should resolve")
            };
            assert_eq!(species.id.as_str(), "species.water");
        }
    }

    #[test]
    fn formula_only_queries_preserve_isomer_ambiguity() {
        let mut registry = SpeciesRegistry::default();
        registry
            .insert(record("species.a", "isomer a", &["a"], "H2O"))
            .expect("insert a");
        registry
            .insert(record("species.b", "isomer b", &["b"], "H2O"))
            .expect("insert b");
        let resolution = registry.resolve(&SpeciesQuery {
            name: None,
            formula: Some("H2O".into()),
            charge: None,
            phase: None,
            external_identifier: None,
        });
        let SpeciesResolution::Ambiguous(ambiguity) = resolution else {
            panic!("formula-only lookup must remain ambiguous")
        };
        assert_eq!(ambiguity.alternatives.len(), 2);
    }
}
