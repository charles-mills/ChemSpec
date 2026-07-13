use serde::{Deserialize, Serialize};

use crate::ByteSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PipelineStage {
    Lexing,
    Parsing,
    Formatting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub stage: PipelineStage,
    pub summary: String,
    pub primary_span: ByteSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_spans: Vec<ByteSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    pub(crate) fn lexical(code: &str, summary: impl Into<String>, span: ByteSpan) -> Self {
        Self {
            code: code.to_owned(),
            severity: Severity::Error,
            stage: PipelineStage::Lexing,
            summary: summary.into(),
            primary_span: span,
            related_spans: Vec::new(),
            help: None,
        }
    }

    pub(crate) fn parse(code: &str, summary: impl Into<String>, span: ByteSpan) -> Self {
        Self {
            code: code.to_owned(),
            severity: Severity::Error,
            stage: PipelineStage::Parsing,
            summary: summary.into(),
            primary_span: span,
            related_spans: Vec::new(),
            help: None,
        }
    }
}
