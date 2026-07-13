use std::{collections::BTreeSet, fmt::Write};

use chems_lang::{
    ByteSpan, CommentPlacement, SectionKind, SourceAst, SourceNode, SourceNodeKind, TokenKind,
    format_source, lex_bytes, lex_source, parse_bytes, parse_source,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ALL_PRODUCTIONS: &str =
    include_str!("../../../conformance/parsing/all-productions-001.chems");
const COMMENT_SOURCE: &str =
    include_str!("../../../conformance/encoding-layout/nested-comments-001.chems");
const FORMAT_INPUT: &str =
    include_str!("../../../conformance/formatting/canonical-comments-001.input.chems");
const FORMAT_EXPECTED: &str =
    include_str!("../../../conformance/formatting/canonical-comments-001.formatted.chems");

#[test]
fn malformed_parser_fixtures_match_exact_diagnostics() {
    for (source, code, span) in [
        (
            include_str!("../../../conformance/parsing/formula-whitespace-001.chems"),
            "CHEMS-P005",
            ByteSpan::new(147, 148),
        ),
        (
            include_str!("../../../conformance/parsing/joined-quantity-001.chems"),
            "CHEMS-P006",
            ByteSpan::new(105, 105),
        ),
        (
            include_str!("../../../conformance/parsing/unsupported-major-001.chems"),
            "CHEMS-P002",
            ByteSpan::new(6, 60),
        ),
    ] {
        let parsed = parse_source(source);
        assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.diagnostics[0].code, code);
        assert_eq!(parsed.diagnostics[0].primary_span, span);
        assert!(!parsed.ast.complete);
        assert!(has_recovery_node(&parsed.ast.document) || code == "CHEMS-P002");
    }
}

#[test]
fn every_normative_production_is_reached() {
    let result = parse_source(ALL_PRODUCTIONS);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let ast = result.ast;
    let expected = grammar_productions(include_str!("../../../grammar/chems.ebnf"));
    let actual: BTreeSet<_> = ast.production_trace.iter().map(String::as_str).collect();
    assert_eq!(actual, expected);

    let reconstructed: String = result
        .cst
        .tokens
        .iter()
        .filter(|token| !token.synthetic)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(reconstructed, ALL_PRODUCTIONS);
    assert_eq!(
        ast.catalogue
            .as_ref()
            .map(|catalogue| catalogue.name.as_str()),
        Some("ChemSpec.Aqueous")
    );
    let experiment = ast.experiment.as_ref().expect("experiment is present");
    assert_eq!(experiment.name, "AllForms");
    assert_eq!(experiment.materials.len(), 4);
    assert_eq!(experiment.procedure.len(), 12);
    assert_eq!(experiment.expectations.len(), 2);
    assert_eq!(experiment.tactics.len(), 11);
}

#[test]
fn complete_cst_and_ast_match_exact_golden_digests() {
    let result = parse_source(ALL_PRODUCTIONS);
    let cst = serde_json::to_vec(&result.cst).expect("CST serializes");
    let ast = serde_json::to_vec(&result.ast).expect("AST serializes");
    assert_eq!(
        hex_sha256(&cst),
        include_str!("../../../conformance/parsing/all-productions-001.cst.digest").trim()
    );
    assert_eq!(
        hex_sha256(&ast),
        include_str!("../../../conformance/parsing/all-productions-001.ast.digest").trim()
    );
    let ast_json = String::from_utf8(ast).expect("serialized AST is UTF-8 JSON");
    assert!(!ast_json.contains("token_indices"));
    assert!(ast_json.contains("\"kind\":\"operation\""));
    assert!(ast_json.contains("\"operation\":\"transfer\""));
}

