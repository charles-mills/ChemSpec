use std::{fmt, str::FromStr};

use num_bigint::BigInt;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{ExactScalar, ScalarError, SourceDecimal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dimension {
    pub mass: i32,
    pub length: i32,
    pub time: i32,
    pub amount: i32,
    pub temperature: i32,
}

impl Dimension {
    pub const DIMENSIONLESS: Self = Self::new(0, 0, 0, 0, 0);
    pub const MASS: Self = Self::new(1, 0, 0, 0, 0);
    pub const LENGTH: Self = Self::new(0, 1, 0, 0, 0);
    pub const TIME: Self = Self::new(0, 0, 1, 0, 0);
    pub const AMOUNT: Self = Self::new(0, 0, 0, 1, 0);
    pub const TEMPERATURE: Self = Self::new(0, 0, 0, 0, 1);
    pub const VOLUME: Self = Self::new(0, 3, 0, 0, 0);
    pub const CONCENTRATION: Self = Self::new(0, -3, 0, 1, 0);
    pub const PRESSURE: Self = Self::new(1, -1, -2, 0, 0);
    pub const MOLAR_MASS: Self = Self::new(1, 0, 0, -1, 0);
    pub const DENSITY: Self = Self::new(1, -3, 0, 0, 0);

    #[must_use]
    pub const fn new(mass: i32, length: i32, time: i32, amount: i32, temperature: i32) -> Self {
        Self {
            mass,
            length,
            time,
            amount,
            temperature,
        }
    }

    /// Multiplies two dimension vectors by adding their exponents.
    ///
    /// # Errors
    ///
    /// Returns [`DimensionError::ExponentOverflow`] if an exponent exceeds the
    /// representation limit.
    pub fn checked_mul(self, right: Self) -> Result<Self, DimensionError> {
        Ok(Self::new(
            self.mass
                .checked_add(right.mass)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.length
                .checked_add(right.length)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.time
                .checked_add(right.time)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.amount
                .checked_add(right.amount)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.temperature
                .checked_add(right.temperature)
                .ok_or(DimensionError::ExponentOverflow)?,
        ))
    }

    /// Raises a dimension vector to an integer power.
    ///
    /// # Errors
    ///
    /// Returns [`DimensionError::ExponentOverflow`] if an exponent exceeds the
    /// representation limit.
    pub fn checked_pow(self, exponent: i32) -> Result<Self, DimensionError> {
        Ok(Self::new(
            self.mass
                .checked_mul(exponent)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.length
                .checked_mul(exponent)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.time
                .checked_mul(exponent)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.amount
                .checked_mul(exponent)
                .ok_or(DimensionError::ExponentOverflow)?,
            self.temperature
                .checked_mul(exponent)
                .ok_or(DimensionError::ExponentOverflow)?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionError {
    ExponentOverflow,
}

impl fmt::Display for DimensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dimension exponent overflow")
    }
}

impl std::error::Error for DimensionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitSymbol {
    #[serde(rename = "kg")]
    Kilogram,
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "mg")]
    Milligram,
    #[serde(rename = "m")]
    Metre,
    #[serde(rename = "cm")]
    Centimetre,
    #[serde(rename = "mm")]
    Millimetre,
    #[serde(rename = "L")]
    Litre,
    #[serde(rename = "mL")]
    Millilitre,
    #[serde(rename = "uL")]
    Microlitre,
    #[serde(rename = "mol")]
    Mole,
    #[serde(rename = "mmol")]
    Millimole,
    #[serde(rename = "umol")]
    Micromole,
    #[serde(rename = "s")]
    Second,
    #[serde(rename = "min")]
    Minute,
    #[serde(rename = "h")]
    Hour,
    #[serde(rename = "K")]
    Kelvin,
    #[serde(rename = "degC")]
    DegreesCelsius,
    #[serde(rename = "Pa")]
    Pascal,
    #[serde(rename = "kPa")]
    Kilopascal,
    #[serde(rename = "atm")]
    Atmosphere,
    #[serde(rename = "M")]
    Molar,
    #[serde(rename = "mM")]
    Millimolar,
    #[serde(rename = "%")]
    Percent,
}

impl UnitSymbol {
    pub const ALL: [Self; 23] = [
        Self::Kilogram,
        Self::Gram,
        Self::Milligram,
        Self::Metre,
        Self::Centimetre,
        Self::Millimetre,
        Self::Litre,
        Self::Millilitre,
        Self::Microlitre,
        Self::Mole,
        Self::Millimole,
        Self::Micromole,
        Self::Second,
        Self::Minute,
        Self::Hour,
        Self::Kelvin,
        Self::DegreesCelsius,
        Self::Pascal,
        Self::Kilopascal,
        Self::Atmosphere,
        Self::Molar,
        Self::Millimolar,
        Self::Percent,
    ];

    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::Kilogram => "kg",
            Self::Gram => "g",
            Self::Milligram => "mg",
            Self::Metre => "m",
            Self::Centimetre => "cm",
            Self::Millimetre => "mm",
            Self::Litre => "L",
            Self::Millilitre => "mL",
            Self::Microlitre => "uL",
            Self::Mole => "mol",
            Self::Millimole => "mmol",
            Self::Micromole => "umol",
            Self::Second => "s",
            Self::Minute => "min",
            Self::Hour => "h",
            Self::Kelvin => "K",
            Self::DegreesCelsius => "degC",
            Self::Pascal => "Pa",
            Self::Kilopascal => "kPa",
            Self::Atmosphere => "atm",
            Self::Molar => "M",
            Self::Millimolar => "mM",
            Self::Percent => "%",
        }
    }

    fn definition(self) -> UnitDefinition {
        match self {
            Self::Kilogram => UnitDefinition::multiplicative(Dimension::MASS, ratio(1, 1)),
            Self::Gram => UnitDefinition::multiplicative(Dimension::MASS, ratio(1, 1_000)),
            Self::Milligram => UnitDefinition::multiplicative(Dimension::MASS, ratio(1, 1_000_000)),
            Self::Metre => UnitDefinition::multiplicative(Dimension::LENGTH, ratio(1, 1)),
            Self::Centimetre => UnitDefinition::multiplicative(Dimension::LENGTH, ratio(1, 100)),
            Self::Millimetre => UnitDefinition::multiplicative(Dimension::LENGTH, ratio(1, 1_000)),
            Self::Litre => UnitDefinition::multiplicative(Dimension::VOLUME, ratio(1, 1_000)),
            Self::Millilitre => {
                UnitDefinition::multiplicative(Dimension::VOLUME, ratio(1, 1_000_000))
            }
            Self::Microlitre => {
                UnitDefinition::multiplicative(Dimension::VOLUME, ratio(1, 1_000_000_000))
            }
            Self::Mole => UnitDefinition::multiplicative(Dimension::AMOUNT, ratio(1, 1)),
            Self::Millimole => UnitDefinition::multiplicative(Dimension::AMOUNT, ratio(1, 1_000)),
            Self::Micromole => {
                UnitDefinition::multiplicative(Dimension::AMOUNT, ratio(1, 1_000_000))
            }
            Self::Second => UnitDefinition::multiplicative(Dimension::TIME, ratio(1, 1)),
            Self::Minute => UnitDefinition::multiplicative(Dimension::TIME, ratio(60, 1)),
            Self::Hour => UnitDefinition::multiplicative(Dimension::TIME, ratio(3_600, 1)),
            Self::Kelvin => UnitDefinition::temperature(TemperatureScale::Kelvin),
            Self::DegreesCelsius => UnitDefinition::temperature(TemperatureScale::DegreesCelsius),
            Self::Pascal => UnitDefinition::multiplicative(Dimension::PRESSURE, ratio(1, 1)),
            Self::Kilopascal => {
                UnitDefinition::multiplicative(Dimension::PRESSURE, ratio(1_000, 1))
            }
            Self::Atmosphere => {
                UnitDefinition::multiplicative(Dimension::PRESSURE, ratio(101_325, 1))
            }
            Self::Molar => {
                UnitDefinition::multiplicative(Dimension::CONCENTRATION, ratio(1_000, 1))
            }
            Self::Millimolar => {
                UnitDefinition::multiplicative(Dimension::CONCENTRATION, ratio(1, 1))
            }
            Self::Percent => UnitDefinition::restricted(Dimension::DIMENSIONLESS, ratio(1, 100)),
        }
    }
}

