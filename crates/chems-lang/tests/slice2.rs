use std::{collections::BTreeSet, fs, path::PathBuf};

use chems_lang::{
    ByteSpan, CommentPlacement, EditError, SafeEdit, SourceAst, SourceObservation, SourcePosition,
    TokenKind, apply_safe_edits, format_source, lex_bytes, lex_source, parse_bytes, parse_source,
};
use serde_json::{Value, json};

const CANONICAL: &str = include_str!("../../../conformance/parsing/canonical-source-001.chems");
const ALL_PRODUCTIONS: &str =
    include_str!("../../../conformance/parsing/all-productions-001.chems");
const COMMENT_SOURCE: &str =
    include_str!("../../../conformance/encoding-layout/nested-comments-001.chems");
const FORMAT_INPUT: &str =
    include_str!("../../../conformance/formatting/canonical-comments-001.input.chems");
const FORMAT_EXPECTED: &str =
    include_str!("../../../conformance/formatting/canonical-comments-001.formatted.chems");
const DIAGNOSTIC_SOURCE: &str =
    include_str!("../../../conformance/diagnostics-tooling/stable-diagnostics-001.chems");
const FORMULA_SUBSCRIPTS: &str =
    include_str!("../../../conformance/parsing/formula-subscripts-001.chems");

#[test]
fn every_normative_production_is_reached_losslessly() {
    let result = parse_source(ALL_PRODUCTIONS);
    assert!(result.is_complete(), "{:#?}", result.diagnostics);
    let expected = grammar_productions(include_str!("../../../grammar/chems.ebnf"));
    let actual = result
        .ast
        .production_trace
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 42);

    let reconstructed = result
        .cst
        .tokens
        .iter()
        .filter(|token| !token.synthetic)
        .map(|token| token.text.as_str())
        .collect::<String>();
    assert_eq!(reconstructed, ALL_PRODUCTIONS);

    let reaction = result.ast.reaction.expect("reaction is typed");
    assert_eq!(reaction.reactants.len(), 2);
    assert_eq!(reaction.products.len(), 4);
    assert_eq!(reaction.observations.unwrap().entries.len(), 4);
}

#[test]
fn canonical_source_matches_independently_authored_cst_and_ast_oracles() {
    let result = parse_source(CANONICAL);
    assert!(result.is_complete(), "{:#?}", result.diagnostics);
    let cst_expected: Value = fixture_json("conformance/parsing/canonical-source-001.cst.json");
    let ast_expected: Value = fixture_json("conformance/parsing/canonical-source-001.ast.json");
    assert_eq!(cst_projection(&result), cst_expected);
    assert_eq!(ast_oracle_projection(&result.ast), ast_expected);
}

#[test]
fn lossless_tokens_exactly_partition_source_with_exact_spans() {
    for source in [CANONICAL, ALL_PRODUCTIONS, COMMENT_SOURCE] {
        let parsed = parse_source(source);
        let concrete = parsed
            .cst
            .tokens
            .iter()
            .filter(|token| !token.synthetic)
            .collect::<Vec<_>>();
        let mut cursor = 0;
        for token in concrete {
            assert_eq!(token.span.start, cursor);
            assert_eq!(&source[token.span.start..token.span.end], token.text);
            cursor = token.span.end;
        }
        assert_eq!(cursor, source.len());
    }
}

#[test]
fn canonical_ast_is_typed_for_the_structural_language() {
    let result = parse_source(CANONICAL);
    let reaction = result.ast.reaction.expect("reaction is present");
    assert_eq!(reaction.name, "LithiumAndWater");
    assert_eq!(reaction.reactants[0].structure, "LithiumMetal");
    assert_eq!(reaction.products[0].coefficient, "2");
    let equation = reaction.equation.expect("equation is present");
    assert_eq!(equation.reactants[0].formula, "Li");
    assert_eq!(equation.products[1].formula, "H2");
    let model = reaction.model.expect("model is present");
    assert_eq!(serde_json::to_value(model.event).unwrap(), "representative");
    let application = reaction
        .rule_application
        .expect("rule application is present");
    assert_eq!(application.rule, "Rules.AlkaliMetalWithWater");
    assert_eq!(application.bindings[2].role, "hydroxide");
}

