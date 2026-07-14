use serde::{Deserialize, Serialize};

use crate::ByteSpan;

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TokenKind {
    Word,
    Number,
    Space,
    Newline,
    LineComment,
    BlockComment,
    Assignment,
    Arrow,
    At,
    Dot,
    Colon,
    Caret,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LeftParen,
    RightParen,
    Hole,
    Indent,
    Dedent,
    Eof,
    Invalid,
}

impl TokenKind {
    #[must_use]
    pub const fn is_trivia(&self) -> bool {
        matches!(self, Self::Space | Self::LineComment | Self::BlockComment)
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
    pub experiment: Option<SourceExperiment>,
    pub document: SourceNode,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceExperiment {
    pub name: String,
    pub span: ByteSpan,
    pub conditions: Vec<SourceNode>,
    pub assumptions: Vec<SourceNode>,
    pub materials: Vec<SourceNode>,
    pub vessels: Vec<SourceNode>,
    pub procedure: Vec<SourceNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<SourceModel>,
    pub expectations: Vec<SourceExpectation>,
    pub tactics: Vec<SourceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceModel {
    pub span: ByteSpan,
    pub event: String,
    pub sequence: String,
    pub structural_rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceExpectation {
    pub span: ByteSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub claims: Vec<SourceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceNode {
    pub kind: SourceNodeKind,
    pub span: ByteSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexeme: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SourceNode>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recovery: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceNodeKind {
    Document,
    Header { form: HeaderKind },
    Experiment,
    Section { section: SectionKind },
    Declaration { form: DeclarationKind },
    Operation { operation: OperationKind },
    Claim { claim: ClaimKind },
    Observation { observation: ObservationKind },
    Tactic { tactic: TacticKind },
    Equation { form: EquationSyntaxKind },
    Chemical { form: ChemicalSyntaxKind },
    Quantity { form: QuantitySyntaxKind },
    Name { form: NameSyntaxKind },
    Hole,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderKind {
    LanguageHeader,
    LanguageVersion,
    CatalogUse,
    CatalogVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionKind {
    Conditions,
    Assumptions,
    Given,
    Vessels,
    Procedure,
    Model,
    Expectation,
    Observation,
    Proof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeclarationKind {
    ConditionEntry,
    Temperature,
    Pressure,
    Medium,
    Assumption,
    Material,
    MaterialExpression,
    SimpleMaterial,
    PreparedMaterial,
    Component,
    Vessel,
    Openness,
    ProcedureEntry,
    StageLabel,
    ModelEvent,
    ModelSequence,
    StructuralRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Operation,
    Place,
    Add,
    Combine,
    Transfer,
    Stir,
    Heat,
    Cool,
    Wait,
    Seal,
    Open,
    Filter,
    Decant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimKind {
    Entry,
    Class,
    ReactionClass,
    Identity,
    IdentityPredicate,
    Equation,
    EquationValue,
    Amount,
    Limiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservationKind {
    Entry,
    Precipitate,
    Gas,
    Colour,
    Temperature,
    TemperatureDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticKind {
    Tactic,
    Dissociate,
    InferProducts,
    Balance,
    Derive,
    CancelSpectators,
    SolveStoichiometry,
    VerifyAtoms,
    VerifyCharge,
    ProveObservations,
    Close,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EquationSyntaxKind {
    Equation,
    Kind,
    Side,
    Term,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChemicalSyntaxKind {
    Species,
    Formula,
    FormulaSegment,
    FormulaPart,
    Element,
    Charge,
    Phase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuantitySyntaxKind {
    Quantity,
    Decimal,
    UnitExpression,
    UnitProduct,
    UnitFactor,
    UnitSymbol,
    UnitName,
    SignedInteger,
    Integer,
    PositiveInteger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameSyntaxKind {
    QualifiedName,
    NameSegment,
    ValueIdentifier,
    TypeIdentifier,
    StageReference,
}
