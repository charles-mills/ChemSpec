use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::Serialize;

use crate::{Charge, ContentDigest, ElementSymbol, Phase, SpeciesId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormulaComposition {
    elements: BTreeMap<ElementSymbol, u64>,
}

impl FormulaComposition {
    /// Parses the closed `.chems` formula subset using exact integer counts.
    ///
    /// # Errors
    ///
    /// Rejects empty formulae, malformed groups, zero counts, invalid element
    /// symbols, or integer overflow.
    pub fn parse(source: &str) -> Result<Self, ReactionDeclarationError> {
        FormulaParser::new(source).parse()
    }

    /// Constructs a formula from an already-normalized element inventory.
    ///
    /// # Errors
    ///
    /// Rejects empty inventories and zero counts.
    pub fn new(
        elements: impl IntoIterator<Item = (ElementSymbol, u64)>,
    ) -> Result<Self, ReactionDeclarationError> {
        let mut normalized = BTreeMap::new();
        for (element, count) in elements {
            if count == 0 {
                return Err(ReactionDeclarationError::ZeroFormulaCount);
            }
            let entry = normalized.entry(element).or_insert(0_u64);
            *entry = entry
                .checked_add(count)
                .ok_or(ReactionDeclarationError::FormulaCountOverflow)?;
        }
        if normalized.is_empty() {
            return Err(ReactionDeclarationError::EmptyFormula);
        }
        Ok(Self {
            elements: normalized,
        })
    }

    #[must_use]
    pub const fn elements(&self) -> &BTreeMap<ElementSymbol, u64> {
        &self.elements
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbalancedReactionTerm {
    pub species: SpeciesId,
    pub display_name: String,
    pub formula_text: String,
    pub formula: FormulaComposition,
    pub charge: Charge,
    pub phase: Phase,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReactionTerm {
    species: SpeciesId,
    display_name: String,
    formula_text: String,
    formula: FormulaComposition,
    charge: Charge,
    phase: Phase,
    coefficient: u32,
}

#[derive(Serialize)]
struct SemanticTerm {
    species: SpeciesId,
    formula: FormulaComposition,
    charge: Charge,
    phase: Phase,
    coefficient: u32,
}

impl PartialEq for ReactionTerm {
    fn eq(&self, other: &Self) -> bool {
        self.species == other.species
            && self.formula == other.formula
            && self.charge == other.charge
            && self.phase == other.phase
            && self.coefficient == other.coefficient
    }
}

impl Eq for ReactionTerm {}

impl ReactionTerm {
    #[must_use]
    pub const fn species(&self) -> &SpeciesId {
        &self.species
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn formula_text(&self) -> &str {
        &self.formula_text
    }

    #[must_use]
    pub const fn formula(&self) -> &FormulaComposition {
        &self.formula
    }

    #[must_use]
    pub const fn charge(&self) -> &Charge {
        &self.charge
    }

    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    #[must_use]
    pub const fn coefficient(&self) -> u32 {
        self.coefficient
    }
}

/// Checked, canonical reaction meaning shared by parsed source and dynamic
/// compilation. Fields are private so neither source syntax nor provider JSON
/// can forge a balanced declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactionDeclaration {
    reactants: Vec<ReactionTerm>,
    products: Vec<ReactionTerm>,
    required_context: String,
    digest: ContentDigest,
}

impl ReactionDeclaration {
    /// Balances an uncoefficiented equation with exact rational arithmetic and
    /// constructs the checked declaration.
    ///
    /// # Errors
    ///
    /// Rejects impossible, underdetermined, non-positive, duplicate-species,
    /// charge-inconsistent, or coefficient-overflowing systems.
    pub fn balance(
        reactants: Vec<UnbalancedReactionTerm>,
        products: Vec<UnbalancedReactionTerm>,
        required_context: impl Into<String>,
    ) -> Result<Self, ReactionDeclarationError> {
        if reactants.is_empty() || products.is_empty() {
            return Err(ReactionDeclarationError::EmptySide);
        }
        ensure_unique_species(&reactants, &products)?;
        let coefficients = solve_coefficients(&reactants, &products)?;
        let reactant_count = reactants.len();
        let reactants = reactants
            .into_iter()
            .zip(&coefficients[..reactant_count])
            .map(|(term, coefficient)| with_coefficient(term, *coefficient))
            .collect::<Vec<_>>();
        let products = products
            .into_iter()
            .zip(&coefficients[reactant_count..])
            .map(|(term, coefficient)| with_coefficient(term, *coefficient))
            .collect::<Vec<_>>();
        Self::from_balanced(reactants, products, required_context)
    }

    /// Checks already-authored exact coefficients and canonicalizes term
    /// ordering. This is the convergence point used by `.chems` parsing.
    ///
    /// # Errors
    ///
    /// Rejects non-positive coefficients, duplicate identities, or element and
    /// charge imbalance.
    pub fn from_balanced(
        mut reactants: Vec<ReactionTerm>,
        mut products: Vec<ReactionTerm>,
        required_context: impl Into<String>,
    ) -> Result<Self, ReactionDeclarationError> {
        if reactants.is_empty() || products.is_empty() {
            return Err(ReactionDeclarationError::EmptySide);
        }
        if reactants
            .iter()
            .chain(&products)
            .any(|term| term.coefficient == 0)
        {
            return Err(ReactionDeclarationError::ZeroCoefficient);
        }
        let unbalanced_reactants = reactants
            .iter()
            .map(without_coefficient)
            .collect::<Vec<_>>();
        let unbalanced_products = products.iter().map(without_coefficient).collect::<Vec<_>>();
        ensure_unique_species(&unbalanced_reactants, &unbalanced_products)?;
        validate_conservation(&reactants, &products)?;
        reactants.sort_by(|left, right| left.species.cmp(&right.species));
        products.sort_by(|left, right| left.species.cmp(&right.species));
        let required_context = required_context.into();
        let semantic_terms = |terms: &[ReactionTerm]| {
            terms
                .iter()
                .map(|term| SemanticTerm {
                    species: term.species.clone(),
                    formula: term.formula.clone(),
                    charge: term.charge.clone(),
                    phase: term.phase,
                    coefficient: term.coefficient,
                })
                .collect::<Vec<_>>()
        };
        let semantic = serde_json::json!({
            "reactants": semantic_terms(&reactants),
            "products": semantic_terms(&products),
            "required_context": required_context,
        });
        let canonical = crate::canonical_json(&semantic)
            .map_err(|error| ReactionDeclarationError::Serialization(error.to_string()))?;
        Ok(Self {
            reactants,
            products,
            required_context,
            digest: ContentDigest::sha256(&canonical),
        })
    }

    #[must_use]
    pub fn reactants(&self) -> &[ReactionTerm] {
        &self.reactants
    }

    #[must_use]
    pub fn products(&self) -> &[ReactionTerm] {
        &self.products
    }

    #[must_use]
    pub fn required_context(&self) -> &str {
        &self.required_context
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Checked construction helper for parsed-source adapters.
///
/// # Errors
///
/// Rejects a zero coefficient.
pub fn reaction_term(
    term: UnbalancedReactionTerm,
    coefficient: u32,
) -> Result<ReactionTerm, ReactionDeclarationError> {
    if coefficient == 0 {
        return Err(ReactionDeclarationError::ZeroCoefficient);
    }
    Ok(with_coefficient(term, coefficient))
}

fn with_coefficient(term: UnbalancedReactionTerm, coefficient: u32) -> ReactionTerm {
    ReactionTerm {
        species: term.species,
        display_name: term.display_name,
        formula_text: term.formula_text,
        formula: term.formula,
        charge: term.charge,
        phase: term.phase,
        coefficient,
    }
}

fn without_coefficient(term: &ReactionTerm) -> UnbalancedReactionTerm {
    UnbalancedReactionTerm {
        species: term.species.clone(),
        display_name: term.display_name.clone(),
        formula_text: term.formula_text.clone(),
        formula: term.formula.clone(),
        charge: term.charge.clone(),
        phase: term.phase,
    }
}

fn ensure_unique_species(
    reactants: &[UnbalancedReactionTerm],
    products: &[UnbalancedReactionTerm],
) -> Result<(), ReactionDeclarationError> {
    let mut identities = BTreeSet::new();
    for term in reactants.iter().chain(products) {
        if !identities.insert(term.species.clone()) {
            return Err(ReactionDeclarationError::DuplicateSpecies(
                term.species.clone(),
            ));
        }
    }
    Ok(())
}

fn solve_coefficients(
    reactants: &[UnbalancedReactionTerm],
    products: &[UnbalancedReactionTerm],
) -> Result<Vec<u32>, ReactionDeclarationError> {
    let terms = reactants.iter().chain(products).collect::<Vec<_>>();
    let elements = terms
        .iter()
        .flat_map(|term| term.formula.elements.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut matrix = elements
        .iter()
        .map(|element| {
            terms
                .iter()
                .enumerate()
                .map(|(index, term)| {
                    let count = term.formula.elements.get(element).copied().unwrap_or(0);
                    let value = BigInt::from(count);
                    BigRational::from_integer(if index < reactants.len() {
                        value
                    } else {
                        -value
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    matrix.push(
        terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                BigRational::from_integer(if index < reactants.len() {
                    term.charge.value().clone()
                } else {
                    -term.charge.value().clone()
                })
            })
            .collect(),
    );
    let (rref, pivots) = rref(matrix);
    let columns = terms.len();
    let nullity = columns.saturating_sub(pivots.len());
    if nullity == 0 {
        return Err(ReactionDeclarationError::ImpossibleBalance);
    }
    if nullity != 1 {
        return Err(ReactionDeclarationError::UnderdeterminedBalance);
    }
    let free = (0..columns)
        .find(|column| !pivots.contains(column))
        .expect("one free column");
    let mut solution = vec![BigRational::zero(); columns];
    solution[free] = BigRational::one();
    for (row, pivot) in pivots.iter().enumerate().rev() {
        let sum = (*pivot + 1..columns)
            .map(|column| &rref[row][column] * &solution[column])
            .fold(BigRational::zero(), |accumulator, value| {
                accumulator + value
            });
        solution[*pivot] = -sum;
    }
    if solution.iter().all(Signed::is_negative) {
        for value in &mut solution {
            *value = -value.clone();
        }
    }
    if solution.iter().any(|value| !value.is_positive()) {
        return Err(ReactionDeclarationError::NoPositiveBalance);
    }
    let denominator_lcm = solution.iter().fold(BigInt::one(), |accumulator, value| {
        lcm(&accumulator, value.denom())
    });
    let mut integers = solution
        .iter()
        .map(|value| value.numer() * (&denominator_lcm / value.denom()))
        .collect::<Vec<_>>();
    let divisor = integers.iter().fold(BigInt::zero(), |accumulator, value| {
        gcd(accumulator, value.clone())
    });
    for value in &mut integers {
        *value /= &divisor;
    }
    integers
        .into_iter()
        .map(|value| {
            value
                .to_u32()
                .filter(|value| *value > 0)
                .ok_or(ReactionDeclarationError::CoefficientOverflow)
        })
        .collect()
}

fn rref(mut matrix: Vec<Vec<BigRational>>) -> (Vec<Vec<BigRational>>, Vec<usize>) {
    if matrix.is_empty() || matrix[0].is_empty() {
        return (matrix, Vec::new());
    }
    let columns = matrix[0].len();
    let mut pivot_row = 0;
    let mut pivots = Vec::new();
    for column in 0..columns {
        let Some(candidate) = (pivot_row..matrix.len()).find(|row| !matrix[*row][column].is_zero())
        else {
            continue;
        };
        matrix.swap(pivot_row, candidate);
        let pivot = matrix[pivot_row][column].clone();
        for value in &mut matrix[pivot_row] {
            *value /= &pivot;
        }
        let pivot_values = matrix[pivot_row].clone();
        for (row, values) in matrix.iter_mut().enumerate() {
            if row == pivot_row || values[column].is_zero() {
                continue;
            }
            let factor = values[column].clone();
            for current_column in column..columns {
                values[current_column] -= &factor * &pivot_values[current_column];
            }
        }
        pivots.push(column);
        pivot_row += 1;
        if pivot_row == matrix.len() {
            break;
        }
    }
    (matrix, pivots)
}

fn gcd(mut left: BigInt, mut right: BigInt) -> BigInt {
    left = left.abs();
    right = right.abs();
    while !right.is_zero() {
        let remainder = &left % &right;
        left = right;
        right = remainder;
    }
    left
}

fn lcm(left: &BigInt, right: &BigInt) -> BigInt {
    if left.is_zero() || right.is_zero() {
        BigInt::zero()
    } else {
        ((left / gcd(left.clone(), right.clone())) * right).abs()
    }
}

fn validate_conservation(
    reactants: &[ReactionTerm],
    products: &[ReactionTerm],
) -> Result<(), ReactionDeclarationError> {
    let mut elements = BTreeMap::<ElementSymbol, BigInt>::new();
    let mut charge = BigInt::zero();
    for (sign, terms) in [(1_i8, reactants), (-1_i8, products)] {
        for term in terms {
            let coefficient = BigInt::from(term.coefficient);
            for (element, count) in &term.formula.elements {
                *elements.entry(element.clone()).or_default() +=
                    BigInt::from(sign) * &coefficient * BigInt::from(*count);
            }
            charge += BigInt::from(sign) * &coefficient * term.charge.value();
        }
    }
    if elements.values().any(|value| !value.is_zero()) {
        return Err(ReactionDeclarationError::ElementImbalance);
    }
    if !charge.is_zero() {
        return Err(ReactionDeclarationError::ChargeImbalance);
    }
    Ok(())
}

struct FormulaParser<'a> {
    source: &'a str,
    bytes: &'a [u8],
    index: usize,
}

impl<'a> FormulaParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            index: 0,
        }
    }

    fn parse(mut self) -> Result<FormulaComposition, ReactionDeclarationError> {
        let mut result = self.segment(None)?;
        while self.index < self.bytes.len() {
            self.expect(b'.')?;
            let multiplier = self.number()?.unwrap_or(1);
            let segment = self.segment(None)?;
            merge_formula(&mut result, segment, multiplier)?;
        }
        FormulaComposition::new(result)
    }

    fn segment(
        &mut self,
        terminator: Option<u8>,
    ) -> Result<BTreeMap<ElementSymbol, u64>, ReactionDeclarationError> {
        let mut result = BTreeMap::new();
        let start = self.index;
        while self.index < self.bytes.len()
            && self.bytes[self.index] != b'.'
            && Some(self.bytes[self.index]) != terminator
        {
            if self.bytes[self.index] == b'(' {
                self.index += 1;
                let nested = self.segment(Some(b')'))?;
                self.expect(b')')?;
                let multiplier = self.number()?.unwrap_or(1);
                merge_formula(&mut result, nested, multiplier)?;
            } else if self.bytes[self.index].is_ascii_uppercase() {
                let element_start = self.index;
                self.index += 1;
                if self
                    .bytes
                    .get(self.index)
                    .is_some_and(u8::is_ascii_lowercase)
                {
                    self.index += 1;
                }
                let element = ElementSymbol::from_str(&self.source[element_start..self.index])
                    .map_err(|_| ReactionDeclarationError::InvalidFormula(self.source.into()))?;
                let count = self.number()?.unwrap_or(1);
                let entry = result.entry(element).or_insert(0_u64);
                *entry = entry
                    .checked_add(count)
                    .ok_or(ReactionDeclarationError::FormulaCountOverflow)?;
            } else {
                return Err(ReactionDeclarationError::InvalidFormula(self.source.into()));
            }
        }
        if self.index == start {
            return Err(ReactionDeclarationError::InvalidFormula(self.source.into()));
        }
        Ok(result)
    }

    fn number(&mut self) -> Result<Option<u64>, ReactionDeclarationError> {
        let start = self.index;
        while self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
        if start == self.index {
            return Ok(None);
        }
        let value = self.source[start..self.index]
            .parse::<u64>()
            .map_err(|_| ReactionDeclarationError::FormulaCountOverflow)?;
        if value == 0 {
            return Err(ReactionDeclarationError::ZeroFormulaCount);
        }
        Ok(Some(value))
    }

    fn expect(&mut self, byte: u8) -> Result<(), ReactionDeclarationError> {
        if self.bytes.get(self.index) == Some(&byte) {
            self.index += 1;
            Ok(())
        } else {
            Err(ReactionDeclarationError::InvalidFormula(self.source.into()))
        }
    }
}

fn merge_formula(
    target: &mut BTreeMap<ElementSymbol, u64>,
    source: BTreeMap<ElementSymbol, u64>,
    multiplier: u64,
) -> Result<(), ReactionDeclarationError> {
    for (element, count) in source {
        let contribution = count
            .checked_mul(multiplier)
            .ok_or(ReactionDeclarationError::FormulaCountOverflow)?;
        let entry = target.entry(element).or_insert(0);
        *entry = entry
            .checked_add(contribution)
            .ok_or(ReactionDeclarationError::FormulaCountOverflow)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionDeclarationError {
    EmptyFormula,
    InvalidFormula(String),
    ZeroFormulaCount,
    FormulaCountOverflow,
    EmptySide,
    DuplicateSpecies(SpeciesId),
    ImpossibleBalance,
    UnderdeterminedBalance,
    NoPositiveBalance,
    CoefficientOverflow,
    ZeroCoefficient,
    ElementImbalance,
    ChargeImbalance,
    Serialization(String),
}

impl fmt::Display for ReactionDeclarationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFormula => formatter.write_str("formula is empty"),
            Self::InvalidFormula(value) => write!(formatter, "invalid formula `{value}`"),
            Self::ZeroFormulaCount => formatter.write_str("formula count must be positive"),
            Self::FormulaCountOverflow => formatter.write_str("formula count overflow"),
            Self::EmptySide => formatter.write_str("reaction requires reactants and products"),
            Self::DuplicateSpecies(id) => write!(formatter, "duplicate reaction species `{id}`"),
            Self::ImpossibleBalance => formatter.write_str("reaction has no nonzero balance"),
            Self::UnderdeterminedBalance => {
                formatter.write_str("reaction balance is underdetermined")
            }
            Self::NoPositiveBalance => formatter.write_str("reaction has no all-positive balance"),
            Self::CoefficientOverflow => formatter.write_str("balanced coefficient exceeds u32"),
            Self::ZeroCoefficient => formatter.write_str("reaction coefficient must be positive"),
            Self::ElementImbalance => formatter.write_str("reaction does not conserve elements"),
            Self::ChargeImbalance => formatter.write_str("reaction does not conserve charge"),
            Self::Serialization(message) => {
                write!(formatter, "reaction serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for ReactionDeclarationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChargeSign;

    fn term(id: &str, formula: &str, charge: i8) -> UnbalancedReactionTerm {
        let charge = if charge == 0 {
            Charge::neutral()
        } else {
            Charge::from_magnitude(
                charge.unsigned_abs().into(),
                if charge.is_positive() {
                    ChargeSign::Positive
                } else {
                    ChargeSign::Negative
                },
            )
            .expect("nonzero charge")
        };
        UnbalancedReactionTerm {
            species: SpeciesId::from_str(id).expect("species id"),
            display_name: id.to_owned(),
            formula_text: formula.to_owned(),
            formula: FormulaComposition::parse(formula).expect("formula"),
            charge,
            phase: Phase::Unknown,
        }
    }

    fn coefficients(declaration: &ReactionDeclaration) -> Vec<u32> {
        declaration
            .reactants()
            .iter()
            .chain(declaration.products())
            .map(ReactionTerm::coefficient)
            .collect()
    }

    #[test]
    fn unique_combustion_and_acid_base_balances_are_exact() {
        let combustion = ReactionDeclaration::balance(
            vec![term("methane", "CH4", 0), term("oxygen", "O2", 0)],
            vec![term("carbonDioxide", "CO2", 0), term("water", "H2O", 0)],
            "combustion",
        )
        .expect("combustion balance");
        assert_eq!(coefficients(&combustion), vec![1, 2, 1, 2]);

        let acid_base = ReactionDeclaration::balance(
            vec![term("proton", "H", 1), term("hydroxide", "OH", -1)],
            vec![term("water", "H2O", 0)],
            "acid base",
        )
        .expect("ionic balance");
        assert_eq!(coefficients(&acid_base), vec![1, 1, 1]);
    }

    #[test]
    fn redox_balance_conserves_charge() {
        let declaration = ReactionDeclaration::balance(
            vec![term("iron2", "Fe", 2), term("cerium4", "Ce", 4)],
            vec![term("iron3", "Fe", 3), term("cerium3", "Ce", 3)],
            "redox",
        )
        .expect("redox balance");
        assert_eq!(coefficients(&declaration), vec![1, 1, 1, 1]);
    }

    #[test]
    fn impossible_and_underdetermined_systems_fail_closed() {
        assert_eq!(
            ReactionDeclaration::balance(
                vec![term("hydrogen", "H2", 0)],
                vec![term("water", "H2O", 0)],
                "impossible",
            )
            .expect_err("impossible balance"),
            ReactionDeclarationError::ImpossibleBalance
        );
        assert_eq!(
            ReactionDeclaration::balance(
                vec![term("carbon", "C", 0), term("oxygen", "O2", 0)],
                vec![term("monoxide", "CO", 0), term("dioxide", "CO2", 0)],
                "underdetermined",
            )
            .expect_err("underdetermined balance"),
            ReactionDeclarationError::UnderdeterminedBalance
        );
    }

    #[test]
    fn grouped_and_adduct_formulae_parse_without_floating_point() {
        let calcium = FormulaComposition::parse("Ca(OH)2").expect("calcium hydroxide");
        assert_eq!(calcium.elements()[&ElementSymbol::new("H").unwrap()], 2);
        let hydrate = FormulaComposition::parse("CuSO4.5H2O").expect("hydrate");
        assert_eq!(hydrate.elements()[&ElementSymbol::new("O").unwrap()], 9);
        assert_eq!(hydrate.elements()[&ElementSymbol::new("H").unwrap()], 10);
    }
}
