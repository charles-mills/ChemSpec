use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_domain::{
    CachedIdentityRecord, CanonicalSpeciesSerialization, Charge, ChargeSign, Count, Element,
    ElementId, ExternalIdentifier, FactId, FormulaComposition, FormulaPart, FormulaSegment,
    FormulaSyntax, IdentityCacheEnvelope, IdentityConfidence, IdentityProvenance, Phase,
    ProtonationPolicy, ResolvedSpecies, ResolvedSpeciesInput, SpeciesId, SpeciesQuery,
    SpeciesRegistry, SpeciesResolution, StaticElementRegistry, StereochemistryPolicy,
    StructureDefinition, SubstanceId, TautomerPolicy,
};
use num_bigint::BigUint;

use crate::AgentError;

const IDENTITY_CACHE_FILE: &str = "species-identities-v1.json";
const MAX_IDENTITY_CACHE_BYTES: u64 = 4 * 1024 * 1024;
static IDENTITY_CACHE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAdapterError(String);

impl IdentityAdapterError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for IdentityAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for IdentityAdapterError {}

/// Narrow boundary for optional local chemistry tooling or public identity
/// sources. Returned cache records are untrusted and are reconstructed through
/// domain validation before they can enter a registry.
pub trait SpeciesIdentityAdapter {
    fn name(&self) -> &str;

    /// # Errors
    ///
    /// Returns a transport, capability, decode, or source-policy error.
    fn resolve(
        &mut self,
        query: &SpeciesQuery,
    ) -> Result<Vec<CachedIdentityRecord>, IdentityAdapterError>;
}

/// Adapter boundary for mature local structure decoders. `ChemSpec` rechecks
/// formula inventory and charge after decoding.
pub trait StructureIdentityDecoder {
    /// # Errors
    ///
    /// Returns a capability or strict structural decode error.
    fn decode(
        &mut self,
        canonical_json: &str,
        expected_digest: chem_domain::ContentDigest,
    ) -> Result<StructureDefinition, IdentityAdapterError>;
}

#[derive(Debug, Default)]
pub struct NoStructureDecoder;

impl StructureIdentityDecoder for NoStructureDecoder {
    fn decode(
        &mut self,
        _canonical_json: &str,
        _expected_digest: chem_domain::ContentDigest,
    ) -> Result<StructureDefinition, IdentityAdapterError> {
        Err(IdentityAdapterError::new(
            "structural identity decoding is unavailable on this device",
        ))
    }
}

#[derive(Debug, Clone)]
pub enum IdentityResolutionOutcome {
    Resolved(Box<ResolvedSpecies>),
    Ambiguous(Vec<ResolvedSpecies>),
    NotFound,
    Unavailable(Vec<String>),
}

/// Builds the exact element registry used to validate external/cache formulae.
///
/// # Errors
///
/// Returns an error for invalid or duplicate catalogue element identities.
pub fn reviewed_element_registry(
    catalogue: &ValidatedCatalogueBundle,
) -> Result<StaticElementRegistry, AgentError> {
    StaticElementRegistry::new(
        catalogue
            .document()
            .elements
            .iter()
            .map(|record| {
                Ok(Element {
                    id: ElementId::new(record.atomic_number)
                        .map_err(|error| AgentError::new("species identity", error.to_string()))?,
                    symbol: record.symbol.clone(),
                })
            })
            .collect::<Result<Vec<_>, AgentError>>()?,
    )
    .map_err(|error| AgentError::new("species identity", error.to_string()))
}

