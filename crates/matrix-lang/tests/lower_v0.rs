use matrix_lang::manifest::{ActionContract, EnumDefinition, FieldDefinition, RecordDefinition};
use matrix_lang::types::DisplayFormat;
use matrix_lang::{
    format_surface, lower_v0, parse_surface, resolve, BindingSnapshot, Manifest, Type,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn fixture() -> (Manifest, BindingSnapshot) {
    let mut manifest = Manifest::default();
    manifest.enums.insert(
        "Model".into(),
        EnumDefinition::new(["qwen3:4b", "qwen3:0.6b"]),
    );
    manifest
        .enums
        .insert("Role".into(), EnumDefinition::new(["me", "them", "system"]));
    manifest.records.insert(
        "Item".into(),
        RecordDefinition {
            fields: BTreeMap::from([("value".into(), Type::String.into())]),
        },
    );
    manifest.records.insert(
        "ModelRow".into(),
        RecordDefinition {
            fields: BTreeMap::from([
                ("name".into(), Type::String.into()),
                ("value".into(), Type::String.into()),
                (
                    "items".into(),
                    Type::List(Box::new(Type::Record("Item".into()))).into(),
                ),
            ]),
        },
    );
    let mut snapshot = BindingSnapshot::default();
    for (name, ty, value) in [
        (
            "fixture.items",
            Type::List(Box::new(Type::Record("Item".into()))),
            json!([{"value":"first"},{"value":"second"},{"value":"third"}]),
        ),
        (
            "fixture.models",
            Type::List(Box::new(Type::Record("ModelRow".into()))),
            json!([
                {"name":"talk","value":"qwen3:4b","items":[{"value":"nested"}]},
                {"name":"router","value":"qwen3:0.6b","items":[]}
            ]),
        ),
        ("text_value", Type::String, json!("Host text")),
        ("number", Type::Number, json!(0.5)),
        ("asset", Type::Asset, json!("asset:1")),
        ("url", Type::Url, json!("https://example.com")),
        ("instant", Type::DateTime, json!("2026-10-10T12:00:00Z")),
        (
            "model_options",
            Type::List(Box::new(Type::Enum("Model".into()))),
            json!(["qwen3:4b", "qwen3:0.6b"]),
        ),
    ] {
        manifest.values.insert(name.into(), ty.into());
        snapshot.values.insert(name.into(), value);
    }
    for (name, ty, initial) in [
        ("display_name", Type::String, json!("owner")),
        ("local_only", Type::Boolean, json!(true)),
        ("models.talk", Type::Enum("Model".into()), json!("qwen3:4b")),
        (
            "models.many",
            Type::List(Box::new(Type::Enum("Model".into()))),
            json!(["qwen3:4b"]),
        ),
        ("amount", Type::Number, json!(5)),
        ("day", Type::Date, json!("2026-10-10")),
        ("time_value", Type::Time, json!("12:30:00")),
    ] {
        manifest.fields.insert(
            name.into(),
            FieldDefinition {
                binding: ty.into(),
                initial,
            },
        );
    }
    manifest
        .actions
        .insert("approve".into(), ActionContract::default());
    manifest.actions.insert(
        "Save".into(),
        ActionContract {
            parameters: BTreeMap::from([("text_value".into(), Type::String)]),
        },
    );
    (manifest, snapshot)
}

fn surface(source: &str) -> String {
    if source.starts_with("screen ") {
        source.into()
    } else {
        format!("screen {{ {source} }}")
    }
}

fn lower(source: &str, manifest: &Manifest, snapshot: &BindingSnapshot) -> Value {
    let parsed = parse_surface(&surface(source)).unwrap();
    let resolved = resolve(&parsed, manifest, snapshot).unwrap();
    let node = lower_v0(&resolved).unwrap();
    let formatted = format_surface(&parsed);
    let reparsed = parse_surface(&formatted).unwrap();
    assert_eq!(
        lower_v0(&resolve(&reparsed, manifest, snapshot).unwrap()).unwrap(),
        node
    );
    let compact = serde_json::to_vec(&node).unwrap();
    assert_eq!(mote_ui::validate(&compact).unwrap(), node);
    serde_json::from_slice(&compact).unwrap()
}

#[test]
fn nine_fragments_match_canonical_snapshots_and_unchanged_consumer_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (manifest, snapshot) = fixture();
    let mut count = 0;
    for entry in fs::read_dir(root.join("matrix/conformance/v0")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_none_or(|extension| extension != "matrix")
        {
            continue;
        }
        let source = surface(&fs::read_to_string(&path).unwrap());
        let parsed = parse_surface(&source).unwrap();
        let before = lower_v0(&resolve(&parsed, &manifest, &snapshot).unwrap()).unwrap();
        let formatted = format_surface(&parsed);
        let after =
            lower_v0(&resolve(&parse_surface(&formatted).unwrap(), &manifest, &snapshot).unwrap())
                .unwrap();
        assert_eq!(before, after, "formatting changed {}", path.display());
        let expected = fs::read_to_string(path.with_extension("json")).unwrap();
        assert_eq!(serde_json::to_string(&before).unwrap(), expected.trim_end());
        let unchanged = fs::read(
            root.join("crates/mote-ui/fixtures")
                .join(path.with_extension("json").file_name().unwrap()),
        )
        .unwrap();
        assert_eq!(before, mote_ui::validate(&unchanged).unwrap());
        assert_eq!(mote_ui::validate(expected.as_bytes()).unwrap(), before);
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn wrapper_elision_preserves_explicit_columns_and_reading_order() {
    let (m, s) = fixture();
    for (source, expected) in [
        ("screen {}", json!({"type":"stack","children":[]})),
        ("text \"one\"", json!({"type":"text","value":"one"})),
        (
            "text \"one\" text \"two\"",
            json!({"type":"stack","children":[{"type":"text","value":"one"},{"type":"text","value":"two"}]}),
        ),
        (
            "col start { text \"one\" }",
            json!({"type":"stack","children":[{"type":"text","value":"one"}]}),
        ),
        (
            "row wrap start { text \"one\" }",
            json!({"type":"row","children":[{"type":"text","value":"one"}]}),
        ),
    ] {
        assert_eq!(lower(source, &m, &s), expected, "{source}");
    }
}

#[test]
fn literal_markup_is_lowered_without_interpretation_or_hidden_nodes() {
    let (m, s) = fixture();
    for literal in [
        "<b>hi</b>",
        "<script>alert('hi')</script>",
        "# **bold** _italic_ `code` [link](https://example.com) ![image](asset)",
    ] {
        let source = format!("text {}", serde_json::to_string(literal).unwrap());
        assert_eq!(lower(&source, &m, &s), json!({"type":"text","value":literal}));
    }
}

#[test]
fn collection_rows_expand_with_nearest_row_values_and_item_wrappers() {
    let (m, s) = fixture();
    assert_eq!(lower("list from fixture.models { item { text .name list from .items { item { text .value } } } }", &m, &s), json!({
        "type":"list","children":[
            {"type":"stack","children":[{"type":"text","value":"talk"},{"type":"list","children":[{"type":"text","value":"nested"}]}]},
            {"type":"stack","children":[{"type":"text","value":"router"},{"type":"list","children":[]}]}
        ]
    }));
    assert_eq!(
        lower(
            "list from fixture.items { item { col { text .value } } }",
            &m,
            &s
        ),
        json!({"type":"list","children":[
            {"type":"stack","children":[{"type":"text","value":"first"}]},
            {"type":"stack","children":[{"type":"text","value":"second"}]},
            {"type":"stack","children":[{"type":"text","value":"third"}]}
        ]})
    );
}

#[test]
fn empty_snapshots_keep_table_columns_without_emitting_template_values() {
    let (m, mut s) = fixture();
    s.values.insert("fixture.models".into(), json!([]));
    assert_eq!(
        lower(
            "table from fixture.models { column \"Name\" .name column \"Model\" .value }",
            &m,
            &s
        ),
        json!({"type":"table","columns":["Name","Model"],"rows":[]})
    );
    assert_eq!(
        lower("list from fixture.models { item { text .name } }", &m, &s),
        json!({"type":"list","children":[]})
    );
}

#[test]
fn host_display_formats_and_current_field_values_reach_the_consumer() {
    let (mut m, mut s) = fixture();
    m.values.get_mut("number").unwrap().display = DisplayFormat::Fixed { decimals: 2 };
    assert_eq!(
        lower("text number", &m, &s),
        json!({"type":"text","value":"0.50"})
    );
    s.fields.insert("display_name".into(), json!("edited"));
    s.fields.insert("local_only".into(), json!(false));
    s.fields.insert("models.talk".into(), json!("qwen3:0.6b"));
    assert_eq!(lower("field \"Name\" display_name kind text toggle \"Local\" local_only select \"Model\" models.talk options model_options", &m, &s), json!({"type":"stack","children":[
        {"type":"field","label":"Name","name":"display_name","value":"edited"},
        {"type":"toggle","label":"Local","name":"local_only","value":false},
        {"type":"select","label":"Model","name":"models.talk","options":["qwen3:4b","qwen3:0.6b"],"value":"qwen3:0.6b"}
    ]}));
}

#[test]
fn determinate_unlabeled_progress_uses_the_existing_progress_node() {
    let (m, s) = fixture();
    assert_eq!(
        lower("progress value number", &m, &s),
        json!({"type":"progress","value":0.5})
    );
}

#[test]
fn progress_rejects_precision_loss_and_underflow_with_source_location() {
    for number in [0.123456789, 1e-46, 0.999999999] {
        let (m, mut s) = fixture();
        s.values.insert("number".into(), json!(number));
        let parsed = parse_surface("screen { progress value number }").unwrap();
        let resolved = resolve(&parsed, &m, &s).unwrap();
        let errors = lower_v0(&resolved).unwrap_err();
        assert_eq!(errors[0].code.as_str(), "E_LOWER_UNSUPPORTED", "{number}");
        assert_eq!(errors[0].span, parsed.screen.children()[0].span);
        assert_eq!(errors[0].declaration_path, ["screen", "0"]);
    }
}

#[test]
fn lossless_progress_preserves_the_serialized_consumer_value() {
    for number in [0.0, 0.1, 0.125, 0.5, 0.75, 1.0] {
        let (m, mut s) = fixture();
        s.values.insert("number".into(), json!(number));
        assert_eq!(
            lower("progress value number", &m, &s),
            json!({"type":"progress","value":number})
        );
    }
}

#[test]
fn every_unsupported_catalog_element_and_modifier_rejects_with_source_location() {
    let (m, s) = fixture();
    for fragment in [
        "screen \"Title\" { text \"x\" }",
        "section { text \"x\" }",
        "card { text \"x\" }",
        "tabs { tab \"One\" { text \"x\" } }",
        "header { text \"x\" }",
        "footer { text \"x\" }",
        "title \"x\"",
        "caption \"x\"",
        "code \"x\"",
        "badge \"x\"",
        "tag \"x\"",
        "status \"x\"",
        "label \"x\"",
        "md text_value",
        "link \"x\" to url",
        "stat \"N\" number",
        "kv \"N\" number",
        "icon \"add\"",
        "image asset alt \"x\"",
        "avatar asset alt \"x\"",
        "timestamp instant",
        "divider",
        "typing",
        "area \"A\" display_name",
        "check \"C\" local_only",
        "time \"T\" time_value",
        "search \"S\" display_name",
        "choice \"C\" models.talk options model_options",
        "multiselect \"M\" models.many options model_options",
        "segmented \"S\" models.talk options [\"qwen3:4b\", \"qwen3:0.6b\"]",
        "slider \"S\" amount min 0 max 10",
        "stepper \"S\" amount",
        "date \"D\" day",
        "form { button \"Submit\" primary -> approve }",
        "grid from fixture.items { tile { text .value } }",
        "progress",
        "progress value number label \"P\"",
        "steps stages [\"Host text\"] current text_value",
        "banner \"B\" tone info",
        "chat from fixture.items { message role them body .value }",
        "compose display_name send -> Save{text_value: display_name}",
        "dialog \"D\" {}",
        "sheet \"S\" {}",
        "menu trigger \"M\" { button \"Go\" -> approve }",
        "tooltip \"T\" { text \"x\" }",
        "row nowrap {}",
        "row center {}",
        "row between {}",
        "col end {}",
        "col around {}",
        "row { text \"x\" fill }",
        "field \"F\" display_name kind email",
        "field \"F\" display_name min 1",
        "field \"F\" display_name max 9",
        "field \"F\" display_name pattern \"a\"",
        "button \"B\" primary -> approve",
        "button \"B\" danger -> approve",
        "button \"B\" confirm -> approve",
        "button \"B\" disabled -> approve",
        "button \"B\" -> Save{text_value}",
        "button \"B\" -> Back",
        "button \"B\" -> Retry",
        "button \"B\" -> Cancel",
        "list from fixture.items key value { item { text .value } }",
        "list from fixture.items select one { item { text .value } }",
        "list from fixture.items select many { item { text .value } }",
        "list from fixture.items { item { text .value } empty \"None\" }",
        "list from fixture.items { item { text .value } error \"Failed\" }",
        "table from fixture.models select one { column \"N\" .name }",
        "table from fixture.models key name { column \"N\" .name }",
        "table from fixture.models { column \"N\" .name empty \"None\" }",
        "list from fixture.items { item { field \"F\" display_name } }",
    ] {
        let source = surface(fragment);
        let resolved = resolve(&parse_surface(&source).expect(fragment), &m, &s).expect(fragment);
        let errors = lower_v0(&resolved).expect_err(fragment);
        for error in errors {
            assert_eq!(
                error.code.as_str(),
                "E_LOWER_UNSUPPORTED",
                "{fragment}: {error:?}"
            );
            assert_eq!(error.severity, matrix_lang::Severity::Error);
            assert_eq!(error.declaration_path.first().unwrap(), "screen");
            assert!(
                error.span.start < error.span.end && error.span.end <= source.len(),
                "{fragment}: {error:?}"
            );
            assert!(!error.message.is_empty());
            assert!(error.hint.is_some());
        }
    }
}

#[test]
fn empty_collections_do_not_hide_unsupported_template_semantics() {
    let (m, mut s) = fixture();
    s.values.insert("fixture.items".into(), json!([]));
    for fragment in [
        "list from fixture.items { item { caption .value } }",
        "list from fixture.items { item { field \"Name\" display_name } }",
        "list from fixture.items { item { row { text .value fill } } }",
    ] {
        let source = surface(fragment);
        let resolved = resolve(&parse_surface(&source).unwrap(), &m, &s).unwrap();
        let errors = lower_v0(&resolved).unwrap_err();
        assert_eq!(errors[0].code.as_str(), "E_LOWER_UNSUPPORTED");
        assert!(errors[0].declaration_path.len() > 2);
    }
}

#[test]
fn enum_option_labels_are_not_silently_lost() {
    let (mut m, s) = fixture();
    m.enums
        .get_mut("Model")
        .unwrap()
        .labels
        .insert("qwen3:0.6b".into(), "Small model".into());
    let source = "screen { select \"Model\" models.talk options model_options }";
    let resolved = resolve(&parse_surface(source).unwrap(), &m, &s).unwrap();
    assert_eq!(
        lower_v0(&resolved).unwrap_err()[0].code.as_str(),
        "E_LOWER_UNSUPPORTED"
    );
}

#[test]
fn compact_json_byte_limit_counts_keys_and_escaped_strings() {
    let (m, mut s) = fixture();
    let overhead = r#"{"type":"field","label":"Name","name":"display_name","value":""}"#.len();
    assert_eq!(overhead, 64);
    let source = parse_surface("screen { field \"Name\" display_name }").unwrap();
    for escaped in [false, true] {
        let available = 262_080;
        let initial = if escaped {
            format!(
                "{}{}",
                "\"".repeat(available / 2),
                "x".repeat(available % 2)
            )
        } else {
            "x".repeat(available)
        };
        s.fields.insert("display_name".into(), json!(initial));
        let resolved = resolve(&source, &m, &s).unwrap();
        let node = lower_v0(&resolved).unwrap();
        let bytes = serde_json::to_vec(&node).unwrap();
        assert_eq!(bytes.len(), 262_144);
        assert_eq!(mote_ui::validate(&bytes).unwrap(), node);
        s.fields
            .insert("display_name".into(), json!(format!("{initial}x")));
        let resolved = resolve(&source, &m, &s).unwrap();
        assert_eq!(overhead + serde_json::to_string(&format!("{initial}x")).unwrap().len() - 2, 262_145);
        let error = &lower_v0(&resolved).unwrap_err()[0];
        assert_eq!(error.code.as_str(), "E_LOWER_LIMIT");
        assert_eq!(error.declaration_path, ["screen", "0"]);
    }
}

#[test]
fn table_scalars_use_host_formatters_without_source_references() {
    let (mut m, mut s) = fixture();
    m.enums
        .get_mut("Model")
        .unwrap()
        .labels
        .insert("qwen3:4b".into(), "Talk model".into());
    m.records.insert(
        "Metrics".into(),
        RecordDefinition {
            fields: BTreeMap::from([
                ("count".into(), Type::Integer.into()),
                (
                    "price".into(),
                    matrix_lang::manifest::BindingDefinition {
                        ty: Type::Number,
                        display: DisplayFormat::Money {
                            symbol: "$".into(),
                            decimals: 2,
                        },
                    },
                ),
                ("model".into(), Type::Enum("Model".into()).into()),
                ("day".into(), Type::Date.into()),
            ]),
        },
    );
    m.values.insert(
        "metrics".into(),
        Type::List(Box::new(Type::Record("Metrics".into()))).into(),
    );
    s.values.insert(
        "metrics".into(),
        json!([{"count":7,"price":3.5,"model":"qwen3:4b","day":"2026-10-10"}]),
    );
    assert_eq!(lower("table from metrics { column \"Count\" .count column \"Price\" .price column \"Model\" .model column \"Day\" .day }", &m, &s), json!({
        "type":"table","columns":["Count","Price","Model","Day"],"rows":[["7","$3.50","Talk model","2026-10-10"]]
    }));
}

#[test]
fn ten_documented_design_cases_resolve_with_only_case_six_lowering() {
    let (mut m, mut s) = fixture();
    for (name, fields) in [
        (
            "Mail",
            vec![("subject", Type::String), ("from", Type::String)],
        ),
        (
            "Message",
            vec![("role", Type::Enum("Role".into())), ("body", Type::String)],
        ),
        ("Card", vec![("id", Type::Integer), ("title", Type::String)]),
    ] {
        m.records.insert(
            name.into(),
            RecordDefinition {
                fields: fields
                    .into_iter()
                    .map(|(name, ty)| (name.into(), ty.into()))
                    .collect(),
            },
        );
    }
    for (name, ty, value) in [
        (
            "mail.unread",
            "Mail",
            json!([{"subject":"first","from":"Ana"},{"subject":"second","from":"Lee"},{"subject":"third","from":"Sam"}]),
        ),
        (
            "thread.messages",
            "Message",
            json!([{"role":"them","body":"Hello"}]),
        ),
        ("board.cards", "Card", json!([{"id":1,"title":"Card"}])),
    ] {
        m.values.insert(
            name.into(),
            Type::List(Box::new(Type::Record(ty.into()))).into(),
        );
        s.values.insert(name.into(), value);
    }
    m.enums.insert(
        "Repeat".into(),
        EnumDefinition::new(["none", "daily", "weekly"]),
    );
    for (name, ty, initial) in [
        ("notify", Type::Boolean, json!(true)),
        ("talk", Type::Enum("Model".into()), json!("qwen3:4b")),
        ("volume", Type::Number, json!(50)),
        ("title", Type::String, json!("Call Ana")),
        ("when", Type::Date, json!("2026-10-10")),
        ("repeat", Type::Enum("Repeat".into()), json!("none")),
        ("draft", Type::String, json!("Hello")),
    ] {
        m.fields.insert(
            name.into(),
            FieldDefinition {
                binding: ty.into(),
                initial,
            },
        );
    }
    for (name, parameters) in [
        (
            "CreateReminder",
            BTreeMap::from([
                ("title".into(), Type::String),
                ("when".into(), Type::Date),
                ("repeat".into(), Type::Enum("Repeat".into())),
            ]),
        ),
        ("Send", BTreeMap::from([("text".into(), Type::String)])),
        ("Compose", BTreeMap::new()),
        ("ArchiveAll", BTreeMap::new()),
    ] {
        m.actions.insert(name.into(), ActionContract { parameters });
    }
    let examples = include_str!("../../../matrix/examples.md");
    for case in 1..=10 {
        let heading = format!("### {case}.");
        let fragment = examples
            .split(&heading)
            .nth(1)
            .unwrap()
            .split("```")
            .nth(1)
            .unwrap()
            .trim();
        let parsed = parse_surface(&surface(fragment)).unwrap();
        let resolved = resolve(&parsed, &m, &s);
        if case == 10 {
            assert_eq!(resolved.unwrap_err()[0].code.as_str(), "E_RESOLVE_SCOPE");
        } else {
            let result = lower_v0(&resolved.unwrap());
            if case == 6 {
                assert_eq!(
                    result.unwrap(),
                    mote_ui::validate(include_bytes!("../../mote-ui/fixtures/04_list.json"))
                        .unwrap()
                );
            } else {
                assert_eq!(
                    result.unwrap_err()[0].code.as_str(),
                    "E_LOWER_UNSUPPORTED",
                    "case {case}"
                );
            }
        }
    }
}

#[test]
fn action_contracts_resolve_shorthand_and_reject_source_defined_contracts() {
    let (m, s) = fixture();
    let original = m.clone();
    let parsed = parse_surface(
        "screen { button \"Save\" -> Save{text_value} button \"Approve\" -> approve }",
    )
    .unwrap();
    let resolved = resolve(&parsed, &m, &s).unwrap();
    let action = resolved.screen.children[0].action.as_ref().unwrap();
    assert_eq!(
        action.arguments["text_value"].value,
        Some(json!("Host text"))
    );
    assert_eq!(
        action.arguments["text_value"].ty,
        m.actions["Save"].parameters["text_value"]
    );
    assert!(resolved.screen.children[1]
        .action
        .as_ref()
        .unwrap()
        .arguments
        .is_empty());
    for (source, expected) in [
        ("screen { button \"Save\" -> Save }", "E_RESOLVE_ACTION"),
        (
            "screen { button \"Save\" -> Save{text_value, extra: text_value} }",
            "E_RESOLVE_ACTION",
        ),
        (
            "screen { button \"Save\" -> Save{text_value, text_value} }",
            "E_PARSE_DUPLICATE_ARGUMENT",
        ),
        (
            "screen { button \"Save\" -> Save{text_value: number} }",
            "E_RESOLVE_TYPE",
        ),
        (
            "screen { button \"Save\" -> Unknown{text_value} }",
            "E_RESOLVE_ACTION",
        ),
        (
            "screen { action Save { text_value } }",
            "E_PARSE_DECLARATION",
        ),
    ] {
        let error = parse_surface(source)
            .and_then(|parsed| resolve(&parsed, &m, &s))
            .unwrap_err();
        assert_eq!(error[0].code.as_str(), expected, "{source}");
    }
    assert_eq!(m, original);
}

#[test]
fn diagnostic_goldens_cover_every_field_and_keep_json_rejection_behavior() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../matrix/conformance/diagnostics.json")).unwrap();
    let (m, s) = fixture();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        if case["boundary"] == "resolver" {
            let parsed = parse_surface(source).unwrap();
            assert!(resolve(&parsed, &m, &s).is_err());
        } else if case["boundary"] == "lowerer" {
            let parsed = parse_surface(source).unwrap();
            resolve(&parsed, &m, &s).unwrap();
        }
        let errors = parse_surface(source)
            .and_then(|parsed| resolve(&parsed, &m, &s))
            .and_then(|resolved| lower_v0(&resolved))
            .unwrap_err();
        let actual: Vec<_> = errors.iter().map(|error| json!({
            "code":error.code.as_str(), "severity":match error.severity { matrix_lang::Severity::Error => "error" },
            "span":{"start":error.span.start,"end":error.span.end}, "declaration_path":error.declaration_path,
            "message":error.message, "hint":error.hint,
        })).collect();
        assert_eq!(json!(actual), case["diagnostics"], "{source}");
    }
    assert_eq!(
        mote_ui::validate_to_node(br#"{"type":"unknown"}"#),
        mote_ui::Node::Text {
            value: "unknown node type: unknown".into()
        }
    );
}