#[test]
fn comments_are_lossless_and_attached_deterministically() {
    let result = parse_source(COMMENT_SOURCE);
    assert!(result.is_complete(), "{:#?}", result.diagnostics);
    assert_eq!(result.ast.comments.len(), 3);
    assert!(
        result
            .ast
            .comments
            .iter()
            .any(|comment| comment.placement == CommentPlacement::Trailing)
    );
    assert_eq!(
        comment_texts(&result.cst.tokens),
        vec![
            "/- outer /- nested -/ comment -/",
            "-- trailing",
            "-- leading observation comment"
        ]
    );

    let formatted = format_source(COMMENT_SOURCE).expect("comment source formats");
    assert_eq!(comment_texts(&lex_source(&formatted).tokens).len(), 3);
    assert!(formatted.contains("    -- leading observation comment\n"));

    let lf_signature = result
        .ast
        .comments
        .iter()
        .map(|comment| (comment.placement, comment.node_kind.clone()))
        .collect::<Vec<_>>();
    let crlf = COMMENT_SOURCE.replace('\n', "\r\n");
    let crlf_signature = parse_source(&crlf)
        .ast
        .comments
        .iter()
        .map(|comment| (comment.placement, comment.node_kind.clone()))
        .collect::<Vec<_>>();
    assert_eq!(lf_signature, crlf_signature);
}

#[test]
fn comments_can_separate_formula_characters_without_changing_meaning() {
    let source = CANONICAL.replacen("H2O[molecular]", "H/- count -/2O[molecular]", 1);
    let parsed = parse_source(&source);
    assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed
            .ast
            .reaction
            .as_ref()
            .unwrap()
            .equation
            .as_ref()
            .unwrap()
            .reactants[1]
            .formula,
        "H2O"
    );
    let formatted = format_source(&source).unwrap();
    assert!(parse_source(&formatted).is_complete());
    assert!(formatted.contains("/- count -/"));
}

#[test]
fn formula_subscripts_are_lossless_input_with_ascii_semantics_and_formatting() {
    let parsed = parse_source(FORMULA_SUBSCRIPTS);
    assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
    let equation = parsed
        .ast
        .reaction
        .as_ref()
        .unwrap()
        .equation
        .as_ref()
        .unwrap();
    assert_eq!(equation.reactants[0].formula, "C10H22");
    assert_eq!(equation.reactants[1].formula, "Ca(OH)2");
    assert_eq!(equation.reactants[2].formula, "CuSO4.5H2O");
    assert_eq!(equation.products[0].formula, "C10H22");
    assert_eq!(equation.products[1].formula, "Ca(OH)2");
    assert_eq!(equation.products[2].formula, "CuSO4.5H2O");

    let subscript_tokens = parsed
        .cst
        .tokens
        .iter()
        .filter(|token| token.kind == TokenKind::SubscriptNumber)
        .collect::<Vec<_>>();
    assert_eq!(
        subscript_tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>(),
        ["₁₀", "₂₂", "₂", "₄", "₂"]
    );
    for token in subscript_tokens {
        assert_eq!(
            &FORMULA_SUBSCRIPTS[token.span.start..token.span.end],
            token.text
        );
    }
    let middle_dot = parsed
        .cst
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::MiddleDot)
        .expect("middle dot token");
    assert_eq!(middle_dot.text, "·");
    assert_eq!(
        &FORMULA_SUBSCRIPTS[middle_dot.span.start..middle_dot.span.end],
        "·"
    );

    let formatted = format_source(FORMULA_SUBSCRIPTS).expect("subscript source formats");
    assert!(!formatted.chars().any(|character| matches!(
        character,
        '₀' | '₁' | '₂' | '₃' | '₄' | '₅' | '₆' | '₇' | '₈' | '₉'
    )));
    assert!(!formatted.contains('·'));
    assert!(formatted.contains("C10H22[molecular] + Ca(OH)2[ionic] + CuSO4.5H2O[ionic]"));
    assert_eq!(format_source(&formatted).unwrap(), formatted);
}

