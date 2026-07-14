use std::fmt;

use chems_lang::ByteSpan;
use serde::Serialize;

/// Stable top-level result class for failed elaboration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionFailureClass {
    InvalidSource,
    UnsupportedChemistry,
    AmbiguousChemistry,
    CorruptTrustedData,
}

/// Typed elaboration/expansion failure with a stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionError {
    class: ExpansionFailureClass,
    code: &'static str,
    message: String,
    span: Option<ByteSpan>,
}

impl ExpansionError {
    pub(crate) fn invalid(
        code: &'static str,
        message: impl Into<String>,
        span: Option<ByteSpan>,
    ) -> Self {
        Self {
            class: ExpansionFailureClass::InvalidSource,
            code,
            message: message.into(),
            span,
        }
    }

    pub(crate) fn unsupported(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            class: ExpansionFailureClass::UnsupportedChemistry,
            code,
            message: message.into(),
            span: None,
        }
    }

    pub(crate) fn ambiguous(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            class: ExpansionFailureClass::AmbiguousChemistry,
            code,
            message: message.into(),
            span: None,
        }
    }

    pub(crate) fn system(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            class: ExpansionFailureClass::CorruptTrustedData,
            code,
            message: message.into(),
            span: None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ExpansionFailureClass {
        self.class
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub const fn span(&self) -> Option<ByteSpan> {
        self.span
    }
}

impl fmt::Display for ExpansionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {:?}: {}",
            self.code, self.class, self.message
        )
    }
}

impl std::error::Error for ExpansionError {}
