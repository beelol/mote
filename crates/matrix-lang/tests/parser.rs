use matrix_lang::{parse_fragment, parse_surface, DiagnosticCode, Span};
use std::{fs, path::Path};

fn blocks(markdown: &str) -> Vec<&str> {
    markdown
        .split("```\n")
        .enumerate()
        .filter_map(|(i, block)| (i % 2 == 1).then_some(block.trim()))
        .collect()
}

fn accept(source: &str) {
    if source.starts_with("screen ") || source.starts_with("screen{") {
        parse_surface(source).unwrap_or_else(|errors| panic!("{source}\n{errors:#?}"));
    } else {
        parse_fragment(source).unwrap_or_else(|errors| panic!("{source}\n{errors:#?}"));
    }
}

#[test]
fn accepts_documented_examples_and_recipes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../matrix");
    let examples = fs::read_to_string(root.join("examples.md")).unwrap();
    let mut count = 0;
    for source in blocks(&examples) {
        if source.starts_with('{') || source.starts_with("PullRequest {") {
            continue;
        }
        accept(source);
        count += 1;
    }
    assert_eq!(count, 13);
    let mut recipes = 0;
    for entry in fs::read_dir(root.join("recipes")).unwrap() {
        let path = entry.unwrap().path();
        let markdown = fs::read_to_string(&path).unwrap();
        for source in blocks(&markdown) {
            if source.starts_with("PullRequest {") {
                continue;
            }
            accept(source);
            recipes += 1;
        }
    }
    assert!(recipes > 60);
}

#[test]
fn accepts_all_nine_v0_fragments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../matrix/conformance/v0");
    let files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "matrix")
        })
        .collect();
    assert_eq!(files.len(), 9);
    for file in files {
        accept(&fs::read_to_string(file).unwrap());
    }
}

#[test]
fn declaration_words_are_case_sensitive() {
    parse_surface("screen {}").unwrap();
    let errors = parse_surface("Screen {}").unwrap_err();
    assert_eq!(errors[0].code.as_str(), "E_PARSE_TOKEN");
    assert_eq!(errors[0].span, Span::new(0, 6));
}

