use matrix_lang::{lex, DiagnosticCode, Severity, Span, TokenKind};

fn shapes(source: &str) -> Vec<(TokenKind, Span)> {
    lex(source)
        .unwrap()
        .into_iter()
        .map(|token| (token.kind, token.span))
        .collect()
}

#[test]
fn lexer_keeps_identifier_and_relative_path_spans() {
    assert_eq!(
        shapes("screen _a9 foo.bar .item"),
        vec![
            (TokenKind::Identifier, Span::new(0, 6)),
            (TokenKind::Identifier, Span::new(7, 10)),
            (TokenKind::Identifier, Span::new(11, 14)),
            (TokenKind::Dot, Span::new(14, 15)),
            (TokenKind::Identifier, Span::new(15, 18)),
            (TokenKind::Dot, Span::new(19, 20)),
            (TokenKind::Identifier, Span::new(20, 24)),
            (TokenKind::Eof, Span::new(24, 24)),
        ]
    );
}

#[test]
fn lexer_keeps_arrow_and_delimiter_spans() {
    assert_eq!(
        shapes("->{}[],:."),
        vec![
            (TokenKind::Arrow, Span::new(0, 2)),
            (TokenKind::LBrace, Span::new(2, 3)),
            (TokenKind::RBrace, Span::new(3, 4)),
            (TokenKind::LBracket, Span::new(4, 5)),
            (TokenKind::RBracket, Span::new(5, 6)),
            (TokenKind::Comma, Span::new(6, 7)),
            (TokenKind::Colon, Span::new(7, 8)),
            (TokenKind::Dot, Span::new(8, 9)),
            (TokenKind::Eof, Span::new(9, 9)),
        ]
    );
}

#[test]
fn lexer_accepts_json_strings_with_byte_spans() {
    assert_eq!(
        shapes("\"é\" \"hi\\n\" \"\\uD83D\\uDE00\""),
        vec![
            (TokenKind::String, Span::new(0, 4)),
            (TokenKind::String, Span::new(5, 11)),
            (TokenKind::String, Span::new(12, 26)),
            (TokenKind::Eof, Span::new(26, 26)),
        ]
    );
}

#[test]
fn lexer_accepts_json_numbers_with_exact_spans() {
    assert_eq!(
        shapes("-12.3e+4 0 1E-2"),
        vec![
            (TokenKind::Number, Span::new(0, 8)),
            (TokenKind::Number, Span::new(9, 10)),
            (TokenKind::Number, Span::new(11, 15)),
            (TokenKind::Eof, Span::new(15, 15)),
        ]
    );
}

#[test]
fn lexer_marks_open_strings_as_incomplete() {
    for source in ["\"open", "\"open\\", "\"\\u12", "\"\\uD800"] {
        let diagnostics = lex(source).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::IncompleteString);
        assert_eq!(diagnostics[0].span, Span::new(0, source.len()));
    }
}

#[test]
fn lexer_rejects_invalid_json_string_content() {
    for (source, code) in [
        ("\"\\q\"", DiagnosticCode::InvalidEscape),
        ("\"\\u12\"", DiagnosticCode::InvalidEscape),
        ("\"\\uD800\"", DiagnosticCode::InvalidEscape),
        ("\"\\uD800\\u0041\"", DiagnosticCode::InvalidEscape),
        ("\"a\nb\"", DiagnosticCode::InvalidString),
        ("\"a\rb\"", DiagnosticCode::InvalidString),
    ] {
        let diagnostics = lex(source).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source:?}");
        assert_eq!(diagnostics[0].code, code, "{source:?}");
    }
}

#[test]
fn lexer_rejects_malformed_json_numbers() {
    for source in ["01", "-01", "1.", "1e", "1e+", "-", "-Infinity", "1e9999"] {
        let diagnostics = lex(source).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source:?}");
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::InvalidNumber,
            "{source:?}"
        );
    }
}

#[test]
fn lexer_rejects_nonfinite_spellings() {
    for source in ["NaN", "Infinity"] {
        let diagnostics = lex(source).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source:?}");
        assert_eq!(diagnostics[0].code, DiagnosticCode::InvalidNumber);
    }
}

#[test]
fn lexer_rejects_comments_and_other_punctuation() {
    for source in ["# note", "// note", "/* note */"] {
        let diagnostics = lex(source).unwrap_err();
        assert_eq!(diagnostics[0].code, DiagnosticCode::CommentNotAllowed);
    }
    let diagnostics = lex("text;").unwrap_err();
    assert_eq!(diagnostics[0].code, DiagnosticCode::InvalidCharacter);
    assert_eq!(diagnostics[0].span, Span::new(4, 5));
}

#[test]
fn lexer_rejects_source_larger_than_256_kib() {
    let source = " ".repeat(256 * 1024 + 1);
    let diagnostics = lex(&source).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::SourceTooLong);
    assert_eq!(diagnostics[0].span, Span::new(0, source.len()));

    let at_limit = " ".repeat(256 * 1024);
    assert_eq!(
        shapes(&at_limit),
        vec![(TokenKind::Eof, Span::new(at_limit.len(), at_limit.len()))]
    );
}

#[test]
fn lexer_returns_structured_stable_diagnostics() {
    let diagnostics = lex("text;").unwrap_err();
    assert_eq!(diagnostics[0].code.as_str(), "E_LEX_CHARACTER");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].span, Span::new(4, 5));
    assert!(diagnostics[0].declaration_path.is_empty());
    assert!(!diagnostics[0].message.is_empty());
}
