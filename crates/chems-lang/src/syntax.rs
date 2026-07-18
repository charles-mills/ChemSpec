use serde::{Deserialize, Serialize};

use crate::ByteSpan;

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TokenKind {
    Bom,
    Word,
    Number,
    SubscriptNumber,
    Space,
    Newline,
    LineComment,
    BlockComment,
    Assignment,
    Arrow,
    At,
    Dot,
    MiddleDot,
    Plus,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Indent,
    Dedent,
    Eof,
    Invalid,
}

impl TokenKind {
    #[must_use]
    pub const fn is_trivia(&self) -> bool {
        matches!(
            self,
            Self::Bom | Self::Space | Self::LineComment | Self::BlockComment
        )
    }

    #[must_use]
    pub const fn is_comment(&self) -> bool {
        matches!(self, Self::LineComment | Self::BlockComment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: ByteSpan,
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntaxNode {
    pub kind: String,
    pub span: ByteSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SyntaxNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_indices: Vec<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recovery: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cst {
    pub schema_version: u32,
    pub source: String,
    pub tokens: Vec<Token>,
    pub root: SyntaxNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommentPlacement {
    Leading,
    Trailing,
    Enclosing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentAttachment {
    pub token_index: usize,
    pub placement: CommentPlacement,
    pub node_kind: String,
    pub node_span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAst {
    pub schema_version: u32,
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<SourceLanguageVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue: Option<SourceCatalogueSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<SourceReaction>,
    pub production_trace: Vec<String>,
    pub comments: Vec<CommentAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLanguageVersion {
    pub lexeme: String,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCatalogueSelection {
    pub name: String,
    pub version: String,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReaction {
    pub name: String,
    pub span: ByteSpan,
    pub reactants: Vec<SourceStructureBinding>,
    pub products: Vec<SourceStructureBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equation: Option<SourceEquation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<SourceModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observations: Option<SourceObservationBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_application: Option<SourceRuleApplication>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStructureBinding {
    pub name: String,
    pub coefficient: String,
    pub structure: String,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEquation {
    pub reactants: Vec<SourceEquationTerm>,
    pub products: Vec<SourceEquationTerm>,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEquationTerm {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coefficient: Option<String>,
    pub formula: String,
    pub representation: SourceRepresentationKind,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceRepresentationKind {
    Molecular,
    Ion,
    Ionic,
    Metallic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceModel {
    pub event: SourceEventModel,
    pub sequence: SourceSequenceModel,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceEventModel {
    Representative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceSequenceModel {
    Explanatory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObservationBlock {
    pub evidence: String,
    pub version: String,
    pub entries: Vec<SourceObservation>,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceObservation {
    GasEvolves {
        gas: String,
        claim: String,
        span: ByteSpan,
    },
    ReactantDisappears {
        reactant: String,
        claim: String,
        span: ByteSpan,
    },
    ProductForms {
        product: String,
        claim: String,
        span: ByteSpan,
    },
    ProductColour {
        product: String,
        colour: String,
        claim: String,
        span: ByteSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRuleApplication {
    pub rule: String,
    pub bindings: Vec<SourceRuleBinding>,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRuleBinding {
    pub role: String,
    pub value: String,
    pub span: ByteSpan,
}
