use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity, Span};

const MAX_SOURCE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier,
    String,
    Number,
    Arrow,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![diagnostic(
            DiagnosticCode::SourceTooLong,
            Span::new(0, source.len()),
            "Matrix source exceeds 256 KiB",
        )]);
    }

    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let start = cursor;
        let byte = bytes[cursor];
        let kind = match byte {
            b if b.is_ascii_whitespace() => {
                cursor += 1;
                continue;
            }
            b if is_identifier_start(b) => {
                cursor += 1;
                while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                    cursor += 1;
                }
                TokenKind::Identifier
            }
            b'"' => {
                cursor = scan_string(bytes, start).map_err(|error| vec![error])?;
                TokenKind::String
            }
            b'-' if bytes.get(cursor + 1) == Some(&b'>') => {
                cursor += 2;
                TokenKind::Arrow
            }
            b'-' | b'0'..=b'9' => {
                cursor = scan_number(source, start).map_err(|error| vec![error])?;
                TokenKind::Number
            }
            b'{' => {
                cursor += 1;
                TokenKind::LBrace
            }
            b'}' => {
                cursor += 1;
                TokenKind::RBrace
            }
            b'[' => {
                cursor += 1;
                TokenKind::LBracket
            }
            b']' => {
                cursor += 1;
                TokenKind::RBracket
            }
            b',' => {
                cursor += 1;
                TokenKind::Comma
            }
            b':' => {
                cursor += 1;
                TokenKind::Colon
            }
            b'.' => {
                cursor += 1;
                TokenKind::Dot
            }
            b'#' | b'/' if byte == b'#' || matches!(bytes.get(cursor + 1), Some(b'/' | b'*')) => {
                let end = start + if byte == b'#' { 1 } else { 2 };
                return Err(vec![diagnostic(
                    DiagnosticCode::CommentNotAllowed,
                    Span::new(start, end),
                    "comments are not part of Matrix source",
                )]);
            }
            _ => {
                let end = start + source[start..].chars().next().unwrap().len_utf8();
                return Err(vec![diagnostic(
                    DiagnosticCode::InvalidCharacter,
                    Span::new(start, end),
                    "character is not part of Matrix source",
                )]);
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, cursor),
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(cursor, cursor),
    });
    Ok(tokens)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn scan_string(bytes: &[u8], start: usize) -> Result<usize, Diagnostic> {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return Ok(cursor + 1),
            b'\\' => {
                let escape_start = cursor;
                cursor += 1;
                let Some(&escaped) = bytes.get(cursor) else {
                    return Err(incomplete_string(start, bytes.len()));
                };
                match escaped {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => cursor += 1,
                    b'u' => {
                        let (unit, end) = unicode_escape(bytes, escape_start, start)?;
                        cursor = end;
                        if (0xD800..=0xDBFF).contains(&unit) {
                            if cursor == bytes.len() || bytes.get(cursor..) == Some(&b"\\"[..]) {
                                return Err(incomplete_string(start, bytes.len()));
                            }
                            if bytes.get(cursor..cursor + 2) != Some(&b"\\u"[..]) {
                                return Err(diagnostic(
                                    DiagnosticCode::InvalidEscape,
                                    Span::new(escape_start, cursor),
                                    "high surrogate must be followed by a low surrogate",
                                ));
                            }
                            let (low, end) = unicode_escape(bytes, cursor, start)?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return Err(diagnostic(
                                    DiagnosticCode::InvalidEscape,
                                    Span::new(cursor, end),
                                    "high surrogate must be followed by a low surrogate",
                                ));
                            }
                            cursor = end;
                        } else if (0xDC00..=0xDFFF).contains(&unit) {
                            return Err(diagnostic(
                                DiagnosticCode::InvalidEscape,
                                Span::new(escape_start, cursor),
                                "low surrogate requires a preceding high surrogate",
                            ));
                        }
                    }
                    _ => {
                        return Err(diagnostic(
                            DiagnosticCode::InvalidEscape,
                            Span::new(escape_start, cursor + 1),
                            "invalid JSON string escape",
                        ));
                    }
                }
            }
            0..=0x1F => {
                return Err(diagnostic(
                    DiagnosticCode::InvalidString,
                    Span::new(cursor, cursor + 1),
                    "raw control characters are not allowed in JSON strings",
                ));
            }
            _ => cursor += 1,
        }
    }
    Err(incomplete_string(start, bytes.len()))
}

fn unicode_escape(
    bytes: &[u8],
    escape_start: usize,
    string_start: usize,
) -> Result<(u16, usize), Diagnostic> {
    let digits_start = escape_start + 2;
    let end = digits_start + 4;
    if end > bytes.len() {
        if bytes[digits_start..].contains(&b'"') {
            return Err(diagnostic(
                DiagnosticCode::InvalidEscape,
                Span::new(escape_start, bytes.len()),
                "Unicode escape must contain four hexadecimal digits",
            ));
        }
        return Err(incomplete_string(string_start, bytes.len()));
    }
    let digits = &bytes[digits_start..end];
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err(diagnostic(
            DiagnosticCode::InvalidEscape,
            Span::new(escape_start, end),
            "Unicode escape must contain four hexadecimal digits",
        ));
    }
    let value = u16::from_str_radix(std::str::from_utf8(digits).unwrap(), 16).unwrap();
    Ok((value, end))
}

fn incomplete_string(start: usize, end: usize) -> Diagnostic {
    diagnostic(
        DiagnosticCode::IncompleteString,
        Span::new(start, end),
        "string is incomplete",
    )
}

fn scan_number(source: &str, start: usize) -> Result<usize, Diagnostic> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    if bytes[cursor] == b'-' {
        cursor += 1;
    }
    match bytes.get(cursor) {
        Some(b'0') => {
            cursor += 1;
            if bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                    cursor += 1;
                }
                return Err(invalid_number(start, cursor));
            }
        }
        Some(b'1'..=b'9') => {
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
        }
        _ => return Err(invalid_number(start, cursor)),
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        if !bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            return Err(invalid_number(start, cursor));
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        if !bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            return Err(invalid_number(start, cursor));
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
    }
    if bytes
        .get(cursor)
        .is_some_and(|byte| is_identifier_continue(*byte) || *byte == b'.')
    {
        while bytes
            .get(cursor)
            .is_some_and(|byte| is_identifier_continue(*byte) || *byte == b'.')
        {
            cursor += 1;
        }
        return Err(invalid_number(start, cursor));
    }
    if !source[start..cursor]
        .parse::<f64>()
        .is_ok_and(f64::is_finite)
    {
        return Err(invalid_number(start, cursor));
    }
    Ok(cursor)
}

fn invalid_number(start: usize, end: usize) -> Diagnostic {
    diagnostic(
        DiagnosticCode::InvalidNumber,
        Span::new(start, end),
        "invalid or nonfinite JSON number",
    )
}

fn diagnostic(code: DiagnosticCode, span: Span, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Error,
        span,
        declaration_path: Vec::new(),
        message: message.to_owned(),
        hint: None,
    }
}
