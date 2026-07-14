use std::{collections::BTreeSet, error::Error, fmt};

use crate::{
    CommentAttachment, CommentPlacement, Diagnostic, SourceNode, SourceNodeKind, SyntaxNode, Token,
    TokenKind, parse_source,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot format source with {} diagnostic(s)",
            self.diagnostics.len()
        )
    }
}

impl Error for FormatError {}

/// Formats a complete `chems 1` document into its canonical source form.
///
/// Invalid or incomplete source is not rewritten: callers receive the same
/// deterministic lexer/parser diagnostics exposed by [`crate::parse_source`].
///
/// # Errors
///
/// Returns [`FormatError`] when either the input or formatted output does not
/// parse as a complete `chems 1` document.
pub fn format_source(source: &str) -> Result<String, FormatError> {
    let parsed = parse_source(source);
    if !parsed.is_complete() {
        return Err(FormatError {
            diagnostics: parsed.diagnostics,
        });
    }

    let omitted = omitted_one_tokens(&parsed.cst.root, &parsed.cst.tokens);
    let comment_indentation = comment_indentation(&parsed.ast.comments, &parsed.cst.tokens);
    let mut compact_spans = Vec::new();
    collect_compact_spans(&parsed.ast.document, &mut compact_spans);
    let compact_comments = compact_comment_tokens(&compact_spans, &parsed.cst.tokens);
    let formatted = format_tokens(
        &parsed.cst.tokens,
        &omitted,
        &comment_indentation,
        &compact_comments,
        &compact_spans,
    );
    let formatted = order_conditions(&formatted);
    let formatted = inline_short_equations(&formatted);
    let formatted = wrap_long_equations(&formatted);
    let reparsed = parse_source(&formatted);
    if !reparsed.is_complete() {
        return Err(FormatError {
            diagnostics: reparsed.diagnostics,
        });
    }
    Ok(formatted)
}

fn format_tokens(
    tokens: &[Token],
    omitted: &BTreeSet<usize>,
    comment_indentation: &std::collections::BTreeMap<usize, usize>,
    compact_comments: &BTreeSet<usize>,
    compact_spans: &[crate::ByteSpan],
) -> String {
    let mut output = String::new();
    let mut indentation = 0_usize;
    let mut line_start = true;
    let mut previous: Option<&Token> = None;
    let mut needs_space_after_comment = false;
    let mut compact_after_comment = false;
    let mut block_depth = 0_i32;
    let mut pending_leading_spaces = None;

    for (index, token) in tokens.iter().enumerate() {
        if omitted.contains(&index) {
            continue;
        }
        match token.kind {
            TokenKind::Space => {
                if line_start {
                    pending_leading_spaces = Some(token.text.len());
                }
            }
            TokenKind::Eof | TokenKind::Invalid => {}
            TokenKind::Indent => indentation += 1,
            TokenKind::Dedent => indentation = indentation.saturating_sub(1),
            TokenKind::Newline => {
                trim_trailing_spaces(&mut output);
                if !output.ends_with('\n') || !token.synthetic {
                    output.push('\n');
                }
                line_start = true;
                previous = None;
                needs_space_after_comment = false;
                compact_after_comment = false;
                pending_leading_spaces = None;
            }
            TokenKind::LineComment | TokenKind::BlockComment => {
                let compact = compact_comments.contains(&index);
                if line_start {
                    if block_depth == 0 {
                        let level = comment_indentation.get(&index).copied().unwrap_or_else(|| {
                            pending_leading_spaces.map_or(indentation, |spaces| spaces / 2)
                        });
                        output.push_str(&"  ".repeat(level));
                    }
                    line_start = false;
                } else if !compact && !output.ends_with([' ', '\n']) {
                    output.push(' ');
                }
                output.push_str(&token.text);
                if token.kind == TokenKind::BlockComment {
                    block_depth += block_depth_delta(&token.text);
                    needs_space_after_comment = block_depth == 0 && !compact;
                    compact_after_comment = block_depth == 0 && compact;
                }
            }
            _ => {
                if line_start {
                    output.push_str(&"  ".repeat(indentation));
                    line_start = false;
                }
                let next = next_semantic(tokens, index + 1);
                if !compact_after_comment
                    && !previous
                        .is_some_and(|prior| shares_compact_span(prior, token, compact_spans))
                    && (needs_space_after_comment
                        || previous.is_some_and(|prior| needs_space(prior, token, next)))
                    && !output.ends_with([' ', '\n'])
                {
                    output.push(' ');
                }
                output.push_str(&token.text);
                previous = Some(token);
                needs_space_after_comment = false;
                compact_after_comment = false;
            }
        }
    }

    while output.ends_with("\n\n\n") {
        output.pop();
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn compact_comment_tokens(compact_spans: &[crate::ByteSpan], tokens: &[Token]) -> BTreeSet<usize> {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            token.kind == TokenKind::BlockComment
                && compact_spans.iter().any(|span: &crate::ByteSpan| {
                    span.start <= token.span.start && span.end >= token.span.end
                })
        })
        .map(|(index, _)| index)
        .collect()
}

