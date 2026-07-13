use crate::{ByteSpan, Diagnostic, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct LexResult {
    pub source: String,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LexResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

#[must_use]
pub fn lex_bytes(bytes: &[u8]) -> LexResult {
    match std::str::from_utf8(bytes) {
        Ok(source) => lex_source(source),
        Err(error) => {
            let start = error.valid_up_to();
            let end = error
                .error_len()
                .map_or(bytes.len(), |length| start + length);
            LexResult {
                source: String::from_utf8_lossy(bytes).into_owned(),
                tokens: vec![synthetic(TokenKind::Eof, 0)],
                diagnostics: vec![Diagnostic::lexical(
                    "CHEMS-L001",
                    "source is not valid UTF-8",
                    ByteSpan::new(start, end),
                )],
            }
        }
    }
}

#[must_use]
pub fn lex_source(source: &str) -> LexResult {
    let mut diagnostics = Vec::new();
    let mut raw = Vec::new();
    let mut index = 0;
    if source.as_bytes().starts_with(&[0xef, 0xbb, 0xbf]) {
        diagnostics.push(Diagnostic::lexical(
            "CHEMS-L002",
            "UTF-8 byte-order marks are forbidden",
            ByteSpan::new(0, 3),
        ));
        index = 3;
    }
    diagnostics.extend(
        source
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\0')
            .map(|(position, _)| {
                Diagnostic::lexical(
                    "CHEMS-L003",
                    "NUL is forbidden in source",
                    ByteSpan::new(position, position + 1),
                )
            }),
    );

    while index < source.len() {
        let bytes = source.as_bytes();
        if bytes[index..].starts_with(b"--") {
            let start = index;
            index += 2;
            while index < source.len() && !matches!(bytes[index], b'\r' | b'\n') {
                index += source[index..].chars().next().map_or(1, char::len_utf8);
            }
            push_token(&mut raw, TokenKind::LineComment, source, start, index);
            continue;
        }
        if bytes[index..].starts_with(b"/-") {
            lex_block_comment(source, &mut index, &mut raw, &mut diagnostics);
            continue;
        }
        if bytes[index..].starts_with(b"-/") {
            push_token(&mut raw, TokenKind::Invalid, source, index, index + 2);
            diagnostics.push(Diagnostic::lexical(
                "CHEMS-L007",
                "unmatched block-comment closer",
                ByteSpan::new(index, index + 2),
            ));
            index += 2;
            continue;
        }
        if bytes[index..].starts_with(b":=") {
            push_token(&mut raw, TokenKind::Assignment, source, index, index + 2);
            index += 2;
            continue;
        }
        if bytes[index..].starts_with(b"->") {
            push_token(&mut raw, TokenKind::Arrow, source, index, index + 2);
            index += 2;
            continue;
        }

        lex_regular_token(source, &mut index, &mut raw, &mut diagnostics);
    }

    let tokens = apply_layout(source, &raw, &mut diagnostics);
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.primary_span.start,
            diagnostic.severity,
            diagnostic.stage,
            diagnostic.code.clone(),
        )
    });
    LexResult {
        source: source.to_owned(),
        tokens,
        diagnostics,
    }
}

fn lex_regular_token(
    source: &str,
    index: &mut usize,
    raw: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bytes = source.as_bytes();
    match bytes[*index] {
        b' ' => {
            let start = *index;
            while *index < source.len() && source.as_bytes()[*index] == b' ' {
                *index += 1;
            }
            push_token(raw, TokenKind::Space, source, start, *index);
        }
        b'\t' => {
            push_token(raw, TokenKind::Invalid, source, *index, *index + 1);
            diagnostics.push(Diagnostic::lexical(
                "CHEMS-L004",
                "tabs are forbidden outside comments",
                ByteSpan::new(*index, *index + 1),
            ));
            *index += 1;
        }
        b'\0' => {
            push_token(raw, TokenKind::Invalid, source, *index, *index + 1);
            *index += 1;
        }
        b'\r' => {
            if source.as_bytes().get(*index + 1) != Some(&b'\n') {
                diagnostics.push(Diagnostic::lexical(
                    "CHEMS-L010",
                    "bare carriage returns are forbidden; use LF or CRLF",
                    ByteSpan::new(*index, *index + 1),
                ));
            }
            lex_newline(source, index, raw);
        }
        b'\n' => lex_newline(source, index, raw),
        byte if byte.is_ascii_alphabetic() || byte == b'_' => {
            let start = *index;
            *index += 1;
            while *index < source.len()
                && (source.as_bytes()[*index].is_ascii_alphanumeric()
                    || source.as_bytes()[*index] == b'_')
            {
                *index += 1;
            }
            push_token(raw, TokenKind::Word, source, start, *index);
        }
        byte if byte.is_ascii_digit() => {
            let start = *index;
            while *index < source.len() && source.as_bytes()[*index].is_ascii_digit() {
                *index += 1;
            }
            push_token(raw, TokenKind::Number, source, start, *index);
        }
        byte if byte.is_ascii() => {
            let kind = match byte {
                b'@' => TokenKind::At,
                b'.' => TokenKind::Dot,
                b':' => TokenKind::Colon,
                b'^' => TokenKind::Caret,
                b'+' => TokenKind::Plus,
                b'-' => TokenKind::Minus,
                b'*' => TokenKind::Star,
                b'/' => TokenKind::Slash,
                b'%' => TokenKind::Percent,
                b'(' => TokenKind::LeftParen,
                b')' => TokenKind::RightParen,
                b'?' => TokenKind::Hole,
                _ => TokenKind::Invalid,
            };
            push_token(raw, kind, source, *index, *index + 1);
            if kind == TokenKind::Invalid {
                diagnostics.push(Diagnostic::lexical(
                    "CHEMS-L009",
                    format!("unexpected source character `{}`", char::from(byte)),
                    ByteSpan::new(*index, *index + 1),
                ));
            }
            *index += 1;
        }
        _ => {
            let Some(character) = source[*index..].chars().next() else {
                return;
            };
            let end = *index + character.len_utf8();
            push_token(raw, TokenKind::Invalid, source, *index, end);
            diagnostics.push(Diagnostic::lexical(
                "CHEMS-L008",
                "non-ASCII characters are allowed only in comments",
                ByteSpan::new(*index, end),
            ));
            *index = end;
        }
    }
}

