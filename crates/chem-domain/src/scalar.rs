use std::{
    fmt,
    ops::{Add, Mul, Neg, Sub},
    str::FromStr,
};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarError {
    ZeroDenominator,
    DivisionByZero,
}

impl fmt::Display for ScalarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => {
                formatter.write_str("an exact scalar denominator cannot be zero")
            }
            Self::DivisionByZero => formatter.write_str("cannot divide an exact scalar by zero"),
        }
    }
}

impl std::error::Error for ScalarError {}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExactScalar(BigRational);

impl ExactScalar {
    /// Constructs a reduced exact rational.
    ///
    /// # Errors
    ///
    /// Returns [`ScalarError::ZeroDenominator`] when `denominator` is zero.
    pub fn new(numerator: BigInt, denominator: BigInt) -> Result<Self, ScalarError> {
        if denominator.is_zero() {
            return Err(ScalarError::ZeroDenominator);
        }
        Ok(Self(BigRational::new(numerator, denominator)))
    }

    #[must_use]
    pub fn from_integer(value: impl Into<BigInt>) -> Self {
        Self(BigRational::from_integer(value.into()))
    }

    #[must_use]
    pub fn zero() -> Self {
        Self(BigRational::zero())
    }

    #[must_use]
    pub fn one() -> Self {
        Self(BigRational::one())
    }

    #[must_use]
    pub fn numerator(&self) -> &BigInt {
        self.0.numer()
    }

    #[must_use]
    pub fn denominator(&self) -> &BigInt {
        self.0.denom()
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.0.is_negative()
    }

    /// Divides two exact values.
    ///
    /// # Errors
    ///
    /// Returns [`ScalarError::DivisionByZero`] if `divisor` is zero.
    pub fn checked_div(&self, divisor: &Self) -> Result<Self, ScalarError> {
        if divisor.is_zero() {
            return Err(ScalarError::DivisionByZero);
        }
        Ok(Self(&self.0 / &divisor.0))
    }

    /// Raises this value to an exact signed integer power.
    ///
    /// # Errors
    ///
    /// Returns [`ScalarError::DivisionByZero`] when zero is raised to a
    /// negative power.
    pub fn pow_i32(&self, exponent: i32) -> Result<Self, ScalarError> {
        if exponent == 0 {
            return Ok(Self::one());
        }
        if exponent < 0 && self.is_zero() {
            return Err(ScalarError::DivisionByZero);
        }
        let magnitude = exponent.unsigned_abs();
        let numerator = self.numerator().pow(magnitude);
        let denominator = self.denominator().pow(magnitude);
        if exponent < 0 {
            Self::new(denominator, numerator)
        } else {
            Self::new(numerator, denominator)
        }
    }
}

impl fmt::Debug for ExactScalar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for ExactScalar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator().is_one() {
            write!(formatter, "{}", self.numerator())
        } else {
            write!(formatter, "{}/{}", self.numerator(), self.denominator())
        }
    }
}

impl Serialize for ExactScalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ExactScalar", 2)?;
        state.serialize_field("numerator", &self.numerator().to_string())?;
        state.serialize_field("denominator", &self.denominator().to_string())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ExactScalar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            numerator: String,
            denominator: String,
        }

        let repr = Repr::deserialize(deserializer)?;
        let numerator = BigInt::from_str(&repr.numerator).map_err(serde::de::Error::custom)?;
        let denominator = BigInt::from_str(&repr.denominator).map_err(serde::de::Error::custom)?;
        Self::new(numerator, denominator).map_err(serde::de::Error::custom)
    }
}

impl Add<&ExactScalar> for &ExactScalar {
    type Output = ExactScalar;

    fn add(self, right: &ExactScalar) -> Self::Output {
        ExactScalar(&self.0 + &right.0)
    }
}

impl Sub<&ExactScalar> for &ExactScalar {
    type Output = ExactScalar;

    fn sub(self, right: &ExactScalar) -> Self::Output {
        ExactScalar(&self.0 - &right.0)
    }
}

impl Mul<&ExactScalar> for &ExactScalar {
    type Output = ExactScalar;

    fn mul(self, right: &ExactScalar) -> Self::Output {
        ExactScalar(&self.0 * &right.0)
    }
}

impl Neg for &ExactScalar {
    type Output = ExactScalar;