impl fmt::Display for UnitSymbol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.source())
    }
}

impl FromStr for UnitSymbol {
    type Err = UnitError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|symbol| symbol.source() == source)
            .ok_or_else(|| UnitError::UnknownSymbol(source.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnitPower {
    symbol: UnitSymbol,
    exponent: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authored_exponent: Option<String>,
}

impl UnitPower {
    /// Resolves a source symbol into a closed-registry unit power.
    ///
    /// # Errors
    ///
    /// Returns [`UnitError::UnknownSymbol`] when `symbol` is not registered.
    pub fn parse(symbol: &str, exponent: i32) -> Result<Self, UnitError> {
        Ok(Self {
            symbol: symbol.parse()?,
            exponent,
            authored_exponent: (exponent != 1).then(|| exponent.to_string()),
        })
    }

    /// Constructs a factor while retaining whether and how its exponent was
    /// written after `^`.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown symbol, invalid signed-integer syntax,
    /// or an exponent outside the dimension representation.
    pub fn parse_authored(
        symbol: &str,
        authored_exponent: Option<&str>,
    ) -> Result<Self, UnitError> {
        let exponent = authored_exponent.map_or(Ok(1), parse_unit_exponent)?;
        Ok(Self {
            symbol: symbol.parse()?,
            exponent,
            authored_exponent: authored_exponent.map(str::to_owned),
        })
    }

    #[must_use]
    pub const fn symbol(&self) -> UnitSymbol {
        self.symbol
    }

    #[must_use]
    pub const fn exponent(&self) -> i32 {
        self.exponent
    }

    #[must_use]
    pub fn authored_exponent(&self) -> Option<&str> {
        self.authored_exponent.as_deref()
    }
}

impl<'de> Deserialize<'de> for UnitPower {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            symbol: UnitSymbol,
            exponent: i32,
            #[serde(default)]
            authored_exponent: Option<String>,
        }

        let repr = Repr::deserialize(deserializer)?;
        let parsed_exponent = repr
            .authored_exponent
            .as_deref()
            .map_or(Ok(1), parse_unit_exponent)
            .map_err(serde::de::Error::custom)?;
        if parsed_exponent != repr.exponent {
            return Err(serde::de::Error::custom(
                "unit exponent value does not match its authored spelling",
            ));
        }
        Ok(Self {
            symbol: repr.symbol,
            exponent: repr.exponent,
            authored_exponent: repr.authored_exponent,
        })
    }
}

