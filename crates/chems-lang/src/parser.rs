use crate::{
    ByteSpan, CommentAttachment, CommentPlacement, Cst, Diagnostic, LexResult, SafeEdit, SourceAst,
    SourceCatalogueSelection, SourceEquation, SourceEquationTerm, SourceEventModel,
    SourceLanguageVersion, SourceModel, SourceObservation, SourceObservationBlock, SourceReaction,
    SourceRepresentationKind, SourceRuleApplication, SourceRuleBinding, SourceSequenceModel,
    SourceStructureBinding, SyntaxNode, Token, TokenKind, lex_bytes, lex_source,
};

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub cst: Cst,
    pub ast: SourceAst,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseResult {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.ast.complete && self.diagnostics.is_empty()
    }
}

#[must_use]
pub fn parse_bytes(bytes: &[u8]) -> ParseResult {
    parse_lexed(lex_bytes(bytes))
}

#[must_use]
pub fn parse_source(source: &str) -> ParseResult {
    parse_lexed(lex_source(source))
}

fn parse_lexed(lexed: LexResult) -> ParseResult {
    if lexed
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "CHEMS-L001")
    {
        return fatal_lex_result(lexed);
    }
    let semantic = lexed
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token.kind.is_trivia())
        .map(|(index, _)| index)
        .collect();
    let mut parser = Parser {
        source: &lexed.source,
        tokens: &lexed.tokens,
        semantic,
        position: 0,
        last_end: 0,
        stack: Vec::new(),
        trace: Vec::new(),
        diagnostics: lexed.diagnostics,
        language: None,
        catalogue: None,
        reaction: None,
        halted: false,
    };
    parser.begin("document");
    parser.parse_document();
    let mut root = parser.finish(false);
    root.span = ByteSpan::new(0, parser.source.len());
    parser.sort_diagnostics();
    let complete = parser.diagnostics.is_empty()
        && parser
            .language
            .as_ref()
            .is_some_and(|value| value.lexeme == "1")
        && parser.catalogue.is_some()
        && parser.reaction.is_some();
    let mut production_trace = std::mem::take(&mut parser.trace);
    production_trace.sort();
    production_trace.dedup();
    let comments = attach_comments(parser.source, parser.tokens, &root);
    let language = parser.language.take();
    let catalogue = parser.catalogue.take();
    let reaction = parser.reaction.take();
    let diagnostics = std::mem::take(&mut parser.diagnostics);
    drop(parser);
    ParseResult {
        cst: Cst {
            schema_version: 1,
            source: lexed.source,
            tokens: lexed.tokens,
            root,
        },
        ast: SourceAst {
            schema_version: 1,
            complete,
            language,
            catalogue,
            reaction,
            production_trace,
            comments,
        },
        diagnostics,
    }
}

fn fatal_lex_result(lexed: LexResult) -> ParseResult {
    let span = ByteSpan::new(0, lexed.source.len());
    let recovery = SyntaxNode {
        kind: "recovery".to_owned(),
        span,
        children: Vec::new(),
        token_indices: Vec::new(),
        recovery: true,
    };
    ParseResult {
        cst: Cst {
            schema_version: 1,
            source: lexed.source,
            tokens: lexed.tokens,
            root: SyntaxNode {
                kind: "document".to_owned(),
                span,
                children: vec![recovery],
                token_indices: Vec::new(),
                recovery: true,
            },
        },
        ast: SourceAst {
            schema_version: 1,
            complete: false,
            language: None,
            catalogue: None,
            reaction: None,
            production_trace: vec!["document".to_owned(), "recovery".to_owned()],
            comments: Vec::new(),
        },
        diagnostics: lexed.diagnostics,
    }
}

struct NodeBuilder {
    kind: String,
    start: usize,
    children: Vec<SyntaxNode>,
    token_indices: Vec<usize>,
}

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    semantic: Vec<usize>,
    position: usize,
    last_end: usize,
    stack: Vec<NodeBuilder>,
    trace: Vec<String>,
    diagnostics: Vec<Diagnostic>,
    language: Option<SourceLanguageVersion>,
    catalogue: Option<SourceCatalogueSelection>,
    reaction: Option<SourceReaction>,
    halted: bool,
}

