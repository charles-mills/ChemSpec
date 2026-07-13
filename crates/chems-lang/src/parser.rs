use crate::{
    ByteSpan, ChemicalSyntaxKind, ClaimKind, CommentAttachment, CommentPlacement, Cst,
    DeclarationKind, Diagnostic, EquationSyntaxKind, HeaderKind, LexResult, NameSyntaxKind,
    ObservationKind, OperationKind, QuantitySyntaxKind, SectionKind, SourceAst,
    SourceCatalogueSelection, SourceExpectation, SourceExperiment, SourceLanguageVersion,
    SourceNode, SourceNodeKind, SyntaxNode, TacticKind, Token, TokenKind, lex_bytes, lex_source,
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
        language_version: None,
        catalogue: None,
        experiment: None,
    };
    parser.begin("document");
    parser.parse_document();
    let root = parser.finish(false);
    parser.sort_diagnostics();
    let complete = parser.diagnostics.is_empty() && parser.language_version.as_deref() == Some("1");
    let mut production_trace = std::mem::take(&mut parser.trace);
    production_trace.sort();
    production_trace.dedup();
    let comments = attach_comments(parser.source, parser.tokens, &root);
    let language_version = parser.language_version.clone();
    let catalogue = parser.catalogue.clone();
    let experiment = parser.experiment.clone();
    let document = lower_source_node(parser.source, &root);
    let language = source_language(language_version, &document);
    let catalogue = source_catalogue(catalogue, &document);
    let experiment = source_experiment(experiment, &document);
    let diagnostics = std::mem::take(&mut parser.diagnostics);
    drop(parser);
    let ast = SourceAst {
        schema_version: 1,
        complete,
        language,
        catalogue,
        experiment,
        document,
        production_trace,
        comments,
    };
    ParseResult {
        cst: Cst {
            schema_version: 1,
            source: lexed.source,
            tokens: lexed.tokens,
            root,
        },
        ast,
        diagnostics,
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
    language_version: Option<String>,
    catalogue: Option<String>,
    experiment: Option<String>,
}

