use std::{fmt, marker::PhantomData, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{CanonicalJsonError, canonical_json, lowercase_hex, sha256};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(sha256(bytes))
    }

    /// Hashes the canonical representation of a JSON value.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalJsonError`] when the value contains a floating-point
    /// number.
    pub fn of_json(value: &Value) -> Result<Self, CanonicalJsonError> {
        Ok(Self::sha256(&canonical_json(value)?))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        lowercase_hex(&self.0)
    }
}

impl fmt::Debug for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&lowercase_hex(&self.0))
    }
}

impl Serialize for ContentDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ContentDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let source = String::deserialize(deserializer)?;
        Self::from_str(&source).map_err(serde::de::Error::custom)
    }
}

impl FromStr for ContentDigest {
    type Err = IdError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        if source.len() != 64
            || !source
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(IdError::InvalidDigest);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in source.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, IdError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(IdError::InvalidDigest),
    }
}

pub trait IdKind {
    const NAME: &'static str;
}

pub struct DigestId<K: IdKind> {
    digest: ContentDigest,
    marker: PhantomData<fn() -> K>,
}

impl<K: IdKind> DigestId<K> {
    #[must_use]
    pub const fn from_digest(digest: ContentDigest) -> Self {
        Self {
            digest,
            marker: PhantomData,
        }
    }

    /// Constructs a typed content ID from canonical JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalJsonError`] if the value contains floating point.
    pub fn of_json(value: &Value) -> Result<Self, CanonicalJsonError> {
        Ok(Self::from_digest(ContentDigest::of_json(value)?))
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

impl<K: IdKind> Clone for DigestId<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: IdKind> Copy for DigestId<K> {}

impl<K: IdKind> PartialEq for DigestId<K> {
    fn eq(&self, other: &Self) -> bool {
        self.digest == other.digest
    }
}

impl<K: IdKind> Eq for DigestId<K> {}

impl<K: IdKind> std::hash::Hash for DigestId<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.digest.hash(state);
    }
}

impl<K: IdKind> fmt::Debug for DigestId<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}({})", K::NAME, self.digest)
    }
}

impl<K: IdKind> fmt::Display for DigestId<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.digest.fmt(formatter)
    }
}

impl<K: IdKind> Serialize for DigestId<K> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.digest.serialize(serializer)
    }
}

impl<'de, K: IdKind> Deserialize<'de> for DigestId<K> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        ContentDigest::deserialize(deserializer).map(Self::from_digest)
    }
}

pub struct DeclaredId<K: IdKind> {
    value: String,
    marker: PhantomData<fn() -> K>,
}

impl<K: IdKind> DeclaredId<K> {
    /// Constructs a typed catalogue-declared identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::InvalidDeclaredId`] unless the value is a nonempty
    /// ASCII identifier containing only alphanumerics, `.`, `_`, `:`, or `-`.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty()
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err(IdError::InvalidDeclaredId(value));
        }
        Ok(Self {
            value,
            marker: PhantomData,
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<K: IdKind> Clone for DeclaredId<K> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            marker: PhantomData,
        }
    }
}

impl<K: IdKind> PartialEq for DeclaredId<K> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<K: IdKind> Eq for DeclaredId<K> {}

impl<K: IdKind> PartialOrd for DeclaredId<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: IdKind> Ord for DeclaredId<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<K: IdKind> std::hash::Hash for DeclaredId<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<K: IdKind> fmt::Debug for DeclaredId<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}({})", K::NAME, self.value)
    }
}

impl<K: IdKind> fmt::Display for DeclaredId<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
    }
}

impl<K: IdKind> Serialize for DeclaredId<K> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.value)
    }
}

impl<'de, K: IdKind> Deserialize<'de> for DeclaredId<K> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl<K: IdKind> FromStr for DeclaredId<K> {
    type Err = IdError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::new(source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    InvalidDigest,
    InvalidDeclaredId(String),
}

impl fmt::Display for IdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDigest => {
                formatter.write_str("a content digest must be 64 lowercase hexadecimal characters")
            }
            Self::InvalidDeclaredId(value) => {
                write!(formatter, "invalid declared identifier `{value}`")
            }
        }
    }
}

impl std::error::Error for IdError {}

macro_rules! digest_id_kind {
    ($kind:ident, $alias:ident, $name:literal) => {
        #[derive(Debug)]
        pub enum $kind {}

        impl IdKind for $kind {
            const NAME: &'static str = $name;
        }

        pub type $alias = DigestId<$kind>;
    };
}

macro_rules! declared_id_kind {
    ($kind:ident, $alias:ident, $name:literal) => {
        #[derive(Debug)]
        pub enum $kind {}

        impl IdKind for $kind {
            const NAME: &'static str = $name;
        }

        pub type $alias = DeclaredId<$kind>;
    };
}

digest_id_kind!(ExperimentKind, ExperimentId, "ExperimentId");
digest_id_kind!(MaterialKind, MaterialId, "MaterialId");
digest_id_kind!(VesselKind, VesselId, "VesselId");
digest_id_kind!(OperationKind, OperationId, "OperationId");
digest_id_kind!(StageKind, StageId, "StageId");
digest_id_kind!(HoleKind, HoleId, "HoleId");
digest_id_kind!(GoalKind, GoalId, "GoalId");
digest_id_kind!(ReactionEventKind, ReactionEventId, "ReactionEventId");
digest_id_kind!(DerivationNodeKind, DerivationNodeId, "DerivationNodeId");
declared_id_kind!(FactKind, FactId, "FactId");
declared_id_kind!(SubstanceKind, SubstanceId, "SubstanceId");
declared_id_kind!(SpeciesKind, SpeciesId, "SpeciesId");
declared_id_kind!(MediumKind, MediumId, "MediumId");
declared_id_kind!(EvidenceSourceKind, EvidenceSourceId, "EvidenceSourceId");
declared_id_kind!(AssumptionKindKind, AssumptionKindId, "AssumptionKindId");
declared_id_kind!(CoverageKind, CoverageId, "CoverageId");