impl Parser<'_> {
    fn parse_document(&mut self) {
        self.consume_newlines();
        if self.at_kind(TokenKind::Eof) {
            self.error_here(
                "CHEMS-P001",
                "source must contain a complete `chems 1` document",
            );
            return;
        }
        self.parse_language_header();
        if self.halted {
            return;
        }
        self.consume_newlines();
        self.parse_catalog_use();
        if self.halted {
            return;
        }
        self.consume_newlines();
        self.parse_reaction_declaration();
        if self.halted {
            return;
        }
        self.consume_newlines();
        if !self.at_kind(TokenKind::Eof) {
            self.error_here(
                "CHEMS-P007",
                "discarded quantitative or extra source syntax is not part of chems 1",
            );
            while !self.at_kind(TokenKind::Eof) {
                self.bump();
            }
        }
        self.expect_kind(TokenKind::Eof, "end of file");
    }

    fn parse_language_header(&mut self) {
        self.begin("language-header");
        let start = self.current_span().start;
        if !self.eat_literal("chems") {
            let insertion = self.current_span().start;
            self.error(
                "CHEMS-P001",
                "source must begin with `chems 1`",
                self.current_span(),
            );
            if self.at_literal("use")
                && let Some(diagnostic) = self.diagnostics.last_mut()
            {
                "Every source unit requires an explicit chems 1 authority header; the parser never guesses it."
                    .clone_into(&mut diagnostic.explanation);
                diagnostic.help = Some("insert `chems 1` as the first logical line".to_owned());
                diagnostic.safe_edits.push(SafeEdit {
                    span: ByteSpan::empty(insertion),
                    replacement: "chems 1\n".to_owned(),
                });
            }
        }
        self.begin("language-version");
        let span = self.current_span();
        let version = self.parse_positive_integer();
        self.finish(version.is_none());
        if let Some(lexeme) = version {
            if lexeme != "1" {
                self.error(
                    "CHEMS-P002",
                    format!("unsupported .chems language major {lexeme}"),
                    span,
                );
            }
            self.language = Some(SourceLanguageVersion { lexeme, span });
        }
        self.expect_newline();
        self.finish(self.last_end <= start);
    }

    fn parse_catalog_use(&mut self) {
        self.begin("catalog-use");
        let start = self.current_span().start;
        self.expect_literal("use");
        self.expect_literal("catalog");
        let name = self.parse_qualified_name();
        self.expect_kind(TokenKind::At, "`@`");
        self.begin("catalog-version");
        let mut version = self.parse_integer().unwrap_or_default();
        while self.eat_kind(TokenKind::Dot) {
            version.push('.');
            if let Some(component) = self.parse_integer() {
                version.push_str(&component);
            }
        }
        self.finish(version.is_empty());
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        if let Some(name) = name
            && !version.is_empty()
        {
            self.catalogue = Some(SourceCatalogueSelection {
                name,
                version,
                span,
            });
        }
        self.finish(false);
    }

    #[allow(clippy::too_many_lines)]
    fn parse_reaction_declaration(&mut self) {
        self.begin("reaction-declaration");
        let start = self.current_span().start;
        self.expect_literal("reaction");
        let name = self.parse_type_identifier();
        self.expect_literal("where");
        self.expect_newline();
        self.expect_indent("an indented reaction body");
        if self.halted {
            self.finish(true);
            return;
        }
        let reactants = self.parse_structure_section(true);
        if self.halted {
            self.finish(true);
            return;
        }
        let products = self.parse_structure_section(false);
        if self.halted {
            self.finish(true);
            return;
        }
        let equation = self.parse_equation_section();
        if self.halted {
            self.finish(true);
            return;
        }
        let model = self.parse_model_section();
        if self.halted {
            self.finish(true);
            return;
        }
        let observations = self.parse_observation_section();
        if self.halted {
            self.finish(true);
            return;
        }
        let rule_application = self.parse_proof_section();
        self.expect_kind(TokenKind::Dedent, "the end of the reaction body");
        let span = ByteSpan::new(start, self.last_end);
        if let Some(name) = name {
            self.reaction = Some(SourceReaction {
                name,
                span,
                reactants,
                products,
                equation,
                model,
                observations,
                rule_application,
            });
        }
        self.finish(false);
    }

    fn parse_structure_section(&mut self, reactants: bool) -> Vec<SourceStructureBinding> {
        let production = if reactants {
            "reactants-section"
        } else {
            "products-section"
        };
        let keyword = if reactants { "reactants" } else { "products" };
        self.begin(production);
        self.expect_literal(keyword);
        self.expect_newline();
        self.expect_indent(&format!("an indented {keyword} block"));
        let mut bindings = Vec::new();
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.consume_block_newlines(keyword);
                continue;
            }
            if self.at_literal(if reactants { "products" } else { "equation" }) {
                break;
            }
            if let Some(binding) = self.parse_structure_declaration(reactants) {
                bindings.push(binding);
            }
        }
        if bindings.is_empty() {
            self.error_here(
                "CHEMS-P003",
                &format!("{keyword} requires at least one declaration"),
            );
        }
        self.expect_kind(TokenKind::Dedent, &format!("the end of {keyword}"));
        self.consume_newlines();
        self.finish(bindings.is_empty());
        bindings
    }

    fn parse_structure_declaration(&mut self, reactant: bool) -> Option<SourceStructureBinding> {
        self.begin(if reactant {
            "reactant-declaration"
        } else {
            "product-declaration"
        });
        let start = self.current_span().start;
        let name = self.parse_value_identifier();
        self.expect_kind(TokenKind::Assignment, "`:=`");
        let coefficient = self.parse_positive_integer();
        self.expect_literal("of");
        let structure = self.parse_qualified_name();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(name.is_none() || coefficient.is_none() || structure.is_none());
        Some(SourceStructureBinding {
            name: name?,
            coefficient: coefficient?,
            structure: structure?,
            span,
        })
    }

    fn parse_equation_section(&mut self) -> Option<SourceEquation> {
        self.begin("equation-section");
        self.expect_literal("equation");
        self.expect_newline();
        self.expect_indent("an indented equation block");
        let equation = self.parse_equation();
        self.expect_newline();
        self.consume_newlines();
        self.expect_kind(TokenKind::Dedent, "the end of equation");
        self.consume_newlines();
        self.finish(equation.is_none());
        equation
    }

    fn parse_equation(&mut self) -> Option<SourceEquation> {
        self.begin("equation");
        let start = self.current_span().start;
        let reactants = self.parse_equation_side();
        if self.at_kind(TokenKind::Newline) && self.peek_kind_n(1) == Some(&TokenKind::Arrow) {
            self.bump();
        }
        self.expect_kind(TokenKind::Arrow, "`->`");
        if self.at_kind(TokenKind::Newline) {
            self.bump();
        }
        let products = self.parse_equation_side();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(reactants.is_empty() || products.is_empty());
        (!reactants.is_empty() && !products.is_empty()).then_some(SourceEquation {
            reactants,
            products,
            span,
        })
    }

    fn parse_equation_side(&mut self) -> Vec<SourceEquationTerm> {
        self.begin("equation-side");
        let mut terms = Vec::new();
        if let Some(term) = self.parse_equation_term() {
            terms.push(term);
        }
        loop {
            if self.at_kind(TokenKind::Plus) {
                self.bump();
                if self.at_kind(TokenKind::Newline) {
                    self.bump();
                }
            } else if self.at_kind(TokenKind::Newline)
                && self.peek_kind_n(1) == Some(&TokenKind::Plus)
            {
                self.bump();
                self.bump();
                if self.at_kind(TokenKind::Newline) {
                    self.bump();
                }
            } else {
                break;
            }
            if let Some(term) = self.parse_equation_term() {
                terms.push(term);
            }
        }
        self.finish(terms.is_empty());
        terms
    }

    fn parse_equation_term(&mut self) -> Option<SourceEquationTerm> {
        self.begin("equation-term");
        let start = self.current_span().start;
        let coefficient = self
            .at_kind(TokenKind::Number)
            .then(|| self.parse_positive_integer())
            .flatten();
        let formula = self.parse_formula();
        self.expect_kind(TokenKind::LeftBracket, "`[`");
        let representation = self.parse_representation_kind();
        self.expect_kind(TokenKind::RightBracket, "`]`");
        let span = ByteSpan::new(start, self.last_end);
        self.finish(formula.is_none() || representation.is_none());
        Some(SourceEquationTerm {
            coefficient,
            formula: formula?,
            representation: representation?,
            span,
        })
    }

    fn parse_representation_kind(&mut self) -> Option<SourceRepresentationKind> {
        self.begin("representation-kind");
        let value = match self.peek_text() {
            Some("molecular") => Some(SourceRepresentationKind::Molecular),
            Some("ion") => Some(SourceRepresentationKind::Ion),
            Some("ionic") => Some(SourceRepresentationKind::Ionic),
            Some("metallic") => Some(SourceRepresentationKind::Metallic),
            _ => None,
        };
        if value.is_some() {
            self.bump();
        } else {
            self.expected("a representation kind");
        }
        self.finish(value.is_none());
        value
    }

    fn parse_model_section(&mut self) -> Option<SourceModel> {
        self.begin("model-section");
        let start = self.current_span().start;
        self.expect_literal("model");
        self.expect_newline();
        self.expect_indent("an indented model block");
        self.begin("event-model-entry");
        self.expect_literal("event");
        self.expect_kind(TokenKind::Assignment, "`:=`");
        let event = self.eat_literal("representative");
        if !event {
            self.expected("`representative`");
        }
        self.expect_newline();
        self.finish(!event);
        self.begin("sequence-model-entry");
        self.expect_literal("sequence");
        self.expect_kind(TokenKind::Assignment, "`:=`");
        let sequence = self.eat_literal("explanatory");
        if !sequence {
            self.expected("`explanatory`");
        }
        self.expect_newline();
        self.finish(!sequence);
        self.consume_newlines();
        self.expect_kind(TokenKind::Dedent, "the end of model");
        self.consume_newlines();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(!event || !sequence);
        (event && sequence).then_some(SourceModel {
            event: SourceEventModel::Representative,
            sequence: SourceSequenceModel::Explanatory,
            span,
        })
    }

    fn parse_observation_section(&mut self) -> Option<SourceObservationBlock> {
        self.begin("observation-section");
        let start = self.current_span().start;
        self.expect_literal("observe");
        self.expect_literal("from");
        self.begin("evidence-reference");
        let evidence = self.parse_qualified_name();
        self.expect_kind(TokenKind::At, "`@`");
        let version = self.parse_catalog_version();
        self.finish(evidence.is_none() || version.is_none());
        self.expect_newline();
        self.expect_indent("an indented observation block");
        let mut entries = Vec::new();
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.consume_block_newlines("observe");
                continue;
            }
            if let Some(entry) = self.parse_observation_entry() {
                entries.push(entry);
            }
        }
        if entries.is_empty() {
            self.error_here(
                "CHEMS-P003",
                "observe requires at least one typed observation",
            );
        }
        self.expect_kind(TokenKind::Dedent, "the end of observe");
        self.consume_newlines();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(evidence.is_none() || version.is_none() || entries.is_empty());
        Some(SourceObservationBlock {
            evidence: evidence?,
            version: version?,
            entries,
            span,
        })
    }

    fn parse_observation_entry(&mut self) -> Option<SourceObservation> {
        self.begin("observation-entry");
        let start = self.current_span().start;
        let result = match self.peek_text() {
            Some("gas") => self.parse_gas_observation(start),
            Some("reactant") => self.parse_disappearance_observation(start),
            Some("product") if self.peek_text_n(2) == Some("forms") => {
                self.parse_formation_observation(start)
            }
            Some("product") => self.parse_colour_observation(start),
            _ => {
                self.recover_line("a typed observation");
                None
            }
        };
        self.finish(result.is_none());
        result
    }

    fn parse_gas_observation(&mut self, start: usize) -> Option<SourceObservation> {
        self.begin("gas-observation");
        self.expect_literal("gas");
        let gas = self.parse_value_identifier();
        self.expect_literal("evolves");
        let claim = self.parse_claim_reference();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(gas.is_none() || claim.is_none());
        Some(SourceObservation::GasEvolves {
            gas: gas?,
            claim: claim?,
            span,
        })
    }

    fn parse_disappearance_observation(&mut self, start: usize) -> Option<SourceObservation> {
        self.begin("disappearance-observation");
        self.expect_literal("reactant");
        let reactant = self.parse_value_identifier();
        self.expect_literal("disappears");
        let claim = self.parse_claim_reference();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(reactant.is_none() || claim.is_none());
        Some(SourceObservation::ReactantDisappears {
            reactant: reactant?,
            claim: claim?,
            span,
        })
    }

    fn parse_formation_observation(&mut self, start: usize) -> Option<SourceObservation> {
        self.begin("formation-observation");
        self.expect_literal("product");
        let product = self.parse_value_identifier();
        self.expect_literal("forms");
        let claim = self.parse_claim_reference();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(product.is_none() || claim.is_none());
        Some(SourceObservation::ProductForms {
            product: product?,
            claim: claim?,
            span,
        })
    }

    fn parse_colour_observation(&mut self, start: usize) -> Option<SourceObservation> {
        self.begin("colour-observation");
        self.expect_literal("product");
        let product = self.parse_value_identifier();
        self.expect_literal("has");
        self.expect_literal("colour");
        let colour = self.parse_qualified_name();
        let claim = self.parse_claim_reference();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(product.is_none() || colour.is_none() || claim.is_none());
        Some(SourceObservation::ProductColour {
            product: product?,
            colour: colour?,
            claim: claim?,
            span,
        })
    }

    fn parse_claim_reference(&mut self) -> Option<String> {
        self.begin("claim-reference");
        self.expect_literal("claim");
        let claim = self.parse_claim_identifier();
        self.finish(claim.is_none());
        claim
    }

    fn parse_proof_section(&mut self) -> Option<SourceRuleApplication> {
        self.begin("proof-section");
        self.expect_literal("by");
        self.expect_newline();
        self.expect_indent("an indented proof block");
        let application = self.parse_rule_application();
        self.expect_kind(TokenKind::Dedent, "the end of by");
        self.finish(application.is_none());
        application
    }

    fn parse_rule_application(&mut self) -> Option<SourceRuleApplication> {
        self.begin("rule-application");
        let start = self.current_span().start;
        self.expect_literal("apply");
        let rule = self.parse_qualified_name();
        self.expect_newline();
        self.expect_indent("indented rule bindings");
        let mut bindings = Vec::new();
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.consume_block_newlines("apply");
                continue;
            }
            if let Some(binding) = self.parse_rule_binding() {
                bindings.push(binding);
            }
        }
        if bindings.is_empty() {
            self.error_here("CHEMS-P003", "apply requires at least one role binding");
        }
        self.expect_kind(TokenKind::Dedent, "the end of rule bindings");
        self.consume_newlines();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(rule.is_none() || bindings.is_empty());
        Some(SourceRuleApplication {
            rule: rule?,
            bindings,
            span,
        })
    }

    fn parse_rule_binding(&mut self) -> Option<SourceRuleBinding> {
        self.begin("rule-binding");
        let start = self.current_span().start;
        let role = self.parse_value_identifier();
        self.expect_kind(TokenKind::Assignment, "`:=`");
        let value = self.parse_value_identifier();
        self.expect_newline();
        let span = ByteSpan::new(start, self.last_end);
        self.finish(role.is_none() || value.is_none());
        Some(SourceRuleBinding {
            role: role?,
            value: value?,
            span,
        })
    }

    fn parse_catalog_version(&mut self) -> Option<String> {
        self.begin("catalog-version");
        let Some(mut version) = self.parse_integer() else {
            self.finish(true);
            return None;
        };
        while self.eat_kind(TokenKind::Dot) {
            version.push('.');
            let Some(component) = self.parse_integer() else {
                self.finish(true);
                return None;
            };
            version.push_str(&component);
        }
        self.finish(false);
        Some(version)
    }

    fn parse_formula(&mut self) -> Option<String> {
        self.begin("formula");
        let position = self.position;
        let mut valid = self.parse_formula_segment();
        while self.at_kind(TokenKind::Dot) {
            self.bump();
            if self.at_kind(TokenKind::Number) {
                valid &= self.parse_positive_integer().is_some();
            }
            valid &= self.parse_formula_segment();
        }
        let formula = self.semantic[position..self.position]
            .iter()
            .filter_map(|index| self.tokens.get(*index))
            .map(|token| token.text.as_str())
            .collect::<String>();
        self.finish(!valid);
        valid.then_some(formula)
    }

    fn parse_formula_segment(&mut self) -> bool {
        self.begin("formula-segment");
        let mut count = 0;
        while self.at_kind(TokenKind::Word) || self.at_kind(TokenKind::LeftParen) {
            if self.at_kind(TokenKind::Word) {
                let parsed = self.parse_formula_word_parts();
                if parsed == 0 {
                    break;
                }
                count += parsed;
            } else if self.parse_parenthesized_formula_part() {
                count += 1;
            } else {
                break;
            }
        }
        if count == 0 {
            self.expected("a formula segment");
        }
        self.finish(count == 0);
        count > 0
    }

    fn parse_formula_word_parts(&mut self) -> usize {
        let Some(token) = self.current_token().cloned() else {
            return 0;
        };
        let mut parts = formula_word_parts(&token);
        if parts.is_empty() {
            self.error(
                "CHEMS-P003",
                "formula words must be element symbols with positive counts",
                token.span,
            );
            self.bump();
            return 0;
        }
        let accepts_external_count = parts.last().is_some_and(|part| part.count.is_none());
        self.bump();
        if accepts_external_count && self.at_kind(TokenKind::Number) {
            let count = self
                .current_token()
                .expect("number token is present")
                .clone();
            if count.text.starts_with('0') {
                self.error(
                    "CHEMS-P003",
                    "expected a positive integer without a leading zero",
                    count.span,
                );
            } else if let Some(part) = parts.last_mut() {
                part.count = Some(count.span);
            }
            self.bump();
        }
        let count = parts.len();
        self.record_formula_word_parts(parts);
        count
    }

    fn parse_parenthesized_formula_part(&mut self) -> bool {
        self.begin("formula-part");
        let valid = if self.eat_kind(TokenKind::LeftParen) {
            let nested = self.parse_formula_segment();
            self.expect_kind(TokenKind::RightParen, "`)`");
            if self.at_kind(TokenKind::Number) {
                let _ = self.parse_positive_integer();
            }
            nested
        } else {
            false
        };
        self.finish(!valid);
        valid
    }

    fn parse_qualified_name(&mut self) -> Option<String> {
        self.begin("qualified-name");
        let Some(mut value) = self.parse_name_segment() else {
            self.finish(true);
            return None;
        };
        while self.eat_kind(TokenKind::Dot) {
            value.push('.');
            let Some(segment) = self.parse_name_segment() else {
                self.finish(true);
                return None;
            };
            value.push_str(&segment);
        }
        self.finish(false);
        Some(value)
    }

    fn parse_name_segment(&mut self) -> Option<String> {
        self.begin("name-segment");
        let value = self.parse_identifier(IdentifierClass::General);
        self.finish(value.is_none());
        value
    }

    fn parse_type_identifier(&mut self) -> Option<String> {
        self.begin("type-identifier");
        let value = self.parse_identifier(IdentifierClass::Type);
        self.finish(value.is_none());
        value
    }

    fn parse_value_identifier(&mut self) -> Option<String> {
        self.begin("value-identifier");
        let value = self.parse_identifier(IdentifierClass::Value);
        self.finish(value.is_none());
        value
    }

    fn parse_claim_identifier(&mut self) -> Option<String> {
        self.begin("claim-identifier");
        let value = self.parse_identifier(IdentifierClass::Claim);
        self.finish(value.is_none());
        value
    }

    fn parse_identifier(&mut self, class: IdentifierClass) -> Option<String> {
        self.begin("identifier");
        let Some(token) = self.current_token().cloned() else {
            self.expected(&format!("a {class} identifier"));
            self.finish(true);
            return None;
        };
        let bytes = token.text.as_bytes();
        let valid = match class {
            IdentifierClass::General => bytes.first().is_some_and(u8::is_ascii_alphabetic),
            IdentifierClass::Type => bytes.first().is_some_and(u8::is_ascii_uppercase),
            IdentifierClass::Value => bytes.first().is_some_and(u8::is_ascii_lowercase),
            IdentifierClass::Claim => {
                bytes.first().is_some_and(u8::is_ascii_uppercase)
                    && bytes.iter().all(|byte| {
                        byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_'
                    })
            }
        } && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if !valid || token.kind != TokenKind::Word {
            self.error(
                "CHEMS-P003",
                format!("`{}` is not a valid {class} identifier", token.text),
                token.span,
            );
            self.bump_unexpected();
            self.finish(true);
            return None;
        }
        if is_reserved(&token.text) {
            self.error(
                "CHEMS-P004",
                format!(
                    "reserved word `{}` cannot be used as an identifier",
                    token.text
                ),
                token.span,
            );
        }
        if token.text.len() > 1 {
            self.record_virtual(
                "identifier-continue",
                ByteSpan::new(token.span.start + 1, token.span.end),
            );
        }
        self.bump();
        self.finish(false);
        Some(token.text)
    }

    fn parse_integer(&mut self) -> Option<String> {
        self.begin("integer");
        let Some(token) = self.current_token().cloned() else {
            self.expected("an integer");
            self.finish(true);
            return None;
        };
        if token.kind != TokenKind::Number {
            self.expected("an integer");
            self.finish(true);
            return None;
        }
        self.bump();
        self.finish(false);
        Some(token.text)
    }

    fn parse_positive_integer(&mut self) -> Option<String> {
        self.begin("positive-integer");
        let Some(token) = self.current_token().cloned() else {
            self.expected("a positive integer");
            self.finish(true);
            return None;
        };
        if token.kind != TokenKind::Number || token.text.starts_with('0') {
            self.error(
                "CHEMS-P003",
                "expected a positive integer without a leading zero",
                token.span,
            );
            self.bump_unexpected();
            self.finish(true);
            return None;
        }
        self.bump();
        self.finish(false);
        Some(token.text)
    }

    fn begin(&mut self, kind: &str) {
        self.stack.push(NodeBuilder {
            kind: kind.to_owned(),
            start: self.current_span().start,
            children: Vec::new(),
            token_indices: Vec::new(),
        });
    }

    fn finish(&mut self, recovery: bool) -> SyntaxNode {
        let builder = self.stack.pop().expect("every syntax node is balanced");
        let node = SyntaxNode {
            kind: builder.kind,
            span: ByteSpan::new(builder.start, self.last_end.max(builder.start)),
            children: builder.children,
            token_indices: builder.token_indices,
            recovery,
        };
        self.trace.push(node.kind.clone());
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(node.clone());
        }
        node
    }

    fn record_virtual(&mut self, kind: &str, span: ByteSpan) {
        let node = SyntaxNode {
            kind: kind.to_owned(),
            span,
            children: Vec::new(),
            token_indices: Vec::new(),
            recovery: kind == "recovery",
        };
        self.trace.push(kind.to_owned());
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(node);
        }
    }

    fn record_formula_word_parts(&mut self, parts: Vec<FormulaWordPart>) {
        for part in parts {
            let mut children = vec![SyntaxNode {
                kind: "element".to_owned(),
                span: part.element,
                children: Vec::new(),
                token_indices: Vec::new(),
                recovery: false,
            }];
            self.trace.push("element".to_owned());
            if let Some(count) = part.count {
                children.push(SyntaxNode {
                    kind: "positive-integer".to_owned(),
                    span: count,
                    children: Vec::new(),
                    token_indices: Vec::new(),
                    recovery: false,
                });
                self.trace.push("positive-integer".to_owned());
            }
            let span = ByteSpan::new(
                part.element.start,
                part.count.map_or(part.element.end, |count| count.end),
            );
            let node = SyntaxNode {
                kind: "formula-part".to_owned(),
                span,
                children,
                token_indices: Vec::new(),
                recovery: false,
            };
            self.trace.push("formula-part".to_owned());
            if let Some(parent) = self.stack.last_mut() {
                parent.children.push(node);
            }
        }
    }

    fn bump(&mut self) {
        if let Some(raw_index) = self.semantic.get(self.position).copied() {
            let token = &self.tokens[raw_index];
            if let Some(node) = self.stack.last_mut() {
                node.token_indices.push(raw_index);
            }
            self.last_end = token.span.end;
            self.position += 1;
        }
    }

    fn bump_unexpected(&mut self) {
        if !matches!(
            self.peek_kind(),
            Some(TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof) | None
        ) {
            self.bump();
        }
    }

    fn eat_literal(&mut self, literal: &str) -> bool {
        if self.at_literal(literal) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kind(&mut self, kind: TokenKind) -> bool {
        if self.at_kind(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_literal(&mut self, literal: &str) {
        if !self.eat_literal(literal) {
            self.expected(&format!("`{literal}`"));
        }
    }

    fn expect_kind(&mut self, kind: TokenKind, description: &str) {
        if self.at_kind(kind) {
            self.bump();
        } else {
            self.expected(description);
        }
    }

    fn expect_newline(&mut self) {
        self.expect_kind(TokenKind::Newline, "a newline");
    }

    fn expect_indent(&mut self, description: &str) {
        while self.at_kind(TokenKind::Newline) {
            if !self.current_newline_has_comment() {
                self.error_here(
                    "CHEMS-P003",
                    "blank lines are not permitted before the first block entry",
                );
            }
            self.bump();
        }
        self.expect_kind(TokenKind::Indent, description);
    }

    fn consume_block_newlines(&mut self, block: &str) {
        while self.at_kind(TokenKind::Newline) {
            let comment_line = self.current_newline_has_comment();
            self.bump();
            if !comment_line && self.next_non_newline_kind() != Some(TokenKind::Dedent) {
                self.error_here(
                    "CHEMS-P003",
                    &format!("blank lines may appear only at the end of `{block}`"),
                );
            }
        }
    }

    fn current_newline_has_comment(&self) -> bool {
        if !self.at_kind(TokenKind::Newline) {
            return false;
        }
        let Some(current) = self.semantic.get(self.position).copied() else {
            return false;
        };
        let previous = self
            .position
            .checked_sub(1)
            .and_then(|position| self.semantic.get(position).copied())
            .map_or(0, |index| index + 1);
        self.tokens[previous..current]
            .iter()
            .any(|token| token.kind.is_comment())
    }

    fn next_non_newline_kind(&self) -> Option<TokenKind> {
        self.semantic[self.position..]
            .iter()
            .filter_map(|index| self.tokens.get(*index))
            .find(|token| token.kind != TokenKind::Newline)
            .map(|token| token.kind)
    }

    fn consume_newlines(&mut self) {
        while self.at_kind(TokenKind::Newline) {
            self.bump();
        }
    }

    fn recover_line(&mut self, expected: &str) {
        self.expected(expected);
        while !matches!(
            self.peek_kind(),
            Some(TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof) | None
        ) {
            self.bump();
        }
        if self.at_kind(TokenKind::Newline) {
            self.bump();
        }
    }

    fn expected(&mut self, expected: &str) {
        if self.halted {
            return;
        }
        let span = self.current_span();
        self.error(
            "CHEMS-P003",
            format!("expected {expected}, found {}", self.found_description()),
            span,
        );
        self.record_virtual("recovery", ByteSpan::empty(span.start));
        if self.at_kind(TokenKind::Eof) {
            self.halted = true;
            return;
        }
        self.bump_unexpected();
    }

    fn error_here(&mut self, code: &str, summary: &str) {
        self.error(code, summary, self.current_span());
    }

    fn error(&mut self, code: &str, summary: impl Into<String>, span: ByteSpan) {
        let mut diagnostic = Diagnostic::parse(code, summary, span);
        diagnostic.emission_index = self.diagnostics.len();
        self.diagnostics.push(diagnostic);
    }

    fn sort_diagnostics(&mut self) {
        self.diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.primary_span.start,
                diagnostic.severity,
                diagnostic.code.clone(),
                diagnostic.emission_index,
            )
        });
        self.diagnostics.dedup();
    }

    fn current_token(&self) -> Option<&Token> {
        self.semantic
            .get(self.position)
            .and_then(|index| self.tokens.get(*index))
    }

    fn current_span(&self) -> ByteSpan {
        self.current_token()
            .map_or(ByteSpan::empty(self.source.len()), |token| token.span)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.current_token().map(|token| &token.kind)
    }

    fn peek_kind_n(&self, offset: usize) -> Option<&TokenKind> {
        self.semantic
            .get(self.position + offset)
            .and_then(|index| self.tokens.get(*index))
            .map(|token| &token.kind)
    }

    fn peek_text(&self) -> Option<&str> {
        self.current_token().map(|token| token.text.as_str())
    }

    fn peek_text_n(&self, offset: usize) -> Option<&str> {
        self.semantic
            .get(self.position + offset)
            .and_then(|index| self.tokens.get(*index))
            .map(|token| token.text.as_str())
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.peek_kind() == Some(&kind)
    }

    fn at_literal(&self, literal: &str) -> bool {
        self.peek_text() == Some(literal)
    }

    fn found_description(&self) -> String {
        match self.current_token() {
            Some(token) if token.kind == TokenKind::Eof => "end of file".to_owned(),
            Some(token) if token.synthetic => format!("{:?}", token.kind),
            Some(token) => format!("`{}`", token.text),
            None => "end of file".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum IdentifierClass {
    General,
    Type,
    Value,
    Claim,
}

impl std::fmt::Display for IdentifierClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::General => "general",
            Self::Type => "type",
            Self::Value => "value",
            Self::Claim => "claim",
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FormulaWordPart {
    element: ByteSpan,
    count: Option<ByteSpan>,
}

fn formula_word_parts(token: &Token) -> Vec<FormulaWordPart> {
    let bytes = token.text.as_bytes();
    let mut index = 0;
    let mut parts = Vec::new();
    while index < bytes.len() {
        if !bytes[index].is_ascii_uppercase() {
            return Vec::new();
        }
        let element_start = index;
        index += 1;
        if index < bytes.len() && bytes[index].is_ascii_lowercase() {
            index += 1;
        }
        let element_end = index;
        let count_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if count_start < index && bytes[count_start] == b'0' {
            return Vec::new();
        }
        parts.push(FormulaWordPart {
            element: ByteSpan::new(
                token.span.start + element_start,
                token.span.start + element_end,
            ),
            count: (count_start < index)
                .then(|| ByteSpan::new(token.span.start + count_start, token.span.start + index)),
        });
    }
    parts
}

fn is_reserved(word: &str) -> bool {
    include_str!("../../../conformance/reserved-words.txt")
        .lines()
        .any(|reserved| reserved == word)
}

fn attach_comments(source: &str, tokens: &[Token], root: &SyntaxNode) -> Vec<CommentAttachment> {
    let mut nodes = Vec::new();
    collect_attachable_nodes(root, &mut nodes);
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.kind.is_comment())
        .map(|(token_index, token)| {
            let line_start = source[..token.span.start]
                .rfind(['\n', '\r'])
                .map_or(0, |index| index + 1);
            let full_line = source[line_start..token.span.start]
                .chars()
                .all(char::is_whitespace);
            let (placement, node) = if full_line {
                let comment_line = logical_line(source, token.span.end);
                nodes
                    .iter()
                    .filter(|node| node.span.start >= token.span.end)
                    .min_by_key(|node| node.span.start)
                    .filter(|node| logical_line(source, node.span.start) <= comment_line + 1)
                    .map_or((CommentPlacement::Enclosing, root), |node| {
                        (CommentPlacement::Leading, *node)
                    })
            } else if token.kind == TokenKind::LineComment {
                nodes
                    .iter()
                    .filter(|node| {
                        node.span.start < token.span.start && node.span.end >= token.span.start
                    })
                    .min_by_key(|node| node.span.end.saturating_sub(node.span.start))
                    .map_or((CommentPlacement::Enclosing, root), |node| {
                        (CommentPlacement::Trailing, *node)
                    })
            } else {
                nodes
                    .iter()
                    .filter(|node| {
                        node.span.start <= token.span.start && node.span.end >= token.span.end
                    })
                    .min_by_key(|node| node.span.end.saturating_sub(node.span.start))
                    .map_or((CommentPlacement::Enclosing, root), |node| {
                        (CommentPlacement::Enclosing, *node)
                    })
            };
            CommentAttachment {
                token_index,
                placement,
                node_kind: node.kind.clone(),
                node_span: node.span,
            }
        })
        .collect()
}

fn logical_line(source: &str, end: usize) -> usize {
    let bytes = &source.as_bytes()[..end.min(source.len())];
    let mut lines = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            lines += 1;
            index += usize::from(bytes.get(index + 1) == Some(&b'\n')) + 1;
        } else {
            lines += usize::from(bytes[index] == b'\n');
            index += 1;
        }
    }
    lines
}

fn collect_attachable_nodes<'a>(node: &'a SyntaxNode, output: &mut Vec<&'a SyntaxNode>) {
    if node.kind == "document"
        || node.kind == "reaction-declaration"
        || node.kind.ends_with("-section")
        || node.kind.ends_with("-declaration")
        || node.kind.ends_with("-entry")
        || node.kind.ends_with("-application")
        || node.kind.ends_with("-binding")
    {
        output.push(node);
    }
    for child in &node.children {
        collect_attachable_nodes(child, output);
    }
}
