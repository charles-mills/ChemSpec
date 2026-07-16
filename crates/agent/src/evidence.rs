use std::{
    collections::BTreeMap,
    fmt, fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use chem_domain::ContentDigest;
use serde::{Deserialize, Serialize};

use crate::{ClaimDisposition, ClaimField, ReactionClaim, ValidatedStaticOutcome};

const MAX_FETCHED_BYTES: usize = 1024 * 1024;
const MAX_REDIRECTS: u8 = 3;
const MAX_DECOMPRESSION_RATIO: usize = 100;
static FETCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceContentType {
    PlainText,
    Html,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievedEvidenceDocument {
    pub requested_url: String,
    pub final_url: String,
    pub content_type: EvidenceContentType,
    pub body: Vec<u8>,
    pub compressed_bytes: Option<usize>,
    pub redirects: u8,
    /// Required for PDF; ignored for plain text and HTML.
    pub extracted_text: Option<String>,
    pub limitation: Option<String>,
}

pub trait EvidenceRetriever {
    /// Retrieves one bounded source without interpreting chemistry.
    ///
    /// # Errors
    ///
    /// Returns a typed transport or adapter error.
    fn retrieve(&mut self, url: &str) -> Result<RetrievedEvidenceDocument, EvidenceError>;
}

/// Capability-checked HTTPS retrieval adapter using the platform `curl` CLI.
/// It does not read user configuration or credentials and retains no cookies.
#[derive(Debug, Clone)]
pub struct CurlEvidenceRetriever {
    executable: PathBuf,
    deadline: Option<Instant>,
}

impl Default for CurlEvidenceRetriever {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("curl"),
            deadline: None,
        }
    }
}

impl CurlEvidenceRetriever {
    #[must_use]
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            deadline: None,
        }
    }

    #[must_use]
    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            executable: PathBuf::from("curl"),
            deadline: Some(deadline),
        }
    }
}

