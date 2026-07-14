use std::{collections::BTreeMap, fmt};

use chem_domain::{ClaimId, ContentDigest, EvidenceSourceId, canonical_json};
use serde::{Deserialize, Serialize};

/// Closed observation predicate domain in an external evidence packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePredicate {
    Evolves,
    Disappears,
    Forms,
    Colour,
}

/// Strict wire form for one externally researched observation claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceClaimRecord {
    pub id: ClaimId,
    pub subject_role: String,
    pub subject: String,
    pub predicate: EvidencePredicate,
    pub sources: Vec<EvidenceSourceId>,
}

/// Strict wire form for one source cited by an observation packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePacketSourceRecord {
    pub id: EvidenceSourceId,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub supports: Vec<ClaimId>,
}

/// Strict, immutable external evidence packet wire form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePacket {
    pub schema_version: u32,
    pub id: String,
    pub claims: Vec<EvidenceClaimRecord>,
    pub sources: Vec<EvidencePacketSourceRecord>,
}

/// Stable parsed packet identity selected by `observe from`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EvidencePacketReference {
    name: String,
    version: String,
}

impl EvidencePacketReference {
    /// Parses `Qualified.Name@version` without accepting empty segments.
    ///
    /// # Errors
    ///
    /// Returns an evidence error for malformed packet identity or version.
    pub fn parse(source: &str) -> Result<Self, EvidenceError> {
        let (name, version) = source.rsplit_once('@').ok_or_else(|| {
            EvidenceError::new("evidence packet ID must contain one `@` version separator")
        })?;
        if !valid_qualified_name(name) || !valid_version(version) || name.contains('@') {
            return Err(EvidenceError::new(format!(
                "invalid evidence packet identity `{source}`"
            )));
        }
        Ok(Self {
            name: name.to_owned(),
            version: version.to_owned(),
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// Validated, indexed and content-addressed evidence packet.
#[derive(Debug, Clone)]
pub struct ValidatedEvidencePacket {
    reference: EvidencePacketReference,
    claims: BTreeMap<ClaimId, EvidenceClaimRecord>,
    sources: BTreeMap<EvidenceSourceId, EvidencePacketSourceRecord>,
    digest: ContentDigest,
}

impl ValidatedEvidencePacket {
    /// Decodes and validates an untrusted external evidence packet.
    ///
    /// # Errors
    ///
    /// Rejects malformed JSON, duplicate IDs or references, empty metadata,
    /// unresolved citations, and non-reciprocal claim/source support.
    pub fn from_json(bytes: &[u8]) -> Result<Self, EvidenceError> {
        let packet: EvidencePacket = serde_json::from_slice(bytes)
            .map_err(|error| EvidenceError::new(format!("invalid evidence JSON: {error}")))?;
        Self::validate(packet)
    }

    /// Validates an already decoded untrusted packet.
    ///
    /// # Errors
    ///
    /// Returns an evidence error for any inconsistent packet content.
    #[allow(clippy::too_many_lines)]
    pub fn validate(mut packet: EvidencePacket) -> Result<Self, EvidenceError> {
        if packet.schema_version != 1 {
            return Err(EvidenceError::new(format!(
                "unsupported evidence schema {}",
                packet.schema_version
            )));
        }
        let reference = EvidencePacketReference::parse(&packet.id)?;
        if packet.claims.is_empty() || packet.sources.is_empty() {
            return Err(EvidenceError::new(
                "evidence packet must contain claims and sources",
            ));
        }
        packet.claims.sort_by(|left, right| left.id.cmp(&right.id));
        packet.sources.sort_by(|left, right| left.id.cmp(&right.id));

        let mut claims = BTreeMap::new();
        for mut claim in packet.claims {
            require_text(&claim.subject_role, "claim subject role")?;
            require_text(&claim.subject, "claim subject")?;
            sort_unique(&mut claim.sources, "claim source")?;
            if claim.sources.is_empty() {
                return Err(EvidenceError::new(format!(
                    "claim `{}` has no source",
                    claim.id
                )));
            }
            let id = claim.id.clone();
            if claims.insert(id.clone(), claim).is_some() {
                return Err(EvidenceError::new(format!("duplicate claim `{id}`")));
            }
        }

        let mut sources = BTreeMap::new();
        for mut source in packet.sources {
            for (field, value) in [
                ("source title", source.title.as_str()),
                ("source publisher", source.publisher.as_str()),
                ("source URL", source.url.as_str()),
            ] {
                require_text(value, field)?;
            }
            sort_unique(&mut source.supports, "supported claim")?;
            if source.supports.is_empty() {
                return Err(EvidenceError::new(format!(
                    "source `{}` supports no claim",
                    source.id
                )));
            }
            let id = source.id.clone();
            if sources.insert(id.clone(), source).is_some() {
                return Err(EvidenceError::new(format!(
                    "duplicate evidence source `{id}`"
                )));
            }
        }

        for claim in claims.values() {
            for source_id in &claim.sources {
                let source = sources.get(source_id).ok_or_else(|| {
                    EvidenceError::new(format!(
                        "claim `{}` cites unknown source `{source_id}`",
                        claim.id
                    ))
                })?;
                if !source.supports.contains(&claim.id) {
                    return Err(EvidenceError::new(format!(
                        "source `{source_id}` does not reciprocally support claim `{}`",
                        claim.id
                    )));
                }
            }
        }
        for source in sources.values() {
            for claim_id in &source.supports {
                let claim = claims.get(claim_id).ok_or_else(|| {
                    EvidenceError::new(format!(
                        "source `{}` supports unknown claim `{claim_id}`",
                        source.id
                    ))
                })?;
                if !claim.sources.contains(&source.id) {
                    return Err(EvidenceError::new(format!(
                        "claim `{claim_id}` does not reciprocally cite source `{}`",
                        source.id
                    )));
                }
            }
        }

        let normalized = EvidencePacket {
            schema_version: 1,
            id: reference.qualified(),
            claims: claims.values().cloned().collect(),
            sources: sources.values().cloned().collect(),
        };
        let value = serde_json::to_value(&normalized)
            .map_err(|error| EvidenceError::new(error.to_string()))?;
        let canonical =
            canonical_json(&value).map_err(|error| EvidenceError::new(error.to_string()))?;
        let digest = ContentDigest::sha256(&canonical);

        Ok(Self {
            reference,
            claims,
            sources,
            digest,
        })
    }

    #[must_use]
    pub const fn reference(&self) -> &EvidencePacketReference {
        &self.reference
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub fn claim(&self, id: &ClaimId) -> Option<&EvidenceClaimRecord> {
        self.claims.get(id)
    }

    #[must_use]
    pub const fn claims(&self) -> &BTreeMap<ClaimId, EvidenceClaimRecord> {
        &self.claims
    }

    #[must_use]
    pub const fn sources(&self) -> &BTreeMap<EvidenceSourceId, EvidencePacketSourceRecord> {
        &self.sources
    }
}

/// Invalid external evidence packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceError {
    message: String,
}

impl EvidenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EvidenceError {}

fn sort_unique<T: Ord + fmt::Display>(values: &mut [T], label: &str) -> Result<(), EvidenceError> {
    values.sort();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(EvidenceError::new(format!("duplicate {label} entry")));
    }
    Ok(())
}

fn require_text(value: &str, label: &str) -> Result<(), EvidenceError> {
    if value.trim().is_empty() {
        Err(EvidenceError::new(format!("{label} must not be empty")))
    } else {
        Ok(())
    }
}

fn valid_qualified_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'))
        })
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('.')
            .all(|segment| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_requires_qualified_name_and_numeric_version() {
        assert_eq!(
            EvidencePacketReference::parse("Evidence.Demo@1.2")
                .unwrap()
                .qualified(),
            "Evidence.Demo@1.2"
        );
        for invalid in [
            "Evidence.Demo",
            "@1",
            "Evidence..Demo@1",
            "Evidence.Demo@v1",
        ] {
            assert!(EvidencePacketReference::parse(invalid).is_err());
        }
        assert!(ClaimId::new("R1").is_ok());
    }
}