impl Parser<'_> {
    fn parse_document(&mut self) {
        self.consume_newlines();
        self.parse_language_header();
        self.consume_newlines();
        self.parse_catalog_use();
        self.consume_newlines();
        self.parse_experiment();
        self.consume_newlines();
        self.expect_kind(TokenKind::Eof, "end of file");
    }

    fn parse_language_header(&mut self) {
        self.begin("language-header");
        if !self.eat_literal("chems") {
            self.error(
                "CHEMS-P001",
                "source must begin with `chems 1`",
                self.current_span(),
            );
        }
        self.begin("language-version");
        let version_span = self.current_span();
        let version = self.parse_positive_integer();
        let missing = version.is_none();
        self.finish(missing);
        if let Some(version) = &version {
            self.language_version = Some(version.clone());
            if !version.starts_with('0') && version != "1" {
                self.error(
                    "CHEMS-P002",
                    format!("unsupported .chems language major {version}"),
                    version_span,
                );
            }
        }
        self.expect_newline();
        self.finish(missing);
    }

    fn parse_catalog_use(&mut self) {
        self.begin("catalog-use");
        self.expect_literal("use");
        self.expect_literal("catalog");
        let name = self.parse_qualified_name();
        self.expect_literal("@");
        self.begin("catalog-version");
        self.parse_integer();
        while self.eat_literal(".") {
            self.parse_integer();
        }
        self.finish(false);
        self.expect_newline();
        self.catalogue = name;
        self.finish(false);
    }

    fn parse_experiment(&mut self) {
        self.begin("experiment");
        self.expect_literal("experiment");
        self.experiment = self.parse_type_identifier();
        self.expect_literal("where");
        self.expect_newline();
        self.expect_indent("an indented experiment body");
        self.parse_conditions_section();
        if self.at_literal("assuming") {
            self.parse_assumptions_section();
        }
        self.parse_given_section();
        self.parse_vessels_section();
        self.parse_procedure_section();
        while self.at_literal("expect") {
            self.parse_expectation_section();
        }
        self.parse_proof_section();
        self.expect_kind(TokenKind::Dedent, "the end of the experiment body");
        self.finish(false);
    }

    fn parse_conditions_section(&mut self) {
        self.begin("conditions-section");
        self.expect_literal("conditions");
        self.expect_newline();
        self.expect_indent("an indented conditions block");
        let mut entries = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.parse_condition_entry();
            entries += 1;
        }
        if entries == 0 {
            self.error_here("CHEMS-P003", "conditions requires at least one entry");
        }
        self.expect_kind(TokenKind::Dedent, "the end of conditions");
        self.consume_newlines();
        self.finish(entries == 0);
    }

    fn parse_condition_entry(&mut self) {
        self.begin("condition-entry");
        match self.peek_text() {
            Some("temperature") => {
                self.begin("temperature-entry");
                self.bump();
                self.expect_literal(":=");
                self.parse_quantity();
                self.expect_newline();
                self.finish(false);
            }
            Some("pressure") => {
                self.begin("pressure-entry");
                self.bump();
                self.expect_literal(":=");
                self.parse_quantity();
                self.expect_newline();
                self.finish(false);
            }
            Some("medium") => {
                self.begin("medium-entry");
                self.bump();
                self.expect_literal(":=");
                if !self.eat_literal("aqueous") {
                    self.parse_qualified_name();
                }
                self.expect_newline();
                self.finish(false);
            }
            _ => self.recover_line("expected temperature, pressure, or medium"),
        }
        self.finish(false);
    }

    fn parse_assumptions_section(&mut self) {
        self.begin("assumptions-section");
        self.expect_literal("assuming");
        self.expect_newline();
        self.expect_indent("an indented assumptions block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.begin("assumption-entry");
            self.parse_qualified_name();
            if self.eat_literal("for") {
                self.parse_value_identifier();
            }
            if self.eat_literal("at") {
                self.parse_stage_reference();
            }
            self.expect_newline();
            self.finish(false);
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "assuming requires at least one entry");
        }
        self.expect_kind(TokenKind::Dedent, "the end of assumptions");
        self.consume_newlines();
        self.finish(count == 0);
    }

    fn parse_given_section(&mut self) {
        self.begin("given-section");
        self.expect_literal("given");
        self.expect_newline();
        self.expect_indent("an indented given block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.parse_material_declaration();
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "given requires at least one material");
        }
        self.expect_kind(TokenKind::Dedent, "the end of given");
        self.consume_newlines();
        self.finish(count == 0);
    }

    fn parse_material_declaration(&mut self) {
        self.begin("material-declaration");
        self.parse_value_identifier();
        self.expect_literal(":=");
        self.begin("material-expression");
        if self.at_literal("prepared") {
            self.begin("prepared-material");
            self.bump();
            self.expect_newline();
            self.expect_indent("prepared components");
            let mut count = 0;
            while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
                if self.at_kind(TokenKind::Newline) {
                    self.bump();
                    continue;
                }
                self.begin("component-entry");
                self.parse_quantity();
                self.expect_literal("of");
                self.parse_species();
                self.expect_newline();
                self.finish(false);
                count += 1;
            }
            if count == 0 {
                self.error_here("CHEMS-P003", "prepared material requires components");
            }
            self.expect_kind(TokenKind::Dedent, "the end of prepared components");
            self.finish(count == 0);
        } else {
            self.begin("simple-material");
            self.parse_quantity();
            self.expect_literal("of");
            if self.starts_quantity() {
                self.parse_quantity();
            }
            self.parse_species();
            self.expect_newline();
            self.finish(false);
        }
        self.finish(false);
        self.finish(false);
    }

    fn parse_vessels_section(&mut self) {
        self.begin("vessels-section");
        self.expect_literal("vessels");
        self.expect_newline();
        self.expect_indent("an indented vessels block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.begin("vessel-declaration");
            self.parse_value_identifier();
            self.expect_literal(":=");
            self.begin("openness");
            if !(self.eat_literal("open") || self.eat_literal("closed")) {
                self.expected("`open` or `closed`");
            }
            self.finish(false);
            self.expect_literal("vessel");
            self.parse_quantity();
            self.expect_newline();
            self.finish(false);
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "vessels requires at least one declaration");
        }
        self.expect_kind(TokenKind::Dedent, "the end of vessels");
        self.consume_newlines();
        self.finish(count == 0);
    }

    fn parse_procedure_section(&mut self) {
        self.begin("procedure-section");
        self.expect_literal("procedure");
        self.expect_newline();
        self.expect_indent("an indented procedure block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.begin("procedure-entry");
            if self.peek_kind_n(1) == Some(&TokenKind::Colon) {
                self.begin("stage-label");
                self.parse_value_identifier();
                self.finish(false);
                self.expect_literal(":");
            }
            self.parse_operation();
            self.expect_newline();
            self.finish(false);
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "procedure requires at least one operation");
        }
        self.expect_kind(TokenKind::Dedent, "the end of procedure");
        self.consume_newlines();
        self.finish(count == 0);
    }

    fn parse_operation(&mut self) {
        self.begin("operation");
        match self.peek_text() {
            Some("place") => self.parse_binary_operation("place-operation", &["place", "in"]),
            Some("add") => self.parse_binary_operation("add-operation", &["add", "to"]),
            Some("combine") => {
                self.begin("combine-operation");
                self.bump();
                self.parse_value_identifier();
                self.expect_literal("with");
                self.parse_value_identifier();
                self.expect_literal("in");
                self.parse_value_identifier();
                self.finish(false);
            }
            Some("transfer") => {
                self.begin("transfer-operation");
                self.bump();
                if self.starts_quantity() {
                    self.parse_quantity();
                }
                self.expect_literal("from");
                self.parse_value_identifier();
                self.expect_literal("to");
                self.parse_value_identifier();
                self.finish(false);
            }
            Some("stir") => {
                self.begin("stir-operation");
                self.bump();
                self.parse_value_identifier();
                if self.eat_literal("for") {
                    self.parse_quantity();
                }
                self.finish(false);
            }
            Some("heat") => self.parse_temperature_operation("heat-operation", "heat"),
            Some("cool") => self.parse_temperature_operation("cool-operation", "cool"),
            Some("wait") => {
                self.begin("wait-operation");
                self.bump();
                self.parse_quantity();
                self.finish(false);
            }
            Some("seal") => self.parse_unary_operation("seal-operation", "seal"),
            Some("open") => self.parse_unary_operation("open-operation", "open"),
            Some("filter") => {
                self.begin("filter-operation");
                self.bump();
                self.parse_value_identifier();
                self.expect_literal("into");
                self.parse_value_identifier();
                self.expect_literal("and");
                self.parse_value_identifier();
                self.finish(false);
            }
            Some("decant") => self.parse_binary_operation("decant-operation", &["decant", "into"]),
            _ => self.recover_to_newline("expected a procedure operation"),
        }
        self.finish(false);
    }

    fn parse_binary_operation(&mut self, kind: &str, keywords: &[&str]) {
        self.begin(kind);
        self.expect_literal(keywords[0]);
        self.parse_value_identifier();
        self.expect_literal(keywords[1]);
        self.parse_value_identifier();
        self.finish(false);
    }

    fn parse_unary_operation(&mut self, kind: &str, keyword: &str) {
        self.begin(kind);
        self.expect_literal(keyword);
        self.parse_value_identifier();
        self.finish(false);
    }

    fn parse_temperature_operation(&mut self, kind: &str, keyword: &str) {
        self.begin(kind);
        self.expect_literal(keyword);
        self.parse_value_identifier();
        self.expect_literal("to");
        self.parse_quantity();
        self.finish(false);
    }

    fn parse_expectation_section(&mut self) {
        self.begin("expectation-section");
        self.expect_literal("expect");
        if self.eat_literal("at") {
            self.parse_stage_reference();
        }
        self.expect_newline();
        self.expect_indent("an indented expectation block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.parse_claim_entry();
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "expect requires at least one claim");
        }
        self.expect_kind(TokenKind::Dedent, "the end of expectation");
        self.consume_newlines();
        self.finish(count == 0);
    }

    fn parse_claim_entry(&mut self) {
        self.begin("claim-entry");
        match self.peek_text() {
            Some("class") => {
                self.begin("class-claim");
                self.bump();
                self.expect_literal(":=");
                if !self.parse_hole() {
                    self.begin("reaction-class");
                    if matches!(
                        self.peek_text(),
                        Some("precipitation" | "neutralization" | "gasFormation" | "noReaction")
                    ) {
                        self.bump();
                    } else {
                        self.expected("a reaction class or `?`");
                    }
                    self.finish(false);
                }
                self.expect_newline();
                self.finish(false);
            }
            Some("produces" | "consumes" | "remains" | "spectator") => {
                self.begin("identity-claim");
                self.begin("identity-predicate");
                self.bump();
                self.finish(false);
                if !self.parse_hole() {
                    self.parse_species();
                }
                self.expect_newline();
                self.finish(false);
            }
            Some("molecular" | "completeIonic" | "netIonic") => {
                self.parse_equation_claim();
            }
            Some("amount") => {
                self.begin("amount-claim");
                self.bump();
                self.parse_species();
                self.expect_literal(":=");
                if !self.parse_hole() {
                    self.parse_quantity();
                }
                self.expect_newline();
                self.finish(false);
            }
            Some("limiting") => {
                self.begin("limiting-claim");
                self.bump();
                self.expect_literal(":=");
                if !self.parse_hole() && !self.eat_literal("none") {
                    self.parse_value_identifier();
                }
                self.expect_newline();
                self.finish(false);
            }
            Some("observe") => self.parse_observation_section(),
            _ => self.recover_line("expected an expectation claim"),
        }
        self.finish(false);
    }

    fn parse_equation_claim(&mut self) {
        self.begin("equation-claim");
        self.begin("equation-kind");
        self.bump();
        self.finish(false);
        self.expect_literal(":=");
        self.begin("equation-claim-value");
        if self.parse_hole() {
            self.expect_newline();
        } else if self.at_kind(TokenKind::Newline) {
            self.bump();
            self.expect_indent("an indented equation");
            self.parse_equation();
            self.expect_newline();
            self.expect_kind(TokenKind::Dedent, "the end of the equation");
        } else {
            self.parse_equation();
            self.expect_newline();
        }
        self.finish(false);
        self.finish(false);
    }

    fn parse_observation_section(&mut self) {
        self.begin("observation-section");
        self.expect_literal("observe");
        self.expect_newline();
        self.expect_indent("an indented observation block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.begin("observation-entry");
            match self.peek_text() {
                Some("precipitate") => {
                    self.begin("precipitate-observation");
                    self.bump();
                    if !self.parse_hole() {
                        self.parse_species();
                    }
                    self.expect_newline();
                    self.finish(false);
                }
                Some("gas") => {
                    self.begin("gas-observation");
                    self.bump();
                    if !self.parse_hole() {
                        self.parse_species();
                    }
                    self.expect_newline();
                    self.finish(false);
                }
                Some("colour") => {
                    self.begin("colour-observation");
                    self.bump();
                    self.expect_literal(":=");
                    if !self.parse_hole() {
                        self.parse_qualified_name();
                    }
                    self.expect_newline();
                    self.finish(false);
                }
                Some("temperatureChange") => {
                    self.begin("temperature-observation");
                    self.bump();
                    self.expect_literal(":=");
                    if !self.parse_hole() {
                        self.begin("temperature-direction");
                        if matches!(self.peek_text(), Some("increase" | "decrease" | "none")) {
                            self.bump();
                        } else {
                            self.expected("increase, decrease, none, or `?`");
                        }
                        self.finish(false);
                    }
                    self.expect_newline();
                    self.finish(false);
                }
                _ => self.recover_line("expected an observation"),
            }
            self.finish(false);
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "observe requires at least one entry");
        }
        self.expect_kind(TokenKind::Dedent, "the end of observations");
        self.finish(count == 0);
    }

    fn parse_proof_section(&mut self) {
        self.begin("proof-section");
        self.expect_literal("by");
        self.expect_newline();
        self.expect_indent("an indented proof block");
        let mut count = 0;
        while !self.at_kind(TokenKind::Dedent) && !self.at_kind(TokenKind::Eof) {
            if self.at_kind(TokenKind::Newline) {
                self.bump();
                continue;
            }
            self.parse_tactic();
            count += 1;
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "by requires at least one tactic");
        }
        self.expect_kind(TokenKind::Dedent, "the end of proof");
        self.finish(count == 0);
    }

    fn parse_tactic(&mut self) {
        self.begin("tactic");
        let kind = match self.peek_text() {
            Some("dissociate") => "dissociate-tactic",
            Some("infer") => "infer-products-tactic",
            Some("balance") => "balance-tactic",
            Some("derive") => "derive-tactic",
            Some("cancel") => "cancel-spectators-tactic",
            Some("solve") => "solve-stoichiometry-tactic",
            Some("verify") if self.peek_text_n(1) == Some("atoms") => "verify-atoms-tactic",
            Some("verify") => "verify-charge-tactic",
            Some("prove") => "prove-observations-tactic",
            Some("close") => "close-tactic",
            Some("auto") => "auto-tactic",
            _ => {
                self.recover_line("expected a proof tactic");
                self.finish(true);
                return;
            }
        };
        self.begin(kind);
        match kind {
            "dissociate-tactic" => {
                self.expect_literal("dissociate");
                self.expect_literal("aqueous");
            }
            "infer-products-tactic" => {
                self.expect_literal("infer");
                self.expect_literal("products");
                self.expect_literal("using");
                self.parse_qualified_name();
            }
            "balance-tactic" => {
                self.expect_literal("balance");
                self.parse_equation_kind();
            }
            "derive-tactic" => {
                self.expect_literal("derive");
                self.parse_equation_kind();
            }
            "cancel-spectators-tactic" => {
                self.expect_literal("cancel");
                self.expect_literal("spectators");
            }
            "solve-stoichiometry-tactic" => {
                self.expect_literal("solve");
                self.expect_literal("stoichiometry");
            }
            "verify-atoms-tactic" => {
                self.expect_literal("verify");
                self.expect_literal("atoms");
            }
            "verify-charge-tactic" => {
                self.expect_literal("verify");
                self.expect_literal("charge");
            }
            "prove-observations-tactic" => {
                self.expect_literal("prove");
                self.expect_literal("observations");
            }
            "close-tactic" => {
                self.expect_literal("close");
            }
            "auto-tactic" => {
                self.expect_literal("auto");
            }
            _ => unreachable!(),
        }
        self.expect_newline();
        self.finish(false);
        self.finish(false);
    }

    fn parse_equation_kind(&mut self) {
        self.begin("equation-kind");
        if matches!(
            self.peek_text(),
            Some("molecular" | "completeIonic" | "netIonic")
        ) {
            self.bump();
        } else {
            self.expected("an equation kind");
        }
        self.finish(false);
    }

    fn parse_equation(&mut self) {
        self.begin("equation");
        self.parse_equation_side();
        if self.at_kind(TokenKind::Newline) && self.peek_text_n(1) == Some("->") {
            self.bump();
        }
        self.expect_literal("->");
        if self.at_kind(TokenKind::Newline) {
            self.bump();
        }
        self.parse_equation_side();
        self.finish(false);
    }

    fn parse_equation_side(&mut self) {
        self.begin("equation-side");
        self.parse_equation_term();
        loop {
            if self.eat_literal("+") {
                if self.at_kind(TokenKind::Newline) {
                    self.bump();
                }
                self.parse_equation_term();
            } else if self.at_kind(TokenKind::Newline) && self.peek_text_n(1) == Some("+") {
                self.bump();
                self.bump();
                if self.at_kind(TokenKind::Newline) {
                    self.bump();
                }
                self.parse_equation_term();
            } else {
                break;
            }
        }
        self.finish(false);
    }

    fn parse_equation_term(&mut self) {
        self.begin("equation-term");
        if self.at_kind(TokenKind::Number) {
            self.parse_positive_integer();
            self.require_separator_after(self.last_end, "an equation coefficient");
        }
        self.parse_species();
        self.finish(false);
    }

    fn parse_species(&mut self) {
        self.begin("species");
        self.parse_formula();
        let formula_end = self.last_end;
        if self.at_literal("^") {
            self.require_compact_after(formula_end, "a formula and its charge");
            self.begin("charge");
            self.bump();
            let caret_end = self.last_end;
            if self.at_kind(TokenKind::Number) {
                self.require_compact_after(caret_end, "a charge marker and magnitude");
                self.parse_positive_integer();
            }
            let magnitude_end = self.last_end;
            self.require_compact_after(magnitude_end, "a charge and its sign");
            if !(self.eat_literal("+") || self.eat_literal("-")) {
                self.expected("a charge sign");
            }
            self.finish(false);
        }
        let charge_end = self.last_end;
        self.require_compact_after(charge_end, "a formula or charge and its phase");
        self.begin("phase");
        self.expect_literal("(");
        let opener_end = self.last_end;
        self.require_compact_after(opener_end, "a phase opener and phase symbol");
        if matches!(self.peek_text(), Some("aq" | "s" | "l" | "g")) {
            self.bump();
        } else {
            self.expected("aq, s, l, or g");
        }
        let phase_end = self.last_end;
        self.require_compact_after(phase_end, "a phase symbol and closer");
        self.expect_literal(")");
        self.finish(false);
        self.finish(false);
    }

    fn parse_formula(&mut self) {
        self.begin("formula");
        self.parse_formula_segment();
        while self.at_literal(".") {
            self.require_compact_after(self.last_end, "formula adduct punctuation");
            self.bump();
            let dot_end = self.last_end;
            if self.at_kind(TokenKind::Number) {
                self.require_compact_after(dot_end, "an adduct dot and coefficient");
                self.parse_positive_integer();
            }
            self.require_compact_after(self.last_end, "an adduct coefficient and formula");
            self.parse_formula_segment();
        }
        self.finish(false);
    }

    fn parse_formula_segment(&mut self) {
        self.begin("formula-segment");
        let mut count = 0;
        let mut previous_end = None;
        while !self.at_phase()
            && !matches!(
                self.peek_kind(),
                Some(
                    TokenKind::Caret
                        | TokenKind::Dot
                        | TokenKind::RightParen
                        | TokenKind::Newline
                        | TokenKind::Eof
                )
            )
            && !matches!(self.peek_text(), Some("+" | "->"))
        {
            if let Some(previous_end) = previous_end {
                self.require_compact_after(previous_end, "adjacent formula parts");
            }
            if self.at_kind(TokenKind::Word) {
                let token = self.current_token().cloned();
                if let Some(token) = token {
                    let mut parts = formula_word_parts(&token);
                    if parts.is_empty() {
                        self.begin("formula-part");
                        self.error(
                            "CHEMS-P003",
                            format!("invalid formula token `{}`", token.text),
                            token.span,
                        );
                        self.bump();
                        self.finish(true);
                        count += 1;
                        previous_end = Some(self.last_end);
                    } else {
                        self.bump();
                        if parts.last().is_some_and(|part| part.count.is_none())
                            && self.at_kind(TokenKind::Number)
                            && self.gap_is_compact(token.span.end, self.current_span().start)
                        {
                            let count_token = self
                                .current_token()
                                .cloned()
                                .expect("number token is present");
                            if count_token.text.starts_with('0') {
                                self.error(
                                    "CHEMS-P003",
                                    "positive integers must begin with 1 through 9",
                                    count_token.span,
                                );
                            }
                            self.bump();
                            if let Some(part) = parts.last_mut() {
                                part.count = Some(count_token.span);
                            }
                        }
                        count += parts.len();
                        self.record_formula_word_parts(parts);
                        previous_end = Some(self.last_end);
                    }
                }
            } else if self.at_literal("(") {
                self.begin("formula-part");
                self.bump();
                self.require_compact_after(self.last_end, "a formula group opener and contents");
                self.parse_formula_segment();
                self.require_compact_after(self.last_end, "formula group contents and closer");
                self.expect_literal(")");
                if self.at_kind(TokenKind::Number) {
                    self.require_compact_after(self.last_end, "a formula group and its count");
                    self.parse_positive_integer();
                }
                self.finish(false);
                count += 1;
                previous_end = Some(self.last_end);
            } else {
                self.begin("formula-part");
                self.expected("a formula element or group");
                self.bump_unexpected();
                self.finish(true);
                count += 1;
                previous_end = Some(self.last_end);
            }
        }
        if count == 0 {
            self.error_here("CHEMS-P003", "formula segment cannot be empty");
        }
        self.finish(count == 0);
    }

    fn at_phase(&self) -> bool {
        self.peek_text() == Some("(")
            && matches!(self.peek_text_n(1), Some("aq" | "s" | "l" | "g"))
            && self.peek_text_n(2) == Some(")")
    }

    fn parse_quantity(&mut self) {
        self.begin("quantity");
        self.parse_decimal();
        self.require_separator_after(self.last_end, "a decimal and its unit expression");
        self.parse_unit_expression();
        self.finish(false);
    }

    fn parse_decimal(&mut self) {
        self.begin("decimal");
        if self.at_literal("+") || self.at_literal("-") {
            self.bump();
            self.require_compact_after(self.last_end, "a decimal sign and integer");
        }
        self.parse_integer();
        if self.at_literal(".") {
            self.require_compact_after(self.last_end, "an integer and decimal point");
            self.bump();
            self.require_compact_after(self.last_end, "a decimal point and fractional digits");
            self.parse_integer();
        }
        self.finish(false);
    }

    fn parse_unit_expression(&mut self) {
        self.begin("unit-expression");
        self.parse_unit_product();
        while self.at_literal("/") {
            self.require_compact_after(self.last_end, "unit division");
            self.bump();
            self.require_compact_after(self.last_end, "unit division");
            self.parse_unit_product();
        }
        self.finish(false);
    }

    fn parse_unit_product(&mut self) {
        self.begin("unit-product");
        self.parse_unit_factor();
        while self.at_literal("*") {
            self.require_compact_after(self.last_end, "unit multiplication");
            self.bump();
            self.require_compact_after(self.last_end, "unit multiplication");
            self.parse_unit_factor();
        }
        self.finish(false);
    }

    fn parse_unit_factor(&mut self) {
        self.begin("unit-factor");
        self.begin("unit-symbol");
        if self.at_kind(TokenKind::Word) {
            self.begin("unit-name");
            let token = self.current_token().cloned();
            if let Some(token) = token {
                if !token
                    .text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || !token
                        .text
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_alphabetic)
                {
                    self.error("CHEMS-P003", "invalid unit name", token.span);
                }
                self.bump();
            }
            self.finish(false);
        } else if self.eat_literal("%") {
            // Percent is a complete unit symbol.
        } else {
            self.expected("a unit name or `%`");
        }
        self.finish(false);
        if self.at_literal("^") {
            self.require_compact_after(self.last_end, "a unit symbol and exponent");
            self.bump();
            self.require_compact_after(self.last_end, "a unit exponent marker and integer");
            self.parse_signed_integer();
        }
        self.finish(false);
    }

    fn parse_signed_integer(&mut self) {
        self.begin("signed-integer");
        if self.at_literal("+") || self.at_literal("-") {
            self.bump();
            self.require_compact_after(self.last_end, "an exponent sign and integer");
        }
        self.parse_integer();
        self.finish(false);
    }

    fn starts_quantity(&self) -> bool {
        self.at_kind(TokenKind::Number)
            || ((self.at_literal("+") || self.at_literal("-"))
                && self.peek_kind_n(1) == Some(&TokenKind::Number))
    }

    fn gap_is_compact(&self, start: usize, end: usize) -> bool {
        !self.tokens.iter().any(|token| {
            token.span.start >= start
                && token.span.end <= end
                && matches!(token.kind, TokenKind::Space | TokenKind::Newline)
        })
    }

    fn require_compact_after(&mut self, start: usize, description: &str) {
        let end = self.current_span().start;
        if let Some(span) = self
            .tokens
            .iter()
            .find(|token| {
                token.span.start >= start && token.span.end <= end && token.kind == TokenKind::Space
            })
            .map(|token| token.span)
        {
            self.error(
                "CHEMS-P005",
                format!("spaces are forbidden within {description}"),
                span,
            );
            self.record_virtual("recovery", ByteSpan::empty(span.start));
        }
    }

    fn require_separator_after(&mut self, start: usize, description: &str) {
        let end = self.current_span().start;
        let separated = self.tokens.iter().any(|token| {
            token.span.start >= start
                && token.span.end <= end
                && matches!(
                    token.kind,
                    TokenKind::Space | TokenKind::LineComment | TokenKind::BlockComment
                )
        });
        if !separated {
            let span = ByteSpan::empty(end);
            self.error(
                "CHEMS-P006",
                format!("a space or comment must separate {description}"),
                span,
            );
            self.record_virtual("recovery", span);
        }
    }

    fn parse_hole(&mut self) -> bool {
        if !self.at_literal("?") {
            return false;
        }
        self.begin("hole");
        self.bump();
        self.finish(false);
        true
    }

    fn parse_qualified_name(&mut self) -> Option<String> {
        self.begin("qualified-name");
        let mut name = self.parse_name_segment();
        while self.eat_literal(".") {
            let segment = self.parse_name_segment();
            if let (Some(name), Some(segment)) = (&mut name, segment) {
                name.push('.');
                name.push_str(&segment);
            } else {
                name = None;
            }
        }
        self.finish(name.is_none());
        name
    }

    fn parse_name_segment(&mut self) -> Option<String> {
        self.begin("name-segment");
        let result = match self.peek_text().and_then(|text| text.as_bytes().first()) {
            Some(first) if first.is_ascii_lowercase() => self.parse_value_identifier(),
            Some(first) if first.is_ascii_uppercase() => self.parse_type_identifier(),
            _ => {
                self.expected("an identifier");
                None
            }
        };
        self.finish(result.is_none());
        result
    }

    fn parse_value_identifier(&mut self) -> Option<String> {
        self.parse_identifier("value-identifier", u8::is_ascii_lowercase)
    }

    fn parse_type_identifier(&mut self) -> Option<String> {
        self.parse_identifier("type-identifier", u8::is_ascii_uppercase)
    }

    fn parse_identifier(
        &mut self,
        kind: &str,
        valid_first: impl FnOnce(&u8) -> bool,
    ) -> Option<String> {
        self.begin(kind);
        let Some(token) = self.current_token().cloned() else {
            self.expected("an identifier");
            self.finish(true);
            return None;
        };
        let valid = token.kind == TokenKind::Word
            && token.text.as_bytes().first().is_some_and(valid_first)
            && token
                .text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        let result = if valid {
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
            self.bump();
            Some(token.text)
        } else {
            self.expected(if kind == "value-identifier" {
                "a lower-case identifier"
            } else {
                "an upper-case identifier"
            });
            None
        };
        self.finish(result.is_none());
        result
    }

    fn parse_integer(&mut self) -> Option<String> {
        self.begin("integer");
        let result = if self.at_kind(TokenKind::Number) {
            let text = self.peek_text().map(str::to_owned);
            self.bump();
            text
        } else {
            self.expected("an integer");
            None
        };
        self.finish(result.is_none());
        result
    }

    fn parse_positive_integer(&mut self) -> Option<String> {
        self.begin("positive-integer");
        let result = if self.at_kind(TokenKind::Number) {
            let token = self.current_token().cloned().expect("number token exists");
            if token.text.starts_with('0') {
                self.error(
                    "CHEMS-P003",
                    "positive integers must begin with 1 through 9",
                    token.span,
                );
            }
            self.bump();
            Some(token.text)
        } else {
            self.expected("a positive integer");
            None
        };
        self.finish(result.is_none());
        result
    }

    fn parse_stage_reference(&mut self) {
        self.begin("stage-reference");
        if self.at_literal("initial") || self.at_literal("final") {
            self.bump();
        } else {
            self.begin("stage-label");
            self.parse_value_identifier();
            self.finish(false);
        }
        self.finish(false);
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
        if !self.at_kind(TokenKind::Newline)
            && !self.at_kind(TokenKind::Dedent)
            && !self.at_kind(TokenKind::Eof)
        {
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
        self.consume_newlines();
        self.expect_kind(TokenKind::Indent, description);
    }

    fn consume_newlines(&mut self) {
        while self.at_kind(TokenKind::Newline) {
            self.bump();
        }
    }

    fn recover_line(&mut self, expected: &str) {
        self.expected(expected);
        self.recover_to_newline(expected);
        self.expect_newline();
    }

    fn recover_to_newline(&mut self, expected: &str) {
        if !self.at_kind(TokenKind::Newline) {
            self.error_here("CHEMS-P003", expected);
        }
        while !matches!(
            self.peek_kind(),
            Some(TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof) | None
        ) {
            self.bump();
        }
    }

    fn expected(&mut self, expected: &str) {
        let span = self.current_span();
        self.error(
            "CHEMS-P003",
            format!("expected {expected}, found {}", self.found_description()),
            span,
        );
        self.record_virtual("recovery", ByteSpan::empty(span.start));
        self.bump_unexpected();
    }

    fn error_here(&mut self, code: &str, summary: &str) {
        self.error(code, summary, self.current_span());
    }

    fn error(&mut self, code: &str, summary: impl Into<String>, span: ByteSpan) {
        self.diagnostics
            .push(Diagnostic::parse(code, summary, span));
    }

    fn sort_diagnostics(&mut self) {
        self.diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.primary_span.start,
                diagnostic.severity,
                diagnostic.stage,
                diagnostic.code.clone(),
                diagnostic.summary.clone(),
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
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let full_line = source[line_start..token.span.start]
                .chars()
                .all(char::is_whitespace);
            let (placement, node) = if full_line {
                let comment_line = source[..token.span.end]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count();
                nodes
                    .iter()
                    .filter(|node| node.span.start >= token.span.end)
                    .min_by_key(|node| node.span.start)
                    .filter(|node| {
                        let node_line = source[..node.span.start]
                            .bytes()
                            .filter(|b| *b == b'\n')
                            .count();
                        node_line <= comment_line + 1
                    })
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

fn collect_attachable_nodes<'a>(node: &'a SyntaxNode, output: &mut Vec<&'a SyntaxNode>) {
    if node.kind == "document"
        || node.kind == "experiment"
        || node.kind.ends_with("-section")
        || node.kind.ends_with("-entry")
        || node.kind.ends_with("-declaration")
        || node.kind.ends_with("-operation")
        || node.kind.ends_with("-claim")
        || node.kind.ends_with("-tactic")
    {
        output.push(node);
    }
    for child in &node.children {
        collect_attachable_nodes(child, output);
    }
}

fn lower_source_node(source: &str, node: &SyntaxNode) -> SourceNode {
    let kind = source_node_kind(&node.kind);
    let lexeme = source_node_has_lexeme(&kind).then(|| {
        source[node.span.start.min(source.len())..node.span.end.min(source.len())]
            .trim()
            .to_owned()
    });
    SourceNode {
        kind,
        span: node.span,
        lexeme,
        children: node
            .children
            .iter()
            .map(|child| lower_source_node(source, child))
            .collect(),
        recovery: node.recovery,
    }
}

fn source_node_has_lexeme(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Header {
            form: HeaderKind::LanguageVersion | HeaderKind::CatalogVersion
        } | SourceNodeKind::Declaration {
            form: DeclarationKind::Openness
        } | SourceNodeKind::Claim {
            claim: ClaimKind::ReactionClass | ClaimKind::IdentityPredicate
        } | SourceNodeKind::Observation {
            observation: ObservationKind::TemperatureDirection
        } | SourceNodeKind::Equation {
            form: EquationSyntaxKind::Kind
        } | SourceNodeKind::Chemical { .. }
            | SourceNodeKind::Quantity { .. }
            | SourceNodeKind::Name { .. }
            | SourceNodeKind::Hole
            | SourceNodeKind::Recovery
    )
}

fn source_language(lexeme: Option<String>, document: &SourceNode) -> Option<SourceLanguageVersion> {
    let lexeme = lexeme?;
    let node = find_source_node(document, |kind| {
        matches!(
            kind,
            SourceNodeKind::Header {
                form: HeaderKind::LanguageVersion
            }
        )
    })?;
    Some(SourceLanguageVersion {
        lexeme,
        span: node.span,
    })
}

fn source_catalogue(
    name: Option<String>,
    document: &SourceNode,
) -> Option<SourceCatalogueSelection> {
    let name = name?;
    let selection = find_source_node(document, |kind| {
        matches!(
            kind,
            SourceNodeKind::Header {
                form: HeaderKind::CatalogUse
            }
        )
    })?;
    let version = find_source_node(selection, |kind| {
        matches!(
            kind,
            SourceNodeKind::Header {
                form: HeaderKind::CatalogVersion
            }
        )
    })
    .and_then(|node| node.lexeme.clone());
    Some(SourceCatalogueSelection {
        name,
        version,
        span: selection.span,
    })
}

fn source_experiment(name: Option<String>, document: &SourceNode) -> Option<SourceExperiment> {
    let name = name?;
    let experiment = find_source_node(document, |kind| matches!(kind, SourceNodeKind::Experiment))?;
    let section = |wanted| {
        experiment.children.iter().find(|node| {
            matches!(
                node.kind,
                SourceNodeKind::Section { section } if section == wanted
            )
        })
    };
    let entries = |wanted, predicate: fn(&SourceNodeKind) -> bool| {
        section(wanted)
            .map(|section| {
                section
                    .children
                    .iter()
                    .filter(|node| predicate(&node.kind))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let expectations = experiment
        .children
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                SourceNodeKind::Section {
                    section: SectionKind::Expectation
                }
            )
        })
        .map(|node| SourceExpectation {
            span: node.span,
            stage: find_source_node(node, |kind| {
                matches!(
                    kind,
                    SourceNodeKind::Name {
                        form: NameSyntaxKind::StageReference
                    }
                )
            })
            .and_then(|stage| stage.lexeme.clone()),
            claims: node
                .children
                .iter()
                .filter(|child| {
                    matches!(
                        child.kind,
                        SourceNodeKind::Claim {
                            claim: ClaimKind::Entry
                        }
                    )
                })
                .cloned()
                .collect(),
        })
        .collect();
    Some(SourceExperiment {
        name,
        span: experiment.span,
        conditions: entries(SectionKind::Conditions, is_condition_entry),
        assumptions: entries(SectionKind::Assumptions, is_assumption_entry),
        materials: entries(SectionKind::Given, is_material_entry),
        vessels: entries(SectionKind::Vessels, is_vessel_entry),
        procedure: entries(SectionKind::Procedure, is_procedure_entry),
        expectations,
        tactics: entries(SectionKind::Proof, is_tactic_entry),
    })
}

fn find_source_node(
    node: &SourceNode,
    predicate: impl Copy + Fn(&SourceNodeKind) -> bool,
) -> Option<&SourceNode> {
    if predicate(&node.kind) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_source_node(child, predicate))
}

