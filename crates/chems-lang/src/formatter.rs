use std::{collections::BTreeMap, error::Error, fmt};

use crate::{CommentAttachment, CommentPlacement, Diagnostic, Token, TokenKind, parse_source};

const ARROW_MARKER: &str = "\0CHEMS_ARROW\0";

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

/// Formats a complete `chems 1` document into canonical structural source.
///
/// Invalid or recovery-bearing source is never rewritten.
///
/// # Errors
///
/// Returns [`FormatError`] when the input or formatter output is not a complete
/// structural `chems 1` document.
pub fn format_source(source: &str) -> Result<String, FormatError> {
    let parsed = parse_source(source);
    if !parsed.is_complete() {
        return Err(FormatError {
            diagnostics: parsed.diagnostics,
        });
    }
    let formatted = format_tokens(&parsed.cst.tokens, &parsed.ast.comments);
    let reparsed = parse_source(&formatted);
    if !reparsed.is_complete() {
        return Err(FormatError {
            diagnostics: reparsed.diagnostics,
        });
    }
    Ok(formatted)
}

fn format_tokens(tokens: &[Token], comments: &[CommentAttachment]) -> String {
    let mut output = String::new();
    let mut indentation = 0_usize;
    let mut line_start = true;
    let mut previous: Option<TokenKind> = None;
    let mut pending_newline = false;
    let comment_indentation = comment_indentation(tokens, comments);
    let mut block_depth = 0_i32;

    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Bom | TokenKind::Space | TokenKind::Eof | TokenKind::Invalid => {}
            TokenKind::Indent => indentation += 1,
            TokenKind::Dedent => indentation = indentation.saturating_sub(1),
            TokenKind::Newline => pending_newline = true,
            TokenKind::LineComment | TokenKind::BlockComment => {
                flush_newline(
                    &mut output,
                    &mut pending_newline,
                    &mut line_start,
                    &mut previous,
                );
                let comment_depth = comment_indentation
                    .get(&index)
                    .copied()
                    .unwrap_or(indentation);
                if block_depth == 0 {
                    indent_if_needed(&mut output, comment_depth, &mut line_start);
                } else {
                    line_start = false;
                }
                if previous.is_some() && !output.ends_with(' ') {
                    output.push(' ');
                }
                output.push_str(&token.text);
                if token.kind == TokenKind::BlockComment {
                    block_depth += block_depth_delta(&token.text);
                }
                previous = Some(token.kind);
            }
            _ => {
                flush_newline(
                    &mut output,
                    &mut pending_newline,
                    &mut line_start,
                    &mut previous,
                );
                indent_if_needed(&mut output, indentation, &mut line_start);
                let next = next_ordinary(tokens, index + 1);
                let follows_formula_count =
                    matches!(token.kind, TokenKind::Word | TokenKind::LeftParen)
                        && previous == Some(TokenKind::Number)
                        && previous_ordinary(tokens, index, 2).is_some_and(|kind| {
                            matches!(
                                kind,
                                TokenKind::Dot | TokenKind::MiddleDot | TokenKind::RightParen
                            )
                        });
                if !follows_formula_count
                    && previous.is_some_and(|kind| needs_space(kind, token.kind, next))
                    && !output.ends_with(' ')
                {
                    output.push(' ');
                }
                if token.kind == TokenKind::Arrow {
                    output.push_str(ARROW_MARKER);
                } else if token.kind == TokenKind::SubscriptNumber {
                    output.extend(token.text.chars().map(ascii_formula_character));
                } else if token.kind == TokenKind::MiddleDot {
                    output.push('.');
                } else {
                    output.push_str(&token.text);
                }
                previous = Some(token.kind);
            }
        }
    }
    trim_trailing_space(&mut output);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    wrap_equation_arrows(&output)
}

fn block_depth_delta(text: &str) -> i32 {
    text.as_bytes().windows(2).fold(0, |depth, pair| {
        if pair == b"/-" {
            depth + 1
        } else if pair == b"-/" {
            depth - 1
        } else {
            depth
        }
    })
}

fn comment_indentation(tokens: &[Token], comments: &[CommentAttachment]) -> BTreeMap<usize, usize> {
    comments
        .iter()
        .filter_map(|comment| {
            let indentation = if comment.placement == CommentPlacement::Leading {
                indentation_at(tokens, comment.node_span.start)
            } else {
                original_line_indentation(tokens, comment.token_index)
            };
            indentation.map(|indentation| (comment.token_index, indentation))
        })
        .collect()
}

fn original_line_indentation(tokens: &[Token], index: usize) -> Option<usize> {
    let line_start = tokens[..index]
        .iter()
        .rposition(|token| token.kind == TokenKind::Newline)
        .map_or(0, |position| position + 1);
    tokens[line_start..index]
        .iter()
        .find(|token| token.kind == TokenKind::Space)
        .map(|token| token.text.len() / 2)
}

