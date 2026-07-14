use serde::{Deserialize, Serialize};

use crate::ByteSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    OutOfBounds(ByteSpan),
    NotCharacterBoundary(ByteSpan),
    Overlapping(ByteSpan, ByteSpan),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid diagnostic safe edits: {self:?}")
    }
}

impl std::error::Error for EditError {}

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
pub struct SafeEdit {
    pub span: ByteSpan,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub stage: PipelineStage,
    pub summary: String,
    pub explanation: String,
    pub primary_span: ByteSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_spans: Vec<ByteSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safe_edits: Vec<SafeEdit>,
    #[serde(default)]
    pub emission_index: usize,
}

impl Diagnostic {
    pub(crate) fn lexical(code: &str, summary: impl Into<String>, span: ByteSpan) -> Self {
        let summary = summary.into();
        Self {
            code: code.to_owned(),
            severity: Severity::Error,
            stage: PipelineStage::Lexing,
            explanation: summary.clone(),
            summary,
            primary_span: span,
            related_spans: Vec::new(),
            help: None,
            safe_edits: Vec::new(),
            emission_index: 0,
        }
    }

    pub(crate) fn parse(code: &str, summary: impl Into<String>, span: ByteSpan) -> Self {
        let summary = summary.into();
        Self {
            code: code.to_owned(),
            severity: Severity::Error,
            stage: PipelineStage::Parsing,
            explanation: summary.clone(),
            summary,
            primary_span: span,
            related_spans: Vec::new(),
            help: None,
            safe_edits: Vec::new(),
            emission_index: 0,
        }
    }
}

/// Applies a set of non-overlapping diagnostic edits to UTF-8 source.
///
/// Edits may be supplied in any order. Their spans always refer to the
/// original source.
///
/// # Errors
///
/// Rejects out-of-bounds spans, non-character boundaries, and overlaps.
pub fn apply_safe_edits(source: &str, edits: &[SafeEdit]) -> Result<String, EditError> {
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| (edit.span.start, edit.span.end));
    let mut previous: Option<ByteSpan> = None;
    for edit in &ordered {
        if edit.span.start > edit.span.end || edit.span.end > source.len() {
            return Err(EditError::OutOfBounds(edit.span));
        }
        if !source.is_char_boundary(edit.span.start) || !source.is_char_boundary(edit.span.end) {
            return Err(EditError::NotCharacterBoundary(edit.span));
        }
        if let Some(previous_span) = previous
            && (edit.span.start < previous_span.end
                || (edit.span.is_empty()
                    && previous_span.is_empty()
                    && edit.span.start == previous_span.start))
        {
            return Err(EditError::Overlapping(previous_span, edit.span));
        }
        previous = Some(edit.span);
    }
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in ordered {
        output.push_str(&source[cursor..edit.span.start]);
        output.push_str(&edit.replacement);
        cursor = edit.span.end;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}