#[test]
fn copied_methane_and_oxygen_formulae_normalize_to_ascii() {
    let source = FORMULA_SUBSCRIPTS.replacen(
        "C₁₀H₂₂[molecular] + Ca(OH)₂[ionic] + CuSO₄·5H₂O[ionic]",
        "CH₄[molecular] + O₂[molecular]",
        1,
    );
    let parsed = parse_source(&source);
    assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
    let formulae = parsed
        .ast
        .reaction
        .unwrap()
        .equation
        .unwrap()
        .reactants
        .into_iter()
        .map(|term| term.formula)
        .collect::<Vec<_>>();
    assert_eq!(formulae, ["CH4", "O2"]);
}

#[test]
fn subscript_count_before_a_group_formats_idempotently_without_whitespace() {
    let source = FORMULA_SUBSCRIPTS.replacen("C₁₀H₂₂[molecular]", "(NH₄)₂(SO₄)[ionic]", 1);
    let parsed = parse_source(&source);
    assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed
            .ast
            .reaction
            .as_ref()
            .unwrap()
            .equation
            .as_ref()
            .unwrap()
            .reactants[0]
            .formula,
        "(NH4)2(SO4)"
    );
    let formatted = format_source(&source).expect("grouped subscript formula formats");
    assert!(formatted.contains("(NH4)2(SO4)[ionic]"));
    assert_eq!(format_source(&formatted).unwrap(), formatted);
}

#[test]
fn formula_subscripts_do_not_broaden_other_integers_or_malformed_counts() {
    let coefficient = FORMULA_SUBSCRIPTS.replacen("C₁₀H₂₂[molecular]", "₂ C₁₀H₂₂[molecular]", 1);
    assert!(!parse_source(&coefficient).is_complete());

    let catalogue_version =
        FORMULA_SUBSCRIPTS.replacen("ChemSpec.Theoretical@1", "ChemSpec.Theoretical@1·2", 1);
    assert!(!parse_source(&catalogue_version).is_complete());

    for malformed in ["C₀₂H₂₂", "C1₀H₂₂", "C₁0H₂₂", "Ca(OH)₀"] {
        let source = FORMULA_SUBSCRIPTS.replacen("C₁₀H₂₂", malformed, 1);
        assert!(!parse_source(&source).is_complete(), "accepted {malformed}");
    }
}

#[test]
fn multiline_nested_comments_remain_parseable_after_formatting() {
    let source = COMMENT_SOURCE.replace(
        "/- outer /- nested -/ comment -/",
        "/- outer\n       /- nested -/\n       comment -/",
    );
    let parsed = parse_source(&source);
    assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
    let before = comment_texts(&parsed.cst.tokens)
        .into_iter()
        .collect::<String>();
    let formatted = format_source(&source).unwrap();
    let reparsed = parse_source(&formatted);
    assert!(reparsed.is_complete(), "{:#?}", reparsed.diagnostics);
    let after = comment_texts(&reparsed.cst.tokens)
        .into_iter()
        .collect::<String>();
    assert_eq!(before, after);
}

#[test]
fn encoding_matrix_has_stable_codes_and_byte_spans() {
    let matrix = fixture_json("conformance/encoding-layout/encoding-matrix-001.input.json");
    for case in matrix["accepted"].as_array().unwrap() {
        let result = if let Some(hex) = case["bytes_hex"].as_str() {
            lex_bytes(&decode_hex(hex))
        } else {
            lex_source(case["source"].as_str().unwrap())
        };
        assert!(
            result.diagnostics.is_empty(),
            "accepted case {}: {:#?}",
            case["name"],
            result.diagnostics
        );
    }
    for case in matrix["rejected"].as_array().unwrap() {
        let result = if let Some(hex) = case["bytes_hex"].as_str() {
            lex_bytes(&decode_hex(hex))
        } else {
            lex_source(case["source"].as_str().unwrap())
        };
        let span = ByteSpan::new(
            usize::try_from(case["span"]["start"].as_u64().unwrap()).unwrap(),
            usize::try_from(case["span"]["end"].as_u64().unwrap()).unwrap(),
        );
        assert!(
            result.diagnostics.iter().any(|diagnostic| diagnostic.code
                == case["code"].as_str().unwrap()
                && diagnostic.primary_span == span),
            "case {}: {:#?}",
            case["name"],
            result.diagnostics
        );
    }

    let fixture_reserved = matrix["reserved_words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|word| word.as_str().unwrap())
        .collect::<Vec<_>>();
    let normative_reserved = include_str!("../../../conformance/reserved-words.txt")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(fixture_reserved, normative_reserved);
    for reserved in fixture_reserved {
        let source = CANONICAL.replacen("metal := lithium", &format!("{reserved} := lithium"), 1);
        assert!(
            parse_source(&source)
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "CHEMS-P004"),
            "reserved word {reserved} was accepted"
        );
    }
}