fn shares_compact_span(previous: &Token, current: &Token, spans: &[crate::ByteSpan]) -> bool {
    spans.iter().any(|span| {
        span.start <= previous.span.start
            && span.end >= previous.span.end
            && span.start <= current.span.start
            && span.end >= current.span.end
    })
}

fn collect_compact_spans(node: &SourceNode, output: &mut Vec<crate::ByteSpan>) {
    if matches!(node.kind, SourceNodeKind::Chemical { .. })
        || matches!(
            node.kind,
            SourceNodeKind::Quantity {
                form: crate::QuantitySyntaxKind::Decimal
                    | crate::QuantitySyntaxKind::UnitExpression
                    | crate::QuantitySyntaxKind::UnitProduct
                    | crate::QuantitySyntaxKind::UnitFactor
                    | crate::QuantitySyntaxKind::UnitSymbol
                    | crate::QuantitySyntaxKind::UnitName
                    | crate::QuantitySyntaxKind::SignedInteger
                    | crate::QuantitySyntaxKind::Integer
                    | crate::QuantitySyntaxKind::PositiveInteger
            }
        )
    {
        output.push(node.span);
    }
    for child in &node.children {
        collect_compact_spans(child, output);
    }
}

fn comment_indentation(
    comments: &[CommentAttachment],
    tokens: &[Token],
) -> std::collections::BTreeMap<usize, usize> {
    comments
        .iter()
        .filter(|comment| comment.placement == CommentPlacement::Leading)
        .filter_map(|comment| {
            indentation_at(tokens, comment.node_span.start)
                .map(|indentation| (comment.token_index, indentation))
        })
        .collect()
}

fn indentation_at(tokens: &[Token], offset: usize) -> Option<usize> {
    let mut indentation = 0_usize;
    for token in tokens {
        match token.kind {
            TokenKind::Indent => indentation += 1,
            TokenKind::Dedent => indentation = indentation.saturating_sub(1),
            _ if !token.synthetic && token.span.start == offset && !token.kind.is_trivia() => {
                return Some(indentation);
            }
            _ => {}
        }
    }
    None
}

fn omitted_one_tokens(root: &SyntaxNode, tokens: &[Token]) -> BTreeSet<usize> {
    let mut omitted = BTreeSet::new();
    collect_omitted_one_tokens(root, tokens, &mut omitted);
    omitted
}

fn collect_omitted_one_tokens(node: &SyntaxNode, tokens: &[Token], omitted: &mut BTreeSet<usize>) {
    if matches!(node.kind.as_str(), "charge" | "equation-term")
        && let Some(integer) = node
            .children
            .iter()
            .find(|child| child.kind == "positive-integer")
        && let Some(index) = first_token_index(integer)
        && tokens.get(index).is_some_and(|token| token.text == "1")
    {
        omitted.insert(index);
    }
    for child in &node.children {
        collect_omitted_one_tokens(child, tokens, omitted);
    }
}

fn first_token_index(node: &SyntaxNode) -> Option<usize> {
    node.token_indices
        .first()
        .copied()
        .or_else(|| node.children.iter().find_map(first_token_index))
}

fn next_semantic(tokens: &[Token], from: usize) -> Option<&Token> {
    tokens[from..].iter().find(|token| {
        !token.kind.is_trivia()
            && !matches!(
                token.kind,
                TokenKind::Indent | TokenKind::Dedent | TokenKind::Newline | TokenKind::Eof
            )
    })
}

fn needs_space(previous: &Token, current: &Token, next: Option<&Token>) -> bool {
    use TokenKind::{
        Arrow, Assignment, At, Caret, Colon, Dot, LeftParen, Minus, Number, Percent, Plus,
        RightParen, Slash, Star,
    };

    if matches!(
        current.kind,
        Dot | At | Caret | LeftParen | RightParen | Colon | Star | Slash
    ) || matches!(previous.kind, Dot | At | Caret | LeftParen | Star | Slash)
    {
        return false;
    }
    if matches!(current.kind, Assignment | Arrow) || matches!(previous.kind, Assignment | Arrow) {
        return true;
    }
    if matches!(current.kind, Plus | Minus) {
        if previous.kind == Caret || next.is_some_and(|token| token.kind == LeftParen) {
            return false;
        }
        if next.is_some_and(|token| token.kind == Number)
            && matches!(previous.kind, Assignment | Colon | Plus | Minus)
        {
            return true;
        }
        return true;
    }
    if matches!(previous.kind, Plus | Minus) && current.kind == Number {
        return false;
    }
    if current.kind == Number && matches!(previous.kind, RightParen | Caret | Dot) {
        return false;
    }
    if current.kind == Percent && previous.kind == Number {
        return true;
    }
    true
}

fn block_depth_delta(text: &str) -> i32 {
    let mut delta = 0;
    let bytes = text.as_bytes();
    for pair in bytes.windows(2) {
        if pair == b"/-" {
            delta += 1;
        } else if pair == b"-/" {
            delta -= 1;
        }
    }
    delta
}

fn trim_trailing_spaces(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
}