#[test]
fn comments_are_lossless_and_attached_deterministically() {
    let result = parse_source(COMMENT_SOURCE);
    assert!(result.is_complete(), "{:#?}", result.diagnostics);
    let ast = result.ast;
    assert_eq!(ast.comments.len(), 3);
    assert!(
        ast.comments
            .iter()
            .any(|comment| comment.placement == CommentPlacement::Trailing)
    );
    assert_eq!(
        comment_texts(&result.cst.tokens),
        vec![
            "/- outer /- nested -/ comment -/",
            "-- trailing",
            "-- leading material comment"
        ]
    );

    let separated = COMMENT_SOURCE.replace(
        "    -- leading material comment\n    water",
        "    -- leading material comment\n\n    water",
    );
    let parsed = parse_source(&separated);
    let ast = parsed.ast;
    let comment = ast
        .comments
        .iter()
        .find(|attachment| {
            parsed.cst.tokens[attachment.token_index].text == "-- leading material comment"
        })
        .expect("material comment is attached");
    assert_eq!(comment.placement, CommentPlacement::Enclosing);
}

#[test]
fn lexical_diagnostics_have_stable_codes_and_byte_spans() {
    assert_diagnostic(&lex_bytes(&[b'a', 0xff]).diagnostics, "CHEMS-L001", 1, 2);
    assert_diagnostic(
        &lex_source("\u{feff}chems 1\n").diagnostics,
        "CHEMS-L002",
        0,
        3,
    );
    assert_diagnostic(&lex_source("\0").diagnostics, "CHEMS-L003", 0, 1);
    assert_diagnostic(&lex_source("\t").diagnostics, "CHEMS-L004", 0, 1);
    assert_diagnostic(
        &lex_source("chems 1\n   bad\n").diagnostics,
        "CHEMS-L005",
        8,
        11,
    );
    assert_diagnostic(&lex_source("/- open").diagnostics, "CHEMS-L006", 0, 7);
    assert_diagnostic(&lex_source("-/").diagnostics, "CHEMS-L007", 0, 2);
    assert_diagnostic(&lex_source("é").diagnostics, "CHEMS-L008", 0, 2);
    assert_diagnostic(&lex_source("&").diagnostics, "CHEMS-L009", 0, 1);
    assert_diagnostic(&lex_source("\r").diagnostics, "CHEMS-L010", 0, 1);
    assert_diagnostic(
        &lex_source("-- comment\0\n").diagnostics,
        "CHEMS-L003",
        10,
        11,
    );

    let crlf = lex_source("chems 1\r\n");
    assert!(crlf.diagnostics.is_empty());
    assert!(crlf.tokens.iter().any(|token| {
        token.kind == TokenKind::Newline && token.text == "\r\n" && !token.synthetic
    }));
}

#[test]
fn parser_never_guesses_a_header_or_accepts_reserved_identifiers() {
    let headerless = parse_source("use catalog ChemSpec.Aqueous@1\n");
    assert!(!headerless.ast.complete);
    assert!(
        headerless
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P001")
    );
    assert!(has_recovery_node(&headerless.ast.document));

    let reserved = ALL_PRODUCTIONS.replacen("water :=", "given :=", 1);
    let parsed = parse_source(&reserved);
    assert!(!parsed.ast.complete);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P004")
    );

    let zero_subscript = ALL_PRODUCTIONS.replacen("H2O(l)", "H0O(l)", 1);
    assert!(
        parse_source(&zero_subscript)
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P003")
    );
}

#[test]
fn compact_syntax_enforces_spaces_without_discarding_comment_boundaries() {
    for malformed in [
        ALL_PRODUCTIONS.replacen("Ca(OH)2(s)", "Ca (OH)2(s)", 1),
        ALL_PRODUCTIONS.replacen("kg*m/s^2", "kg * m / s ^ 2", 1),
    ] {
        let parsed = parse_source(&malformed);
        assert!(!parsed.is_complete());
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "CHEMS-P005")
        );
    }
    let joined_quantity = ALL_PRODUCTIONS.replacen("10.0 mL", "10.0mL", 1);
    assert!(
        parse_source(&joined_quantity)
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P006")
    );

    for valid in [
        ALL_PRODUCTIONS.replacen("H2O(l)", "H/- count -/2O(l)", 1),
        ALL_PRODUCTIONS.replacen("kg*m/s^2", "kg/- a -/*m/- b -//s^2", 1),
        ALL_PRODUCTIONS.replacen("1.00 mol", "1.00/- unit -/mol", 1),
    ] {
        let parsed = parse_source(&valid);
        assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
        let formatted = format_source(&valid).expect("comment boundary formats");
        assert!(parse_source(&formatted).is_complete());
    }
}