/// Projects every validated reviewed catalogue structure onto a stable species
/// identity without weakening the catalogue's graph and premise boundaries.
///
/// # Errors
///
/// Returns an error if a catalogue element, stable ID, formula, graph, charge,
/// or canonical structure serialization cannot satisfy the identity contract.
#[allow(clippy::too_many_lines)]
pub fn reviewed_species_registry(
    catalogue: &ValidatedCatalogueBundle,
) -> Result<SpeciesRegistry, AgentError> {
    let elements = reviewed_element_registry(catalogue)?;
    let mut registry = SpeciesRegistry::default();
    for (structure_id, structure) in catalogue.structures() {
        let stable_suffix = structure_id.as_str();
        let species_id = SpeciesId::from_str(&format!("catalogue.{stable_suffix}"))
            .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        let substance = SubstanceId::from_str(&format!("catalogue.{stable_suffix}"))
            .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        let identity_premise = FactId::from_str(&format!("identity.{stable_suffix}"))
            .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        let formula_text = catalogue_formula(catalogue, structure_id)
            .unwrap_or_else(|| inventory_formula(structure.formula()));
        let formula = FormulaSyntax {
            segments: vec![FormulaSegment {
                coefficient: Count::one(),
                parts: structure
                    .formula()
                    .elements()
                    .iter()
                    .map(|(symbol, count)| {
                        Ok(FormulaPart::Element {
                            symbol: symbol.clone(),
                            count: Count::new(BigUint::from(*count)).map_err(|error| {
                                AgentError::new("species identity", error.to_string())
                            })?,
                        })
                    })
                    .collect::<Result<Vec<_>, AgentError>>()?,
            }],
        }
        .normalize(&elements)
        .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        let net_charge = structure.graph().system_net_charge();
        let charge = if net_charge == 0 {
            Charge::neutral()
        } else {
            let magnitude = BigUint::from(net_charge.unsigned_abs());
            Charge::from_magnitude(
                magnitude,
                if net_charge.is_positive() {
                    ChargeSign::Positive
                } else {
                    ChargeSign::Negative
                },
            )
            .map_err(|error| AgentError::new("species identity", error.to_string()))?
        };
        let canonical_graph = structure
            .graph()
            .canonical_json()
            .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        let display_name = display_name(stable_suffix);
        let mut aliases = BTreeSet::new();
        aliases.insert(normalize_alias(&display_name));
        aliases.insert(normalize_alias(stable_suffix));
        if let Some(application) = catalogue.structure_application(structure_id) {
            aliases.extend(
                application
                    .aliases
                    .iter()
                    .map(|alias| normalize_alias(alias)),
            );
        }
        let species = ResolvedSpecies::validate_identity(
            ResolvedSpeciesInput {
                id: species_id,
                substance,
                display_name,
                normalized_aliases: aliases,
                formula_text,
                formula,
                charge,
                phase: Phase::Unknown,
                structure: Some(structure.clone()),
                canonical_serialization: Some(CanonicalSpeciesSerialization {
                    media_type: "application/vnd.chemspec.structural+json".to_owned(),
                    value: String::from_utf8(canonical_graph.clone())
                        .map_err(|error| AgentError::new("species identity", error.to_string()))?,
                    digest: chem_domain::ContentDigest::sha256(&canonical_graph),
                }),
                external_identifiers: BTreeSet::<ExternalIdentifier>::new(),
                stereochemistry_policy: StereochemistryPolicy::Unspecified,
                tautomer_policy: TautomerPolicy::ExactTautomer,
                protonation_policy: ProtonationPolicy::ExactChargeState,
                identity_confidence: IdentityConfidence::Reviewed,
                identity_provenance: vec![IdentityProvenance::HostPinned {
                    catalogue_digest: catalogue.digest(),
                }],
                identity_premise,
            },
            &elements,
        )
        .map_err(|error| AgentError::new("species identity", error.to_string()))?;
        registry
            .insert(species)
            .map_err(|error| AgentError::new("species identity", error.to_string()))?;
    }
    Ok(registry)
}

