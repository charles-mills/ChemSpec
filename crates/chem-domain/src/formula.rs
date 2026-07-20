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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    Aqueous,
    Solid,
    Liquid,
    Gas,
    Unknown,
}

/// One reviewed route through the sealed phase-synthesis chamber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseSynthesisRoute {
    SolidGas,
    GasGas,
}

/// Classifies a two-reactant synthesis of one gaseous product by reviewed
/// phases. This is the single shared core for the static catalogue classifier
/// and the dynamic outcome classifier, so the two paths cannot drift.
///
/// Hydrogen/oxygen burning is intentionally not represented by the
/// concentration-chamber synthesis asset; every other oxygen-containing
/// combination stays eligible once the more specific combustion and
/// surface-oxidation classifiers have declined.
#[must_use]
pub fn classify_phase_synthesis(
    first: (&[(&str, u64)], Phase),
    second: (&[(&str, u64)], Phase),
    product_phase: Phase,
) -> Option<PhaseSynthesisRoute> {
    let exactly = |formula: &[(&str, u64)], symbol: &str, count: u64| {
        formula.len() == 1 && formula[0] == (symbol, count)
    };
    let dioxygen = |formula| exactly(formula, "O", 2);
    let dihydrogen = |formula| exactly(formula, "H", 2);
    if (dioxygen(first.0) && dihydrogen(second.0)) || (dioxygen(second.0) && dihydrogen(first.0)) {
        return None;
    }
    if product_phase != Phase::Gas {
        return None;
    }
    match [first.1, second.1] {
        [Phase::Solid, Phase::Gas] | [Phase::Gas, Phase::Solid] => {
            Some(PhaseSynthesisRoute::SolidGas)
        }
        [Phase::Gas, Phase::Gas] => Some(PhaseSynthesisRoute::GasGas),
        _ => None,
    }
}

/// IUPAC element sequence for formula writing (Red Book Table VI), most
/// electropositive first: the earlier element is written first (`NaCl`,
/// `SO2`, `Cl2O`, `OF2`). Symbols outside the sequence sort after it,
/// alphabetically.
const ELECTROPOSITIVE_SEQUENCE: [&str; 103] = [
    "He", "Ne", "Ar", "Kr", "Xe", "Rn", // noble gases
    "Fr", "Cs", "Rb", "K", "Na", "Li", // alkali
    "Ra", "Ba", "Sr", "Ca", "Mg", "Be", // alkaline earth
    "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk", "Cf", "Es", "Fm", "Md", "No",
    "Lr", // actinoids
    "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb", "Dy", "Ho", "Er", "Tm", "Yb",
    "Lu", // lanthanoids
    "Y", "Sc", "Hf", "Zr", "Ti", "Ta", "Nb", "V", "W", "Mo", "Cr", "Re", "Tc", "Mn", "Os", "Ru",
    "Fe", "Ir", "Rh", "Co", "Pt", "Pd", "Ni", "Au", "Ag", "Cu", "Hg", "Cd",
    "Zn", // transition metals
    "Tl", "In", "Ga", "Al", "B", // boron group
    "Pb", "Sn", "Ge", "Si", "C", // carbon group
    "Bi", "Sb", "As", "P", "N", // nitrogen group
    "H", //
    "Po", "Te", "Se", "S", // chalcogens bar oxygen
    "At", "I", "Br", "Cl", // halogens bar fluorine
    "O", "F",
];

fn electropositive_rank(symbol: &str) -> usize {
    ELECTROPOSITIVE_SEQUENCE
        .iter()
        .position(|candidate| *candidate == symbol)
        .unwrap_or(ELECTROPOSITIVE_SEQUENCE.len())
}

/// Conventional display ordering for a formula derived from bare element
/// counts: Hill order for organics (C, H, rest alphabetical), `M(OH)n` for
/// hydroxides, metals-then-H-then-nonmetals for salts (`NaCl`, `CaCO3`,
/// `NaHCO3`), acid H first (`H2SO4`, `HNO3`), and the IUPAC
/// electronegativity sequence for the rest (`H2O`, `NH3`, `SO2`, `Cl2O`).
/// This is display text only — parsing, digests, and identity all use
/// normalized compositions, never this string.
// ponytail: counts alone cannot recover NH4+ salts (NH4NO3 renders as
// H4N2O3) or carbonic acid vs Hill (CH2O3); those need the graph-aware
// paths, which the naming layer prefers wherever a structure exists.
#[must_use]
pub fn conventional_formula<'a>(counts: impl IntoIterator<Item = (&'a str, u64)>) -> String {
    let mut entries: Vec<(&str, u64)> =
        counts.into_iter().filter(|(_, count)| *count > 0).collect();
    entries.sort_by_key(|(symbol, _)| (electropositive_rank(symbol), *symbol));
    let count_of = {
        let entries = entries.clone();
        move |target: &str| {
            entries
                .iter()
                .find(|(symbol, _)| *symbol == target)
                .map_or(0, |(_, count)| *count)
        }
    };
    let metal_count = entries
        .iter()
        .filter(|(symbol, _)| crate::periodic::is_metal(symbol))
        .count();

    let mut formula = String::new();
    let append = |formula: &mut String, symbol: &str, count: u64| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };

    // Organics: Hill order. Only without metals, so carbonates and
    // bicarbonates stay conventional.
    if count_of("C") > 0 && count_of("H") > 0 && metal_count == 0 {
        for symbol in ["C", "H"] {
            append(&mut formula, symbol, count_of(symbol));
        }
        entries.sort_by_key(|(symbol, _)| *symbol);
        for (symbol, count) in entries {
            if symbol != "C" && symbol != "H" {
                append(&mut formula, symbol, count);
            }
        }
        return formula;
    }

    // Hydroxides: one metal plus matching O and H counts writes OH groups.
    let hydroxide_groups = count_of("O");
    if metal_count == 1
        && entries.len() == 3
        && hydroxide_groups > 0
        && hydroxide_groups == count_of("H")
    {
        let (metal, count) = entries[0];
        append(&mut formula, metal, count);
        if hydroxide_groups == 1 {
            formula.push_str("OH");
        } else {
            formula.push_str("(OH)");
            formula.push_str(&hydroxide_groups.to_string());
        }
        return formula;
    }

    // Salts write hydrogen straight after the metals (NaH, NaHCO3, CaH2);
    // acids lead with it (H2SO4, HNO3). Binary molecular compounds follow
    // the sequence alone, which already places H correctly on both sides
    // of it (H2O, H2S, NH3, PH3).
    let hydrogen_first =
        count_of("H") > 0 && (metal_count > 0 || (count_of("O") > 0 && entries.len() >= 3));
    if hydrogen_first {
        let mut hydrogen_appended = false;
        for (symbol, count) in entries {
            if symbol == "H" {
                continue;
            }
            if !hydrogen_appended && !crate::periodic::is_metal(symbol) {
                append(&mut formula, "H", count_of("H"));
                hydrogen_appended = true;
            }
            append(&mut formula, symbol, count);
        }
        if !hydrogen_appended {
            append(&mut formula, "H", count_of("H"));
        }
        return formula;
    }

    for (symbol, count) in entries {
        append(&mut formula, symbol, count);
    }
    formula
}

