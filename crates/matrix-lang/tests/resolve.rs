use matrix_lang::manifest::{
    ActionContract, BindingDefinition, EnumDefinition, FieldDefinition, RecordDefinition,
};
use matrix_lang::resolve::{ActionTarget, Element, FieldIdentity, NodeIdentity, RowKey};
use matrix_lang::types::DisplayFormat;
use matrix_lang::{parse_surface, resolve, BindingSnapshot, Manifest, ResolvedSurface, Type};
use serde_json::{json, Value};
use std::collections::BTreeMap;

struct AllocationProbe;

thread_local! {
    static ALLOCATION_PROBE: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

unsafe impl std::alloc::GlobalAlloc for AllocationProbe {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = ALLOCATION_PROBE.try_with(|probe| {
            let (size, count) = probe.get();
            if size != 0 && layout.size() == size {
                probe.set((size, count + 1));
            }
        });
        std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        std::alloc::GlobalAlloc::dealloc(&std::alloc::System, ptr, layout);
    }
}

#[global_allocator]
static ALLOCATOR: AllocationProbe = AllocationProbe;

fn count_allocations<T>(size: usize, f: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATION_PROBE.with(|probe| probe.set((size, 0)));
    let result = f();
    let count = ALLOCATION_PROBE.with(|probe| probe.replace((0, 0)).1);
    (result, count)
}

fn fixture() -> (Manifest, BindingSnapshot) {
    let mut manifest = Manifest::default();
    manifest
        .enums
        .insert("State".into(), EnumDefinition::new(["open", "closed"]));
    manifest
        .enums
        .insert("Role".into(), EnumDefinition::new(["me", "them", "system"]));
    manifest.records.insert(
        "Child".into(),
        RecordDefinition {
            fields: BTreeMap::from([("name".into(), Type::String.into())]),
        },
    );
    manifest.records.insert(
        "Row".into(),
        RecordDefinition {
            fields: BTreeMap::from([
                ("id".into(), Type::Integer.into()),
                ("name".into(), Type::String.into()),
                ("url".into(), Type::Url.into()),
                (
                    "children".into(),
                    Type::List(Box::new(Type::Record("Child".into()))).into(),
                ),
            ]),
        },
    );
    let values = [
        ("title", Type::String, json!("Host title")),
        ("number", Type::Number, json!(0.5)),
        ("flag", Type::Boolean, json!(true)),
        ("url", Type::Url, json!("https://example.com/item")),
        ("asset", Type::Asset, json!("avatar:1")),
        ("day", Type::Date, json!("2026-10-10")),
        ("later", Type::Date, json!("2026-10-11")),
        ("instant", Type::DateTime, json!("2026-10-10T12:00:00Z")),
        ("state", Type::Enum("State".into()), json!("open")),
        ("role", Type::Enum("Role".into()), json!("them")),
        (
            "options",
            Type::List(Box::new(Type::Enum("State".into()))),
            json!(["open", "closed"]),
        ),
        (
            "rows",
            Type::List(Box::new(Type::Record("Row".into()))),
            json!([
                {"id": 1, "name": "First", "url": "https://example.com/1", "children": [{"name": "Inner"}]},
                {"id": 2, "name": "Second", "url": "https://example.com/2", "children": []}
            ]),
        ),
    ];
    let mut snapshot = BindingSnapshot::default();
    for (name, ty, value) in values {
        manifest.values.insert(name.into(), ty.into());
        snapshot.values.insert(name.into(), value);
    }
    for (name, ty, initial) in [
        ("draft", Type::String, json!("initial")),
        ("enabled", Type::Boolean, json!(true)),
        ("amount", Type::Number, json!(5)),
        ("date", Type::Date, json!("2026-10-10")),
        ("time", Type::Time, json!("12:30:00")),
        ("choice", Type::Enum("State".into()), json!("open")),
        (
            "choices",
            Type::List(Box::new(Type::Enum("State".into()))),
            json!(["open"]),
        ),
    ] {
        manifest.fields.insert(
            name.into(),
            FieldDefinition {
                binding: ty.into(),
                initial,
            },
        );
    }
    for (name, parameters) in [
        ("Save", BTreeMap::from([("text".into(), Type::String)])),
        (
            "Bulk",
            BTreeMap::from([(
                "rows".into(),
                Type::List(Box::new(Type::Record("Row".into()))),
            )]),
        ),
        (
            "Pick",
            BTreeMap::from([(
                "row".into(),
                Type::Option(Box::new(Type::Record("Row".into()))),
            )]),
        ),
        ("Refresh", BTreeMap::new()),
    ] {
        manifest
            .actions
            .insert(name.into(), ActionContract { parameters });
    }
    (manifest, snapshot)
}

fn run(
    source: &str,
    manifest: &Manifest,
    snapshot: &BindingSnapshot,
) -> Result<ResolvedSurface, Vec<matrix_lang::Diagnostic>> {
    resolve(&parse_surface(source).expect(source), manifest, snapshot)
}

fn rejects(source: &str, code: &str) {
    let (m, s) = fixture();
    let errors = run(source, &m, &s).unwrap_err();
    assert_eq!(errors[0].code.as_str(), code, "{source}: {errors:?}");
    assert!(!errors[0].declaration_path.is_empty());
}