fn lex_block_comment(
    source: &str,
    index: &mut usize,
    tokens: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let outer_start = *index;
    let mut segment_start = *index;
    let mut depth = 0_u32;
    while *index < source.len() {
        let bytes = source.as_bytes();
        if bytes[*index..].starts_with(b"/-") {
            depth += 1;
            *index += 2;
        } else if bytes[*index..].starts_with(b"-/") {
            depth -= 1;
            *index += 2;
            if depth == 0 {
                push_token(
                    tokens,
                    TokenKind::BlockComment,
                    source,
                    segment_start,
                    *index,
                );
                return;
            }
        } else if matches!(bytes[*index], b'\r' | b'\n') {
            if segment_start < *index {
                push_token(
                    tokens,
                    TokenKind::BlockComment,
                    source,
                    segment_start,
                    *index,
                );
            }
            lex_newline(source, index, tokens);
            segment_start = *index;
        } else {
            *index += source[*index..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if segment_start < *index {
        push_token(
            tokens,
            TokenKind::BlockComment,
            source,
            segment_start,
            *index,
        );
    }
    diagnostics.push(Diagnostic::lexical(
        "CHEMS-L006",
        "unclosed block comment",
        ByteSpan::new(outer_start, source.len()),
    ));
}

fn lex_newline(source: &str, index: &mut usize, tokens: &mut Vec<Token>) {
    let start = *index;
    if source.as_bytes()[*index] == b'\r'
        && source
            .as_bytes()
            .get(*index + 1)
            .is_some_and(|byte| *byte == b'\n')
    {
        *index += 2;
    } else {
        *index += 1;
    }
    push_token(tokens, TokenKind::Newline, source, start, *index);
}

fn apply_layout(source: &str, raw: &[Token], diagnostics: &mut Vec<Diagnostic>) -> Vec<Token> {
    let mut output = Vec::with_capacity(raw.len() + 8);
    let mut indentation = vec![0_usize];
    let mut line_start = 0;
    while line_start < raw.len() {
        let line_end = raw[line_start..]
            .iter()
            .position(|token| token.kind == TokenKind::Newline)
            .map_or(raw.len(), |offset| line_start + offset + 1);
        let line = &raw[line_start..line_end];
        let semantic = line
            .iter()
            .find(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline);
        if let Some(first) = semantic {
            let spaces = line
                .first()
                .filter(|token| token.kind == TokenKind::Space)
                .map_or(0, |token| token.text.len());
            let current = *indentation
                .last()
                .expect("indentation stack is never empty");
            if spaces == current + 2 {
                indentation.push(spaces);
                output.push(synthetic(TokenKind::Indent, first.span.start));
            } else if spaces > current {
                diagnostics.push(Diagnostic::lexical(
                    "CHEMS-L005",
                    "indentation must move exactly two spaces deeper",
                    ByteSpan::new(first.span.start.saturating_sub(spaces), first.span.start),
                ));
            } else if spaces < current {
                while indentation.last().is_some_and(|level| *level > spaces) {
                    indentation.pop();
                    output.push(synthetic(TokenKind::Dedent, first.span.start));
                }
                if indentation.last().is_none_or(|level| *level != spaces) {
                    diagnostics.push(Diagnostic::lexical(
                        "CHEMS-L005",
                        "dedent does not match an earlier indentation level",
                        ByteSpan::new(first.span.start.saturating_sub(spaces), first.span.start),
                    ));
                }
            }
        }
        output.extend(line.iter().cloned());
        line_start = line_end;
    }

    if output
        .last()
        .is_none_or(|token| token.kind != TokenKind::Newline)
    {
        output.push(synthetic(TokenKind::Newline, source.len()));
    }
    while indentation.len() > 1 {
        indentation.pop();
        output.push(synthetic(TokenKind::Dedent, source.len()));
    }
    output.push(synthetic(TokenKind::Eof, source.len()));
    output
}

fn push_token(tokens: &mut Vec<Token>, kind: TokenKind, source: &str, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        text: source[start..end].to_owned(),
        span: ByteSpan::new(start, end),
        synthetic: false,
    });
}

fn synthetic(kind: TokenKind, at: usize) -> Token {
    Token {
        kind,
        text: String::new(),
        span: ByteSpan::empty(at),
        synthetic: true,
    }
}