#[test]
fn bom_crlf_and_bare_cr_are_accepted_and_format_to_lf() {
    for source in [
        format!("\u{feff}{CANONICAL}"),
        CANONICAL.replace('\n', "\r\n"),
        CANONICAL.replace('\n', "\r"),
    ] {
        let parsed = parse_source(&source);
        assert!(parsed.is_complete(), "{:#?}", parsed.diagnostics);
        assert_eq!(parsed.cst.root.span, ByteSpan::new(0, source.len()));
        let formatted = format_source(&source).unwrap();
        assert!(!formatted.starts_with('\u{feff}'));
        assert!(!formatted.contains('\r'));
        assert!(formatted.ends_with('\n'));
    }
    assert_eq!(
        lex_source("\u{feff}chems 1\n").tokens[0].kind,
        TokenKind::Bom
    );
}

#[test]
fn source_positions_treat_lf_crlf_and_cr_as_one_logical_newline() {
    for source in ["a\nb", "a\r\nb", "a\rb"] {
        let position = SourcePosition::at(source, source.len());
        assert_eq!(position.line, 1);
        assert_eq!(position.scalar_column, 1);
    }
    assert_eq!(SourcePosition::at("a\rb", 2).line, 1);
    assert_eq!(SourcePosition::at("a\rb", 2).scalar_column, 0);
}

#[test]
fn formatter_matches_golden_is_idempotent_and_preserves_meaning() {
    let formatted = format_source(FORMAT_INPUT).expect("input fixture formats");
    assert_eq!(formatted, FORMAT_EXPECTED);
    assert_eq!(format_source(&formatted).unwrap(), formatted);
    assert_eq!(
        ast_meaning_projection(&parse_source(FORMAT_INPUT).ast),
        ast_meaning_projection(&parse_source(&formatted).ast)
    );
    let mut before = comment_texts(&lex_source(FORMAT_INPUT).tokens);
    let mut after = comment_texts(&lex_source(&formatted).tokens);
    before.sort_unstable();
    after.sort_unstable();
    assert_eq!(before, after);
}

#[test]
fn full_grammar_fixture_round_trips_through_canonical_formatting() {
    let before = parse_source(ALL_PRODUCTIONS);
    let formatted = format_source(ALL_PRODUCTIONS).expect("full grammar fixture formats");
    assert_eq!(format_source(&formatted).unwrap(), formatted);
    let after = parse_source(&formatted);
    assert!(after.is_complete(), "{:#?}", after.diagnostics);
    assert_eq!(
        ast_meaning_projection(&before.ast),
        ast_meaning_projection(&after.ast)
    );
    assert_eq!(before.ast.production_trace, after.ast.production_trace);
}

#[test]
fn long_equation_wraps_only_before_arrow_and_reparses() {
    let repeated = std::iter::repeat_n("H2O[molecular]", 8)
        .collect::<Vec<_>>()
        .join(" + ");
    let source = CANONICAL.replace(
        "2 Li[metallic] + 2 H2O[molecular]\n    -> 2 LiOH[ionic] + H2[molecular]",
        &format!("{repeated} -> {repeated}"),
    );
    let formatted = format_source(&source).expect("long equation formats");
    assert!(formatted.contains("\n    -> "));
    assert!(parse_source(&formatted).is_complete());
    assert_eq!(format_source(&formatted).unwrap(), formatted);
}

