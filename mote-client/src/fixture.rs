use mote_ui::Node;
use std::ffi::OsString;

pub fn load(mut args: impl Iterator<Item = OsString>) -> Result<Option<Node>, String> {
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    let usage = "usage: mote-client [--gallery | --fixture <json-path>]";
    if flag == "--gallery" {
        return if args.next().is_none() {
            Ok(Some(crate::gallery::tree()))
        } else {
            Err(usage.into())
        };
    }
    if flag != "--fixture" {
        return Err(usage.into());
    }
    let path = args.next().ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }
    let path = std::path::PathBuf::from(path);
    let bytes =
        std::fs::read(&path).map_err(|e| format!("cannot read fixture {}: {e}", path.display()))?;
    mote_ui::validate(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid fixture {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn args(values: &[&str]) -> impl Iterator<Item = OsString> {
        values
            .iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn no_arguments_keeps_stream_mode() {
        assert_eq!(load(args(&[])).unwrap(), None);
    }

    #[test]
    fn gallery_mode_opens_without_a_fixture_path() {
        assert!(load(args(&["--gallery"])).unwrap().is_some());
        assert!(load(args(&["--gallery", "extra"]))
            .unwrap_err()
            .contains("usage:"));
    }

    #[test]
    fn gallery_sections_cover_every_surface_variant() {
        fn collect(node: &Node, found: &mut BTreeSet<&'static str>) {
            match node {
                Node::Text { .. } => {
                    found.insert("Text");
                }
                Node::Stack { children } => {
                    found.insert("Stack");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::Row { children } => {
                    found.insert("Row");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::List { children } => {
                    found.insert("List");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::Table { .. } => {
                    found.insert("Table");
                }
                Node::Field { .. } => {
                    found.insert("Field");
                }
                Node::Toggle { .. } => {
                    found.insert("Toggle");
                }
                Node::Select { .. } => {
                    found.insert("Select");
                }
                Node::Action { .. } => {
                    found.insert("Action");
                }
                Node::Progress { .. } => {
                    found.insert("Progress");
                }
                Node::Chat { children } => {
                    found.insert("Chat");
                    for child in children {
                        collect(child, found);
                    }
                }
            }
        }

        let gallery = load(args(&["--gallery"])).unwrap().unwrap();
        let Node::Stack { children } = &gallery else {
            panic!("gallery must have one page");
        };
        let headings: BTreeSet<&str> = children
            .iter()
            .filter_map(|node| match node {
                Node::Text { value } => Some(value.as_str()),
                _ => None,
            })
            .collect();
        let expected = [
            "Text", "Stack", "Row", "List", "Table", "Field", "Toggle", "Select", "Action",
            "Progress", "Chat",
        ];
        for heading in expected {
            assert!(
                headings.contains(heading),
                "missing labeled {heading} section"
            );
        }
        let mut found = BTreeSet::new();
        collect(&gallery, &mut found);
        assert_eq!(found, expected.into_iter().collect());
        let encoded = serde_json::to_vec(&gallery).unwrap();
        assert_eq!(mote_ui::validate(&encoded).unwrap(), gallery);
    }

    #[test]
    fn fixture_mode_validates_the_existing_tree() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/mote-ui/fixtures/10_chat_progress.json"
        );
        assert_eq!(
            load(args(&["--fixture", path])).unwrap(),
            Some(Node::Chat {
                children: vec![
                    Node::Text {
                        value: "working".into()
                    },
                    Node::Progress { value: 0.42 }
                ],
            })
        );
    }

    #[test]
    fn malformed_and_missing_files_fail_with_diagnostics() {
        let source = concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs");
        assert!(load(args(&["--fixture", source]))
            .unwrap_err()
            .contains("not valid JSON"));
        assert!(load(args(&["--fixture", "/nonexistent/mote-fixture.json"]))
            .unwrap_err()
            .contains("cannot read"));
    }

    #[test]
    fn invalid_arguments_fail_with_usage() {
        for values in [
            &["--fixture"][..],
            &["--other"][..],
            &["--fixture", "file", "extra"][..],
        ] {
            assert!(load(args(values)).unwrap_err().contains("usage:"));
        }
    }
}