#[test]
fn unsupported_language_major_is_unbounded() {
    let source = ALL_PRODUCTIONS.replacen(
        "chems 1",
        "chems 999999999999999999999999999999999999999999999999999999",
        1,
    );
    let parsed = parse_source(&source);
    let parse_codes = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with("CHEMS-P"))
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(parse_codes, vec!["CHEMS-P002"]);
}

#[test]
fn formatter_matches_golden_is_idempotent_and_preserves_comments() {
    let formatted = format_source(FORMAT_INPUT).expect("input fixture formats");
    assert_eq!(formatted, FORMAT_EXPECTED);
    assert_eq!(format_source(&formatted).unwrap(), formatted);

    let before = lex_source(FORMAT_INPUT);
    let after = lex_source(&formatted);
    let mut comments_before = comment_texts(&before.tokens);
    let mut comments_after = comment_texts(&after.tokens);
    comments_before.sort_unstable();
    comments_after.sort_unstable();
    assert_eq!(comments_before, comments_after);
    let before_ast = parse_source(FORMAT_INPUT).ast;
    let after_ast = parse_source(&formatted).ast;
    assert_eq!(before_ast.production_trace, after_ast.production_trace);
    assert_eq!(
        semantic_signature(&before_ast),
        semantic_signature(&after_ast)
    );
    assert_eq!(
        attachment_signature(FORMAT_INPUT),
        attachment_signature(&formatted)
    );

    let crlf = FORMAT_INPUT.replace('\n', "\r\n");
    assert_eq!(format_source(&crlf).unwrap(), FORMAT_EXPECTED);
}

#[test]
fn formatter_preserves_wrapped_equation_comment_attachment() {
    let long_side = std::iter::repeat_n("H2O(l)", 12)
        .collect::<Vec<_>>()
        .join(" + ");
    let source = ALL_PRODUCTIONS.replace(
        "2 H2(g) + O2(g) -> 2 H2O(l)",
        &format!("{long_side} -> {long_side} -- equation note"),
    );
    let before = attachment_signature(&source);
    let formatted = format_source(&source).expect("commented equation formats");
    assert!(formatted.contains("-- equation note"));
    assert_eq!(before, attachment_signature(&formatted));
}

#[test]
fn formatter_indents_leading_comments_from_their_attachment() {
    let source = FORMAT_INPUT.replace("    -- medium follows", "-- medium follows");
    let formatted = format_source(&source).expect("misindented comment formats");
    assert!(formatted.contains("\n    -- medium follows\n    medium := aqueous"));
}

#[test]
fn full_grammar_fixture_round_trips_through_canonical_formatting() {
    let formatted = format_source(ALL_PRODUCTIONS).expect("full fixture formats");
    assert_eq!(format_source(&formatted).unwrap(), formatted);
    let reparsed = parse_source(&formatted);
    assert!(reparsed.is_complete(), "{:#?}", reparsed.diagnostics);
    assert!(formatted.contains("produces Ca^2+(aq)"));
    assert_eq!(
        parse_source(ALL_PRODUCTIONS).ast.production_trace,
        reparsed.ast.production_trace
    );
}

#[test]
fn long_equations_are_wrapped_and_remain_parseable() {
    let long_side = std::iter::repeat_n("H2O(l)", 12)
        .collect::<Vec<_>>()
        .join(" + ");
    let source = ALL_PRODUCTIONS.replace(
        "2 H2(g) + O2(g) -> 2 H2O(l)",
        &format!("{long_side} -> {long_side}"),
    );
    let formatted = format_source(&source).expect("long equation formats");
    assert!(formatted.contains("    molecular :=\n      "));
    assert!(formatted.contains("\n      ->\n"));
    assert!(formatted.lines().all(|line| line.len() <= 100));
    assert!(parse_source(&formatted).is_complete());
}