fn parse_unit_exponent(source: &str) -> Result<i32, UnitError> {
    let unsigned = source.strip_prefix(['+', '-']).unwrap_or(source);
    if unsigned.is_empty() || !unsigned.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UnitError::InvalidExponent(source.to_owned()));
    }
    source
        .parse()
        .map_err(|_| UnitError::InvalidExponent(source.to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitProduct {
    factors: Vec<UnitPower>,
}

impl UnitProduct {
    #[must_use]
    pub fn new(factors: Vec<UnitPower>) -> Self {
        Self { factors }
    }

    #[must_use]
    pub fn factors(&self) -> &[UnitPower] {
        &self.factors
    }
}

/// The grammar-preserving shape `unit-product { "/" unit-product }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitExpression {
    dividend: UnitProduct,
    divisors: Vec<UnitProduct>,
}

impl UnitExpression {
    #[must_use]
    pub fn single(symbol: UnitSymbol) -> Self {
        Self::new(vec![UnitPower {
            symbol,
            exponent: 1,
            authored_exponent: None,
        }])
    }

    #[must_use]
    pub fn new(factors: Vec<UnitPower>) -> Self {
        Self {
            dividend: UnitProduct::new(factors),
            divisors: Vec::new(),
        }
    }

    #[must_use]
    pub fn quotient(dividend: UnitProduct, divisors: Vec<UnitProduct>) -> Self {
        Self { dividend, divisors }
    }

    #[must_use]
    pub const fn dividend(&self) -> &UnitProduct {
        &self.dividend
    }

    #[must_use]
    pub fn divisors(&self) -> &[UnitProduct] {
        &self.divisors
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TemperatureScale {
    Kelvin,
    DegreesCelsius,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResolvedUnit {
    Multiplicative {
        dimension: Dimension,
        factor: ExactScalar,
    },
    TemperaturePoint {
        scale: TemperatureScale,
    },
}

/// One exact alias-and-power expansion within a unit conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConversionStep {
    symbol: UnitSymbol,
    exponent: i32,
    unit_dimension: Dimension,
    unit_factor: ExactScalar,
    expanded_dimension: Dimension,
    expanded_factor: ExactScalar,
}

impl<'de> Deserialize<'de> for UnitConversionStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            symbol: UnitSymbol,
            exponent: i32,
            unit_dimension: Dimension,
            unit_factor: ExactScalar,
            expanded_dimension: Dimension,
            expanded_factor: ExactScalar,
        }

        let repr = Repr::deserialize(deserializer)?;
        let definition = repr.symbol.definition();
        if matches!(definition.kind, UnitKind::Temperature(_))
            || matches!(definition.kind, UnitKind::Restricted) && repr.exponent != 1
        {
            return Err(serde::de::Error::custom(
                "restricted unit cannot appear in this conversion step",
            ));
        }
        let expanded_dimension = definition
            .dimension
            .checked_pow(repr.exponent)
            .map_err(serde::de::Error::custom)?;
        let expanded_factor = definition
            .factor
            .pow_i32(repr.exponent)
            .map_err(serde::de::Error::custom)?;
        if repr.unit_dimension != definition.dimension
            || repr.unit_factor != definition.factor
            || repr.expanded_dimension != expanded_dimension
            || repr.expanded_factor != expanded_factor
        {
            return Err(serde::de::Error::custom(
                "unit conversion step does not match the closed unit registry",
            ));
        }
        Ok(Self {
            symbol: repr.symbol,
            exponent: repr.exponent,
            unit_dimension: repr.unit_dimension,
            unit_factor: repr.unit_factor,
            expanded_dimension: repr.expanded_dimension,
            expanded_factor: repr.expanded_factor,
        })
    }
}

