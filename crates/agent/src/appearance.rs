use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use chem_domain::ContentDigest;
use serde::{Deserialize, Serialize};

use crate::{AgentError, TrustTier};

pub const OXIDE_APPEARANCE_SCHEMA_VERSION: u32 = 1;
const OXIDE_APPEARANCE_CACHE_SCHEMA_VERSION: u32 = 1;
const OXIDE_APPEARANCE_CONTRACT_VERSION: u32 = 1;
const MAX_APPEARANCE_BYTES: u64 = 96 * 1024;
static APPEARANCE_CACHE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Exact validated oxide identity supplied to the bounded appearance lookup.
///
/// This request cannot select chemistry: the product has already crossed the
/// catalogue and kernel boundary. The provider may only enrich its observable
/// representative solid colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OxideAppearanceRequest {
    pub schema_version: u32,
    pub product_binding: String,
    pub product_structure_id: String,
    pub product_formula: String,
    pub product_display_name: String,
    pub catalogue_digest: ContentDigest,
}

impl OxideAppearanceRequest {
    /// Constructs a request bound to one exact validated product.
    #[must_use]
    pub fn new(
        product_binding: impl Into<String>,
        product_structure_id: impl Into<String>,
        product_formula: impl Into<String>,
        product_display_name: impl Into<String>,
        catalogue_digest: ContentDigest,
    ) -> Self {
        Self {
            schema_version: OXIDE_APPEARANCE_SCHEMA_VERSION,
            product_binding: product_binding.into(),
            product_structure_id: product_structure_id.into(),
            product_formula: product_formula.into(),
            product_display_name: product_display_name.into(),
            catalogue_digest,
        }
    }

    /// Stable identity used for generation-scoped result rejection and cache
    /// binding.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be serialized canonically.
    pub fn binding_digest(&self) -> Result<ContentDigest, AgentError> {
        self.validate()?;
        let bytes = serde_json::to_vec(&(
            OXIDE_APPEARANCE_CONTRACT_VERSION,
            self,
            OXIDE_APPEARANCE_SCHEMA_VERSION,
        ))
        .map_err(|error| AgentError::new("oxide appearance request", error.to_string()))?;
        Ok(ContentDigest::sha256(&bytes))
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.schema_version != OXIDE_APPEARANCE_SCHEMA_VERSION {
            return Err(AgentError::new(
                "oxide appearance request",
                "unsupported appearance schema version",
            ));
        }
        validate_bounded_text(&self.product_binding, 1, 200, "product binding")?;
        validate_bounded_text(&self.product_structure_id, 1, 300, "product structure id")?;
        validate_bounded_text(&self.product_formula, 1, 200, "product formula")?;
        validate_bounded_text(&self.product_display_name, 1, 240, "product display name")
    }
}

/// Closed representative solid-colour families accepted from the provider.
///
/// Keeping RGB selection local prevents a model from injecting arbitrary
/// rendering values and gives every reaction the same restrained visual
/// language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OxideColourFamily {
    White,
    OffWhite,
    PaleYellow,
    Yellow,
    Orange,
    Red,
    RedBrown,
    Brown,
    DarkBrown,
    Green,
    Blue,
    Purple,
    Grey,
    Black,
}

impl OxideColourFamily {
    #[must_use]
    pub const fn srgb(self) -> [u8; 3] {
        match self {
            Self::White => [0xee, 0xf1, 0xef],
            Self::OffWhite => [0xdf, 0xdc, 0xcb],
            Self::PaleYellow => [0xdf, 0xd5, 0x86],
            Self::Yellow => [0xd4, 0xb7, 0x3f],
            Self::Orange => [0xd0, 0x72, 0x35],
            Self::Red => [0xb9, 0x42, 0x3b],
            Self::RedBrown => [0x8e, 0x46, 0x36],
            Self::Brown => [0x76, 0x52, 0x3d],
            Self::DarkBrown => [0x4f, 0x3b, 0x32],
            Self::Green => [0x66, 0x83, 0x59],
            Self::Blue => [0x52, 0x70, 0x8d],
            Self::Purple => [0x72, 0x58, 0x7f],
            Self::Grey => [0x78, 0x7d, 0x82],
            Self::Black => [0x20, 0x23, 0x24],
        }
    }
}