#[test]
fn long_comment_arrows_are_never_treated_as_equation_tokens() {
    let comment = format!(" -- {} [note] -> [retained]", "a".repeat(120));
    let source = CANONICAL.replacen(
        "reaction LithiumAndWater where",
        &format!("reaction LithiumAndWater where{comment}"),
        1,
    );
    let formatted = format_source(&source).expect("long comment remains valid");
    assert!(formatted.contains("[note] -> [retained]"));
    assert!(parse_source(&formatted).is_complete());
}

#[test]
fn missing_header_has_an_exact_diagnostic_and_working_safe_edit() {
    let source = CANONICAL.strip_prefix("chems 1\n").unwrap();
    let parsed = parse_source(source);
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "CHEMS-P001")
        .expect("missing header diagnostic");
    assert_eq!(diagnostic.primary_span, ByteSpan::new(0, 3));
    assert_eq!(diagnostic.safe_edits.len(), 1);
    assert!(diagnostic.explanation.contains("never guesses"));
    let repaired = apply_safe_edits(source, &diagnostic.safe_edits).unwrap();
    assert!(parse_source(&repaired).is_complete());
}

#[test]
fn missing_header_edit_is_offered_only_when_insertion_is_safe() {
    let parsed = parse_source(&format!("1\n{CANONICAL}"));
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "CHEMS-P001")
        .unwrap();
    assert!(diagnostic.safe_edits.is_empty());
}

#[test]
fn malformed_fixture_matches_the_exact_diagnostic_oracle() {
    let parsed = parse_source(DIAGNOSTIC_SOURCE);
    assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.diagnostics[0].code, "CHEMS-P002");
    assert_eq!(parsed.diagnostics[0].primary_span, ByteSpan::new(6, 7));
}

#[test]
fn safe_edits_reject_overlap_bounds_and_split_utf8() {
    assert_eq!(
        apply_safe_edits(
            "abcd",
            &[
                SafeEdit {
                    span: ByteSpan::new(0, 2),
                    replacement: "x".to_owned(),
                },
                SafeEdit {
                    span: ByteSpan::new(1, 3),
                    replacement: "y".to_owned(),
                }
            ]
        ),
        Err(EditError::Overlapping(
            ByteSpan::new(0, 2),
            ByteSpan::new(1, 3)
        ))
    );
    assert!(matches!(
        apply_safe_edits(
            "a",
            &[SafeEdit {
                span: ByteSpan::new(0, 2),
                replacement: String::new()
            }]
        ),
        Err(EditError::OutOfBounds(_))
    ));
    assert!(matches!(
        apply_safe_edits(
            "é",
            &[SafeEdit {
                span: ByteSpan::new(1, 2),
                replacement: String::new()
            }]
        ),
        Err(EditError::NotCharacterBoundary(_))
    ));
    assert!(matches!(
        apply_safe_edits(
            "abc",
            &[
                SafeEdit {
                    span: ByteSpan::empty(1),
                    replacement: "x".to_owned()
                },
                SafeEdit {
                    span: ByteSpan::empty(1),
                    replacement: "y".to_owned()
                }
            ]
        ),
        Err(EditError::Overlapping(_, _))
    ));
}

#[test]
fn reserved_and_identifier_classes_are_enforced() {
    let reserved = CANONICAL.replacen("lithium :=", "reaction :=", 1);
    assert!(
        parse_source(&reserved)
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P004")
    );
    let lowercase_claim = CANONICAL.replacen("claim R1", "claim r1", 1);
    assert!(
        parse_source(&lowercase_claim)
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "CHEMS-P003")
    );
    let bad_value = CANONICAL.replacen("lithium :=", "Lithium :=", 1);
    assert!(!parse_source(&bad_value).is_complete());
}

