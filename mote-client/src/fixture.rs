use mote_ui::Node;
use std::ffi::OsString;

pub fn load(mut args: impl Iterator<Item = OsString>) -> Result<Option<Node>, String> {
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    let usage = "usage: mote-client [--fixture <json-path>]";
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