#[test]
fn formatter_omits_unit_charge_and_coefficient_magnitudes() {
    let source = ALL_PRODUCTIONS
        .replace("Ca^2+(aq)", "Ca^1+(aq)")
        .replace("2 H2(g)", "1 H2(g)");
    let formatted = format_source(&source).expect("explicit unit magnitudes format");
    assert!(formatted.contains("produces Ca^+(aq)"));
    assert!(formatted.contains("molecular := H2(g) + O2(g) -> 2 H2O(l)"));
    assert!(!formatted.contains("^1+"));
}

#[test]
fn arbitrary_bytes_and_utf8_do_not_panic() {
    for byte in 0_u8..=u8::MAX {
        let _ = parse_bytes(&[byte]);
    }
    let mut state = 0x9e37_79b9_u32;
    for length in 0..512 {
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            bytes.push((state >> 24) as u8);
        }
        let _ = parse_bytes(&bytes);
        let utf8 = String::from_utf8_lossy(&bytes);
        let _ = parse_source(&utf8);
    }
}

fn assert_diagnostic(diagnostics: &[chems_lang::Diagnostic], code: &str, start: usize, end: usize) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code
            && diagnostic.primary_span == ByteSpan::new(start, end)),
        "missing {code} [{start}, {end}) in {diagnostics:#?}"
    );
}

fn comment_texts(tokens: &[chems_lang::Token]) -> Vec<&str> {
    tokens
        .iter()
        .filter(|token| token.kind.is_comment())
        .map(|token| token.text.as_str())
        .collect()
}

fn attachment_signature(source: &str) -> Vec<(String, CommentPlacement, String)> {
    let parsed = parse_source(source);
    let ast = parsed.ast;
    let mut signature = ast
        .comments
        .iter()
        .map(|attachment| {
            (
                parsed.cst.tokens[attachment.token_index].text.clone(),
                attachment.placement,
                attachment.node_kind.clone(),
            )
        })
        .collect::<Vec<_>>();
    signature.sort_by(|left, right| left.0.cmp(&right.0));
    signature
}

fn has_recovery_node(node: &chems_lang::SourceNode) -> bool {
    node.recovery || node.children.iter().any(has_recovery_node)
}

fn semantic_signature(ast: &SourceAst) -> Value {
    json!({
        "languageVersion": ast.language.as_ref().map(|language| &language.lexeme),
        "catalogue": ast.catalogue.as_ref().map(|catalogue| (&catalogue.name, &catalogue.version)),
        "experiment": ast.experiment.as_ref().map(|experiment| &experiment.name),
        "document": semantic_node(&ast.document),
    })
}

fn semantic_node(node: &SourceNode) -> Value {
    let mut children = node.children.iter().map(semantic_node).collect::<Vec<_>>();
    if node.kind
        == (SourceNodeKind::Section {
            section: SectionKind::Conditions,
        })
    {
        children.sort_by_key(Value::to_string);
    }
    json!({
        "kind": node.kind,
        "lexeme": node.lexeme.as_deref().map(compact_semantic_lexeme),
        "children": children,
        "recovery": node.recovery,
    })
}

fn compact_semantic_lexeme(lexeme: &str) -> String {
    lex_source(lexeme)
        .tokens
        .iter()
        .filter(|token| {
            !token.kind.is_trivia()
                && !matches!(
                    token.kind,
                    TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
                )
        })
        .map(|token| token.text.as_str())
        .collect()
}

fn grammar_productions(grammar: &str) -> BTreeSet<&str> {
    let mut in_comment = false;
    grammar
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("(*") {
                in_comment = true;
                return None;
            }
            if trimmed.ends_with("*)") {
                in_comment = false;
                return None;
            }
            if in_comment {
                return None;
            }
            let (name, _) = line.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'))
            .then_some(name)
        })
        .collect()
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}