#[test]
fn section_order_and_required_entries_are_not_recovered_as_valid() {
    let wrong_order = CANONICAL
        .replacen("  reactants", "  TEMP", 1)
        .replacen("  products", "  reactants", 1)
        .replacen("  TEMP", "  products", 1);
    let parsed = parse_source(&wrong_order);
    assert!(!parsed.is_complete());
    assert!(parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "CHEMS-P003" && diagnostic.summary.contains("reactants")
    }));

    let no_bindings = CANONICAL.replace(
        "      metal := lithium\n      water := water\n      hydroxide := lithiumHydroxide\n      gasProduct := hydrogen\n",
        "",
    );
    assert!(!parse_source(&no_bindings).is_complete());

    for source in [
        CANONICAL.replacen("  reactants\n", "  reactants\n\n", 1),
        CANONICAL.replacen(
            "    lithium := 2 of LithiumMetal\n    water",
            "    lithium := 2 of LithiumMetal\n\n    water",
            1,
        ),
    ] {
        let parsed = parse_source(&source);
        assert!(!parsed.is_complete());
        assert!(parsed.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "CHEMS-P003" && diagnostic.summary.contains("blank lines")
        }));
    }
}

#[test]
fn discarded_quantitative_syntax_is_only_negative_input() {
    let old_source = include_str!("../../../conformance/parsing/formula-whitespace-001.chems");
    let parsed = parse_source(old_source);
    assert!(!parsed.is_complete());
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| { matches!(diagnostic.code.as_str(), "CHEMS-P003" | "CHEMS-P007") })
    );
    assert!(!parsed.ast.production_trace.iter().any(|name| {
        matches!(
            name.as_str(),
            "experiment" | "conditions-section" | "quantity" | "procedure-section"
        )
    }));
}

#[test]
fn unsupported_language_major_is_unbounded_and_distinct() {
    let source = CANONICAL.replacen(
        "chems 1",
        "chems 999999999999999999999999999999999999999999999999999999",
        1,
    );
    let parsed = parse_source(&source);
    let codes = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with("CHEMS-P"))
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(codes, vec!["CHEMS-P002"]);
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
        let _ = parse_source(&String::from_utf8_lossy(&bytes));
    }
}

#[test]
fn fatal_encoding_and_truncated_eof_have_bounded_recovery() {
    let invalid = parse_bytes(&[0xff]);
    assert_eq!(invalid.diagnostics.len(), 1);
    assert_eq!(invalid.diagnostics[0].code, "CHEMS-L001");
    assert!(invalid.cst.root.recovery);

    for source in [
        "",
        "chems 1\n",
        "chems 1\nuse catalog ChemSpec.Test@1\nreaction",
    ] {
        let parsed = parse_source(source);
        assert!(parsed.diagnostics.len() <= 3, "{:#?}", parsed.diagnostics);
        assert!(parsed.ast.production_trace.len() <= 16);
    }
}

fn fixture_json(path: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
}

fn comment_texts(tokens: &[chems_lang::Token]) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| token.kind.is_comment())
        .map(|token| token.text.clone())
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

fn cst_projection(result: &chems_lang::ParseResult) -> Value {
    let count = |kind| {
        result
            .cst
            .tokens
            .iter()
            .filter(|token| token.kind == kind)
            .count()
    };
    let reaction = result
        .cst
        .root
        .children
        .iter()
        .find(|node| node.kind == "reaction-declaration")
        .unwrap();
    let mut formulae = Vec::new();
    collect_nodes(&result.cst.root, "formula", &mut formulae);
    json!({
        "schema_version": result.cst.schema_version,
        "source_length": result.cst.source.len(),
        "token_count": result.cst.tokens.len(),
        "root_span": result.cst.root.span,
        "top_level": result.cst.root.children.iter().map(|node| json!({
            "kind": node.kind,
            "span": node.span,
        })).collect::<Vec<_>>(),
        "layout": {
            "indent": count(TokenKind::Indent),
            "dedent": count(TokenKind::Dedent),
            "newline": count(TokenKind::Newline),
        },
        "reaction_children":reaction.children.iter().map(|node| json!({
            "kind":node.kind, "span":node.span
        })).collect::<Vec<_>>(),
        "formula_shapes":formulae.into_iter().map(node_shape).collect::<Vec<_>>(),
    })
}