/// One source location returned by the live-search provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub supporting_excerpt: String,
}

/// Model-authored appearance claim. Storage does not confer authority; every
/// instance is rebound and validated before use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OxideAppearanceClaim {
    pub schema_version: u32,
    pub product_binding: String,
    pub product_structure_id: String,
    pub product_formula: String,
    pub catalogue_digest: ContentDigest,
    pub colour_family: OxideColourFamily,
    pub representative_condition: String,
    pub sources: Vec<AppearanceSource>,
    pub limitations: String,
}

impl OxideAppearanceClaim {
    /// Strictly decodes and validates a provider response against the exact
    /// current product identity.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized JSON, schema drift, stale identity
    /// bindings, weak source records, or procedural content.
    pub fn from_json_for(
        bytes: &[u8],
        request: &OxideAppearanceRequest,
    ) -> Result<ValidatedOxideAppearance, AgentError> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_APPEARANCE_BYTES {
            return Err(AgentError::new(
                "oxide appearance",
                "response exceeds the 96 KiB contract limit",
            ));
        }
        let claim: Self = serde_json::from_slice(bytes)
            .map_err(|error| AgentError::new("oxide appearance", error.to_string()))?;
        claim.validate_for(request)
    }

    fn validate_for(
        self,
        request: &OxideAppearanceRequest,
    ) -> Result<ValidatedOxideAppearance, AgentError> {
        request.validate()?;
        if self.schema_version != OXIDE_APPEARANCE_SCHEMA_VERSION
            || request.schema_version != OXIDE_APPEARANCE_SCHEMA_VERSION
        {
            return Err(AgentError::new(
                "oxide appearance",
                "unsupported appearance schema version",
            ));
        }
        if self.product_binding != request.product_binding
            || self.product_structure_id != request.product_structure_id
            || self.product_formula != request.product_formula
            || self.catalogue_digest != request.catalogue_digest
        {
            return Err(AgentError::new(
                "oxide appearance",
                "claim is not bound to the current validated oxide product",
            ));
        }
        validate_bounded_text(
            &self.representative_condition,
            8,
            240,
            "representative condition",
        )?;
        validate_bounded_text(&self.limitations, 3, 600, "limitations")?;
        if self.sources.is_empty() || self.sources.len() > 3 {
            return Err(AgentError::new(
                "oxide appearance",
                "one to three supporting sources are required",
            ));
        }
        let mut urls = BTreeSet::new();
        for source in &self.sources {
            validate_bounded_text(&source.title, 3, 240, "source title")?;
            validate_bounded_text(&source.publisher, 2, 160, "source publisher")?;
            validate_bounded_text(&source.supporting_excerpt, 5, 1_200, "source excerpt")?;
            if !source.url.starts_with("https://")
                || source.url.chars().any(char::is_whitespace)
                || !urls.insert(source.url.as_str())
            {
                return Err(AgentError::new(
                    "oxide appearance",
                    "sources require unique HTTPS URLs",
                ));
            }
            reject_procedural_text(&source.supporting_excerpt)?;
        }
        reject_procedural_text(&self.limitations)?;
        Ok(ValidatedOxideAppearance { claim: self })
    }
}

/// Revalidated model-asserted appearance. This type intentionally cannot be
/// promoted to reviewed catalogue authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedOxideAppearance {
    claim: OxideAppearanceClaim,
}

impl ValidatedOxideAppearance {
    #[must_use]
    pub const fn colour_family(&self) -> OxideColourFamily {
        self.claim.colour_family
    }

    #[must_use]
    pub const fn claim(&self) -> &OxideAppearanceClaim {
        &self.claim
    }