#[test]
fn published_names_are_case_sensitive() {
    let mut manifest = Manifest::default();
    manifest.actions.insert("CheckIn".into(), ActionContract::default());
    let snapshot = BindingSnapshot::default();
    let result = run(r#"screen { button "Go" -> CheckIn }"#, &manifest, &snapshot).unwrap();
    let action = result.screen.children[0].action.as_ref().unwrap();
    assert_eq!(action.name, "CheckIn");
    assert!(action.arguments.is_empty());
    for name in ["checkIn", "check_in"] {
        let source = format!(r#"screen {{ button "Go" -> {name} }}"#);
        let errors = run(&source, &manifest, &snapshot).unwrap_err();
        assert_eq!(errors[0].code.as_str(), "E_RESOLVE_ACTION", "{source}");
        assert_eq!(
            &source[errors[0].span.start..errors[0].span.end],
            format!("-> {name}")
        );
    }
}

#[test]
fn references_contain_validated_values_and_host_display_formats() {
    let (mut m, s) = fixture();
    m.values.get_mut("number").unwrap().display = DisplayFormat::Fixed { decimals: 2 };
    m.enums
        .get_mut("State")
        .unwrap()
        .labels
        .insert("open".into(), "Open".into());
    let result = run("screen { text title text number badge state }", &m, &s).unwrap();
    assert_eq!(result.screen.children[0].element, Element::Text);
    assert_eq!(
        result.screen.children[0].values["text"].value,
        Some(json!("Host title"))
    );
    assert_eq!(
        result.screen.children[1].values["text"].display.as_deref(),
        Some("0.50")
    );
    assert_eq!(
        result.screen.children[2].values["text"].display.as_deref(),
        Some("Open")
    );
}

#[test]
fn checks_every_display_and_control_contract() {
    let (m, s) = fixture();
    for source in [
        "screen { title title caption number code state tag state status state live md title link title to url stat \"N\" number delta title kv \"K\" state image asset alt title avatar asset alt \"Avatar\" timestamp instant relative progress value number steps stages [\"open\", \"closed\"] current state }",
        "screen { field \"Draft\" draft kind password min 0 max 20 pattern \"^[a-z]+$\" toggle \"On\" enabled slider \"Amount\" amount min 0 max 10 step 0.1 date \"Date\" date min day max later time \"Time\" time select \"State\" choice options options multiselect \"States\" choices options [\"open\", \"closed\"] }",
        "screen { area \"Draft\" draft check \"On\" enabled stepper \"Amount\" amount min 0 max 10 segmented \"State\" choice options [\"open\", \"closed\"] }",
        "screen { search \"Find\" draft choice \"State\" choice options [\"open\", \"closed\"] }",
        "screen { compose draft send -> Save{text: draft} }",
        "screen { field \"N\" amount kind number min 0.01 max 9.99 }",
        "screen { chat from rows { message role role body .name { text .name } } }",
    ] { run(source, &m, &s).unwrap_or_else(|e| panic!("{source}: {e:?}")); }
    for source in [
        "screen { text flag }",
        "screen { md number }",
        "screen { link title to title }",
        "screen { stat \"N\" state }",
        "screen { image url alt title }",
        "screen { avatar asset alt number }",
        "screen { timestamp day }",
        "screen { field \"X\" amount }",
        "screen { field \"X\" draft kind number }",
        "screen { area \"X\" enabled }",
        "screen { toggle \"X\" draft }",
        "screen { check \"X\" amount }",
        "screen { slider \"X\" draft min 0 max 1 }",
        "screen { stepper \"X\" draft }",
        "screen { date \"X\" time }",
        "screen { time \"X\" date }",
        "screen { search \"X\" enabled }",
        "screen { compose enabled send -> Save{text: title} }",
        "screen { select \"X\" draft options [\"open\"] }",
        "screen { multiselect \"X\" choice options options }",
        "screen { date \"X\" date min title }",
        "screen { chat from rows { message role state body .name } }",
    ] {
        rejects(source, "E_RESOLVE_TYPE");
    }
}

#[test]
fn snapshot_validation_precedes_source_resolution() {
    let (m, mut s) = fixture();
    s.values.insert("flag".into(), json!("false"));
    let errors = run("screen { text unknown }", &m, &s).unwrap_err();
    assert_eq!(errors[0].code.as_str(), "E_SNAPSHOT");
    for (name, value) in [
        ("state", json!("other")),
        ("day", json!("2026-02-30")),
        ("instant", json!("yesterday")),
        ("url", json!("/tmp/file")),
        ("asset", Value::Null),
        ("rows", json!([{"id":1}])),
    ] {
        let (m, mut s) = fixture();
        s.values.insert(name.into(), value);
        assert_eq!(
            run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
            "E_SNAPSHOT",
            "{name}"
        );
    }
    let (m, mut s) = fixture();
    s.fields.insert("draft".into(), json!(false));
    assert!(run("screen {}", &m, &s).is_err());
    let (m, mut s) = fixture();
    s.values.insert("unknown".into(), json!("x"));
    assert!(run("screen {}", &m, &s).is_err());
    let (m, mut s) = fixture();
    s.manifest_version = 99;
    assert!(run("screen {}", &m, &s).is_err());
}

#[test]
fn manifest_rejects_reserved_roots_unknown_types_and_shadowed_actions() {
    for field in [false, true] {
        let (mut m, s) = fixture();
        if field {
            m.fields.insert(
                "selected.name".into(),
                FieldDefinition {
                    binding: Type::String.into(),
                    initial: json!(""),
                },
            );
        } else {
            m.values.insert("selected.name".into(), Type::String.into());
        }
        assert_eq!(
            run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
            "E_MANIFEST"
        );
    }
    for name in ["Open", "Copy", "Back", "Dismiss", "Retry", "Cancel"] {
        let (mut m, s) = fixture();
        m.actions.insert(name.into(), ActionContract::default());
        assert_eq!(
            run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
            "E_MANIFEST"
        );
    }
    let (mut m, s) = fixture();
    m.values
        .insert("mystery".into(), Type::Record("Unknown".into()).into());
    assert_eq!(
        run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
        "E_MANIFEST"
    );
}

#[test]
fn resolves_nested_rows_and_rejects_outer_and_unknown_projections() {
    let (m, s) = fixture();
    let result = run("screen { list from rows key id { item { text .name list from .children { item { text .name } } } } }", &m, &s).unwrap();
    let list = &result.screen.children[0];
    assert_eq!(list.rows.len(), 2);
    assert_eq!(list.rows[0].identity.key, RowKey::Integer("1".into()));
    assert_eq!(
        list.rows[0].children[0].children[0].values["text"].value,
        Some(json!("First"))
    );
    assert_eq!(
        list.rows[0].children[0].children[1].rows[0].children[0].children[0].values["text"].value,
        Some(json!("Inner"))
    );
    rejects("screen { text .name }", "E_RESOLVE_SCOPE");
    rejects("screen { text absent }", "E_RESOLVE_PATH");
    rejects(
        "screen { list from rows { item { text .absent } } }",
        "E_RESOLVE_PATH",
    );
    rejects(
        "screen { list from rows { item { list from .children { item { text .url } } } } }",
        "E_RESOLVE_PATH",
    );
    rejects(
        "screen { list from number { item { text \"x\" } } }",
        "E_RESOLVE_TYPE",
    );
}

#[test]
fn empty_collections_still_validate_template_types_and_actions() {
    let (m, mut s) = fixture();
    s.values.insert("rows".into(), json!([]));
    for source in [
        "screen { list from rows { item { text .missing } } }",
        "screen { list from rows { item { button \"X\" -> Unknown } } }",
    ] {
        assert!(run(source, &m, &s).is_err());
    }
}

#[test]
fn field_ownership_is_order_independent_and_namespaced_by_row() {
    let (m, mut s) = fixture();
    s.fields.insert("draft".into(), json!("edited"));
    let result = run(
        "screen { button \"Save\" -> Save{text: draft} section { field \"Draft\" draft } }",
        &m,
        &s,
    )
    .unwrap();
    assert_eq!(
        result.screen.children[0].action.as_ref().unwrap().arguments["text"].value,
        Some(json!("edited"))
    );
    let result = run("screen { list from rows key id { item { field \"Draft\" draft button \"Save\" -> Save{text: draft} } } }", &m, &s).unwrap();
    let rows = &result.screen.children[0].rows;
    assert_ne!(
        rows[0].children[0].children[0].field,
        rows[1].children[0].children[0].field
    );
    rejects(
        "screen { field \"X\" draft section { area \"Y\" draft } }",
        "E_RESOLVE_FIELD",
    );
    rejects(
        "screen { button \"Save\" -> Save{text: draft} }",
        "E_RESOLVE_SCOPE",
    );
    rejects("screen { list from rows { item { field \"X\" draft } } button \"Save\" -> Save{text: draft} }", "E_RESOLVE_SCOPE");
    rejects("screen { field \"X\" draft text draft }", "E_RESOLVE_PATH");
}

#[test]
fn row_field_snapshots_override_only_their_instance() {
    let (m, mut s) = fixture();
    let source = "screen { list from rows key id { item { field \"Draft\" draft button \"Save\" -> Save{text: draft} } } }";
    let first = run(source, &m, &s).unwrap();
    let id: FieldIdentity = first.screen.children[0].rows[0].children[0].children[0]
        .field
        .clone()
        .unwrap();
    s.row_fields.insert(id, json!("row edit"));
    let next = run(source, &m, &s).unwrap();
    let rows = &next.screen.children[0].rows;
    assert_eq!(
        rows[0].children[0].children[1]
            .action
            .as_ref()
            .unwrap()
            .arguments["text"]
            .value,
        Some(json!("row edit"))
    );
    assert_eq!(
        rows[1].children[0].children[1]
            .action
            .as_ref()
            .unwrap()
            .arguments["text"]
            .value,
        Some(json!("initial"))
    );
}

#[test]
fn selection_publishes_in_the_immediate_parent_block_and_disables_missing_one() {
    let (m, mut s) = fixture();
    let source = "screen { button \"Open\" -> Open{url: selected.url} list from rows key id select one { item { text .name } } footer { button \"Pick\" -> Pick{row: selected} } }";
    let result = run(source, &m, &s).unwrap();
    assert!(result.screen.children[0].action.as_ref().unwrap().disabled);
    let id: NodeIdentity = result.screen.children[1].identity.clone();
    s.selections.insert(id, vec![1]);
    let result = run(source, &m, &s).unwrap();
    let action = result.screen.children[0].action.as_ref().unwrap();
    assert!(!action.disabled);
    assert_eq!(
        action.arguments["url"].value,
        Some(json!("https://example.com/2"))
    );
    rejects("screen { section { list from rows select one { item { text .name } } } button \"Open\" -> Open{url: selected.url} }", "E_RESOLVE_SCOPE");
    rejects("screen { list from rows { item { list from .children select one { item { text .name } } } } button \"Open\" -> Open{url: selected.url} }", "E_RESOLVE_SCOPE");
    rejects("screen { list from rows select one { item { text .name } } grid from rows select many { tile { text .name } } }", "E_RESOLVE_SCOPE");
}

#[test]
fn selection_many_has_no_projection_and_validates_snapshot_membership() {
    let (m, mut s) = fixture();
    let source = "screen { list from rows select many { item { text .name } } button \"Bulk\" -> Bulk{rows: selected} }";
    let result = run(source, &m, &s).unwrap();
    s.selections
        .insert(result.screen.children[0].identity.clone(), vec![0, 1]);
    let result = run(source, &m, &s).unwrap();
    assert_eq!(
        result.screen.children[1].action.as_ref().unwrap().arguments["rows"]
            .value
            .as_ref()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for indices in [vec![0, 0], vec![2]] {
        s.selections
            .insert(result.screen.children[0].identity.clone(), indices);
        assert_eq!(
            run(source, &m, &s).unwrap_err()[0].code.as_str(),
            "E_SNAPSHOT"
        );
    }
    rejects("screen { list from rows select many { item { text .name } } button \"Open\" -> Open{url: selected.url} }", "E_RESOLVE_SCOPE");
}

#[test]
fn keys_require_unique_nonnull_host_strings_or_integers() {
    let (m, mut s) = fixture();
    s.values.get_mut("rows").unwrap()[1]["id"] = json!(1);
    assert_eq!(
        run(
            "screen { list from rows key id { item { text .name } } }",
            &m,
            &s
        )
        .unwrap_err()[0]
            .code
            .as_str(),
        "E_RESOLVE_KEY"
    );
    let (mut m, mut s) = fixture();
    m.records
        .get_mut("Row")
        .unwrap()
        .fields
        .insert("id".into(), Type::Number.into());
    s.values.get_mut("rows").unwrap()[0]["id"] = json!(1.5);
    assert_eq!(
        run(
            "screen { list from rows key id { item { text .name } } }",
            &m,
            &s
        )
        .unwrap_err()[0]
            .code
            .as_str(),
        "E_RESOLVE_KEY"
    );
    rejects(
        "screen { list from rows key missing { item { text .name } } }",
        "E_RESOLVE_PATH",
    );
}

#[test]
fn options_stages_progress_dates_and_patterns_fail_closed() {
    for source in [
        "screen { select \"State\" choice options [\"other\"] }",
        "screen { steps stages [\"other\"] current state }",
        "screen { date \"Date\" date min later max day }",
        "screen { field \"Draft\" draft pattern \"(?=x)\" }",
    ] {
        rejects(source, "E_RESOLVE_CONSTRAINT");
    }
    for options in [json!([]), json!(["open", "open"])] {
        let (m, mut s) = fixture();
        s.values.insert("options".into(), options);
        assert_eq!(
            run("screen { select \"State\" choice options options }", &m, &s).unwrap_err()[0]
                .code
                .as_str(),
            "E_RESOLVE_CONSTRAINT"
        );
    }
    let (m, mut s) = fixture();
    s.values.insert("number".into(), json!(1.01));
    assert_eq!(
        run("screen { progress value number }", &m, &s).unwrap_err()[0]
            .code
            .as_str(),
        "E_RESOLVE_CONSTRAINT"
    );
}

#[test]
fn validates_manifest_actions_and_all_builtin_contexts() {
    let (m, s) = fixture();
    let result = run("screen { button \"Open\" -> Open{url} button \"Copy\" -> Copy{text: title} button \"Back\" -> Back button \"Retry\" -> Retry button \"Cancel\" -> Cancel dialog \"D\" { sheet \"S\" { button \"Close\" -> Dismiss } } list from rows { item { text .name } error \"Failed\" -> Retry } button \"Refresh\" -> Refresh }", &m, &s).unwrap();
    assert_eq!(
        result.screen.children[3].action.as_ref().unwrap().target,
        ActionTarget::SurfaceFailures
    );
    assert!(matches!(
        result.screen.children[6].children[0]
            .action
            .as_ref()
            .unwrap()
            .target,
        ActionTarget::Collection(_)
    ));
    assert_eq!(
        result.screen.children[5].children[0].children[0]
            .action
            .as_ref()
            .unwrap()
            .target,
        ActionTarget::Overlay(result.screen.children[5].children[0].identity.clone())
    );
    for source in [
        "screen { button \"X\" -> Unknown }",
        "screen { button \"X\" -> Save }",
        "screen { button \"X\" -> Save{text: title, extra: title} }",
        "screen { button \"X\" -> Back{x: title} }",
    ] {
        rejects(source, "E_RESOLVE_ACTION");
    }
    rejects(
        "screen { button \"X\" -> Save{text: number} }",
        "E_RESOLVE_TYPE",
    );
    rejects(
        "screen { button \"X\" -> Open{url: title} }",
        "E_RESOLVE_TYPE",
    );
    rejects("screen { button \"X\" -> Dismiss }", "E_RESOLVE_SCOPE");
}

#[test]
fn column_kind_requires_matching_type_and_host_formatter() {
    let (mut m, mut s) = fixture();
    m.records.get_mut("Row").unwrap().fields.insert(
        "price".into(),
        BindingDefinition {
            ty: Type::Number,
            display: DisplayFormat::Money {
                symbol: "$".into(),
                decimals: 2,
            },
        },
    );
    for row in s.values.get_mut("rows").unwrap().as_array_mut().unwrap() {
        row["price"] = json!(12.5);
    }
    let result = run(
        "screen { table from rows { column \"Name\" .name column \"Price\" .price kind money } }",
        &m,
        &s,
    )
    .unwrap();
    assert_eq!(
        result.screen.children[0].rows[0].children[1].values["value"]
            .display
            .as_deref(),
        Some("$12.50")
    );
    rejects(
        "screen { table from rows { column \"Name\" .name kind number } }",
        "E_RESOLVE_TYPE",
    );
    rejects(
        "screen { table from rows { column \"ID\" .id kind money } }",
        "E_RESOLVE_TYPE",
    );
}

#[test]
fn expansion_bounds_reject_before_returning_partial_surfaces() {
    let (m, mut s) = fixture();
    let row = s.values["rows"][0].clone();
    s.values
        .insert("rows".into(), Value::Array(vec![row; 1000]));
    let errors = run("screen { list from rows { item { text .name } } }", &m, &s).unwrap_err();
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
    let (m, mut s) = fixture();
    s.values.insert("title".into(), json!("x".repeat(262_144)));
    assert_eq!(
        run("screen { text title }", &m, &s).unwrap_err()[0]
            .code
            .as_str(),
        "E_RESOLVE_LIMIT"
    );
}

#[test]
fn expanded_nodes_accept_2000_and_reject_2001() {
    let (m, mut s) = fixture();
    let row = s.values["rows"][0].clone();
    s.values.insert("rows".into(), json!(vec![row; 999]));
    let accepted = run("screen { list from rows { item { text .name } } }", &m, &s).unwrap();
    assert_eq!(accepted.screen.children[0].rows.len(), 999);
    assert!(accepted.screen.children[0]
        .rows
        .iter()
        .all(|r| r.children.len() == 1 && r.children[0].children.len() == 1));
    let source = "screen { list from rows { item { text .name } } divider }";
    let error = &run(source, &m, &s).unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(error.declaration_path, ["screen", "1"]);
    assert_eq!(&source[error.span.start..error.span.end], "divider");
}

#[test]
fn expanded_declaration_depth_accepts_12_and_rejects_synthetic_13() {
    let (m, s) = fixture();
    let source = format!(
        "screen {{ list from rows {{ item {{ {}text .name{} }} }} }}",
        "col {".repeat(8),
        "}".repeat(8)
    );
    let mut parsed = parse_surface(&source).unwrap();
    assert!(resolve(&parsed, &m, &s).is_ok());
    let mut leaf = &mut parsed.screen;
    for _ in 0..11 {
        leaf = match &mut leaf.kind {
            matrix_lang::DeclarationKind::Screen(c) => &mut c.children[0],
            matrix_lang::DeclarationKind::Item(c) => &mut c[0],
            matrix_lang::DeclarationKind::List(c) => &mut c.children[0],
            matrix_lang::DeclarationKind::Col { children, .. } => &mut children[0],
            _ => panic!("expected container"),
        };
    }
    let original = leaf.clone();
    leaf.kind = matrix_lang::DeclarationKind::Col {
        alignment: None,
        distribution: None,
        children: vec![original],
    };
    let span = leaf.span;
    let error = &resolve(&parsed, &m, &s).unwrap_err()[0];
    assert_eq!(error.code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(error.declaration_path.len(), 13);
    assert_eq!(error.span, span);
}

#[test]
fn retained_resolved_bytes_accept_262144_and_reject_262145() {
    let (m, mut s) = fixture();
    for (size, accepted) in [(262_110, true), (262_111, false)] {
        s.values.insert("title".into(), json!("x".repeat(size)));
        let result = run("screen { text title }", &m, &s);
        if accepted {
            assert_eq!(
                result.unwrap().screen.children[0].values["text"]
                    .display
                    .as_ref()
                    .unwrap()
                    .len(),
                262_110
            );
        } else {
            let error = &result.unwrap_err()[0];
            assert_eq!(error.code.as_str(), "E_RESOLVE_LIMIT");
            assert_eq!(error.declaration_path, ["screen", "0"]);
            assert_eq!(error.span, matrix_lang::Span::new(9, 19));
        }
    }
}

#[test]
fn expanded_retained_bytes_accept_262144_and_reject_262145() {
    let mut m = Manifest::default();
    m.records.insert(
        "R".into(),
        RecordDefinition {
            fields: BTreeMap::from([("s".into(), Type::String.into())]),
        },
    );
    m.values.insert(
        "rows".into(),
        Type::List(Box::new(Type::Record("R".into()))).into(),
    );
    let mut s = BindingSnapshot::default();
    let parsed = parse_surface("screen { list from rows { item { text .s } } }").unwrap();
    for (second, accepted) in [(131_022, true), (131_023, false)] {
        s.values.insert(
            "rows".into(),
            json!([{"s":"x".repeat(131_022)}, {"s":"x".repeat(second)}]),
        );
        let result = resolve(&parsed, &m, &s);
        if accepted {
            let result = result.unwrap();
            assert_eq!(result.screen.children[0].rows.len(), 2);
            assert_eq!(
                result.screen.children[0].rows[1].children[0].children[0].values["text"]
                    .display
                    .as_ref()
                    .unwrap()
                    .len(),
                131_022
            );
        } else {
            let error = &result.unwrap_err()[0];
            assert_eq!(error.code.as_str(), "E_RESOLVE_LIMIT");
            assert_eq!(error.declaration_path, ["screen", "0", "0", "0"]);
            assert_eq!(error.span, matrix_lang::Span::new(33, 40));
        }
    }
}

#[test]
fn retained_formatted_bytes_count_exact_escaped_display_and_money_symbol() {
    let mut m = Manifest::default();
    m.enums.insert("E".into(), EnumDefinition::new(["a"]));
    m.values
        .insert("value".into(), Type::Enum("E".into()).into());
    let mut s = BindingSnapshot::default();
    s.values.insert("value".into(), json!("a"));
    let parsed = parse_surface("screen { text value }").unwrap();
    for (suffix, accepted) in [("x", true), ("xx", false)] {
        m.enums
            .get_mut("E")
            .unwrap()
            .labels
            .insert("a".into(), format!("{}{suffix}", "\"".repeat(131_054)));
        assert_eq!(resolve(&parsed, &m, &s).is_ok(), accepted);
    }
    m.values.insert(
        "value".into(),
        BindingDefinition {
            ty: Type::Integer,
            display: DisplayFormat::Money {
                symbol: "x".repeat(131_054),
                decimals: 0,
            },
        },
    );
    for (value, accepted) in [(10, true), (100, false)] {
        s.values.insert("value".into(), json!(value));
        assert_eq!(resolve(&parsed, &m, &s).is_ok(), accepted);
    }
}

#[test]
fn aggregate_limit_precedes_retained_literal_copies() {
    let (m, s) = fixture();
    let source = format!(
        "screen {{ text \"{}\" text \"{}\" }}",
        "x".repeat(131_047),
        "x".repeat(131_047)
    );
    let parsed = parse_surface(&source).unwrap();
    let (result, allocations) = count_allocations(131_047, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 2,
        "only the first retained raw and display literals may be copied"
    );
}

#[test]
fn aggregate_limit_precedes_retained_literal_list_copy() {
    let (m, mut s) = fixture();
    s.values.insert("title".into(), json!("a"));
    let mut parsed = parse_surface(&format!(
        "screen {{ steps stages [\"a\", \"{}\"] current title }}",
        "x".repeat(131_038)
    ))
    .unwrap();
    let matrix_lang::DeclarationKind::Screen(screen) = &mut parsed.screen.kind else {
        panic!("screen")
    };
    screen.children.push(screen.children[0].clone());
    let (result, allocations) = count_allocations(131_038, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 1,
        "only the first retained stages list may be copied"
    );
}

#[test]
fn aggregate_limit_precedes_retained_formatter_copy() {
    let (mut m, mut s) = fixture();
    m.values.insert(
        "number".into(),
        BindingDefinition {
            ty: Type::Integer,
            display: DisplayFormat::Money {
                symbol: "x".repeat(65_524),
                decimals: 0,
            },
        },
    );
    s.values.insert("number".into(), json!(10));
    let parsed = parse_surface("screen { text number text number }").unwrap();
    let (result, allocations) = count_allocations(65_524, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 1,
        "only the first retained formatter symbol may be copied"
    );
}

#[test]
fn aggregate_limit_precedes_retained_string_copies() {
    let (m, mut s) = fixture();
    s.values.insert("title".into(), json!("x".repeat(131_047)));
    let parsed = parse_surface("screen { text title text title }").unwrap();
    let (result, allocations) = count_allocations(131_047, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 2,
        "only the first raw and display strings may be copied"
    );
}

#[test]
fn aggregate_limit_precedes_retained_bound_list_copy() {
    let mut m = Manifest::default();
    m.values
        .insert("items".into(), Type::List(Box::new(Type::String)).into());
    m.actions.insert(
        "Take".into(),
        ActionContract {
            parameters: BTreeMap::from([("items".into(), Type::List(Box::new(Type::String)))]),
        },
    );
    let mut s = BindingSnapshot::default();
    s.values
        .insert("items".into(), json!(["x".repeat(131_047)]));
    let parsed =
        parse_surface("screen { button \"A\" -> Take{items} button \"B\" -> Take{items} }")
            .unwrap();
    let (result, allocations) = count_allocations(131_047, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 1,
        "only the first retained list argument may be copied"
    );
}

#[test]
fn aggregate_limit_precedes_retained_property_copy() {
    let (m, s) = fixture();
    let source = format!(
        "screen {{ label \"{}\" label \"{}\" }}",
        "x".repeat(131_050),
        "x".repeat(131_050)
    );
    let parsed = parse_surface(&source).unwrap();
    let (result, allocations) = count_allocations(131_050, || resolve(&parsed, &m, &s));
    assert_eq!(result.unwrap_err()[0].code.as_str(), "E_RESOLVE_LIMIT");
    assert_eq!(
        allocations, 1,
        "only the first retained property may be copied"
    );
}

#[test]
fn exact_decimal_constraints_survive_resolution_without_float_rounding() {
    use matrix_lang::resolve::Property;
    let (m, s) = fixture();
    let result = run("screen { slider \"N\" amount min 9007199254740992 max 9007199254740993 step 1e-9999999999999999999999999999 }", &m, &s).unwrap();
    let properties = &result.screen.children[0].properties;
    assert!(properties.contains(&Property::Max("9007199254740993".into())));
    assert!(properties.contains(&Property::Step("1e-9999999999999999999999999999".into())));
}

#[test]
fn keyed_row_field_identity_survives_reordering() {
    let (m, mut s) = fixture();
    let source = "screen { list from rows key id { item { field \"Draft\" draft } } }";
    let result = run(source, &m, &s).unwrap();
    let identity = result.screen.children[0].rows[0].children[0].children[0]
        .field
        .clone()
        .unwrap();
    s.row_fields
        .insert(identity.clone(), json!("retained edit"));
    s.values
        .get_mut("rows")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .reverse();
    let result = run(source, &m, &s).unwrap();
    let field = &result.screen.children[0].rows[1].children[0].children[0];
    assert_eq!(field.field.as_ref(), Some(&identity));
    assert_eq!(field.values["field"].value, Some(json!("retained edit")));
}

#[test]
fn nested_selection_stays_in_its_row_instance() {
    let (m, mut s) = fixture();
    let source = "screen { list from rows key id { item { list from .children select one { item { text .name } } button \"Copy\" -> Copy{text: selected.name} } } }";
    let result = run(source, &m, &s).unwrap();
    let collection = result.screen.children[0].rows[0].children[0].children[0]
        .identity
        .clone();
    s.selections.insert(collection, vec![0]);
    let result = run(source, &m, &s).unwrap();
    let rows = &result.screen.children[0].rows;
    let selected = rows[0].children[0].children[1].action.as_ref().unwrap();
    assert_eq!(selected.arguments["text"].value, Some(json!("Inner")));
    assert!(!selected.disabled);
    assert!(
        rows[1].children[0].children[1]
            .action
            .as_ref()
            .unwrap()
            .disabled
    );
}

#[test]
fn root_record_projections_preserve_their_declared_formatter() {
    let (mut m, mut s) = fixture();
    m.values
        .insert("github.viewer".into(), Type::Record("Child".into()).into());
    s.values
        .insert("github.viewer".into(), json!({"name":"Viewer"}));
    let result = run("screen { text github.viewer.name }", &m, &s).unwrap();
    assert_eq!(
        result.screen.children[0].values["text"].value,
        Some(json!("Viewer"))
    );
    assert!(run("screen { text github.viewer.unknown }", &m, &s).is_err());
}

#[test]
fn rejects_invalid_initial_values_and_ambiguous_binding_roots() {
    let (mut m, s) = fixture();
    m.fields.get_mut("draft").unwrap().initial = json!(false);
    assert_eq!(
        run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
        "E_MANIFEST"
    );
    let (mut m, s) = fixture();
    m.values.insert("title.child".into(), Type::String.into());
    assert_eq!(
        run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
        "E_MANIFEST"
    );
    let (mut m, s) = fixture();
    m.records
        .get_mut("Child")
        .unwrap()
        .fields
        .insert("cycle".into(), Type::Record("Child".into()).into());
    assert_eq!(
        run("screen {}", &m, &s).unwrap_err()[0].code.as_str(),
        "E_MANIFEST"
    );
}

#[test]
fn the_documented_design_case_ten_is_a_scope_rejection() {
    let examples = include_str!("../../../matrix/examples.md");
    let case = examples
        .split("### 10.")
        .nth(1)
        .unwrap()
        .split("```")
        .nth(1)
        .unwrap();
    let source = format!("screen {{ {} }}", case.trim());
    let (m, s) = fixture();
    let parsed = parse_surface(&source).unwrap();
    let errors = resolve(&parsed, &m, &s).unwrap_err();
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_SCOPE");
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "selected.url"
    );
}

#[test]
fn column_values_accept_only_scalar_types_and_declared_kinds() {
    rejects(
        "screen { table from rows { column \"Children\" .children } }",
        "E_RESOLVE_TYPE",
    );
    let (mut m, mut s) = fixture();
    m.records
        .get_mut("Row")
        .unwrap()
        .fields
        .insert("date".into(), Type::Date.into());
    m.records
        .get_mut("Row")
        .unwrap()
        .fields
        .insert("state".into(), Type::Enum("State".into()).into());
    for row in s.values.get_mut("rows").unwrap().as_array_mut().unwrap() {
        row["date"] = json!("2026-10-10");
        row["state"] = json!("open");
    }
    run("screen { table from rows { column \"Date\" .date kind date column \"State\" .state kind status } }", &m, &s).unwrap();
}

#[test]
fn independent_collection_keys_do_not_merge_field_identity() {
    let (m, s) = fixture();
    let source = "screen { list from rows key id { item { field \"Draft\" draft } } list from rows key id { item { field \"Draft\" draft } } }";
    let result = run(source, &m, &s).unwrap();
    let first = &result.screen.children[0].rows[0].children[0].children[0].field;
    let second = &result.screen.children[1].rows[0].children[0].children[0].field;
    assert_ne!(first, second);
}

#[test]
fn byte_bound_counts_only_retained_resolved_content() {
    let (m, mut s) = fixture();
    s.values.insert("title".into(), json!("x".repeat(150_000)));
    let result = run("screen { text title }", &m, &s).unwrap();
    assert_eq!(
        result.screen.children[0].values["text"]
            .display
            .as_ref()
            .unwrap()
            .len(),
        150_000
    );
}

#[test]
fn repeated_action_names_are_bounded_before_the_next_name_is_copied() {
    for (length, accepted) in [(130_000, true), (132_000, false)] {
        let (mut m, s) = fixture();
        let name = "A".repeat(length);
        m.actions.insert(name.clone(), ActionContract::default());
        let source = format!("screen {{ list from rows {{ item {{ button \"Go\" -> {name} }} }} }}");
        let result = run(&source, &m, &s);
        if accepted {
            let resolved = result.unwrap();
            for row in &resolved.screen.children[0].rows {
                assert_eq!(row.children[0].children[0].action.as_ref().unwrap().name, name);
            }
        } else {
            let errors = result.err().expect("repeated names must exceed the byte budget");
            assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
            assert_eq!(&source[errors[0].span.start..errors[0].span.end], name);
            assert_eq!(errors[0].declaration_path, ["screen", "0", "0", "0"]);
        }
    }
}

#[test]
fn repeated_argument_names_count_toward_the_resolved_byte_limit() {
    let (mut m, s) = fixture();
    let argument = "a".repeat(132_000);
    m.actions.insert("Act".into(), ActionContract {
        parameters: BTreeMap::from([(argument.clone(), Type::String)]),
    });
    let source = format!("screen {{ list from rows {{ item {{ button \"Go\" -> Act{{{argument}: title}} }} }} }}");
    let errors = run(&source, &m, &s).err().expect("argument names exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn repeated_field_paths_count_toward_the_resolved_byte_limit() {
    let (mut m, s) = fixture();
    let field = "f".repeat(132_000);
    m.fields.insert(field.clone(), FieldDefinition {
        binding: Type::String.into(),
        initial: json!(""),
    });
    let source = format!("screen {{ list from rows {{ item {{ field \"F\" {field} }} }} }}");
    let errors = run(&source, &m, &s).err().expect("field paths exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn repeated_key_paths_count_toward_the_resolved_byte_limit() {
    let (mut m, mut s) = fixture();
    let key = "k".repeat(132_000);
    m.records.get_mut("Child").unwrap().fields.insert(key.clone(), Type::String.into());
    s.values.get_mut("rows").unwrap()[0]["children"] = json!([]);
    let source = format!("screen {{ list from rows {{ item {{ list from .children key {key} {{ item {{ text \"x\" }} }} }} }} }}");
    let errors = run(&source, &m, &s).err().expect("key paths exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn repeated_type_names_count_toward_the_resolved_byte_limit() {
    let (mut m, s) = fixture();
    let name = "E".repeat(132_000);
    m.enums.insert(name.clone(), EnumDefinition::new(["open"]));
    m.values.insert("state".into(), Type::Enum(name).into());
    let errors = run("screen { list from rows { item { text state } } }", &m, &s)
        .err().expect("type names exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn repeated_row_keys_count_toward_the_resolved_byte_limit() {
    let (mut m, mut s) = fixture();
    m.records.get_mut("Row").unwrap().fields.insert("id".into(), Type::String.into());
    s.values.get_mut("rows").unwrap()[0]["id"] = json!("a".repeat(70_000));
    s.values.get_mut("rows").unwrap()[1]["id"] = json!("b".repeat(70_000));
    let errors = run("screen { list from rows key id { item { button \"Go\" -> Refresh } } }", &m, &s)
        .err().expect("row identities exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn action_targets_count_retained_row_keys_toward_the_byte_limit() {
    let (mut m, mut s) = fixture();
    m.records.get_mut("Row").unwrap().fields.insert("id".into(), Type::String.into());
    s.values.get_mut("rows").unwrap().as_array_mut().unwrap().truncate(1);
    s.values.get_mut("rows").unwrap()[0]["id"] = json!("a".repeat(55_000));
    let errors = run("screen { list from rows key id { item { dialog \"D\" { button \"Go\" -> Dismiss } } } }", &m, &s)
        .err().expect("action target identity exceeds the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn field_identities_count_retained_row_keys_toward_the_byte_limit() {
    let (mut m, mut s) = fixture();
    m.records.get_mut("Row").unwrap().fields.insert("id".into(), Type::String.into());
    s.values.get_mut("rows").unwrap().as_array_mut().unwrap().truncate(1);
    s.values.get_mut("rows").unwrap()[0]["id"] = json!("a".repeat(70_000));
    let errors = run("screen { list from rows key id { item { field \"F\" draft } } }", &m, &s)
        .err().expect("field identity exceeds the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn empty_nested_template_fields_charge_only_their_retained_identity_rows() {
    let (mut m, mut s) = fixture();
    m.records
        .get_mut("Row")
        .unwrap()
        .fields
        .insert("id".into(), Type::String.into());
    s.values
        .get_mut("rows")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .truncate(1);
    s.values.get_mut("rows").unwrap()[0]["id"] = json!("a".repeat(45_000));
    s.values.get_mut("rows").unwrap()[0]["children"] = json!([]);

    let result = run(
        "screen { list from rows key id { item { list from .children { item { field \"F\" draft } } } } }",
        &m,
        &s,
    )
    .expect("an empty nested template retains no outer rows in its field identity");

    let field = result.screen.children[0].rows[0].children[0].children[0].empty_templates[0]
        .children[0]
        .field
        .as_ref()
        .unwrap();
    assert!(field.rows.is_empty());
}

#[test]
fn retained_formatter_symbols_count_toward_the_resolved_byte_limit() {
    let (mut m, s) = fixture();
    m.values.get_mut("number").unwrap().display = DisplayFormat::Money {
        symbol: "s".repeat(70_000),
        decimals: 2,
    };
    let errors = run("screen { list from rows { item { text number } } }", &m, &s)
        .err().expect("retained formatters exceed the byte budget");
    assert_eq!(errors[0].code.as_str(), "E_RESOLVE_LIMIT");
}

#[test]
fn empty_template_fields_do_not_read_surface_field_edits() {
    let (m, mut s) = fixture();
    s.fields.insert("choice".into(), json!("closed"));
    s.values.insert("rows".into(), json!([]));
    let source = "screen { select \"Surface\" choice options [\"closed\"] list from rows { item { select \"Row\" choice options [\"open\"] } } }";
    let result = run(source, &m, &s).unwrap();
    assert_eq!(
        result.screen.children[0].values["field"].value,
        Some(json!("closed"))
    );
    assert!(result.screen.children[1].rows.is_empty());
}

#[test]
fn empty_nested_template_fields_do_not_read_outer_row_edits() {
    let (m, mut s) = fixture();
    let source = "screen { list from rows key id { item { select \"Outer\" choice options [\"open\", \"closed\"] list from .children { item { select \"Inner\" choice options [\"open\"] } } } } }";
    let initial = run(source, &m, &s).unwrap();
    let outer_field = initial.screen.children[0].rows[1].children[0].children[0]
        .field
        .clone()
        .unwrap();
    s.row_fields.insert(outer_field, json!("closed"));
    let result = run(source, &m, &s).unwrap();
    let outer = &result.screen.children[0].rows[1].children[0];
    assert_eq!(
        outer.children[0].values["field"].value,
        Some(json!("closed"))
    );
    assert!(outer.children[1].rows.is_empty());
}

#[test]
fn collection_type_replacement_invalidates_selection_and_row_field_identity() {
    let (m, s) = fixture();
    let list = "screen { list from rows key id select one { item { field \"Draft\" draft } } }";
    let grid = "screen { grid from rows key id select one { tile { field \"Draft\" draft } } }";
    let before = run(list, &m, &s).unwrap();
    let after = run(grid, &m, &s).unwrap();
    let list = &before.screen.children[0];
    let replacement = &after.screen.children[0];
    assert_ne!(list.identity, replacement.identity);
    assert_ne!(list.rows[0].identity, replacement.rows[0].identity);
    let field = list.rows[0].children[0].children[0].field.clone().unwrap();
    assert_ne!(
        Some(&field),
        replacement.rows[0].children[0].children[0].field.as_ref()
    );
    let mut stale_selection = s.clone();
    stale_selection
        .selections
        .insert(list.identity.clone(), vec![0]);
    assert_eq!(
        run(grid, &m, &stale_selection).unwrap_err()[0]
            .code
            .as_str(),
        "E_SNAPSHOT"
    );
    let mut stale_field = s;
    stale_field.row_fields.insert(field, json!("old list edit"));
    assert_eq!(
        run(grid, &m, &stale_field).unwrap_err()[0].code.as_str(),
        "E_SNAPSHOT"
    );
}

#[test]
fn unkeyed_identity_includes_the_node_type() {
    let (m, s) = fixture();
    let before = run("screen { text title }", &m, &s).unwrap();
    let after = run("screen { caption title }", &m, &s).unwrap();
    assert_ne!(
        before.screen.children[0].identity,
        after.screen.children[0].identity
    );
}

#[test]
fn url_values_require_a_valid_host_and_port() {
    for url in [
        "https://[",
        "https://example.com:bad",
        "https://example.com:65536",
        "https://exa[mple.com",
        "https://",
        "/tmp/file",
    ] {
        let (m, mut s) = fixture();
        s.values.insert("url".into(), json!(url));
        let errors = run("screen { button \"Open\" -> Open{url} }", &m, &s).unwrap_err();
        assert_eq!(errors[0].code.as_str(), "E_SNAPSHOT", "{url}");
    }
    for url in [
        "https://example.com:8443/path?q=1#part",
        "https://[::1]:443/path",
        "mailto:user@example.com",
        "app:resource/1",
    ] {
        let (m, mut s) = fixture();
        s.values.insert("url".into(), json!(url));
        let result = run("screen { button \"Open\" -> Open{url} }", &m, &s).unwrap();
        assert_eq!(
            result.screen.children[0].action.as_ref().unwrap().arguments["url"].value,
            Some(json!(url))
        );
    }
}