impl UnitConversionStep {
    #[must_use]
    pub const fn symbol(&self) -> UnitSymbol {
        self.symbol
    }

    #[must_use]
    pub const fn exponent(&self) -> i32 {
        self.exponent
    }

    #[must_use]
    pub const fn unit_dimension(&self) -> Dimension {
        self.unit_dimension
    }

    #[must_use]
    pub const fn unit_factor(&self) -> &ExactScalar {
        &self.unit_factor
    }

    #[must_use]
    pub const fn expanded_dimension(&self) -> Dimension {
        self.expanded_dimension
    }

    #[must_use]
    pub const fn expanded_factor(&self) -> &ExactScalar {
        &self.expanded_factor
    }
}

/// Exact evidence for expansion of a multiplicative unit expression into
/// canonical base units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConversionDerivation {
    steps: Vec<UnitConversionStep>,
    result_dimension: Dimension,
    result_factor: ExactScalar,
}

impl<'de> Deserialize<'de> for UnitConversionDerivation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            steps: Vec<UnitConversionStep>,
            result_dimension: Dimension,
            result_factor: ExactScalar,
        }

        let repr = Repr::deserialize(deserializer)?;
        if repr.steps.is_empty()
            || repr.steps.len() != 1
                && repr
                    .steps
                    .iter()
                    .any(|step| matches!(step.symbol.definition().kind, UnitKind::Restricted))
        {
            return Err(serde::de::Error::custom(
                "invalid unit conversion derivation shape",
            ));
        }
        let mut result_dimension = Dimension::DIMENSIONLESS;
        let mut result_factor = ExactScalar::one();
        for step in &repr.steps {
            result_dimension = result_dimension
                .checked_mul(step.expanded_dimension)
                .map_err(serde::de::Error::custom)?;
            result_factor = &result_factor * &step.expanded_factor;
        }
        if repr.result_dimension != result_dimension || repr.result_factor != result_factor {
            return Err(serde::de::Error::custom(
                "unit conversion result does not match its steps",
            ));
        }
        Ok(Self {
            steps: repr.steps,
            result_dimension: repr.result_dimension,
            result_factor: repr.result_factor,
        })
    }
}