/// Builds one model-proposed species identity around a structure that already
/// crossed full catalogue validation inside an isolated working bundle. The
/// provenance names the model proposal explicitly; the confidence never rises
/// above `ExternalUnverified`.
///
/// # Errors
///
/// Returns an error when the structure, formula, charge, or identity contract
/// cannot be satisfied.
pub(crate) fn model_proposed_species(
    id: &SpeciesId,
    display_name: &str,
    formula_text: &str,
    phase: Phase,
    structure: &StructureDefinition,
    bundle: &ValidatedCatalogueBundle,
) -> Result<ResolvedSpecies, AgentError> {
    let elements = reviewed_element_registry(bundle)?;
    let formula = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: Count::one(),
            parts: structure
                .formula()
                .elements()
                .iter()
                .map(|(symbol, count)| {
                    Ok(FormulaPart::Element {
                        symbol: symbol.clone(),
                        count: Count::new(BigUint::from(*count)).map_err(|error| {
                            AgentError::new("species identity", error.to_string())
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, AgentError>>()?,
        }],
    }
    .normalize(&elements)
    .map_err(|error| AgentError::new("species identity", error.to_string()))?;
    let net_charge = structure.graph().system_net_charge();
    let charge = if net_charge == 0 {
        Charge::neutral()
    } else {
        let magnitude = BigUint::from(net_charge.unsigned_abs());
        Charge::from_magnitude(
            magnitude,
            if net_charge.is_positive() {
                ChargeSign::Positive
            } else {
                ChargeSign::Negative
            },
        )
        .map_err(|error| AgentError::new("species identity", error.to_string()))?
    };
    let canonical_graph = structure
        .graph()
        .canonical_json()
        .map_err(|error| AgentError::new("species identity", error.to_string()))?;
    let mut aliases = BTreeSet::new();
    aliases.insert(normalize_alias(display_name));
    ResolvedSpecies::validate_identity(
        ResolvedSpeciesInput {
            id: id.clone(),
            substance: SubstanceId::from_str(id.as_str())
                .map_err(|error| AgentError::new("species identity", error.to_string()))?,
            display_name: display_name.to_owned(),
            normalized_aliases: aliases,
            formula_text: formula_text.to_owned(),
            formula,
            charge,
            phase,
            structure: Some(structure.clone()),
            canonical_serialization: Some(CanonicalSpeciesSerialization {
                media_type: "application/vnd.chemspec.structural+json".to_owned(),
                value: String::from_utf8(canonical_graph.clone())
                    .map_err(|error| AgentError::new("species identity", error.to_string()))?,
                digest: chem_domain::ContentDigest::sha256(&canonical_graph),
            }),
            external_identifiers: BTreeSet::<ExternalIdentifier>::new(),
            stereochemistry_policy: StereochemistryPolicy::Unspecified,
            tautomer_policy: TautomerPolicy::ExactTautomer,
            protonation_policy: ProtonationPolicy::ExactChargeState,
            identity_confidence: IdentityConfidence::ExternalUnverified,
            identity_provenance: vec![IdentityProvenance::External {
                resolver: "model_structure_proposal".to_owned(),
                source_url: "urn:chemspec:model-proposed-structure".to_owned(),
                retrieved_at_unix_ms: 0,
                content_digest: chem_domain::ContentDigest::sha256(&canonical_graph),
            }],
            identity_premise: FactId::from_str(&format!("identity.{}", structure.id()))
                .map_err(|error| AgentError::new("species identity", error.to_string()))?,
        },
        &elements,
    )
    .map_err(|error| AgentError::new("species identity", error.to_string()))
}

/// Loads a strict, canonically ordered device-local identity cache. Corrupt,
/// oversized, and old-schema files are cache misses and remain untouched.
#[must_use]
pub fn load_identity_cache(directory: Option<&Path>) -> Option<IdentityCacheEnvelope> {
    let path = directory?.join(IDENTITY_CACHE_FILE);
    if fs::metadata(&path).ok()?.len() > MAX_IDENTITY_CACHE_BYTES {
        return None;
    }
    IdentityCacheEnvelope::from_json(&fs::read(path).ok()?).ok()
}

/// Atomically persists a canonical device-local identity cache.
///
/// # Errors
///
/// Returns a directory, serialization, write, or replacement error.
pub fn store_identity_cache(
    directory: &Path,
    envelope: &IdentityCacheEnvelope,
) -> Result<(), AgentError> {
    fs::create_dir_all(directory)
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    let bytes = envelope
        .canonical_json()
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    if bytes.len() as u64 > MAX_IDENTITY_CACHE_BYTES {
        return Err(AgentError::new(
            "identity cache",
            "cache exceeds size limit",
        ));
    }
    let temporary = directory.join(format!(
        ".identity-{}-{}.tmp",
        std::process::id(),
        IDENTITY_CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    atomic_replace(&temporary, &directory.join(IDENTITY_CACHE_FILE))
}

/// Resolves host-pinned identities first, then the device cache, then optional
/// adapters in declared order. Every cache/adapter record is reconstructed and
/// validated before use; ambiguous alternatives are returned to the caller.
///
/// # Errors
///
/// Returns a deterministic validation or cache-persistence error. Adapter
/// availability failures are retained in `Unavailable` when no result exists.
pub fn resolve_species_identity(
    query: &SpeciesQuery,
    host: &SpeciesRegistry,
    cache_directory: Option<&Path>,
    elements: &StaticElementRegistry,
    decoder: &mut dyn StructureIdentityDecoder,
    adapters: &mut [&mut dyn SpeciesIdentityAdapter],
) -> Result<IdentityResolutionOutcome, AgentError> {
    if let Some(outcome) = owned_resolution(host.resolve(query), host) {
        return Ok(outcome);
    }
    let cached = load_identity_cache(cache_directory);
    if let Some(envelope) = &cached {
        let registry = registry_from_cache(envelope, elements, decoder)?;
        if let Some(outcome) = owned_resolution(registry.resolve(query), &registry) {
            return Ok(outcome);
        }
    }

    let mut failures = Vec::new();
    let mut discovered = Vec::new();
    for adapter in adapters {
        match adapter.resolve(query) {
            Ok(records) => discovered.extend(records),
            Err(error) => failures.push(format!("{}: {error}", adapter.name())),
        }
    }
    if discovered.is_empty() {
        return Ok(if failures.is_empty() {
            IdentityResolutionOutcome::NotFound
        } else {
            IdentityResolutionOutcome::Unavailable(failures)
        });
    }
    let merged = merge_identity_records(
        cached.map_or_else(Vec::new, |value| value.records),
        discovered,
    )?;
    let envelope = IdentityCacheEnvelope::new(merged)
        .map_err(|error| AgentError::new("identity adapter", error.to_string()))?;
    let registry = registry_from_cache(&envelope, elements, decoder)?;
    let outcome = owned_resolution(registry.resolve(query), &registry)
        .unwrap_or(IdentityResolutionOutcome::NotFound);
    if matches!(
        outcome,
        IdentityResolutionOutcome::Resolved(_) | IdentityResolutionOutcome::Ambiguous(_)
    ) && let Some(directory) = cache_directory
    {
        store_identity_cache(directory, &envelope)?;
    }
    Ok(outcome)
}

fn owned_resolution(
    resolution: SpeciesResolution<'_>,
    registry: &SpeciesRegistry,
) -> Option<IdentityResolutionOutcome> {
    match resolution {
        SpeciesResolution::Resolved(species) => Some(IdentityResolutionOutcome::Resolved(
            Box::new(species.clone()),
        )),
        SpeciesResolution::Ambiguous(ambiguity) => Some(IdentityResolutionOutcome::Ambiguous(
            ambiguity
                .alternatives
                .iter()
                .filter_map(|id| registry.get(id).cloned())
                .collect(),
        )),
        SpeciesResolution::NotFound => None,
    }
}

fn merge_identity_records(
    cached: Vec<CachedIdentityRecord>,
    discovered: Vec<CachedIdentityRecord>,
) -> Result<Vec<CachedIdentityRecord>, AgentError> {
    let mut records = BTreeMap::new();
    for record in cached.into_iter().chain(discovered) {
        if let Some(previous) = records.insert(record.species_id.clone(), record.clone())
            && previous != record
        {
            return Err(AgentError::new(
                "identity adapter",
                format!("conflicting records for `{}`", record.species_id),
            ));
        }
    }
    Ok(records.into_values().collect())
}

fn registry_from_cache(
    envelope: &IdentityCacheEnvelope,
    elements: &StaticElementRegistry,
    decoder: &mut dyn StructureIdentityDecoder,
) -> Result<SpeciesRegistry, AgentError> {
    let mut registry = SpeciesRegistry::default();
    for record in &envelope.records {
        registry
            .insert(reconstruct_cached_identity(record, elements, decoder)?)
            .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    }
    Ok(registry)
}

fn reconstruct_cached_identity(
    record: &CachedIdentityRecord,
    elements: &StaticElementRegistry,
    decoder: &mut dyn StructureIdentityDecoder,
) -> Result<ResolvedSpecies, AgentError> {
    let composition = FormulaComposition::parse(&record.formula)
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    let formula = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: Count::one(),
            parts: composition
                .elements()
                .iter()
                .map(|(symbol, count)| {
                    Ok(FormulaPart::Element {
                        symbol: symbol.clone(),
                        count: Count::new(BigUint::from(*count)).map_err(|error| {
                            AgentError::new("identity cache", error.to_string())
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, AgentError>>()?,
        }],
    }
    .normalize(elements)
    .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    let structure = match (&record.canonical_structure_json, record.structure_digest) {
        (None, None) => None,
        (Some(json), Some(digest)) => {
            if chem_domain::ContentDigest::sha256(json.as_bytes()) != digest {
                return Err(AgentError::new(
                    "identity cache",
                    "canonical structure digest changed",
                ));
            }
            Some(
                decoder
                    .decode(json, digest)
                    .map_err(|error| AgentError::new("identity cache", error.to_string()))?,
            )
        }
        _ => {
            return Err(AgentError::new(
                "identity cache",
                "structure bytes and digest must be present together",
            ));
        }
    };
    let substance = SubstanceId::from_str(record.species_id.as_str())
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    let identity_premise = FactId::from_str(&format!("identity.{}", record.species_id.as_str()))
        .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
    ResolvedSpecies::validate_identity(
        ResolvedSpeciesInput {
            id: record.species_id.clone(),
            substance,
            display_name: record.display_name.clone(),
            normalized_aliases: record.normalized_aliases.clone(),
            formula_text: record.formula.clone(),
            formula,
            charge: record.charge.clone(),
            phase: record.phase,
            structure,
            canonical_serialization: record
                .canonical_structure_json
                .as_ref()
                .zip(record.structure_digest)
                .map(|(value, digest)| CanonicalSpeciesSerialization {
                    media_type: "application/vnd.chemspec.structural+json".into(),
                    value: value.clone(),
                    digest,
                }),
            external_identifiers: record.external_identifiers.clone(),
            stereochemistry_policy: record.stereochemistry_policy,
            tautomer_policy: record.tautomer_policy,
            protonation_policy: record.protonation_policy,
            identity_confidence: IdentityConfidence::ExternalUnverified,
            identity_provenance: record.provenance.clone(),
            identity_premise,
        },
        elements,
    )
    .map_err(|error| AgentError::new("identity cache", error.to_string()))
}

fn atomic_replace(temporary: &Path, destination: &Path) -> Result<(), AgentError> {
    #[cfg(target_os = "windows")]
    {
        let backup = std::path::PathBuf::from(format!("{}.backup", destination.display()));
        if destination.exists() {
            fs::rename(destination, &backup)
                .map_err(|error| AgentError::new("identity cache", error.to_string()))?;
        }
        match fs::rename(temporary, destination) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, destination);
                Err(AgentError::new("identity cache", error.to_string()))
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(temporary, destination)
            .map_err(|error| AgentError::new("identity cache", error.to_string()))
    }
}

fn catalogue_formula(
    catalogue: &ValidatedCatalogueBundle,
    id: &chem_domain::StructureId,
) -> Option<String> {
    catalogue
        .document()
        .structures
        .iter()
        .find(|record| record.id() == id)
        .map(|record| record.formula().to_owned())
        .or_else(|| {
            catalogue
                .structure_application(id)
                .map(|application| application.formula.clone())
        })
}

fn inventory_formula(inventory: &chem_domain::ElementInventory) -> String {
    inventory
        .elements()
        .iter()
        .map(|(symbol, count)| {
            if *count == 1 {
                symbol.to_string()
            } else {
                format!("{symbol}{count}")
            }
        })
        .collect()
}

fn display_name(id: &str) -> String {
    id.rsplit('.').next().unwrap_or(id).to_owned()
}

fn normalize_alias(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_catalogue::{CatalogueEnvelope, ValidatedCatalogueBundle};
    use chem_domain::ContentDigest;

    struct FakeAdapter {
        records: Vec<CachedIdentityRecord>,
    }

    impl SpeciesIdentityAdapter for FakeAdapter {
        fn name(&self) -> &'static str {
            "fake-public-identity"
        }

        fn resolve(
            &mut self,
            _query: &SpeciesQuery,
        ) -> Result<Vec<CachedIdentityRecord>, IdentityAdapterError> {
            Ok(std::mem::take(&mut self.records))
        }
    }

    fn external_record(id: &str, name: &str, inchi: &str) -> CachedIdentityRecord {
        CachedIdentityRecord {
            species_id: SpeciesId::from_str(id).expect("species ID"),
            display_name: name.into(),
            normalized_aliases: [normalize_alias(name)].into_iter().collect(),
            formula: "H2O2".into(),
            charge: Charge::neutral(),
            phase: Phase::Unknown,
            canonical_structure_json: None,
            structure_digest: None,
            external_identifiers: [ExternalIdentifier::Inchi(inchi.into())]
                .into_iter()
                .collect(),
            stereochemistry_policy: StereochemistryPolicy::NotApplicable,
            tautomer_policy: TautomerPolicy::ExactTautomer,
            protonation_policy: ProtonationPolicy::ExactChargeState,
            provenance: vec![IdentityProvenance::External {
                resolver: "fake-public-identity".into(),
                source_url: "https://example.test/identity".into(),
                retrieved_at_unix_ms: 1,
                content_digest: ContentDigest::sha256(name.as_bytes()),
            }],
        }
    }

    #[test]
    fn reviewed_catalogue_structures_gain_stable_identities() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue fixture");
        let mut envelope: CatalogueEnvelope =
            serde_json::from_slice(&bytes).expect("catalogue envelope");
        envelope.digest = envelope.computed_digest().expect("computed digest");
        let digest = envelope.digest;
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");

        let identities = reviewed_species_registry(&catalogue).expect("identity registry");
        assert_eq!(identities.records().len(), catalogue.structures().len());
        assert!(identities.records().values().all(|species| {
            species.structure.is_some()
                && species.identity_provenance
                    == vec![IdentityProvenance::HostPinned {
                        catalogue_digest: digest,
                    }]
        }));
        assert_ne!(digest, ContentDigest::sha256(b"unrelated"));
    }

    #[test]
    fn external_formula_isomers_remain_ambiguous_and_replay_from_device_cache() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
                .expect("catalogue fixture");
        let mut envelope: CatalogueEnvelope =
            serde_json::from_slice(&bytes).expect("catalogue envelope");
        envelope.digest = envelope.computed_digest().expect("computed digest");
        let catalogue = ValidatedCatalogueBundle::validate(envelope).expect("valid catalogue");
        let host = SpeciesRegistry::default();
        let elements = reviewed_element_registry(&catalogue).expect("elements");
        let directory = std::env::temp_dir().join(format!(
            "chemspec-identity-test-{}-{}",
            std::process::id(),
            IDENTITY_CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let query = SpeciesQuery {
            name: None,
            formula: Some("H2O2".into()),
            charge: Some(Charge::neutral()),
            phase: None,
            external_identifier: None,
        };
        let mut adapter = FakeAdapter {
            records: vec![
                external_record(
                    "external.hydrogen-peroxide-a",
                    "hydrogen peroxide identity a",
                    "InChI=1S/H2O2/a",
                ),
                external_record(
                    "external.hydrogen-peroxide-b",
                    "hydrogen peroxide identity b",
                    "InChI=1S/H2O2/b",
                ),
            ],
        };
        let mut decoder = NoStructureDecoder;
        let first = resolve_species_identity(
            &query,
            &host,
            Some(&directory),
            &elements,
            &mut decoder,
            &mut [&mut adapter],
        )
        .expect("adapter resolution");
        let IdentityResolutionOutcome::Ambiguous(alternatives) = first else {
            panic!("formula isomers must remain ambiguous")
        };
        assert_eq!(alternatives.len(), 2);
        let cached = load_identity_cache(Some(&directory)).expect("persisted cache");
        assert_eq!(cached.records.len(), 2);

        let started = std::time::Instant::now();
        let replay = resolve_species_identity(
            &query,
            &host,
            Some(&directory),
            &elements,
            &mut decoder,
            &mut [],
        )
        .expect("offline replay");
        assert!(
            matches!(replay, IdentityResolutionOutcome::Ambiguous(values) if values.len() == 2)
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "identity-cache replay exceeded the 250 ms local-hit budget"
        );
        std::fs::remove_dir_all(directory).expect("cleanup");
    }
}