#[test]
fn leaves_manifest_and_scope_checks_to_the_resolver() {
    accept(r#"button "Open" -> Open{url: selected.url}"#);
    accept(r#"text .unknown field "Name" unknown.path button "Go" -> Unknown{x: .anything}"#);
    accept(r#"row { list from a select one { item {} } list from b select one { item {} } }"#);
}

#[test]
fn retains_decoded_operands_actions_and_source_spans() {
    use matrix_lang::{ast::*, DeclarationKind::*};
    let source = r#"row nowrap end around { text "\u00e9\n" fill button "Go" primary confirm disabled -> Act{x, target: .title} }"#;
    let declarations = parse_fragment(source).unwrap();
    let declaration = &declarations[0];
    assert_eq!(declaration.span, Span::new(0, source.len()));
    let Row {
        wrapping,
        alignment,
        distribution,
        children,
    } = &declaration.kind
    else {
        panic!()
    };
    assert_eq!(wrapping.as_ref().unwrap().value, Wrapping::NoWrap);
    assert_eq!(alignment.as_ref().unwrap().value, Alignment::End);
    assert_eq!(distribution.as_ref().unwrap().value, Distribution::Around);
    let Text(DisplayOperand::Literal(text)) = &children[0].kind else {
        panic!()
    };
    assert_eq!(text.value, "é\n");
    assert_eq!(&source[text.span.start..text.span.end], r#""\u00e9\n""#);
    assert!(children[0].fill.is_some());
    let Button {
        intent,
        confirm,
        disabled,
        action,
        ..
    } = &children[1].kind
    else {
        panic!()
    };
    assert_eq!(intent.as_ref().unwrap().value, ButtonIntent::Primary);
    assert!(confirm.is_some() && disabled.is_some());
    assert_eq!(action.name.value, "Act");
    assert_eq!(action.arguments.len(), 2);
    assert!(action.arguments[0].shorthand);
    assert_eq!(action.arguments[0].reference.path.segments, ["x"]);
    assert!(!action.arguments[1].shorthand);
    assert!(action.arguments[1].reference.relative);
    assert_eq!(action.arguments[1].reference.path.segments, ["title"]);
    assert_eq!(
        &source[action.span.start..action.span.end],
        "-> Act{x, target: .title}"
    );
    let field = parse_fragment(r#"field "N" n kind number min -1.25e2 max 0"#).unwrap();
    let Field { min, max, .. } = &field[0].kind else {
        panic!()
    };
    assert_eq!(min.as_ref().unwrap().value, -125.0);
    assert_eq!(min.as_ref().unwrap().source, "-1.25e2");
    assert_eq!(max.as_ref().unwrap().value, 0.0);
}

#[test]
fn accepts_the_full_catalog_and_modifier_order() {
    accept(
        r#"screen "" {
      header {} section {} card collapsible {} tabs { tab "A" {} }
      row nowrap center around {
        col end between fill { title "" text title caption caption code code badge badge tag tag }
        label "Label" fill md read fill link "" to url fill
        stat "Stat" number delta trend fill kv "Key" value fill
        icon "add" fill image asset alt "" fill avatar asset alt name fill
        timestamp date relative fill divider fill typing fill
        field "Field" bind kind number min -1 max 2 pattern "[0-9]" fill
        area "Area" area fill toggle "Toggle" toggle fill check "Check" check fill
        time "Time" time fill search "Search" search fill
        choice "Choice" choice options ["a", "b"] fill
        select "Select" select options values fill multiselect "Multi" multi options values fill
        segmented "Segment" segment options ["a"] fill
        slider "Slider" slider min 0 max 1 step 0.1 fill
        stepper "Step" stepper min 0 max 9 step 1 fill
        date "Date" date min earliest max latest fill
        button "Button" danger confirm disabled fill -> Act{arg: value, other}
        progress value percent label "Progress" fill status "" live fill
        steps stages ["a", "b"] current current fill
        banner "" tone error fill -> Retry
        banner "" tone info fill compose draft send fill -> Send{text: draft}
        form "Form" fill { row { button "Submit" primary -> Submit } }
      }
      list from rows key id select one { item {} empty "" error "" -> Retry }
      grid from rows select many { tile {} }
      table from rows { column "Col" .value kind money empty "Empty" error "Error" }
      chat from rows key id { message role me body .body { text "Extra" } }
      dialog "Dialog" {} sheet "Sheet" {} menu trigger "Menu" { button "Go" -> Go }
      menu trigger icon "more" { button "Go" -> Go }
      tooltip "Help" { text "" } footer {}
    }"#,
    );
    for kind in ["text", "email", "url", "number", "phone", "password"] {
        accept(&format!(r#"field "Field" field kind {kind}"#));
    }
    for kind in ["text", "number", "money", "date", "status"] {
        accept(&format!(
            r#"table from data {{ column "Field" .field kind {kind} }}"#
        ));
    }
    for role in ["me", "them", "system", "roles.current", ".role"] {
        accept(&format!(
            "chat from data {{ message role {role} body .body }}"
        ));
    }
}

#[test]
fn rejects_invalid_nesting() {
    for source in [
        "screen { screen {} }",
        "section { header {} }",
        "screen { header {} header {} }",
        "screen { footer {} footer {} }",
        "tabs {}",
        "tabs { text x }",
        "tab \"A\" {}",
        "list from x {}",
        "list from x { tile {} }",
        "list from x { item {} item {} }",
        "list from x { empty \"E\" item {} }",
        "list from x { item {} error \"E\" empty \"E\" }",
        "list from x { item {} empty \"E\" empty \"E\" }",
        "grid from x { item {} }",
        "table from x {}",
        "table from x { column \"C\" .c error \"E\" column \"D\" .d }",
        "chat from x {}",
        "chat from x { message role me body x message role me body x }",
        "item {}",
        "tile {}",
        "column \"C\" .c",
        "message role me body x",
        "empty \"E\"",
        "error \"E\"",
        "menu trigger \"M\" {}",
        "menu trigger \"M\" { text x }",
        "tooltip \"T\" {}",
        "tooltip \"T\" { text a text b }",
        "form {}",
        "form { button \"A\" primary -> A button \"B\" primary -> B }",
        "form { form { button \"A\" primary -> A } button \"B\" primary -> B }",
        "text x fill",
        "row { section { text x fill } }",
    ] {
        let errors = if source.starts_with("screen ") {
            parse_surface(source).unwrap_err()
        } else {
            parse_fragment(source).unwrap_err()
        };
        assert_eq!(
            errors[0].code,
            DiagnosticCode::InvalidNesting,
            "{source}: {errors:?}"
        );
    }
}

#[test]
fn rejects_unknown_duplicate_and_out_of_order_syntax() {
    for source in [
        "mystery {}",
        "row sideways {}",
        "row between center {}",
        "row center nowrap {}",
        "row wrap nowrap {}",
        "card collapsible collapsible {}",
        "status x live live",
        "field \"F\" f max 2 min 1",
        "field \"F\" f min 1 kind number",
        "field \"F\" f kind unknown",
        "button \"B\" primary danger -> A",
        "button \"B\" disabled confirm -> A",
        "button \"B\" confirm confirm -> A",
        "row { button \"B\" -> A fill }",
        "row { text x fill fill }",
        "col wrap {}",
        "text x relative",
        "text x secondary",
        "progress label \"P\" value v",
        "list from x select one key id { item {} }",
        "chat from x select one { message role me body x }",
        "image asset",
        "section",
        "text \"a\" {}",
        "search \"S\" q send -> A",
        "list from x { item {} -> A }",
        "button \"B\" -> A{}",
        "button \"B\" -> A{x,}",
        "button \"B\" -> A(x)",
        "button \"B\" -> A{x: \"literal\"}",
        "button \"B\" -> A{x: 1}",
        "table from x { column \"C\" root.path }",
        "field \"F\" .field",
        "list from x key .id { item {} }",
        "segmented \"S\" s options values",
        "list from x { item {} error \"E\" -> Other }",
        "list from x { item {} error \"E\" -> Retry{x} }",
        "text 1",
        "md \"literal\"",
        "text x;",
        "text count.of(x)",
    ] {
        assert!(parse_fragment(source).is_err(), "accepted {source}");
    }
    for source in [
        "",
        "text x",
        "screen {} screen {}",
        "screen {} text x",
        "screen {} }",
    ] {
        assert!(parse_surface(source).is_err(), "accepted surface {source}");
    }
}

#[test]
fn rejects_invalid_literal_constraints() {
    for source in [
        "select \"S\" s options []",
        "select \"S\" s options [\"\"]",
        "select \"S\" s options [\"a\", \"\\u0061\"]",
        "select \"S\" s options [\"a\",]",
        "steps stages [] current s",
        "steps stages [\"x\", \"x\"] current s",
        "field \"\" f",
        "tab \"\" {}",
        "icon \"\"",
        "icon \"unknown\"",
        "field \"F\" f min -1",
        "field \"F\" f min 0.5",
        "field \"F\" f min 4 max 2",
        "slider \"S\" s min 3 max 1",
        "stepper \"S\" s step 0",
        "stepper \"S\" s step -1",
        "field \"F\" f kind number min NaN",
        "field \"F\" f min 01",
        "field \"F\" f min 1e999",
    ] {
        assert!(parse_fragment(source).is_err(), "accepted {source}");
    }
}

#[test]
fn rejects_fractional_text_bounds_below_float_precision() {
    let error = &parse_fragment(r#"field "F" f min 1.0000000000000001"#).unwrap_err()[0];
    assert_eq!(error.code, DiagnosticCode::InvalidLiteral);
    assert_eq!(error.span, Span::new(16, 34));
}

#[test]
fn rejects_negative_text_bounds_below_float_range() {
    let error = &parse_fragment(r#"field "F" f min -1e-400"#).unwrap_err()[0];
    assert_eq!(error.code, DiagnosticCode::InvalidLiteral);
    assert_eq!(error.span, Span::new(16, 23));
}

#[test]
fn rejects_inverted_ranges_above_float_integer_precision() {
    let error = &parse_fragment(r#"slider "S" s min 9007199254740993 max 9007199254740992"#)
        .unwrap_err()[0];
    assert_eq!(error.code, DiagnosticCode::InvalidLiteral);
    assert_eq!(error.span, Span::new(38, 54));
}

#[test]
fn compares_decimal_constraints_without_rounding() {
    for source in [
        r#"field "F" f min 1e-400"#,
        r#"field "F" f max 0.99999999999999999"#,
        r#"slider "S" s min 1.0000000000000001 max 1"#,
        r#"slider "S" s min 0 max -1e-400"#,
        r#"slider "S" s min -1e-400 max -2e-400"#,
        r#"stepper "S" s step -1e-400"#,
        r#"stepper "S" s step -0.000e-999"#,
        r#"slider "S" s min 1e-9999999999999999999999999999 max 1e-10000000000000000000000000000"#,
        r#"slider "S" s min 0.00101e-9999999999999999999999999999 max 0.01e-10000000000000000000000000000"#,
    ] {
        assert_eq!(
            parse_fragment(source).unwrap_err()[0].code,
            DiagnosticCode::InvalidLiteral,
            "{source}"
        );
    }
    for source in [
        r#"field "F" f min -0.000e-999 max 1.0000000000000000"#,
        r#"field "F" f min 0.001e3 max 100e-2"#,
        r#"field "F" f min 9007199254740993 max 9007199254740994"#,
        r#"field "F" f min 1000e-0000000000000000000000000003 max 1"#,
        r#"field "F" f min 0e-9999999999999999999999999999 max 0e9999999999999999999999999999"#,
        r#"slider "S" s min -2e-400 max -1e-400 step 1e-400"#,
        r#"slider "S" s min 0.001e-9999999999999999999999999999 max 0.01e-10000000000000000000000000000"#,
        r#"slider "S" s min 1e-10000000000000000000000000000 max 1e-9999999999999999999999999999"#,
        r#"slider "S" s min 1.01 max 1.1"#,
    ] {
        accept(source);
    }
}

#[test]
fn accepts_empty_display_text_without_empty_required_labels() {
    accept(r#"progress label "" tooltip "" { text "" } section "" {}"#);
    for source in [
        r#"button "" -> A"#,
        r#"label """#,
        r#"dialog "" {}"#,
        r#"kv "" value"#,
    ] {
        assert_eq!(
            parse_fragment(source).unwrap_err()[0].code,
            DiagnosticCode::InvalidLiteral
        );
    }
}

#[test]
fn retains_all_fixed_icons_and_identifier_spellings() {
    for name in [
        "add", "back", "bell", "calendar", "check", "clock", "close", "copy", "delete", "down",
        "edit", "error", "file", "folder", "image", "info", "left", "mail", "more", "open",
        "right", "search", "settings", "up", "user", "warning",
    ] {
        accept(&format!("icon \"{name}\""));
    }
    accept("text NaN text Infinity button \"Go\" -> Infinity{NaN}");
    accept("chat from rows { message role me.current body .body }");
}

#[test]
fn truncated_input_never_panics() {
    let source = r#"screen "é" { form { select "A" a options ["x", "y"] row { button "Go" primary fill -> A{x: record.path} } } }"#;
    for (end, _) in source.char_indices() {
        assert!(
            parse_surface(&source[..end]).is_err(),
            "accepted incomplete prefix at {end}"
        );
    }
    assert!(parse_surface(source).is_ok());
}

#[test]
fn unexpected_literals_do_not_enter_diagnostic_declaration_paths() {
    let source = r#"screen { "{\"password\":\"planted-private-password\"}" }"#;
    let error = &parse_surface(source).unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_PARSE_TOKEN");
    assert_eq!(error.span, Span::new(9, 54));
    assert_eq!(error.declaration_path, ["screen[0]"]);
    assert_eq!(error.message, "expected an identifier");
    assert!(error.hint.is_none());

    for literal in [r#""private note""#, "12345678901234567890"] {
        let source = format!("screen {{ row {{ {literal} }} }}");
        let error = &parse_surface(&source).unwrap_err()[0];
        assert_eq!(error.code.as_str(), "E_PARSE_TOKEN");
        assert_eq!(error.declaration_path, ["screen[0]", "row[0]"]);
        assert!(!format!("{error:?}").contains(literal));
    }

    let error = &parse_surface(r#"screen { row { text "ok" text } }"#).unwrap_err()[0];
    assert_eq!(error.declaration_path, ["screen[0]", "row[0]", "text[1]"]);
}

#[test]
fn reports_stable_codes_byte_spans_and_declaration_paths() {
    let source = "screen { row { mystery } }";
    let error = &parse_surface(source).unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_PARSE_DECLARATION");
    assert_eq!(error.span, Span::new(15, 22));
    assert_eq!(
        error.declaration_path,
        ["screen[0]", "row[0]", "mystery[0]"]
    );
    let source = "screen { text \"é\"";
    let error = &parse_surface(source).unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_PARSE_INCOMPLETE");
    assert_eq!(error.span, Span::new(source.len(), source.len()));
    let error = &parse_fragment("button \"B\" -> A{x, x}").unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_PARSE_DUPLICATE_ARGUMENT");
    assert_eq!(error.span, Span::new(19, 20));
    for (source, span) in [
        ("text x # comment", Span::new(7, 8)),
        ("text x // comment", Span::new(7, 9)),
    ] {
        let error = &parse_fragment(source).unwrap_err()[0];
        assert_eq!(error.code, DiagnosticCode::CommentNotAllowed);
        assert_eq!(error.span, span);
    }
}

#[test]
fn forbidden_source_corpus_rejects_at_the_declared_boundary() {
    use serde_json::{json, Value};
    let cases: Vec<Value> = serde_json::from_str(include_str!("../../../matrix/conformance/diagnostics.json")).unwrap();
    let mut checked = 0;
    for case in cases.iter().filter(|case| case.get("category").is_some()) {
        checked += 1;
        let source = case["source"].as_str().unwrap();
        let errors = match case["boundary"].as_str().unwrap() {
            "lexer" => matrix_lang::lex(source).unwrap_err(),
            "parser" => {
                matrix_lang::lex(source).unwrap();
                parse_surface(source).unwrap_err()
            }
            "resolver" | "lowerer" => {
                parse_surface(source).unwrap();
                continue;
            }
            boundary => panic!("unknown corpus boundary {boundary}"),
        };
        let actual: Vec<_> = errors.iter().map(|error| json!({
            "code":error.code.as_str(), "severity":"error",
            "span":{"start":error.span.start,"end":error.span.end},
            "declaration_path":error.declaration_path, "message":error.message, "hint":error.hint,
        })).collect();
        assert_eq!(json!(actual), case["diagnostics"], "{}: {source}", case["category"]);
    }
    assert_eq!(checked, 17);
}

#[test]
fn enforces_depth_and_node_limits_at_the_boundary() {
    let depth12 = format!("screen {{{}text x{}}}", "col {".repeat(10), "}".repeat(10));
    assert!(parse_surface(&depth12).is_ok());
    let depth13 = format!("screen {{{}text x{}}}", "col {".repeat(11), "}".repeat(11));
    assert_eq!(
        parse_surface(&depth13).unwrap_err()[0].code.as_str(),
        "E_PARSE_DEPTH"
    );
    assert!(parse_surface(&format!("screen {{{}}}", "text x ".repeat(1999))).is_ok());
    assert_eq!(
        parse_surface(&format!("screen {{{}}}", "text x ".repeat(2000))).unwrap_err()[0]
            .code
            .as_str(),
        "E_PARSE_NODES"
    );
}