impl UnitConversionDerivation {
    #[must_use]
    pub fn steps(&self) -> &[UnitConversionStep] {
        &self.steps
    }

    #[must_use]
    pub const fn result_dimension(&self) -> Dimension {
        self.result_dimension
    }

    #[must_use]
    pub const fn result_factor(&self) -> &ExactScalar {
        &self.result_factor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitError {
    EmptyExpression,
    UnknownSymbol(String),
    InvalidExponent(String),
    RestrictedUnitMustBeStandalone(UnitSymbol),
    Dimension(DimensionError),
    Scalar(ScalarError),
}

impl fmt::Display for UnitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExpression => formatter.write_str("a unit expression cannot be empty"),
            Self::UnknownSymbol(symbol) => write!(formatter, "unknown unit symbol `{symbol}`"),
            Self::InvalidExponent(exponent) => {
                write!(formatter, "invalid unit exponent `{exponent}`")
            }
            Self::RestrictedUnitMustBeStandalone(symbol) => {
                write!(
                    formatter,
                    "unit `{symbol}` must appear alone with exponent one"
                )
            }
            Self::Dimension(error) => error.fmt(formatter),
            Self::Scalar(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for UnitError {}

impl From<DimensionError> for UnitError {
    fn from(error: DimensionError) -> Self {
        Self::Dimension(error)
    }
}

impl From<ScalarError> for UnitError {
    fn from(error: ScalarError) -> Self {
        Self::Scalar(error)
    }
}

/// Resolves a closed-registry expression into an exact canonical conversion.
///
/// # Errors
///
/// Returns an error for an empty expression, restricted temperature/percent
/// composition, exponent overflow, or an invalid exact inversion.
pub fn resolve_unit_expression(expression: &UnitExpression) -> Result<ResolvedUnit, UnitError> {
    resolve_unit_expression_with_derivation(expression).map(|(resolved, _)| resolved)
}

fn resolve_unit_expression_with_derivation(
    expression: &UnitExpression,
) -> Result<(ResolvedUnit, Option<UnitConversionDerivation>), UnitError> {
    let powers = effective_unit_powers(expression)?;
    if powers.len() == 1 && powers[0].exponent == 1 {
        let power = &powers[0];
        match power.symbol.definition().kind {
            UnitKind::Temperature(scale) => {
                return Ok((ResolvedUnit::TemperaturePoint { scale }, None));
            }
            UnitKind::Restricted => {
                let definition = power.symbol.definition();
                let derivation = UnitConversionDerivation {
                    steps: vec![UnitConversionStep {
                        symbol: power.symbol,
                        exponent: 1,
                        unit_dimension: definition.dimension,
                        unit_factor: definition.factor.clone(),
                        expanded_dimension: definition.dimension,
                        expanded_factor: definition.factor.clone(),
                    }],
                    result_dimension: definition.dimension,
                    result_factor: definition.factor.clone(),
                };
                return Ok((
                    ResolvedUnit::Multiplicative {
                        dimension: definition.dimension,
                        factor: definition.factor,
                    },
                    Some(derivation),
                ));
            }
            UnitKind::Multiplicative => {}
        }
    }

    let mut dimension = Dimension::DIMENSIONLESS;
    let mut factor = ExactScalar::one();
    let mut steps = Vec::with_capacity(powers.len());
    for power in &powers {
        let definition = power.symbol.definition();
        if !matches!(definition.kind, UnitKind::Multiplicative) {
            return Err(UnitError::RestrictedUnitMustBeStandalone(power.symbol));
        }
        let expanded_dimension = definition.dimension.checked_pow(power.exponent)?;
        let expanded_factor = definition.factor.pow_i32(power.exponent)?;
        dimension = dimension.checked_mul(expanded_dimension)?;
        factor = &factor * &expanded_factor;
        steps.push(UnitConversionStep {
            symbol: power.symbol,
            exponent: power.exponent,
            unit_dimension: definition.dimension,
            unit_factor: definition.factor,
            expanded_dimension,
            expanded_factor,
        });
    }
    let derivation = UnitConversionDerivation {
        steps,
        result_dimension: dimension,
        result_factor: factor.clone(),
    };
    Ok((
        ResolvedUnit::Multiplicative { dimension, factor },
        Some(derivation),
    ))
}

fn effective_unit_powers(expression: &UnitExpression) -> Result<Vec<UnitPower>, UnitError> {
    if expression.dividend.factors.is_empty()
        || expression
            .divisors
            .iter()
            .any(|product| product.factors.is_empty())
    {
        return Err(UnitError::EmptyExpression);
    }
    let factor_count = expression.dividend.factors.len()
        + expression
            .divisors
            .iter()
            .map(|product| product.factors.len())
            .sum::<usize>();
    let mut powers = Vec::with_capacity(factor_count);
    powers.extend(expression.dividend.factors.iter().cloned());
    for divisor in &expression.divisors {
        for factor in &divisor.factors {
            let exponent = factor
                .exponent
                .checked_neg()
                .ok_or(DimensionError::ExponentOverflow)?;
            powers.push(UnitPower {
                symbol: factor.symbol,
                exponent,
                authored_exponent: factor.authored_exponent.clone(),
            });
        }
    }
    Ok(powers)
}

#[derive(Debug, Clone)]
struct UnitDefinition {
    kind: UnitKind,
    dimension: Dimension,
    factor: ExactScalar,
}

impl UnitDefinition {
    fn multiplicative(dimension: Dimension, factor: ExactScalar) -> Self {
        Self {
            kind: UnitKind::Multiplicative,
            dimension,
            factor,
        }
    }

    fn restricted(dimension: Dimension, factor: ExactScalar) -> Self {
        Self {
            kind: UnitKind::Restricted,
            dimension,
            factor,
        }
    }

    fn temperature(scale: TemperatureScale) -> Self {
        Self {
            kind: UnitKind::Temperature(scale),
            dimension: Dimension::TEMPERATURE,
            factor: ExactScalar::one(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum UnitKind {
    Multiplicative,
    Restricted,
    Temperature(TemperatureScale),
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    canonical_value: ExactScalar,
    dimension: Dimension,
    source_decimal: SourceDecimal,
    source_unit: UnitExpression,
    conversion_derivation: UnitConversionDerivation,
}

impl Quantity {
    /// Constructs a multiplicative quantity in canonical base units.
    ///
    /// # Errors
    ///
    /// Returns an error when the expression is invalid or denotes an affine
    /// temperature point.
    pub fn new(decimal: SourceDecimal, unit: UnitExpression) -> Result<Self, QuantityError> {
        let (ResolvedUnit::Multiplicative { dimension, factor }, Some(conversion_derivation)) =
            resolve_unit_expression_with_derivation(&unit)?
        else {
            return Err(QuantityError::ExpectedMultiplicativeUnit);
        };
        let canonical_value = &decimal.exact_value() * &factor;
        Ok(Self {
            canonical_value,
            dimension,
            source_decimal: decimal,
            source_unit: unit,
            conversion_derivation,
        })
    }

    #[must_use]
    pub const fn canonical_value(&self) -> &ExactScalar {
        &self.canonical_value
    }

    #[must_use]
    pub const fn dimension(&self) -> Dimension {
        self.dimension
    }

    #[must_use]
    pub const fn source_decimal(&self) -> &SourceDecimal {
        &self.source_decimal
    }

    #[must_use]
    pub const fn source_unit(&self) -> &UnitExpression {
        &self.source_unit
    }

    #[must_use]
    pub const fn conversion_derivation(&self) -> &UnitConversionDerivation {
        &self.conversion_derivation
    }

    /// Converts the canonical value to another compatible unit expression.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, affine, or dimensionally incompatible
    /// target expressions.
    pub fn value_in(&self, target: &UnitExpression) -> Result<ExactScalar, QuantityError> {
        self.convert_to(target)
            .map(|conversion| conversion.exact_value)
    }

    /// Converts to a compatible target and returns the exact source and target
    /// unit-expansion evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, affine, or dimensionally incompatible
    /// target expressions.
    pub fn convert_to(&self, target: &UnitExpression) -> Result<QuantityConversion, QuantityError> {
        let (ResolvedUnit::Multiplicative { dimension, factor }, Some(target_to_canonical)) =
            resolve_unit_expression_with_derivation(target)?
        else {
            return Err(QuantityError::ExpectedMultiplicativeUnit);
        };
        if dimension != self.dimension {
            return Err(QuantityError::DimensionMismatch {
                expected: self.dimension,
                actual: dimension,
            });
        }
        let exact_value = self
            .canonical_value
            .checked_div(&factor)
            .map_err(QuantityError::Scalar)?;
        Ok(QuantityConversion {
            exact_value,
            source_to_canonical: self.conversion_derivation.clone(),
            target_unit: target.clone(),
            target_to_canonical,
        })
    }

    /// Compares both the normalized value and the authored representation.
    #[must_use]
    pub fn source_eq(&self, other: &Self) -> bool {
        self == other
            && self.source_decimal == other.source_decimal
            && self.source_unit == other.source_unit
            && self.conversion_derivation == other.conversion_derivation
    }
}

impl PartialEq for Quantity {
    fn eq(&self, other: &Self) -> bool {
        self.dimension == other.dimension && self.canonical_value == other.canonical_value
    }
}

impl Eq for Quantity {}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            canonical_value: ExactScalar,
            dimension: Dimension,
            source_decimal: SourceDecimal,
            source_unit: UnitExpression,
            conversion_derivation: UnitConversionDerivation,
        }

        let repr = Repr::deserialize(deserializer)?;
        let quantity =
            Self::new(repr.source_decimal, repr.source_unit).map_err(serde::de::Error::custom)?;
        if quantity.canonical_value != repr.canonical_value
            || quantity.dimension != repr.dimension
            || quantity.conversion_derivation != repr.conversion_derivation
        {
            return Err(serde::de::Error::custom(
                "quantity derived fields do not match its authored value and unit",
            ));
        }
        Ok(quantity)
    }
}

/// An exact conversion result with evidence for both canonicalization legs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityConversion {
    exact_value: ExactScalar,
    source_to_canonical: UnitConversionDerivation,
    target_unit: UnitExpression,
    target_to_canonical: UnitConversionDerivation,
}

impl QuantityConversion {
    #[must_use]
    pub const fn exact_value(&self) -> &ExactScalar {
        &self.exact_value
    }

    #[must_use]
    pub const fn source_to_canonical(&self) -> &UnitConversionDerivation {
        &self.source_to_canonical
    }

    #[must_use]
    pub const fn target_unit(&self) -> &UnitExpression {
        &self.target_unit
    }

    #[must_use]
    pub const fn target_to_canonical(&self) -> &UnitConversionDerivation {
        &self.target_to_canonical
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantityError {
    Unit(UnitError),
    Scalar(ScalarError),
    ExpectedMultiplicativeUnit,
    DimensionMismatch {
        expected: Dimension,
        actual: Dimension,
    },
}

impl fmt::Display for QuantityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit(error) => error.fmt(formatter),
            Self::Scalar(error) => error.fmt(formatter),
            Self::ExpectedMultiplicativeUnit => {
                formatter.write_str("expected a multiplicative unit, found a temperature point")
            }
            Self::DimensionMismatch { expected, actual } => {
                write!(
                    formatter,
                    "dimension mismatch: expected {expected:?}, found {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for QuantityError {}

impl From<UnitError> for QuantityError {
    fn from(error: UnitError) -> Self {
        Self::Unit(error)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemperaturePoint {
    kelvin: ExactScalar,
    source_decimal: SourceDecimal,
    scale: TemperatureScale,
}

impl TemperaturePoint {
    /// Constructs an affine temperature point and rejects values below absolute
    /// zero.
    ///
    /// # Errors
    ///
    /// Returns [`TemperaturePointError::BelowAbsoluteZero`] when the exact
    /// kelvin result is negative.
    pub fn new(
        decimal: SourceDecimal,
        scale: TemperatureScale,
    ) -> Result<Self, TemperaturePointError> {
        let authored = decimal.exact_value();
        let kelvin = match scale {
            TemperatureScale::Kelvin => authored,
            TemperatureScale::DegreesCelsius => &authored + &ratio(5_463, 20),
        };
        if kelvin.is_negative() {
            return Err(TemperaturePointError::BelowAbsoluteZero);
        }
        Ok(Self {
            kelvin,
            source_decimal: decimal,
            scale,
        })
    }

    /// Converts this point to an exact number in the requested temperature
    /// scale.
    #[must_use]
    pub fn value_in(&self, scale: TemperatureScale) -> ExactScalar {
        match scale {
            TemperatureScale::Kelvin => self.kelvin.clone(),
            TemperatureScale::DegreesCelsius => &self.kelvin - &ratio(5_463, 20),
        }
    }

    #[must_use]
    pub const fn kelvin(&self) -> &ExactScalar {
        &self.kelvin
    }

    #[must_use]
    pub const fn source_decimal(&self) -> &SourceDecimal {
        &self.source_decimal
    }

    #[must_use]
    pub const fn scale(&self) -> TemperatureScale {
        self.scale
    }

    /// Returns the signed difference `self - earlier` in kelvin.
    #[must_use]
    pub fn difference_from(&self, earlier: &Self) -> TemperatureDifference {
        TemperatureDifference {
            kelvin: &self.kelvin - &earlier.kelvin,
        }
    }

    /// Compares both the exact point and the authored representation.
    #[must_use]
    pub fn source_eq(&self, other: &Self) -> bool {
        self == other && self.source_decimal == other.source_decimal && self.scale == other.scale
    }
}

impl PartialEq for TemperaturePoint {
    fn eq(&self, other: &Self) -> bool {
        self.kelvin == other.kelvin
    }
}

impl Eq for TemperaturePoint {}

impl<'de> Deserialize<'de> for TemperaturePoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            kelvin: ExactScalar,
            source_decimal: SourceDecimal,
            scale: TemperatureScale,
        }

        let repr = Repr::deserialize(deserializer)?;
        let point = Self::new(repr.source_decimal, repr.scale).map_err(serde::de::Error::custom)?;
        if point.kelvin != repr.kelvin {
            return Err(serde::de::Error::custom(
                "temperature kelvin value does not match its authored point",
            ));
        }
        Ok(point)
    }
}

/// A signed temperature interval. Source syntax constructs points only;
/// differences arise from typed operations such as point subtraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureDifference {
    kelvin: ExactScalar,
}

impl TemperatureDifference {
    #[must_use]
    pub const fn kelvin(&self) -> &ExactScalar {
        &self.kelvin
    }

    #[must_use]
    pub fn value_in(&self, scale: TemperatureScale) -> ExactScalar {
        match scale {
            TemperatureScale::Kelvin | TemperatureScale::DegreesCelsius => self.kelvin.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperaturePointError {
    BelowAbsoluteZero,
}

impl fmt::Display for TemperaturePointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("temperature point is below absolute zero")
    }
}

impl std::error::Error for TemperaturePointError {}

fn ratio(numerator: i64, denominator: i64) -> ExactScalar {
    ExactScalar::new(BigInt::from(numerator), BigInt::from(denominator))
        .expect("unit registry denominators are nonzero")
}
