use serde::{Deserialize, Serialize};

use chems_lang::{ByteSpan, Severity};

/// The semantic result class associated with an elaboration diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElaborationStatus {
    IllTyped,
    Unsupported,
    Invalid,
}

/// A stable source-linked type or catalogue-resolution diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElaborationDiagnostic {
    pub code: String,
    pub severity: Severity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ElaborationStatus>,
    pub summary: String,
    pub primary_span: ByteSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_spans: Vec<ByteSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl ElaborationDiagnostic {
    pub(crate) fn new(
        code: &str,
        status: ElaborationStatus,
        summary: impl Into<String>,
        primary_span: ByteSpan,
    ) -> Self {
        Self {
            code: code.to_owned(),
            severity: Severity::Error,
            status: Some(status),
            summary: summary.into(),
            primary_span,
            related_spans: Vec::new(),
            help: None,
        }
    }

    pub(crate) fn warning(code: &str, summary: impl Into<String>, primary_span: ByteSpan) -> Self {
        Self {
            code: code.to_owned(),
            severity: Severity::Warning,
            status: None,
            summary: summary.into(),
            primary_span,
            related_spans: Vec::new(),
            help: None,
        }
    }

    pub(crate) fn related(mut self, span: ByteSpan) -> Self {
        self.related_spans.push(span);
        self
    }

    pub(crate) fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}