fn indentation_at(tokens: &[Token], offset: usize) -> Option<usize> {
    let mut indentation = 0_usize;
    for token in tokens {
        match token.kind {
            TokenKind::Indent => indentation += 1,
            TokenKind::Dedent => indentation = indentation.saturating_sub(1),
            TokenKind::Space
            | TokenKind::Newline
            | TokenKind::LineComment
            | TokenKind::BlockComment
            | TokenKind::Bom => {}
            _ if token.span.start >= offset => return Some(indentation),
            _ => {}
        }
    }
    None
}

fn flush_newline(
    output: &mut String,
    pending: &mut bool,
    line_start: &mut bool,
    previous: &mut Option<TokenKind>,
) {
    if *pending {
        trim_trailing_space(output);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        *line_start = true;
        *previous = None;
        *pending = false;
    }
}

fn indent_if_needed(output: &mut String, indentation: usize, line_start: &mut bool) {
    if *line_start {
        output.push_str(&"  ".repeat(indentation));
        *line_start = false;
    }
}

fn next_ordinary(tokens: &[Token], from: usize) -> Option<TokenKind> {
    tokens[from..]
        .iter()
        .find(|token| {
            !token.kind.is_trivia()
                && !matches!(
                    token.kind,
                    TokenKind::Indent | TokenKind::Dedent | TokenKind::Newline | TokenKind::Eof
                )
        })
        .map(|token| token.kind)
}

fn previous_ordinary(tokens: &[Token], before: usize, count: usize) -> Option<TokenKind> {
    tokens[..before]
        .iter()
        .rev()
        .filter(|token| {
            !token.kind.is_trivia()
                && !matches!(
                    token.kind,
                    TokenKind::Indent | TokenKind::Dedent | TokenKind::Newline | TokenKind::Eof
                )
        })
        .nth(count.saturating_sub(1))
        .map(|token| token.kind)
}

fn needs_space(previous: TokenKind, current: TokenKind, next: Option<TokenKind>) -> bool {
    if current == TokenKind::SubscriptNumber
        || (previous == TokenKind::SubscriptNumber
            && matches!(current, TokenKind::Word | TokenKind::LeftParen))
        || (previous == TokenKind::RightParen && current == TokenKind::Word)
    {
        return false;
    }
    if matches!(
        current,
        TokenKind::Dot
            | TokenKind::MiddleDot
            | TokenKind::At
            | TokenKind::RightParen
            | TokenKind::LeftBracket
            | TokenKind::RightBracket
    ) || matches!(
        previous,
        TokenKind::Dot
            | TokenKind::MiddleDot
            | TokenKind::At
            | TokenKind::LeftParen
            | TokenKind::LeftBracket
    ) {
        return false;
    }
    if current == TokenKind::LeftParen {
        return !matches!(previous, TokenKind::Word | TokenKind::RightParen);
    }
    if current == TokenKind::Number && previous == TokenKind::RightParen {
        return false;
    }
    if current == TokenKind::Plus || previous == TokenKind::Plus {
        return true;
    }
    if current == TokenKind::Arrow || previous == TokenKind::Arrow {
        return true;
    }
    if current == TokenKind::Assignment || previous == TokenKind::Assignment {
        return true;
    }
    if previous == TokenKind::Number && current == TokenKind::Word {
        return true;
    }
    if previous == TokenKind::BlockComment || current == TokenKind::BlockComment {
        return true;
    }
    !matches!(next, Some(TokenKind::RightBracket | TokenKind::RightParen))
}

const fn ascii_formula_character(character: char) -> char {
    match character {
        '₀' => '0',
        '₁' => '1',
        '₂' => '2',
        '₃' => '3',
        '₄' => '4',
        '₅' => '5',
        '₆' => '6',
        '₇' => '7',
        '₈' => '8',
        '₉' => '9',
        other => other,
    }
}

fn trim_trailing_space(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
}

fn wrap_equation_arrows(source: &str) -> String {
    let mut output = String::new();
    for line in source.lines() {
        if let Some((left, right)) = line.split_once(ARROW_MARKER) {
            let inline = format!("{left}->{right}");
            if inline.len() > 100 && !left.trim().is_empty() {
                output.push_str(left.trim_end());
                output.push('\n');
                let indentation = line.len() - line.trim_start().len();
                output.push_str(&" ".repeat(indentation));
                output.push_str("-> ");
                output.push_str(right.trim_start());
                output.push('\n');
            } else {
                output.push_str(&inline);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::format_source;

    const DOCUMENT: &str = "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction Demo where\n  reactants\n    water:=1 of Water\n  products\n    waterProduct:=1 of Water\n  equation\n    H2O[molecular]->H2O[molecular]\n  model\n    event:=representative\n    sequence:=explanatory\n  observe from Evidence.Demo@1\n    product waterProduct forms claim R1\n  by\n    apply Rules.Identity\n      input:=water\n";

    #[test]
    fn canonical_format_is_idempotent() {
        let once = format_source(DOCUMENT).expect("fixture formats");
        let twice = format_source(&once).expect("canonical source formats");
        assert_eq!(once, twice);
        assert!(once.contains("water := 1 of Water"));
        assert!(once.contains("H2O[molecular] -> H2O[molecular]"));
    }
}