    #[must_use]
    pub const fn trust_tier(&self) -> TrustTier {
        TrustTier::ModelAsserted
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppearanceCacheEnvelope {
    schema_version: u32,
    contract_version: u32,
    request_binding: ContentDigest,
    claim_digest: ContentDigest,
    trust_tier: TrustTier,
    provider: String,
    model: String,
    claim: OxideAppearanceClaim,
}

/// Computes the digest-bound path for one provisional appearance claim.
///
/// # Errors
///
/// Returns an error if the request cannot be serialized.
pub fn oxide_appearance_cache_path(
    directory: &Path,
    request: &OxideAppearanceRequest,
) -> Result<PathBuf, AgentError> {
    Ok(directory.join(format!(
        "oxide-appearance-v1-{}.json",
        request.binding_digest()?.to_hex()
    )))
}

/// Loads and revalidates a cached model claim. Corrupt, old, or stale entries
/// are cache misses.
#[must_use]
pub fn load_oxide_appearance_cache(
    directory: Option<&Path>,
    request: &OxideAppearanceRequest,
) -> Option<ValidatedOxideAppearance> {
    let path = oxide_appearance_cache_path(directory?, request).ok()?;
    if fs::metadata(&path).ok()?.len() > MAX_APPEARANCE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let envelope: AppearanceCacheEnvelope = serde_json::from_slice(&bytes).ok()?;
    let request_binding = request.binding_digest().ok()?;
    let expected_claim_digest = claim_digest(&envelope.claim).ok()?;
    if envelope.schema_version != OXIDE_APPEARANCE_CACHE_SCHEMA_VERSION
        || envelope.contract_version != OXIDE_APPEARANCE_CONTRACT_VERSION
        || envelope.request_binding != request_binding
        || envelope.claim_digest != expected_claim_digest
        || envelope.trust_tier != TrustTier::ModelAsserted
    {
        return None;
    }
    envelope.claim.validate_for(request).ok()
}

/// Atomically stores a revalidated model claim without promoting it into the
/// catalogue.
///
/// # Errors
///
/// Returns an error for stale claims, serialization, directory, or write
/// failures.
pub fn store_oxide_appearance_cache(
    directory: &Path,
    request: &OxideAppearanceRequest,
    appearance: &ValidatedOxideAppearance,
    provider: &str,
    model: &str,
) -> Result<(), AgentError> {
    let revalidated = appearance.claim.clone().validate_for(request)?;
    let path = oxide_appearance_cache_path(directory, request)?;
    let envelope = AppearanceCacheEnvelope {
        schema_version: OXIDE_APPEARANCE_CACHE_SCHEMA_VERSION,
        contract_version: OXIDE_APPEARANCE_CONTRACT_VERSION,
        request_binding: request.binding_digest()?,
        claim_digest: claim_digest(revalidated.claim())?,
        trust_tier: revalidated.trust_tier(),
        provider: provider.to_owned(),
        model: model.to_owned(),
        claim: revalidated.claim,
    };
    let bytes = serde_json::to_vec(&envelope)
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_APPEARANCE_BYTES {
        return Err(AgentError::new(
            "oxide appearance cache",
            "entry exceeds the 96 KiB contract limit",
        ));
    }
    fs::create_dir_all(directory)
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))?;
    let sequence = APPEARANCE_CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        ".oxide-appearance-{}-{sequence}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))?;
    replace_file(&temporary, &path)
}

fn claim_digest(claim: &OxideAppearanceClaim) -> Result<ContentDigest, AgentError> {
    serde_json::to_vec(claim)
        .map(|bytes| ContentDigest::sha256(&bytes))
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))
}

fn validate_bounded_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    field: &'static str,
) -> Result<(), AgentError> {
    let length = value.trim().chars().count();
    if length < minimum || length > maximum {
        return Err(AgentError::new(
            "oxide appearance",
            format!("{field} is outside its bounded length"),
        ));
    }
    Ok(())
}

