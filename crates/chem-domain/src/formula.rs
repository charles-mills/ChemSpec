use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use num_bigint::{BigInt, BigUint};
use num_traits::{One, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ElementSymbol(String);

impl ElementSymbol {
    /// Constructs a syntactically valid ASCII element symbol.
    ///
    /// # Errors
    ///
    /// Returns [`FormulaError::InvalidElementSymbol`] unless `source` is one
    /// uppercase ASCII letter followed by at most one lowercase ASCII letter.
    pub fn new(source: impl Into<String>) -> Result<Self, FormulaError> {
        let source = source.into();
        let bytes = source.as_bytes();
        if !(bytes.len() == 1 || bytes.len() == 2)
            || !bytes[0].is_ascii_uppercase()
            || bytes.get(1).is_some_and(|byte| !byte.is_ascii_lowercase())
        {
            return Err(FormulaError::InvalidElementSymbol(source));
        }
        Ok(Self(source))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ElementSymbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ElementSymbol {
    type Err = FormulaError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::new(source)
    }
}

impl<'de> Deserialize<'de> for ElementSymbol {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let source = String::deserialize(deserializer)?;
        Self::new(source).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ElementId(u16);

impl ElementId {
    /// Constructs an element identity from a nonzero atomic number.
    ///
    /// # Errors
    ///
    /// Returns [`FormulaError::InvalidAtomicNumber`] for zero.
    pub const fn new(atomic_number: u16) -> Result<Self, FormulaError> {
        if atomic_number == 0 {
            Err(FormulaError::InvalidAtomicNumber)
        } else {
            Ok(Self(atomic_number))
        }
    }

    #[must_use]
    pub const fn atomic_number(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for ElementId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let atomic_number = u16::deserialize(deserializer)?;
        Self::new(atomic_number).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: ElementId,
    pub symbol: ElementSymbol,
}

pub trait ElementRegistry {
    fn resolve(&self, symbol: &ElementSymbol) -> Option<&Element>;
}

#[derive(Debug, Clone)]
pub struct StaticElementRegistry {
    by_symbol: BTreeMap<ElementSymbol, Element>,
}

impl StaticElementRegistry {
    /// Constructs a deterministic registry and rejects conflicting identities.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate symbols or atomic numbers.
    pub fn new(elements: impl IntoIterator<Item = Element>) -> Result<Self, FormulaError> {
        let mut by_symbol = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for element in elements {
            if by_symbol.contains_key(&element.symbol) {
                return Err(FormulaError::DuplicateElementSymbol(element.symbol));
            }
            if !ids.insert(element.id) {
                return Err(FormulaError::DuplicateElementId(element.id));
            }
            by_symbol.insert(element.symbol.clone(), element);
        }
        Ok(Self { by_symbol })
    }
}

impl ElementRegistry for StaticElementRegistry {
    fn resolve(&self, symbol: &ElementSymbol) -> Option<&Element> {
        self.by_symbol.get(symbol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Count(BigUint);

impl Count {
    /// Constructs a positive arbitrary-precision formula count.
    ///
    /// # Errors
    ///
    /// Returns [`FormulaError::ZeroCount`] for zero.
    pub fn new(value: BigUint) -> Result<Self, FormulaError> {
        if value.is_zero() {
            Err(FormulaError::ZeroCount)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub fn one() -> Self {
        Self(BigUint::one())
    }

    #[must_use]
    pub fn value(&self) -> &BigUint {
        &self.0
    }
}

impl Serialize for Count {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Count {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let value = BigUint::from_str(&value).map_err(serde::de::Error::custom)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FormulaPart {
    Element {
        symbol: ElementSymbol,
        count: Count,
    },
    Group {
        parts: Vec<FormulaPart>,
        count: Count,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaSegment {
    pub coefficient: Count,
    pub parts: Vec<FormulaPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaSyntax {
    pub segments: Vec<FormulaSegment>,
}

impl FormulaSyntax {
    /// Resolves elements and recursively normalizes groups and adduct segments.
    ///
    /// # Errors
    ///
    /// Returns an error for empty structure or an element absent from the
    /// supplied registry.
    pub fn normalize<R: ElementRegistry>(
        &self,
        registry: &R,
    ) -> Result<NormalizedFormula, FormulaError> {
        if self.segments.is_empty() {
            return Err(FormulaError::EmptyFormula);
        }
        let mut composition = BTreeMap::<ElementId, BigUint>::new();
        for segment in &self.segments {
            if segment.parts.is_empty() {
                return Err(FormulaError::EmptySegment);
            }
            normalize_parts(
                &segment.parts,
                segment.coefficient.value(),
                registry,
                &mut composition,
            )?;
        }
        Ok(NormalizedFormula {
            composition,
            syntax: self.clone(),
        })
    }
}

fn normalize_parts<R: ElementRegistry>(
    parts: &[FormulaPart],
    multiplier: &BigUint,
    registry: &R,
    composition: &mut BTreeMap<ElementId, BigUint>,
) -> Result<(), FormulaError> {
    if parts.is_empty() {
        return Err(FormulaError::EmptyGroup);
    }
    for part in parts {
        match part {
            FormulaPart::Element { symbol, count } => {
                let element = registry
                    .resolve(symbol)
                    .ok_or_else(|| FormulaError::UnknownElement(symbol.clone()))?;
                let contribution = multiplier * count.value();
                *composition.entry(element.id).or_default() += contribution;
            }
            FormulaPart::Group { parts, count } => {
                let nested_multiplier = multiplier * count.value();
                normalize_parts(parts, &nested_multiplier, registry, composition)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NormalizedFormula {
    composition: BTreeMap<ElementId, BigUint>,
    syntax: FormulaSyntax,
}

impl NormalizedFormula {
    #[must_use]
    pub const fn composition(&self) -> &BTreeMap<ElementId, BigUint> {
        &self.composition
    }

    #[must_use]
    pub const fn syntax(&self) -> &FormulaSyntax {
        &self.syntax
    }

    #[must_use]
    pub fn source_eq(&self, other: &Self) -> bool {
        self.syntax == other.syntax
    }
}

impl PartialEq for NormalizedFormula {
    fn eq(&self, other: &Self) -> bool {
        self.composition == other.composition
    }
}

impl Eq for NormalizedFormula {}

impl Serialize for NormalizedFormula {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct CompositionEntry {
            element: ElementId,
            count: String,
        }

        let composition = self
            .composition
            .iter()
            .map(|(element, count)| CompositionEntry {
                element: *element,
                count: count.to_string(),
            })
            .collect::<Vec<_>>();
        let mut state = serializer.serialize_struct("NormalizedFormula", 2)?;
        state.serialize_field("composition", &composition)?;
        state.serialize_field("syntax", &self.syntax)?;
        state.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaError {
    InvalidElementSymbol(String),
    InvalidAtomicNumber,
    DuplicateElementSymbol(ElementSymbol),
    DuplicateElementId(ElementId),
    ZeroCount,
    EmptyFormula,
    EmptySegment,
    EmptyGroup,
    UnknownElement(ElementSymbol),
    ZeroChargeMagnitude,
}

impl fmt::Display for FormulaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidElementSymbol(symbol) => {
                write!(formatter, "invalid element symbol `{symbol}`")
            }
            Self::InvalidAtomicNumber => {
                formatter.write_str("an element atomic number cannot be zero")
            }
            Self::DuplicateElementSymbol(symbol) => {
                write!(formatter, "duplicate element symbol `{symbol}`")
            }
            Self::DuplicateElementId(id) => {
                write!(formatter, "duplicate atomic number {}", id.atomic_number())
            }
            Self::ZeroCount => formatter.write_str("formula counts must be positive"),
            Self::EmptyFormula => {
                formatter.write_str("a formula must contain at least one segment")
            }
            Self::EmptySegment => {
                formatter.write_str("a formula segment must contain at least one part")
            }
            Self::EmptyGroup => {
                formatter.write_str("a formula group must contain at least one part")
            }
            Self::UnknownElement(symbol) => write!(formatter, "unknown element symbol `{symbol}`"),
            Self::ZeroChargeMagnitude => {
                formatter.write_str("a non-neutral charge magnitude must be positive")
            }
        }
    }
}

impl std::error::Error for FormulaError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChargeSign {
    Positive,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Charge(BigInt);

impl Charge {
    #[must_use]
    pub fn neutral() -> Self {
        Self(BigInt::ZERO)
    }

    /// Constructs a non-neutral integral charge from a positive magnitude.
    ///
    /// # Errors
    ///
    /// Returns [`FormulaError::ZeroChargeMagnitude`] when magnitude is zero.
    pub fn from_magnitude(magnitude: BigUint, sign: ChargeSign) -> Result<Self, FormulaError> {
        if magnitude.is_zero() {
            return Err(FormulaError::ZeroChargeMagnitude);
        }
        let magnitude = BigInt::from(magnitude);
        Ok(Self(match sign {
            ChargeSign::Positive => magnitude,
            ChargeSign::Negative => -magnitude,
        }))
    }

    #[must_use]
    pub fn value(&self) -> &BigInt {
        &self.0
    }
}

impl Serialize for Charge {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Charge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        BigInt::from_str(&value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    Aqueous,
    Solid,
    Liquid,
    Gas,
}
