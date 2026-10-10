use matrix_lang::{format_fragment, format_surface, parse_fragment, parse_surface};
use std::{fs, path::Path};

fn blocks(markdown: &str) -> Vec<&str> {
    markdown
        .split("```\n")
        .enumerate()
        .filter_map(|(index, block)| (index % 2 == 1).then_some(block.trim()))
        .collect()
}

fn round_trip(source: &str) {
    if source.starts_with("screen ") || source.starts_with("screen{") {
        let original = parse_surface(source).unwrap();
        let formatted = format_surface(&original);
        let reparsed = parse_surface(&formatted).unwrap();
        assert_eq!(format_surface(&reparsed), formatted, "{source}");
        assert_eq!(
            without_spans(&original),
            without_spans(&reparsed),
            "{source}"
        );
    } else {
        let original = parse_fragment(source).unwrap();
        let formatted = format_fragment(&original);
        let reparsed = parse_fragment(&formatted).unwrap();
        assert_eq!(format_fragment(&reparsed), formatted, "{source}");
        assert_eq!(
            without_spans(&original),
            without_spans(&reparsed),
            "{source}"
        );
    }
}

fn without_spans(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut normalized = String::new();
    let mut remaining = debug.as_str();
    while let Some(index) = remaining.find("Span { start: ") {
        normalized.push_str(&remaining[..index]);
        remaining = &remaining[index..];
        let end = remaining.find('}').unwrap() + 1;
        normalized.push_str("Span");
        remaining = &remaining[end..];
    }
    normalized.push_str(remaining);
    normalized
}

#[test]
fn formats_every_positive_fixture_idempotently() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../matrix");
    let examples = fs::read_to_string(root.join("examples.md")).unwrap();
    for source in blocks(&examples) {
        if !source.starts_with('{') && !source.starts_with("PullRequest {") {
            round_trip(source);
        }
    }
    for entry in fs::read_dir(root.join("recipes")).unwrap() {
        let markdown = fs::read_to_string(entry.unwrap().path()).unwrap();
        for source in blocks(&markdown) {
            if !source.starts_with("PullRequest {") {
                round_trip(source);
            }
        }
    }
    for entry in fs::read_dir(root.join("conformance/v0")).unwrap() {
        round_trip(&fs::read_to_string(entry.unwrap().path()).unwrap());
    }
}

#[test]
fn preserves_exact_number_source_and_uses_json_escaping() {
    let source =
        "screen { row { field \"Line\\n\\\"É\" amount kind number min -1.25e2 max 0 fill } }";
    let formatted = format_surface(&parse_surface(source).unwrap());
    assert_eq!(
        formatted,
        "screen {\n  row {\n    field \"Line\\n\\\"É\" amount kind number min -1.25e2 max 0 fill\n  }\n}\n"
    );
    assert!(
        parse_surface(&formatted).unwrap().screen.children()[0].children()[0]
            .fill
            .is_some()
    );
}

#[test]
fn canonicalizes_all_argument_shapes() {
    let source = "screen{row nowrap end around{button \"Go\" primary confirm disabled fill->Act{x,target:.title} col end between fill{text .name fill} list from rows key id select one fill{item{text .name}}}}";
    let formatted = format_surface(&parse_surface(source).unwrap());
    assert_eq!(formatted, "screen {\n  row nowrap end around {\n    button \"Go\" primary confirm disabled fill -> Act{x, target: .title}\n    col end between fill {\n      text .name fill\n    }\n    list from rows key id select one fill {\n      item {\n        text .name\n      }\n    }\n  }\n}\n");
    round_trip(&formatted);
}

#[test]
fn round_trips_one_thousand_deterministic_bounded_asts() {
    let mut seed = 0x9e37_79b9_u64;
    for _ in 0..1000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let count = (seed % 12 + 1) as usize;
        let mut source = String::from("screen { ");
        for index in 0..count {
            match (seed >> (index % 16)) % 6 {
                0 => source.push_str(&format!("text value_{index} ")),
                1 => source.push_str(&format!("caption \"C{index}\\n\" ")),
                2 => source.push_str("divider "),
                3 => source.push_str(&format!("row {{ text value_{index} }} ")),
                4 => source.push_str(&format!("field \"F{index}\" field_{index} min 0 max 1 ")),
                _ => source.push_str(&format!(
                    "button \"B{index}\" -> Act{{value: value_{index}}} "
                )),
            }
        }
        source.push('}');
        round_trip(&source);
    }
}