#[cfg(test)]
mod conventional_formula_tests {
    use super::conventional_formula;
    use std::collections::BTreeMap;

    fn formula(parts: &[(&str, u64)]) -> String {
        // BTreeMap input mirrors the alphabetical maps every caller holds.
        let counts: BTreeMap<&str, u64> = parts.iter().copied().collect();
        conventional_formula(counts.iter().map(|(symbol, count)| (*symbol, *count)))
    }

    #[test]
    fn salts_and_oxides_lead_with_the_metal() {
        assert_eq!(formula(&[("Cl", 1), ("Na", 1)]), "NaCl");
        assert_eq!(formula(&[("Fe", 2), ("O", 3)]), "Fe2O3");
        assert_eq!(formula(&[("Cl", 2), ("Mg", 1)]), "MgCl2");
        assert_eq!(formula(&[("C", 1), ("Ca", 1), ("O", 3)]), "CaCO3");
        assert_eq!(formula(&[("K", 1), ("Mn", 1), ("O", 4)]), "KMnO4");
        assert_eq!(formula(&[("Cu", 1), ("O", 4), ("S", 1)]), "CuSO4");
        assert_eq!(formula(&[("H", 1), ("Li", 1)]), "LiH");
        assert_eq!(
            formula(&[("C", 1), ("H", 1), ("Na", 1), ("O", 3)]),
            "NaHCO3"
        );
    }

    #[test]
    fn hydroxides_write_the_oh_group() {
        assert_eq!(formula(&[("H", 1), ("Li", 1), ("O", 1)]), "LiOH");
        assert_eq!(formula(&[("H", 1), ("Na", 1), ("O", 1)]), "NaOH");
        assert_eq!(formula(&[("Ca", 1), ("H", 2), ("O", 2)]), "Ca(OH)2");
        assert_eq!(formula(&[("Al", 1), ("H", 3), ("O", 3)]), "Al(OH)3");
    }

    #[test]
    fn acids_lead_with_hydrogen_and_molecules_follow_the_sequence() {
        assert_eq!(formula(&[("H", 2), ("O", 4), ("S", 1)]), "H2SO4");
        assert_eq!(formula(&[("H", 1), ("N", 1), ("O", 3)]), "HNO3");
        assert_eq!(formula(&[("H", 3), ("O", 4), ("P", 1)]), "H3PO4");
        assert_eq!(formula(&[("H", 2), ("O", 1)]), "H2O");
        assert_eq!(formula(&[("H", 2), ("O", 2)]), "H2O2");
        assert_eq!(formula(&[("Cl", 1), ("H", 1)]), "HCl");
        assert_eq!(formula(&[("H", 2), ("S", 1)]), "H2S");
        assert_eq!(formula(&[("H", 3), ("N", 1)]), "NH3");
        assert_eq!(formula(&[("Cl", 1), ("H", 4), ("N", 1)]), "NH4Cl");
    }

    #[test]
    fn molecular_compounds_follow_the_electronegativity_sequence() {
        assert_eq!(formula(&[("C", 1), ("O", 2)]), "CO2");
        assert_eq!(formula(&[("O", 2), ("S", 1)]), "SO2");
        assert_eq!(formula(&[("N", 1), ("O", 2)]), "NO2");
        assert_eq!(formula(&[("Cl", 2), ("O", 1)]), "Cl2O");
        assert_eq!(formula(&[("F", 2), ("O", 1)]), "OF2");
        assert_eq!(formula(&[("O", 2)]), "O2");
        assert_eq!(formula(&[("Fe", 1)]), "Fe");
    }

    #[test]
    fn organics_use_hill_order() {
        assert_eq!(formula(&[("C", 1), ("H", 4)]), "CH4");
        assert_eq!(formula(&[("C", 2), ("H", 6), ("O", 1)]), "C2H6O");
        assert_eq!(formula(&[("C", 1), ("H", 5), ("N", 1)]), "CH5N");
    }
}