fn is_condition_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Declaration {
            form: DeclarationKind::ConditionEntry
        }
    )
}

fn is_assumption_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Declaration {
            form: DeclarationKind::Assumption
        }
    )
}

fn is_material_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Declaration {
            form: DeclarationKind::Material
        }
    )
}

fn is_vessel_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Declaration {
            form: DeclarationKind::Vessel
        }
    )
}

fn is_procedure_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Declaration {
            form: DeclarationKind::ProcedureEntry
        }
    )
}

fn is_tactic_entry(kind: &SourceNodeKind) -> bool {
    matches!(
        kind,
        SourceNodeKind::Tactic {
            tactic: TacticKind::Tactic
        }
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive normative-production mapping is clearest as one auditable match"
)]
fn source_node_kind(production: &str) -> SourceNodeKind {
    match production {
        "document" => SourceNodeKind::Document,
        "language-header" => SourceNodeKind::Header {
            form: HeaderKind::LanguageHeader,
        },
        "language-version" => SourceNodeKind::Header {
            form: HeaderKind::LanguageVersion,
        },
        "catalog-use" => SourceNodeKind::Header {
            form: HeaderKind::CatalogUse,
        },
        "catalog-version" => SourceNodeKind::Header {
            form: HeaderKind::CatalogVersion,
        },
        "experiment" => SourceNodeKind::Experiment,
        "conditions-section" => SourceNodeKind::Section {
            section: SectionKind::Conditions,
        },
        "assumptions-section" => SourceNodeKind::Section {
            section: SectionKind::Assumptions,
        },
        "given-section" => SourceNodeKind::Section {
            section: SectionKind::Given,
        },
        "vessels-section" => SourceNodeKind::Section {
            section: SectionKind::Vessels,
        },
        "procedure-section" => SourceNodeKind::Section {
            section: SectionKind::Procedure,
        },
        "expectation-section" => SourceNodeKind::Section {
            section: SectionKind::Expectation,
        },
        "observation-section" => SourceNodeKind::Section {
            section: SectionKind::Observation,
        },
        "proof-section" => SourceNodeKind::Section {
            section: SectionKind::Proof,
        },
        "condition-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::ConditionEntry,
        },
        "temperature-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::Temperature,
        },
        "pressure-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::Pressure,
        },
        "medium-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::Medium,
        },
        "assumption-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::Assumption,
        },
        "material-declaration" => SourceNodeKind::Declaration {
            form: DeclarationKind::Material,
        },
        "material-expression" => SourceNodeKind::Declaration {
            form: DeclarationKind::MaterialExpression,
        },
        "simple-material" => SourceNodeKind::Declaration {
            form: DeclarationKind::SimpleMaterial,
        },
        "prepared-material" => SourceNodeKind::Declaration {
            form: DeclarationKind::PreparedMaterial,
        },
        "component-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::Component,
        },
        "vessel-declaration" => SourceNodeKind::Declaration {
            form: DeclarationKind::Vessel,
        },
        "openness" => SourceNodeKind::Declaration {
            form: DeclarationKind::Openness,
        },
        "procedure-entry" => SourceNodeKind::Declaration {
            form: DeclarationKind::ProcedureEntry,
        },
        "stage-label" => SourceNodeKind::Declaration {
            form: DeclarationKind::StageLabel,
        },
        "operation" => operation_node(OperationKind::Operation),
        "place-operation" => operation_node(OperationKind::Place),
        "add-operation" => operation_node(OperationKind::Add),
        "combine-operation" => operation_node(OperationKind::Combine),
        "transfer-operation" => operation_node(OperationKind::Transfer),
        "stir-operation" => operation_node(OperationKind::Stir),
        "heat-operation" => operation_node(OperationKind::Heat),
        "cool-operation" => operation_node(OperationKind::Cool),
        "wait-operation" => operation_node(OperationKind::Wait),
        "seal-operation" => operation_node(OperationKind::Seal),
        "open-operation" => operation_node(OperationKind::Open),
        "filter-operation" => operation_node(OperationKind::Filter),
        "decant-operation" => operation_node(OperationKind::Decant),
        "claim-entry" => claim_node(ClaimKind::Entry),
        "class-claim" => claim_node(ClaimKind::Class),
        "reaction-class" => claim_node(ClaimKind::ReactionClass),
        "identity-claim" => claim_node(ClaimKind::Identity),
        "identity-predicate" => claim_node(ClaimKind::IdentityPredicate),
        "equation-claim" => claim_node(ClaimKind::Equation),
        "equation-claim-value" => claim_node(ClaimKind::EquationValue),
        "amount-claim" => claim_node(ClaimKind::Amount),
        "limiting-claim" => claim_node(ClaimKind::Limiting),
        "observation-entry" => observation_node(ObservationKind::Entry),
        "precipitate-observation" => observation_node(ObservationKind::Precipitate),
        "gas-observation" => observation_node(ObservationKind::Gas),
        "colour-observation" => observation_node(ObservationKind::Colour),
        "temperature-observation" => observation_node(ObservationKind::Temperature),
        "temperature-direction" => observation_node(ObservationKind::TemperatureDirection),
        "tactic" => tactic_node(TacticKind::Tactic),
        "dissociate-tactic" => tactic_node(TacticKind::Dissociate),
        "infer-products-tactic" => tactic_node(TacticKind::InferProducts),
        "balance-tactic" => tactic_node(TacticKind::Balance),
        "derive-tactic" => tactic_node(TacticKind::Derive),
        "cancel-spectators-tactic" => tactic_node(TacticKind::CancelSpectators),
        "solve-stoichiometry-tactic" => tactic_node(TacticKind::SolveStoichiometry),
        "verify-atoms-tactic" => tactic_node(TacticKind::VerifyAtoms),
        "verify-charge-tactic" => tactic_node(TacticKind::VerifyCharge),
        "prove-observations-tactic" => tactic_node(TacticKind::ProveObservations),
        "close-tactic" => tactic_node(TacticKind::Close),
        "auto-tactic" => tactic_node(TacticKind::Auto),
        "equation" => equation_node(EquationSyntaxKind::Equation),
        "equation-kind" => equation_node(EquationSyntaxKind::Kind),
        "equation-side" => equation_node(EquationSyntaxKind::Side),
        "equation-term" => equation_node(EquationSyntaxKind::Term),
        "species" => chemical_node(ChemicalSyntaxKind::Species),
        "formula" => chemical_node(ChemicalSyntaxKind::Formula),
        "formula-segment" => chemical_node(ChemicalSyntaxKind::FormulaSegment),
        "formula-part" => chemical_node(ChemicalSyntaxKind::FormulaPart),
        "element" => chemical_node(ChemicalSyntaxKind::Element),
        "charge" => chemical_node(ChemicalSyntaxKind::Charge),
        "phase" => chemical_node(ChemicalSyntaxKind::Phase),
        "quantity" => quantity_node(QuantitySyntaxKind::Quantity),
        "decimal" => quantity_node(QuantitySyntaxKind::Decimal),
        "unit-expression" => quantity_node(QuantitySyntaxKind::UnitExpression),
        "unit-product" => quantity_node(QuantitySyntaxKind::UnitProduct),
        "unit-factor" => quantity_node(QuantitySyntaxKind::UnitFactor),
        "unit-symbol" => quantity_node(QuantitySyntaxKind::UnitSymbol),
        "unit-name" => quantity_node(QuantitySyntaxKind::UnitName),
        "signed-integer" => quantity_node(QuantitySyntaxKind::SignedInteger),
        "integer" => quantity_node(QuantitySyntaxKind::Integer),
        "positive-integer" => quantity_node(QuantitySyntaxKind::PositiveInteger),
        "qualified-name" => name_node(NameSyntaxKind::QualifiedName),
        "name-segment" => name_node(NameSyntaxKind::NameSegment),
        "value-identifier" => name_node(NameSyntaxKind::ValueIdentifier),
        "type-identifier" => name_node(NameSyntaxKind::TypeIdentifier),
        "stage-reference" => name_node(NameSyntaxKind::StageReference),
        "hole" => SourceNodeKind::Hole,
        "recovery" => SourceNodeKind::Recovery,
        unknown => panic!("parser emitted unmapped source production `{unknown}`"),
    }
}

const fn operation_node(operation: OperationKind) -> SourceNodeKind {
    SourceNodeKind::Operation { operation }
}

const fn claim_node(claim: ClaimKind) -> SourceNodeKind {
    SourceNodeKind::Claim { claim }
}

const fn observation_node(observation: ObservationKind) -> SourceNodeKind {
    SourceNodeKind::Observation { observation }
}

const fn tactic_node(tactic: TacticKind) -> SourceNodeKind {
    SourceNodeKind::Tactic { tactic }
}

const fn equation_node(form: EquationSyntaxKind) -> SourceNodeKind {
    SourceNodeKind::Equation { form }
}

const fn chemical_node(form: ChemicalSyntaxKind) -> SourceNodeKind {
    SourceNodeKind::Chemical { form }
}

const fn quantity_node(form: QuantitySyntaxKind) -> SourceNodeKind {
    SourceNodeKind::Quantity { form }
}

const fn name_node(form: NameSyntaxKind) -> SourceNodeKind {
    SourceNodeKind::Name { form }
}