impl EvidenceRetriever for CurlEvidenceRetriever {
    fn retrieve(&mut self, url: &str) -> Result<RetrievedEvidenceDocument, EvidenceError> {
        if !url.starts_with("https://") {
            return Err(EvidenceError::RedirectOutOfPolicy);
        }
        let temporary = FetchRun::create()?;
        let body_path = temporary.path.join("body");
        let header_path = temporary.path.join("headers");
        let max_time = self.deadline.map_or(15, |deadline| {
            let remaining = deadline.saturating_duration_since(Instant::now());
            u64::from(remaining.subsec_nanos() > 0)
                .saturating_add(remaining.as_secs())
                .min(15)
        });
        if max_time == 0 {
            return Err(EvidenceError::Timeout);
        }
        let output = Command::new(&self.executable)
            .args([
                "--silent",
                "--show-error",
                "--fail-with-body",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "3",
                "--connect-timeout",
                "5",
                "--max-time",
            ])
            .arg(max_time.to_string())
            .args(["--max-filesize", "1048576", "--compressed", "--output"])
            .arg(&body_path)
            .arg("--dump-header")
            .arg(&header_path)
            .args([
                "--write-out",
                "%{url_effective}\n%{content_type}\n%{num_redirects}\n%{size_download}\n",
                url,
            ])
            .output()
            .map_err(|error| EvidenceError::Unreachable(error.to_string()))?;
        if !output.status.success() {
            return Err(EvidenceError::Unreachable(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        let metadata = String::from_utf8(output.stdout).map_err(|_| EvidenceError::InvalidText)?;
        let mut metadata = metadata.lines();
        let final_url = metadata
            .next()
            .ok_or_else(|| EvidenceError::Unreachable("curl omitted final URL".into()))?
            .to_owned();
        let media_type = metadata
            .next()
            .and_then(|value| value.split(';').next())
            .unwrap_or_default();
        let redirects = metadata
            .next()
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or_else(|| EvidenceError::Unreachable("curl omitted redirect count".into()))?;
        let body =
            fs::read(&body_path).map_err(|error| EvidenceError::Unreachable(error.to_string()))?;
        if body.len() > MAX_FETCHED_BYTES {
            return Err(EvidenceError::Oversized);
        }
        let headers = fs::read_to_string(&header_path).unwrap_or_default();
        let compressed_bytes = compressed_length(&headers);
        let content_type = match media_type {
            "text/plain" => EvidenceContentType::PlainText,
            "text/html" | "application/xhtml+xml" => EvidenceContentType::Html,
            "application/pdf" => EvidenceContentType::Pdf,
            _ => return Err(EvidenceError::UnsupportedContent),
        };
        let extracted_text = (content_type == EvidenceContentType::Pdf)
            .then(|| extract_pdf_text(&body_path))
            .flatten();
        let limitation = (content_type == EvidenceContentType::Pdf && extracted_text.is_none())
            .then(|| "PDF text extraction is unavailable on this device".to_owned());
        Ok(RetrievedEvidenceDocument {
            requested_url: url.to_owned(),
            final_url,
            content_type,
            body,
            compressed_bytes,
            redirects,
            extracted_text,
            limitation,
        })
    }
}

struct FetchRun {
    path: PathBuf,
}

impl FetchRun {
    fn create() -> Result<Self, EvidenceError> {
        let path = std::env::temp_dir().join(format!(
            "chemspec-evidence-{}-{}",
            std::process::id(),
            FETCH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|error| EvidenceError::Unreachable(error.to_string()))?;
        Ok(Self { path })
    }
}

impl Drop for FetchRun {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn compressed_length(headers: &str) -> Option<usize> {
    let mut encoded = false;
    let mut length = None;
    for line in headers.lines().rev() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("http/") && (encoded || length.is_some()) {
            break;
        }
        if let Some(value) = lower.strip_prefix("content-encoding:") {
            encoded = value.trim() != "identity";
        }
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().ok();
        }
    }
    encoded.then_some(length).flatten()
}

fn extract_pdf_text(path: &std::path::Path) -> Option<String> {
    let output = Command::new("pdftotext")
        .args(["-q", "-nopgbrk"])
        .arg(path)
        .arg("-")
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > MAX_FETCHED_BYTES {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceQuality {
    Government,
    Academic,
    StandardsBody,
    Institutional,
    General,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedEvidenceSource {
    pub id: String,
    pub requested_url: String,
    pub final_url: String,
    pub content_digest: ContentDigest,
    pub excerpt_digest: ContentDigest,
    pub quality: SourceQuality,
    pub supports: Vec<ClaimField>,
    pub limitation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSnapshot {
    pub schema_version: u32,
    pub claim_digest: ContentDigest,
    pub sources: Vec<VerifiedEvidenceSource>,
    pub coverage: BTreeMap<ClaimField, Vec<String>>,
    pub digest: ContentDigest,
}

#[derive(Debug, Clone)]
pub struct EvidenceBackedOutcome {
    outcome: ValidatedStaticOutcome,
    snapshot: EvidenceSnapshot,
}

impl EvidenceBackedOutcome {
    #[must_use]
    pub const fn outcome(&self) -> &ValidatedStaticOutcome {
        &self.outcome
    }

    #[must_use]
    pub const fn snapshot(&self) -> &EvidenceSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn into_outcome(self) -> ValidatedStaticOutcome {
        self.outcome
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    Unreachable(String),
    Timeout,
    RedirectOutOfPolicy,
    Oversized,
    DecompressionBomb,
    UnsupportedContent,
    InvalidText,
    ExcerptMismatch(String),
    ClaimMismatch,
    Conflict(String),
    IncompleteCoverage(ClaimField),
    InvalidSnapshot(String),
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(message) => write!(formatter, "source unreachable: {message}"),
            Self::Timeout => formatter.write_str("evidence retrieval exceeded its total deadline"),
            Self::RedirectOutOfPolicy => formatter.write_str("redirect left the HTTPS source host"),
            Self::Oversized => formatter.write_str("source exceeded the fetched-byte limit"),
            Self::DecompressionBomb => {
                formatter.write_str("source exceeded the decompression ratio")
            }
            Self::UnsupportedContent => formatter.write_str("source content type is unsupported"),
            Self::InvalidText => formatter.write_str("source text is not valid UTF-8"),
            Self::ExcerptMismatch(id) => write!(
                formatter,
                "source `{id}` excerpt was not found in fetched bytes"
            ),
            Self::ClaimMismatch => {
                formatter.write_str("source-locating response changed the factual claim")
            }
            Self::Conflict(id) => {
                write!(formatter, "source `{id}` conflicts with the selected claim")
            }
            Self::IncompleteCoverage(field) => {
                write!(formatter, "no fetched source covers `{field:?}`")
            }
            Self::InvalidSnapshot(message) => {
                write!(formatter, "invalid evidence snapshot: {message}")
            }
        }
    }
}

impl std::error::Error for EvidenceError {}

/// Ensures a source-locating response changed only source metadata.
///
/// # Errors
///
/// Returns `ClaimMismatch` when products, observations, disposition, context,
/// or ambiguity changed.
pub fn bind_source_locating_claim(
    original: &ReactionClaim,
    located: ReactionClaim,
) -> Result<ReactionClaim, EvidenceError> {
    if original.schema_version != located.schema_version
        || original.disposition != located.disposition
        || original.products != located.products
        || original.required_context != located.required_context
        || original.observations != located.observations
        || original.ambiguity != located.ambiguity
    {
        return Err(EvidenceError::ClaimMismatch);
    }
    Ok(located)
}

/// Fetches every cited source, verifies each excerpt against fetched text, and
/// upgrades a static outcome only after complete field-level coverage.
///
/// # Errors
///
/// Returns typed hostile-data, mismatch, conflict, or coverage failures. The
/// input outcome is consumed only on success; callers retain a clone while the
/// optional verification task runs.
pub fn verify_evidence<R: EvidenceRetriever>(
    outcome: ValidatedStaticOutcome,
    retriever: &mut R,
) -> Result<EvidenceBackedOutcome, EvidenceError> {
    let claim = outcome.claim();
    let mut sources = Vec::new();
    let mut coverage = BTreeMap::<ClaimField, Vec<String>>::new();
    for source in &claim.sources {
        let document = retriever.retrieve(&source.url)?;
        validate_transport(&source.url, &document)?;
        let text = extract_text(&document)?;
        let normalized_text = normalize_text(&text);
        let normalized_excerpt = normalize_text(&source.supporting_excerpt);
        if normalized_excerpt.is_empty() || !normalized_text.contains(&normalized_excerpt) {
            return Err(EvidenceError::ExcerptMismatch(source.id.clone()));
        }
        if explicit_conflict(&normalized_excerpt) {
            return Err(EvidenceError::Conflict(source.id.clone()));
        }
        adjudicate_supporting_region(claim, source, &normalized_excerpt)?;
        for field in &source.supports {
            coverage.entry(*field).or_default().push(source.id.clone());
        }
        sources.push(VerifiedEvidenceSource {
            id: source.id.clone(),
            requested_url: document.requested_url,
            final_url: document.final_url.clone(),
            content_digest: ContentDigest::sha256(&document.body),
            excerpt_digest: ContentDigest::sha256(normalized_excerpt.as_bytes()),
            quality: classify_source_quality(&source.publisher, &document.final_url),
            supports: source.supports.clone(),
            limitation: document.limitation,
        });
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    for ids in coverage.values_mut() {
        ids.sort();
        ids.dedup();
    }
    for field in required_fields(claim) {
        if !coverage.contains_key(&field) {
            return Err(EvidenceError::IncompleteCoverage(field));
        }
    }
    let claim_digest = digest_json(claim)?;
    let mut snapshot = EvidenceSnapshot {
        schema_version: 1,
        claim_digest,
        sources,
        coverage,
        digest: ContentDigest::sha256(b"pending evidence snapshot"),
    };
    snapshot.digest = digest_json(&snapshot_without_digest(&snapshot))?;
    Ok(EvidenceBackedOutcome {
        outcome: outcome.mark_evidence_backed(),
        snapshot,
    })
}

/// Restores an evidence-backed capability from a digest-bound offline
/// snapshot without requiring the source to remain online.
///
/// # Errors
///
/// Returns an error when the claim, snapshot digest, source IDs, or field
/// coverage no longer match.
pub fn restore_evidence_backed(
    outcome: ValidatedStaticOutcome,
    snapshot: &EvidenceSnapshot,
) -> Result<ValidatedStaticOutcome, EvidenceError> {
    if snapshot.schema_version != 1 || snapshot.claim_digest != digest_json(outcome.claim())? {
        return Err(EvidenceError::InvalidSnapshot(
            "claim binding or schema changed".into(),
        ));
    }
    if snapshot.digest != digest_json(&snapshot_without_digest(snapshot))? {
        return Err(EvidenceError::InvalidSnapshot(
            "snapshot digest changed".into(),
        ));
    }
    for field in required_fields(outcome.claim()) {
        let Some(ids) = snapshot.coverage.get(&field) else {
            return Err(EvidenceError::IncompleteCoverage(field));
        };
        if ids.is_empty()
            || ids.iter().any(|id| {
                !snapshot
                    .sources
                    .iter()
                    .any(|source| &source.id == id && source.supports.contains(&field))
            })
        {
            return Err(EvidenceError::InvalidSnapshot(
                "coverage references an absent source".into(),
            ));
        }
    }
    Ok(outcome.mark_evidence_backed())
}

fn validate_transport(
    requested_url: &str,
    document: &RetrievedEvidenceDocument,
) -> Result<(), EvidenceError> {
    if requested_url != document.requested_url
        || !requested_url.starts_with("https://")
        || !document.final_url.starts_with("https://")
        || document.redirects > MAX_REDIRECTS
        || normalized_host(requested_url) != normalized_host(&document.final_url)
    {
        return Err(EvidenceError::RedirectOutOfPolicy);
    }
    if document.body.len() > MAX_FETCHED_BYTES {
        return Err(EvidenceError::Oversized);
    }
    if document.compressed_bytes.is_some_and(|compressed| {
        compressed > 0 && document.body.len() / compressed > MAX_DECOMPRESSION_RATIO
    }) {
        return Err(EvidenceError::DecompressionBomb);
    }
    Ok(())
}

fn extract_text(document: &RetrievedEvidenceDocument) -> Result<String, EvidenceError> {
    match document.content_type {
        EvidenceContentType::PlainText => {
            String::from_utf8(document.body.clone()).map_err(|_| EvidenceError::InvalidText)
        }
        EvidenceContentType::Html => {
            let html =
                std::str::from_utf8(&document.body).map_err(|_| EvidenceError::InvalidText)?;
            Ok(strip_html(html))
        }
        EvidenceContentType::Pdf => document
            .extracted_text
            .clone()
            .ok_or(EvidenceError::UnsupportedContent),
    }
}

fn strip_html(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                text.push(' ');
            }
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn explicit_conflict(excerpt: &str) -> bool {
    ["does not react", "no reaction occurs", "products are not"]
        .iter()
        .any(|marker| excerpt.contains(marker))
}

/// Conservative, non-browsing adjudication over the already-fetched and
/// excerpt-matched passage. This cannot confer trust by itself: it only
/// rejects source/field mappings that do not mention the claimed product or
/// observation vocabulary they purport to support.
fn adjudicate_supporting_region(
    claim: &ReactionClaim,
    source: &crate::ClaimSource,
    excerpt: &str,
) -> Result<(), EvidenceError> {
    if source.supports.contains(&ClaimField::Products)
        && !claim.products.iter().any(|product| {
            excerpt.contains(&normalize_text(&product.name))
                || excerpt.contains(&normalize_text(&product.formula))
        })
    {
        return Err(EvidenceError::ExcerptMismatch(source.id.clone()));
    }
    if source.supports.contains(&ClaimField::Observations)
        && !claim.observations.iter().any(|observation| {
            excerpt.contains(&normalize_text(&observation.subject))
                || observation
                    .value
                    .as_ref()
                    .is_some_and(|value| excerpt.contains(&normalize_text(value)))
        })
    {
        return Err(EvidenceError::ExcerptMismatch(source.id.clone()));
    }
    Ok(())
}

fn required_fields(claim: &ReactionClaim) -> Vec<ClaimField> {
    match claim.disposition {
        ClaimDisposition::Reaction => {
            let mut fields = vec![ClaimField::Products, ClaimField::RequiredContext];
            if !claim.observations.is_empty() {
                fields.push(ClaimField::Observations);
            }
            fields
        }
        ClaimDisposition::NoReaction => vec![ClaimField::NoReaction, ClaimField::RequiredContext],
        ClaimDisposition::Ambiguous | ClaimDisposition::Unsupported => Vec::new(),
    }
}

fn normalized_host(url: &str) -> Option<&str> {
    let host = url
        .strip_prefix("https://")?
        .split(['/', '?', '#'])
        .next()?;
    Some(host.strip_prefix("www.").unwrap_or(host))
}

fn classify_source_quality(publisher: &str, url: &str) -> SourceQuality {
    let host = normalized_host(url)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let publisher = publisher.to_ascii_lowercase();
    if host.strip_suffix(".gov").is_some() || host.strip_suffix(".gov.uk").is_some() {
        SourceQuality::Government
    } else if host.strip_suffix(".edu").is_some() || host.strip_suffix(".ac.uk").is_some() {
        SourceQuality::Academic
    } else if publisher.contains("iupac") || publisher.contains("standards") {
        SourceQuality::StandardsBody
    } else if publisher.contains("university")
        || publisher.contains("institute")
        || publisher.contains("society")
    {
        SourceQuality::Institutional
    } else if !publisher.trim().is_empty() {
        SourceQuality::General
    } else {
        SourceQuality::Unknown
    }
}

#[derive(Serialize)]
struct SnapshotDigest<'a> {
    schema_version: u32,
    claim_digest: ContentDigest,
    sources: &'a [VerifiedEvidenceSource],
    coverage: &'a BTreeMap<ClaimField, Vec<String>>,
}

fn snapshot_without_digest(snapshot: &EvidenceSnapshot) -> SnapshotDigest<'_> {
    SnapshotDigest {
        schema_version: snapshot.schema_version,
        claim_digest: snapshot.claim_digest,
        sources: &snapshot.sources,
        coverage: &snapshot.coverage,
    }
}

fn digest_json(value: &impl Serialize) -> Result<ContentDigest, EvidenceError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| EvidenceError::InvalidSnapshot(error.to_string()))?;
    Ok(ContentDigest::sha256(&bytes))
}

#[cfg(test)]
mod tests {
    use chem_catalogue::TrustedCatalogue;
    use serde_json::json;

    use super::*;
    use crate::{
        ClaimMode, CompiledClaimOutcome, ReactantInput, ReactionBuildRequest,
        compile_claim_outcome, reviewed_species_registry,
    };

    struct FakeRetriever {
        document: Option<RetrievedEvidenceDocument>,
        error: Option<EvidenceError>,
    }

    impl EvidenceRetriever for FakeRetriever {
        fn retrieve(&mut self, _url: &str) -> Result<RetrievedEvidenceDocument, EvidenceError> {
            if let Some(error) = self.error.take() {
                Err(error)
            } else {
                self.document
                    .take()
                    .ok_or_else(|| EvidenceError::Unreachable("missing fixture".into()))
            }
        }
    }

    fn trusted() -> TrustedCatalogue {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        TrustedCatalogue::from_canonical_json(
            &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
                .expect("catalogue"),
            &std::fs::read(root.join("catalogue/trusted/core-chemistry/review.json"))
                .expect("review"),
        )
        .expect("trusted")
    }

    fn claim(context: &str) -> ReactionClaim {
        let value = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context": context,
            "observations": [],
            "sources": [{
                "id":"s1",
                "title":"Reviewed outcome",
                "publisher":"Example University",
                "url":"https://example.edu/reaction",
                "supporting_excerpt":"Lithium and water form lithium hydroxide and hydrogen.",
                "supports":["products","required_context"]
            }],
            "ambiguity": null
        });
        ReactionClaim::from_json(
            &serde_json::to_vec(&value).expect("claim JSON"),
            ClaimMode::Researcher,
        )
        .expect("claim")
    }

    fn outcome() -> ValidatedStaticOutcome {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "LithiumMetal".into(),
                    atomic_numbers: vec![3],
                    species_id: None,
                },
                ReactantInput {
                    display: "H2O".into(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let CompiledClaimOutcome::Static(outcome) = compile_claim_outcome(
            &request,
            claim("representative educational outcome under the reviewed standard-outcome premise"),
            &identities,
        )
        .expect("compiled") else {
            panic!("static")
        };
        outcome
    }

    fn document() -> RetrievedEvidenceDocument {
        RetrievedEvidenceDocument {
            requested_url: "https://example.edu/reaction".into(),
            final_url: "https://www.example.edu/reaction/".into(),
            content_type: EvidenceContentType::Html,
            body: b"<html><p>Lithium and water form lithium hydroxide and hydrogen.</p></html>"
                .to_vec(),
            compressed_bytes: None,
            redirects: 1,
            extracted_text: None,
            limitation: None,
        }
    }

    #[test]
    fn fetched_claim_coverage_upgrades_only_after_exact_excerpt_match() {
        let mut retriever = FakeRetriever {
            document: Some(document()),
            error: None,
        };
        let verified = verify_evidence(outcome(), &mut retriever).expect("verified");
        assert_eq!(
            verified.outcome().trust_tier(),
            crate::TrustTier::EvidenceBacked
        );
        assert!(
            verified
                .snapshot()
                .coverage
                .contains_key(&ClaimField::Products)
        );
        assert_eq!(
            verified.snapshot().sources[0].quality,
            SourceQuality::Academic
        );
    }

    #[test]
    fn hostile_retrieval_and_claim_mutation_fail_closed() {
        let original =
            claim("representative educational outcome under the reviewed standard-outcome premise");
        let changed = claim("a changed context");
        assert_eq!(
            bind_source_locating_claim(&original, changed),
            Err(EvidenceError::ClaimMismatch)
        );

        let cases = [
            {
                let mut value = document();
                value.final_url = "https://attacker.invalid/reaction".into();
                (value, EvidenceError::RedirectOutOfPolicy)
            },
            {
                let mut value = document();
                value.body = vec![b'x'; MAX_FETCHED_BYTES + 1];
                (value, EvidenceError::Oversized)
            },
            {
                let mut value = document();
                value.body.extend(std::iter::repeat_n(b' ', 10_000));
                value.compressed_bytes = Some(1);
                (value, EvidenceError::DecompressionBomb)
            },
            {
                let mut value = document();
                value.content_type = EvidenceContentType::Pdf;
                value.extracted_text = None;
                (value, EvidenceError::UnsupportedContent)
            },
            {
                let mut value = document();
                value.body = b"<p>Unrelated text.</p>".to_vec();
                (value, EvidenceError::ExcerptMismatch("s1".into()))
            },
        ];
        for (document, expected) in cases {
            let mut retriever = FakeRetriever {
                document: Some(document),
                error: None,
            };
            assert_eq!(
                verify_evidence(outcome(), &mut retriever).expect_err("must fail closed"),
                expected
            );
        }

        let mut retriever = FakeRetriever {
            document: None,
            error: Some(EvidenceError::Unreachable("offline".into())),
        };
        assert!(matches!(
            verify_evidence(outcome(), &mut retriever),
            Err(EvidenceError::Unreachable(_))
        ));

        let mut expired = CurlEvidenceRetriever::with_deadline(
            Instant::now()
                .checked_sub(std::time::Duration::from_secs(1))
                .expect("one-second earlier instant"),
        );
        assert_eq!(
            expired.retrieve("https://example.edu/reaction"),
            Err(EvidenceError::Timeout)
        );
    }
}
