use std::ffi::OsString;

pub use mote_client_core::state::ChatStateFixture;

pub fn chat_state(args: &[OsString]) -> Result<Option<ChatStateFixture>, String> {
    let Some(flag) = args.first() else {
        return Ok(None);
    };
    if flag != "--chat-state" {
        return Ok(None);
    }
    let usage = "usage: mote-client --chat-state <sending|sent|delivered|read|typing>";
    if args.len() != 2 {
        return Err(usage.into());
    }
    let state = match args[1].to_str() {
        Some("sending") => ChatStateFixture::Sending,
        Some("sent") => ChatStateFixture::Sent,
        Some("delivered") => ChatStateFixture::Delivered,
        Some("read") => ChatStateFixture::Read,
        Some("typing") => ChatStateFixture::Typing,
        _ => return Err(usage.into()),
    };
    Ok(Some(state))
}

pub fn load(
    mut args: impl Iterator<Item = OsString>,
) -> Result<Option<crate::contract::Surface>, String> {
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    let usage = "usage: mote-client [--gallery | --fixture <json-path> | --chat-state <state>]";
    if flag == "--gallery" {
        return if args.next().is_none() {
            Ok(Some(crate::gallery::preview()))
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
        .map(crate::contract::Surface::V0)
        .map(Some)
        .map_err(|e| format!("invalid fixture {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_ui::Node;

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
        assert_eq!(
            load(args(&["--gallery"])).unwrap(),
            Some(crate::gallery::preview())
        );
        assert!(load(args(&["--gallery", "extra"]))
            .unwrap_err()
            .contains("usage:"));
    }


    #[test]
    fn fixture_mode_validates_the_existing_tree() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/mote-ui/fixtures/10_chat_progress.json"
        );
        assert_eq!(
            load(args(&["--fixture", path])).unwrap(),
            Some(crate::contract::Surface::V0(Node::Chat {
                children: vec![
                    Node::Text {
                        value: "working".into()
                    },
                    Node::Progress { value: 0.42 }
                ],
            }))
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

    #[test]
    fn chat_state_fixture_accepts_each_v0_message_state() {
        for (state, expected) in [
            ("sending", ChatStateFixture::Sending),
            ("sent", ChatStateFixture::Sent),
            ("delivered", ChatStateFixture::Delivered),
            ("read", ChatStateFixture::Read),
            ("typing", ChatStateFixture::Typing),
        ] {
            let args = [OsString::from("--chat-state"), OsString::from(state)];
            assert_eq!(chat_state(&args).unwrap(), Some(expected));
        }
    }

    #[test]
    fn chat_state_fixture_rejects_unknown_or_extra_arguments() {
        for args in [
            vec![OsString::from("--chat-state")],
            vec![OsString::from("--chat-state"), OsString::from("unknown")],
            vec![
                OsString::from("--chat-state"),
                OsString::from("sent"),
                OsString::from("extra"),
            ],
        ] {
            assert!(chat_state(&args).unwrap_err().contains("usage:"));
        }
    }
}