fn reject_procedural_text(value: &str) -> Result<(), AgentError> {
    const PROCEDURAL_MARKERS: [&str; 8] = [
        "step 1",
        "step one",
        "add ",
        "mix ",
        "heat to",
        "grams",
        "millilit",
        "procedure",
    ];
    let lowered = value.to_ascii_lowercase();
    if PROCEDURAL_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        return Err(AgentError::new(
            "oxide appearance",
            "procedural laboratory content is not allowed",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(temporary: &Path, destination: &Path) -> Result<(), AgentError> {
    fs::rename(temporary, destination)
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))
}

#[cfg(target_os = "windows")]
fn replace_file(temporary: &Path, destination: &Path) -> Result<(), AgentError> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))?;
    }
    fs::rename(temporary, destination)
        .map_err(|error| AgentError::new("oxide appearance cache", error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn request() -> OxideAppearanceRequest {
        OxideAppearanceRequest::new(
            "oxideProduct",
            "Structures.SodiumOxide",
            "Na2O",
            "sodium oxide",
            ContentDigest::from_str(
                "1111111111111111111111111111111111111111111111111111111111111111",
            )
            .expect("digest"),
        )
    }

    fn claim() -> OxideAppearanceClaim {
        let request = request();
        OxideAppearanceClaim {
            schema_version: OXIDE_APPEARANCE_SCHEMA_VERSION,
            product_binding: request.product_binding,
            product_structure_id: request.product_structure_id,
            product_formula: request.product_formula,
            catalogue_digest: request.catalogue_digest,
            colour_family: OxideColourFamily::White,
            representative_condition: "Representative dry solid at ordinary ambient conditions"
                .to_owned(),
            sources: vec![AppearanceSource {
                title: "Sodium oxide".to_owned(),
                publisher: "Reference publisher".to_owned(),
                url: "https://example.org/sodium-oxide".to_owned(),
                supporting_excerpt: "The compound is described as a white solid.".to_owned(),
            }],
            limitations: "Colour can vary with impurities and physical form.".to_owned(),
        }
    }

    #[test]
    fn exact_bound_claim_validates_to_closed_palette() {
        let bytes = serde_json::to_vec(&claim()).expect("claim JSON");
        let validated =
            OxideAppearanceClaim::from_json_for(&bytes, &request()).expect("valid appearance");
        assert_eq!(validated.colour_family(), OxideColourFamily::White);
        assert_eq!(validated.trust_tier(), TrustTier::ModelAsserted);
    }

    #[test]
    fn stale_product_identity_is_rejected() {
        let mut claim = claim();
        claim.product_formula = "NaO".to_owned();
        let error = claim
            .validate_for(&request())
            .expect_err("mismatched formula must fail");
        assert!(error.to_string().contains("not bound"));
    }

    #[test]
    fn procedural_source_text_is_rejected() {
        let mut claim = claim();
        claim.sources[0].supporting_excerpt =
            "Step 1: heat to a target temperature before observing the solid.".to_owned();
        let error = claim
            .validate_for(&request())
            .expect_err("procedural content must fail");
        assert!(error.to_string().contains("procedural"));
    }

    #[test]
    fn cache_revalidates_and_rejects_tampering() {
        let directory = std::env::temp_dir().join(format!(
            "chemspec-oxide-appearance-test-{}",
            APPEARANCE_CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let request = request();
        let validated = claim().validate_for(&request).expect("valid claim");
        store_oxide_appearance_cache(
            &directory,
            &request,
            &validated,
            "test-provider",
            "test-model",
        )
        .expect("cache write");
        assert_eq!(
            load_oxide_appearance_cache(Some(&directory), &request)
                .expect("cache hit")
                .colour_family(),
            OxideColourFamily::White
        );

        let path = oxide_appearance_cache_path(&directory, &request).expect("path");
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read")).expect("JSON");
        envelope["claim"]["product_formula"] = serde_json::Value::String("NaO".to_owned());
        fs::write(path, serde_json::to_vec(&envelope).expect("tampered JSON")).expect("tamper");
        assert!(load_oxide_appearance_cache(Some(&directory), &request).is_none());
        let _ = fs::remove_dir_all(directory);
    }
}