fn collect_nodes<'a>(
    node: &'a chems_lang::SyntaxNode,
    kind: &str,
    output: &mut Vec<&'a chems_lang::SyntaxNode>,
) {
    if node.kind == kind {
        output.push(node);
    }
    for child in &node.children {
        collect_nodes(child, kind, output);
    }
}

fn node_shape(node: &chems_lang::SyntaxNode) -> Value {
    json!({
        "kind":node.kind,
        "span":node.span,
        "children":node.children.iter().map(node_shape).collect::<Vec<_>>(),
    })
}

#[allow(clippy::too_many_lines)]
fn ast_oracle_projection(ast: &SourceAst) -> Value {
    let reaction = ast.reaction.as_ref().expect("complete AST has reaction");
    let equation = reaction
        .equation
        .as_ref()
        .expect("complete AST has equation");
    let observations = reaction
        .observations
        .as_ref()
        .expect("complete AST has observations");
    let model = reaction.model.as_ref().expect("complete AST has model");
    let application = reaction
        .rule_application
        .as_ref()
        .expect("complete AST has application");
    let observation = |entry: &SourceObservation| match entry {
        SourceObservation::GasEvolves {
            gas, claim, span, ..
        } => {
            json!({"kind":"gasEvolves", "subject":gas, "claim":claim, "span":span})
        }
        SourceObservation::ReactantDisappears {
            reactant,
            claim,
            span,
            ..
        } => json!({"kind":"reactantDisappears", "subject":reactant, "claim":claim, "span":span}),
        SourceObservation::ProductForms {
            product,
            claim,
            span,
            ..
        } => {
            json!({"kind":"productForms", "subject":product, "claim":claim, "span":span})
        }
        SourceObservation::ProductColour {
            product,
            colour,
            claim,
            span,
            ..
        } => json!({
            "kind":"productColour", "subject":product, "colour":colour, "claim":claim,
            "span":span
        }),
    };
    let term = |term: &chems_lang::SourceEquationTerm| {
        json!({
            "coefficient": term.coefficient,
            "formula": term.formula,
            "representation": term.representation,
            "span":term.span,
        })
    };
    let binding = |binding: &chems_lang::SourceStructureBinding| {
        json!({
            "name":binding.name,
            "coefficient":binding.coefficient,
            "structure":binding.structure,
            "span":binding.span,
        })
    };
    json!({
        "schema_version": ast.schema_version,
        "complete":ast.complete,
        "comments":ast.comments,
        "language": ast.language.as_ref().map(|value| json!({
            "lexeme":value.lexeme, "span":value.span
        })),
        "catalogue": ast.catalogue.as_ref().map(|value| json!({
            "name":value.name, "version":value.version, "span":value.span
        })),
        "reaction": {
            "name":reaction.name,
            "span":reaction.span,
            "reactants":reaction.reactants.iter().map(binding).collect::<Vec<_>>(),
            "products":reaction.products.iter().map(binding).collect::<Vec<_>>(),
            "equation": {
                "span":equation.span,
                "reactants":equation.reactants.iter().map(term).collect::<Vec<_>>(),
                "products":equation.products.iter().map(term).collect::<Vec<_>>(),
            },
            "model":{"event":model.event, "sequence":model.sequence, "span":model.span},
            "observations":{
                "evidence":observations.evidence,
                "version":observations.version,
                "span":observations.span,
                "entries":observations.entries.iter().map(observation).collect::<Vec<_>>(),
            },
            "rule_application":{
                "rule":application.rule,
                "span":application.span,
                "bindings":application.bindings.iter().map(|binding| json!({
                    "role":binding.role, "value":binding.value, "span":binding.span
                })).collect::<Vec<_>>(),
            }
        }
    })
}

fn ast_meaning_projection(ast: &SourceAst) -> Value {
    let mut value = ast_oracle_projection(ast);
    strip_source_metadata(&mut value);
    value
}

fn strip_source_metadata(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("span");
            object.remove("comments");
            object.remove("complete");
            for value in object.values_mut() {
                strip_source_metadata(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_source_metadata(value);
            }
        }
        _ => {}
    }
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(pair, 16).unwrap()
        })
        .collect()
}