fn order_conditions(source: &str) -> String {
    let mut lines: Vec<&str> = source.lines().collect();
    let Some(section) = lines.iter().position(|line| line.trim() == "conditions") else {
        return source.to_owned();
    };
    let section_indent = leading_spaces(lines[section]);
    let end = ((section + 1)..lines.len())
        .find(|index| {
            let line = lines[*index];
            !line.trim().is_empty()
                && leading_spaces(line) <= section_indent
                && !line.trim_start().starts_with("--")
                && !line.trim_start().starts_with("/-")
        })
        .unwrap_or(lines.len());
    if end <= section + 1 {
        return source.to_owned();
    }

    let body = &lines[section + 1..end];
    let mut chunks: Vec<Vec<&str>> = Vec::new();
    let mut pending = Vec::new();
    let mut block_depth = 0_i32;
    for line in body {
        let trimmed = line.trim_start();
        let is_comment = block_depth > 0
            || trimmed.starts_with("--")
            || trimmed.starts_with("/-")
            || trimmed.is_empty();
        block_depth += block_depth_delta(trimmed);
        if is_comment {
            pending.push(*line);
            continue;
        }
        pending.push(*line);
        chunks.push(std::mem::take(&mut pending));
    }
    if !pending.is_empty() {
        if let Some(last) = chunks.last_mut() {
            last.extend(pending);
        } else {
            chunks.push(pending);
        }
    }
    chunks.sort_by_key(|chunk| {
        chunk
            .iter()
            .find_map(|line| condition_rank(line.trim_start()))
            .unwrap_or(usize::MAX)
    });
    let ordered: Vec<&str> = chunks.into_iter().flatten().collect();
    lines.splice(section + 1..end, ordered);
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn condition_rank(line: &str) -> Option<usize> {
    ["temperature", "pressure", "medium"]
        .iter()
        .position(|name| line.starts_with(name))
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn inline_short_equations(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !matches!(trimmed, "molecular :=" | "completeIonic :=" | "netIonic :=") {
            output.push(line.to_owned());
            index += 1;
            continue;
        }
        let heading_indent = leading_spaces(line);
        let mut end = index + 1;
        while end < lines.len()
            && (!lines[end].trim().is_empty() && leading_spaces(lines[end]) > heading_indent)
        {
            end += 1;
        }
        let continuation = &lines[index + 1..end];
        let value = continuation
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let inline = format!("{line} {value}");
        if !continuation.is_empty()
            && inline.len() <= 100
            && !value.contains("--")
            && !value.contains("/-")
        {
            output.push(inline);
            index = end;
        } else {
            output.push(line.to_owned());
            index += 1;
        }
    }
    let mut formatted = output.join("\n");
    formatted.push('\n');
    formatted
}

fn wrap_long_equations(source: &str) -> String {
    let mut output = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let is_equation = ["molecular :=", "completeIonic :=", "netIonic :="]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix));
        if line.len() <= 100 || !is_equation {
            output.push(line.to_owned());
            continue;
        }
        let Some((heading, equation)) = line.split_once(":=") else {
            output.push(line.to_owned());
            continue;
        };
        let Some((left, right)) = equation.trim().split_once(" -> ") else {
            output.push(line.to_owned());
            continue;
        };
        let indent = " ".repeat(leading_spaces(line) + 2);
        output.push(format!("{} :=", heading.trim_end()));
        push_wrapped_equation_side(&mut output, &indent, left.trim());
        output.push(format!("{indent}->"));
        push_wrapped_equation_side(&mut output, &indent, right.trim());
    }
    let mut formatted = output.join("\n");
    formatted.push('\n');
    formatted
}

fn push_wrapped_equation_side(output: &mut Vec<String>, indent: &str, side: &str) {
    let mut terms = side.split(" + ");
    let Some(first) = terms.next() else {
        return;
    };
    let mut line = first.to_owned();
    for term in terms {
        if indent.len() + line.len() + 3 + term.len() <= 100 {
            line.push_str(" + ");
            line.push_str(term);
        } else {
            output.push(format!("{indent}{line}"));
            line = format!("+ {term}");
        }
    }
    output.push(format!("{indent}{line}"));
}

#[cfg(test)]
mod tests {
    use super::format_source;

    const DOCUMENT: &str = "chems 1\nuse catalog ChemSpec.Aqueous@1\nexperiment Demo where\n  conditions\n    medium := aqueous\n    temperature := 25 degC\n    pressure := 1 atm\n  given\n    water := 10 mL of H2O(l)\n  vessels\n    beaker := open vessel 100 mL\n  procedure\n    place water in beaker\n  model\n    event := representative\n    sequence := explanatory\n    structuralRule := ChemSpec.Structural.Test.NoReaction\n  expect\n    class := noReaction\n  by\n    close\n";

    #[test]
    fn canonical_format_is_idempotent() {
        let once = format_source(DOCUMENT).expect("fixture formats");
        let twice = format_source(&once).expect("canonical source formats");
        assert_eq!(once, twice);
        assert!(once.find("temperature").unwrap() < once.find("pressure").unwrap());
        assert!(once.find("pressure").unwrap() < once.find("medium").unwrap());
    }
}