    fn neg(self) -> Self::Output {
        ExactScalar(-&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrittenPrecision {
    pub decimal_places: u32,
    pub written_digits: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significant_digits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDecimalError {
    Empty,
    InvalidSyntax,
    TooManyDigits,
}

impl fmt::Display for SourceDecimalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a source decimal cannot be empty"),
            Self::InvalidSyntax => formatter.write_str("invalid source decimal syntax"),
            Self::TooManyDigits => {
                formatter.write_str("source decimal precision exceeds supported metadata limits")
            }
        }
    }
}

impl std::error::Error for SourceDecimalError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDecimal {
    lexeme: String,
    coefficient: BigInt,
    scale: u32,
    precision: WrittenPrecision,
}

impl SourceDecimal {
    /// Parses the language's signed decimal representation without binary
    /// floating point.
    ///
    /// # Errors
    ///
    /// Returns a [`SourceDecimalError`] for empty input, non-decimal syntax, or
    /// metadata counts that cannot fit in the public schema.
    pub fn parse(source: &str) -> Result<Self, SourceDecimalError> {
        if source.is_empty() {
            return Err(SourceDecimalError::Empty);
        }
        let (negative, unsigned) = match source.as_bytes()[0] {
            b'+' => (false, &source[1..]),
            b'-' => (true, &source[1..]),
            _ => (false, source),
        };
        if unsigned.is_empty() {
            return Err(SourceDecimalError::InvalidSyntax);
        }
        let mut pieces = unsigned.split('.');
        let integer = pieces.next().unwrap_or_default();
        let fractional = pieces.next();
        if pieces.next().is_some()
            || integer.is_empty()
            || !integer.bytes().all(|byte| byte.is_ascii_digit())
            || fractional.is_some_and(|part| {
                part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(SourceDecimalError::InvalidSyntax);
        }

        let fractional = fractional.unwrap_or_default();
        let scale =
            u32::try_from(fractional.len()).map_err(|_| SourceDecimalError::TooManyDigits)?;
        let written_digits = u32::try_from(integer.len() + fractional.len())
            .map_err(|_| SourceDecimalError::TooManyDigits)?;
        let digits = format!("{integer}{fractional}");
        let mut coefficient =
            BigInt::from_str(&digits).map_err(|_| SourceDecimalError::InvalidSyntax)?;
        if negative {
            coefficient = -coefficient;
        }
        let significant_digits = digits
            .bytes()
            .position(|byte| byte != b'0')
            .map(|first| u32::try_from(digits.len() - first))
            .transpose()
            .map_err(|_| SourceDecimalError::TooManyDigits)?;

        Ok(Self {
            lexeme: source.to_owned(),
            coefficient,
            scale,
            precision: WrittenPrecision {
                decimal_places: scale,
                written_digits,
                significant_digits,
            },
        })
    }

    #[must_use]
    pub fn lexeme(&self) -> &str {
        &self.lexeme
    }

    #[must_use]
    pub fn canonical_lexeme(&self) -> &str {
        self.lexeme.strip_prefix('+').unwrap_or(&self.lexeme)
    }

    #[must_use]
    pub fn coefficient(&self) -> &BigInt {
        &self.coefficient
    }

    #[must_use]
    pub const fn scale(&self) -> u32 {
        self.scale
    }

    #[must_use]
    pub const fn precision(&self) -> &WrittenPrecision {
        &self.precision
    }

    #[must_use]
    pub fn exact_value(&self) -> ExactScalar {
        let denominator = BigInt::from(10_u8).pow(self.scale);
        ExactScalar(BigRational::new(self.coefficient.clone(), denominator))
    }
}

impl Serialize for SourceDecimal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("SourceDecimal", 4)?;
        state.serialize_field("lexeme", &self.lexeme)?;
        state.serialize_field("coefficient", &self.coefficient.to_string())?;
        state.serialize_field("scale", &self.scale)?;
        state.serialize_field("precision", &self.precision)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for SourceDecimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Repr {
            lexeme: String,
            coefficient: String,
            scale: u32,
            precision: WrittenPrecision,
        }

        let repr = Repr::deserialize(deserializer)?;
        let parsed = Self::parse(&repr.lexeme).map_err(serde::de::Error::custom)?;
        if parsed.coefficient.to_string() != repr.coefficient
            || parsed.scale != repr.scale
            || parsed.precision != repr.precision
        {
            return Err(serde::de::Error::custom(
                "source decimal derived fields do not match its lexeme",
            ));
        }
        Ok(parsed)
    }
}

impl FromStr for SourceDecimal {
    type Err = SourceDecimalError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::parse(source)
    }
}
