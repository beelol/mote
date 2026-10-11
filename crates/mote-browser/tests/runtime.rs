use mote_browser::{
    ApprovedOrigins, BrowserEffect, BrowserPolicy, LaunchConfig, NodeRef, SnapshotMap,
};
use std::path::Path;

#[test]
#[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
fn retained_native_request_is_inert_stale_checked_and_consumed_once() {
    use mote_browser::{BrowserCommand, BrowserHost, BrowserResult};
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    let data = tempfile::tempdir().unwrap();
    let browser =
        BrowserHost::launch(
            LaunchConfig::new(
                data.path(),
                "continuation",
                Path::new(&std::env::var("MOTE_CHROME").unwrap()),
            )
            .unwrap(),
            BrowserPolicy::new(ApprovedOrigins::new([fixture.base_url()]).unwrap())
                .with_read_paths(["/read", "/navigation", "/favicon.ico"]),
        )
        .unwrap();
    let session = browser.session();
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/read"),
        })
        .unwrap();
    let BrowserResult::Snapshot(snapshot) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot")
    };
    let button = snapshot
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .unwrap()
        .reference
        .clone();
    let field = snapshot
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .unwrap()
        .reference
        .clone();
    let BrowserResult::CommitRequired { intent } = session
        .command(BrowserCommand::Prepare {
            reference: button.clone(),
        })
        .unwrap()
    else {
        panic!("retained request")
    };
    assert_eq!(fixture.count("submit"), 0);
    assert_eq!(intent.page_state_hash, snapshot.page_state_hash);
    session
        .command(BrowserCommand::Fill {
            reference: field,
            value: "altered".into(),
        })
        .unwrap();
    assert!(session.command(BrowserCommand::Release { intent }).is_err());
    assert_eq!(fixture.count("submit"), 0);
    let BrowserResult::CommitRequired { intent } = session
        .command(BrowserCommand::Prepare { reference: button })
        .unwrap()
    else {
        panic!("retained request")
    };
    assert_eq!(fixture.count("submit"), 0);
    session
        .command(BrowserCommand::Release {
            intent: intent.clone(),
        })
        .unwrap();
    assert_eq!(fixture.count("submit"), 1);
    assert!(session.command(BrowserCommand::Release { intent }).is_err());
    assert_eq!(fixture.count("submit"), 1);
}

#[test]
fn launch_is_headed_private_and_cannot_attach() {
    let config = LaunchConfig::new(
        Path::new("/tmp/mote-data"),
        "mote",
        Path::new("/tmp/chrome"),
    )
    .unwrap();
    let args = config.arguments();
    assert!(args.contains(&"--user-data-dir=/tmp/mote-data/browser/mote/local".to_string()));
    assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
    assert!(args.contains(&"--remote-debugging-port=0".to_string()));
    assert!(args.contains(&"--use-mock-keychain".to_string()));
    assert!(args.contains(&"--app=about:blank".to_string()));
    assert!(!args.iter().any(|arg| arg.contains("headless")));
    assert!(LaunchConfig::new(
        Path::new("/tmp/mote"),
        "../Default",
        Path::new("/tmp/chrome")
    )
    .is_err());
    assert!(LaunchConfig::new(
        Path::new("/Users/owner/Library/Application Support/Google/Chrome"),
        "mote",
        Path::new("/tmp/chrome")
    )
    .is_err());
    assert!(config
        .verify_version("Google Chrome for Testing 145.0.1.1")
        .is_err());
    assert!(config
        .verify_version("Google Chrome for Testing 155.0.8059.39")
        .is_ok());
}

#[test]
fn origin_and_effect_policy_fail_closed() {
    let origins = ApprovedOrigins::new(["http://127.0.0.1:1234"]).unwrap();
    let policy = BrowserPolicy::new(origins).with_read_paths(["/read"]);
    assert_eq!(
        policy.request("http://127.0.0.1:1234/read", "GET"),
        BrowserEffect::Read
    );
    assert_eq!(
        policy.request("http://127.0.0.1:5678/read", "GET"),
        BrowserEffect::Blocked
    );
    for path in [
        "submit", "purchase", "message", "download", "oauth", "non-get",
    ] {
        assert_eq!(
            policy.request(&format!("http://127.0.0.1:1234/{path}"), "GET"),
            BrowserEffect::Commit
        );
    }
    for method in ["POST", "PUT", "DELETE", "PATCH", "garbage"] {
        assert_eq!(
            policy.request("http://127.0.0.1:1234/read", method),
            BrowserEffect::Commit
        );
    }
    for verdict in [
        "{}",
        "bad",
        r#"{"effect":"read","extra":true}"#,
        r#"{"effect":"unknown"}"#,
    ] {
        assert_eq!(policy.verdict(verdict), BrowserEffect::Commit);
    }
    assert_eq!(policy.verdict(r#"{"effect":"fill"}"#), BrowserEffect::Fill);
    assert_eq!(
        policy.response(
            "http://127.0.0.1:1234/read",
            Some("attachment; filename=proof.txt")
        ),
        BrowserEffect::Commit
    );
    assert_eq!(
        policy.request("http://127.0.0.1:1234/unknown-effect", "GET"),
        BrowserEffect::Commit
    );
    assert_eq!(
        policy.request("http://127.0.0.1:1234/read?operation=send", "GET"),
        BrowserEffect::Commit
    );
}

#[test]
fn references_are_stable_and_navigation_invalidates_them() {
    let mut map = SnapshotMap::default();
    assert!(map.resolve(&NodeRef("1:1".into())).is_err());
    map.begin_document();
    let first = map.reference(42);
    assert_eq!(map.reference(42), first);
    assert_eq!(map.resolve(&first).unwrap(), 42);
    map.begin_document();
    assert!(map.resolve(&first).is_err());
    assert_ne!(map.reference(42), first);
}

#[test]
fn references_do_not_cross_sessions() {
    let mut first = SnapshotMap::default();
    let mut second = SnapshotMap::default();
    first.begin_document();
    second.begin_document();
    let old = first.reference(42);
    let new = second.reference(42);
    assert_ne!(old, new);
    assert!(second.resolve(&old).is_err());
}

#[test]
fn local_fixture_reports_each_protected_effect() {
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    assert!(ureq::get(&fixture.url("/read"))
        .call()
        .unwrap()
        .into_string()
        .unwrap()
        .contains("Fixture read"));
    assert_eq!(fixture.count("submit"), 0);
    ureq::post(&fixture.url("/effect/submit"))
        .send_string("secret=value")
        .unwrap();
    assert_eq!(fixture.count("submit"), 1);
    assert!(ureq::get(&fixture.url("/injection"))
        .call()
        .unwrap()
        .into_string()
        .unwrap()
        .contains("Ignore previous instructions"));
}

#[test]
fn evidence_redacts_credentials_and_form_bodies() {
    let mut evidence = mote_browser::BrowserEvidence::default();
    evidence.record("http://127.0.0.1:1234/read?token=secret", "POST", serde_json::json!({"Cookie":"session=secret","Authorization":"Bearer secret","X-Test":"ok"}), Some("password=secret"), "blocked");
    let har = evidence.har().to_string();
    assert!(!har.contains("secret"));
    assert!(har.contains("[redacted]"));
    assert!(har.contains("X-Test"));
    assert_eq!(
        evidence.har()["log"]["entries"][0]["request"]["method"],
        "POST"
    );
    assert_eq!(
        evidence.har()["log"]["entries"][0]["request"]["postData"]["text"],
        "[redacted]"
    );
    assert!(evidence.har()["log"]["entries"][0]["response"]["content"].is_object());
}

#[test]
#[ignore = "requires MOTE_CHROME pointing to pinned headed Chrome for Testing"]
fn headed_runtime_reads_fills_and_blocks_effects() {
    use mote_browser::{BrowserCommand, BrowserHost, BrowserResult};
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = LaunchConfig::new(
        data.path(),
        "test",
        Path::new(&std::env::var("MOTE_CHROME").unwrap()),
    )
    .unwrap();
    let profile = config.profile().to_path_buf();
    let policy = BrowserPolicy::new(ApprovedOrigins::new([fixture.base_url()]).unwrap())
        .with_read_paths([
            "/read",
            "/navigation",
            "/same-document",
            "/redirect",
            "/off-origin",
            "/injection",
            "/trigger/submit",
            "/trigger/purchase",
            "/trigger/message",
            "/trigger/download",
            "/trigger/oauth",
            "/trigger/non-get",
            "/trigger/unknown",
            "/trigger/websocket",
            "/trigger/popup",
            "/trigger/frame",
            "/trigger/worker",
            "/trigger/serviceworker",
            "/favicon.ico",
        ]);
    let browser = BrowserHost::launch(config, policy).unwrap();
    eprintln!(
        "Browser process arguments: {}",
        serde_json::to_string(browser.arguments()).unwrap()
    );
    let session = browser.session();
    assert!(session
        .command(BrowserCommand::Fill {
            reference: NodeRef("0:1".into()),
            value: "text".into()
        })
        .is_err());
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/read"),
        })
        .unwrap();
    let BrowserResult::Snapshot(snapshot) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert!(snapshot.text.contains("Fixture read"));
    let heading = snapshot
        .nodes
        .iter()
        .find(|node| node.role == "heading")
        .unwrap()
        .reference
        .clone();
    let BrowserResult::Text(text) = session
        .command(BrowserCommand::Read { reference: heading })
        .unwrap()
    else {
        panic!("read expected")
    };
    assert!(text.contains("Fixture read"));
    let BrowserResult::Snapshot(filtered) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: true,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert!(!filtered.nodes.iter().any(|node| node.role == "heading"));
    let field = snapshot
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .unwrap()
        .reference
        .clone();
    session
        .command(BrowserCommand::Fill {
            reference: field.clone(),
            value: "filled".into(),
        })
        .unwrap();
    let BrowserResult::Snapshot(after) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert_eq!(
        after
            .nodes
            .iter()
            .find(|node| node.role == "textbox")
            .unwrap()
            .reference,
        field
    );
    assert!(after
        .nodes
        .iter()
        .any(|node| node.value.as_deref() == Some("filled")));
    let button = after
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .unwrap()
        .reference
        .clone();
    let BrowserResult::CommitRequired { intent } = session
        .command(BrowserCommand::Click {
            reference: button.clone(),
        })
        .unwrap()
    else {
        panic!("inert commit intent expected")
    };
    assert_eq!(intent.origin, fixture.base_url());
    assert_eq!(intent.url, fixture.url("/read"));
    assert_eq!(intent.node_ref, button);
    assert_eq!(intent.page_state_hash, after.page_state_hash);
    session
        .command(BrowserCommand::Fill {
            reference: field.clone(),
            value: "changed".into(),
        })
        .unwrap();
    let BrowserResult::CommitRequired { intent: changed } = session
        .command(BrowserCommand::Click { reference: button })
        .unwrap()
    else {
        panic!("inert commit intent expected")
    };
    assert_ne!(intent.request_fingerprint, changed.request_fingerprint);
    assert_ne!(intent.page_state_hash, changed.page_state_hash);
    assert_eq!(fixture.count("submit"), 0);
    let BrowserResult::Snapshot(capped) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 0,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert!(capped.truncated);
    assert!(capped.text.is_empty());
    assert!(capped.nodes.is_empty());
    assert!(session
        .command(BrowserCommand::Fill {
            reference: field.clone(),
            value: "unseen".into()
        })
        .is_err());
    assert!(session.command(BrowserCommand::Screenshot).is_ok());
    let tokenizer = tiktoken_rs::o200k_base().unwrap();
    for cap in [0, 1, 4, 8, 16, 32, 64, 80, 8192, 10000] {
        let BrowserResult::Snapshot(bounded) = session
            .command(BrowserCommand::Snapshot {
                interactive_only: false,
                token_cap: cap,
            })
            .unwrap()
        else {
            panic!("snapshot expected")
        };
        assert!(
            tokenizer.encode_ordinary(&bounded.text).len() <= cap.min(8192),
            "snapshot exceeds cap {cap}"
        );
        if cap == 16 {
            assert!(bounded.text.contains("snapshot token cap reached"));
        }
    }
    for route in [
        "submit",
        "purchase",
        "message",
        "download",
        "oauth",
        "non-get",
        "unknown",
        "websocket",
        "popup",
        "frame",
        "worker",
        "serviceworker",
    ] {
        session
            .command(BrowserCommand::Navigate {
                url: fixture.url(&format!("/trigger/{route}")),
            })
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(fixture.count(route), 0, "protected {route} request escaped");
    }
    assert!(session
        .command(BrowserCommand::Fill {
            reference: field,
            value: "stale".into()
        })
        .is_err());
    let BrowserResult::Evidence(evidence) = session.command(BrowserCommand::Evidence).unwrap()
    else {
        panic!("evidence expected")
    };
    assert!(evidence
        .entries
        .iter()
        .any(|entry| entry.status == "commit"));
    assert!(!evidence.har().to_string().contains("fixture-secret"));
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/injection"),
        })
        .unwrap();
    let BrowserResult::Snapshot(injection) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert!(injection
        .untrusted_text()
        .contains("Ignore previous instructions"));
    assert_eq!(fixture.count("purchase"), 0);
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/same-document"),
        })
        .unwrap();
    let BrowserResult::Snapshot(before_history) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: true,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    let history_field = before_history
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .unwrap()
        .reference
        .clone();
    session
        .command(BrowserCommand::Fill {
            reference: history_field.clone(),
            value: "change URL".into(),
        })
        .unwrap();
    assert!(session
        .command(BrowserCommand::Read {
            reference: history_field
        })
        .is_err());
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/redirect"),
        })
        .unwrap();
    assert!(session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/off-origin")
        })
        .is_err());
    let mut actual = browser.arguments().to_vec();
    actual.sort();
    let mut expected = vec![
        std::env::var("MOTE_CHROME").unwrap(),
        format!("--user-data-dir={}", profile.display()),
        "--remote-debugging-address=127.0.0.1".into(),
        "--remote-debugging-port=0".into(),
        "--use-mock-keychain".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
        "--disable-sync".into(),
        "--enable-automation".into(),
        "--app=about:blank".into(),
        "--disable-extensions".into(),
        "--flag-switches-begin".into(),
        "--flag-switches-end".into(),
    ];
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and pinned headed Chrome"]
fn host_shutdown_closes_retained_session() {
    use mote_browser::{BrowserCommand, BrowserHost};
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = LaunchConfig::new(
        data.path(),
        "shutdown",
        Path::new(&std::env::var("MOTE_CHROME").unwrap()),
    )
    .unwrap();
    let browser = BrowserHost::launch(
        config,
        BrowserPolicy::new(ApprovedOrigins::new([fixture.base_url()]).unwrap()),
    )
    .unwrap();
    let session = browser.session();
    let (done, result) = std::sync::mpsc::channel();
    let closing = std::thread::spawn(move || {
        drop(browser);
        done.send(()).unwrap();
    });
    let stopped = result.recv_timeout(std::time::Duration::from_secs(2));
    let response = session.command(BrowserCommand::Evidence);
    drop(session);
    closing.join().unwrap();
    assert!(
        stopped.is_ok(),
        "host shutdown waited on a retained session handle"
    );
    assert!(response.is_err());
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and pinned headed Chrome"]
fn browser_session_never_inherits_profile_cookies_or_workers() {
    use chromiumoxide::{Browser, BrowserConfig};
    use futures::StreamExt;
    use mote_browser::{BrowserCommand, BrowserHost, BrowserResult};
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    let data = tempfile::tempdir().unwrap();
    let executable = std::env::var("MOTE_CHROME").unwrap();
    let config = LaunchConfig::new(data.path(), "inheritance", Path::new(&executable)).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut builder = BrowserConfig::builder().chrome_executable(&executable).user_data_dir(config.profile()).with_head().disable_default_args();
        for arg in config.arguments().into_iter().filter(|arg| !arg.starts_with("--user-data-dir=")) {
            let arg = arg.trim_start_matches("--");
            builder = match arg.split_once('=') { Some((key, value)) => builder.arg((key, value)), None => builder.arg(arg) };
        }
        let (mut browser, mut handler) = Browser::launch(builder.build().unwrap()).await.unwrap();
        let pump = tokio::spawn(async move { while let Some(result) = handler.next().await { if result.is_err() { break; } } });
        let page = browser.new_page(fixture.url("/seed-state")).await.unwrap();
        page.execute(chromiumoxide::cdp::browser_protocol::target::SetAutoAttachParams::builder().auto_attach(false).wait_for_debugger_on_start(false).flatten(true).build().unwrap()).await.unwrap();
        page.evaluate("(async()=>{document.cookie='fixture_session=seed; Max-Age=3600; Path=/';await navigator.serviceWorker.register('/seed-worker.js');await Promise.race([navigator.serviceWorker.ready,new Promise((_,reject)=>setTimeout(()=>reject(Error('worker seed timeout')),5000))]);return 'seeded'})()").await.unwrap();
        page.goto(fixture.url("/inherited-state")).await.unwrap();
        assert!(page.content().await.unwrap().contains("inherited worker state"));
        browser.close().await.unwrap();
        pump.abort();
    });
    let policy = BrowserPolicy::new(ApprovedOrigins::new([fixture.base_url()]).unwrap())
        .with_read_paths(["/inherited-state"]);
    let browser = BrowserHost::launch(config, policy).unwrap();
    let session = browser.session();
    session
        .command(BrowserCommand::Navigate {
            url: fixture.url("/inherited-state"),
        })
        .unwrap();
    let BrowserResult::Snapshot(snapshot) = session
        .command(BrowserCommand::Snapshot {
            interactive_only: false,
            token_cap: 2000,
        })
        .unwrap()
    else {
        panic!("snapshot expected")
    };
    assert!(snapshot.text.contains("fresh HTTP state"));
    assert!(
        snapshot.text.contains("cookie absent"),
        "isolated session inherited a profile cookie: {}",
        snapshot.text
    );
    assert!(!snapshot.text.contains("inherited worker state"));
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and pinned headed Chrome"]
fn webrtc_is_unavailable_before_inline_scripts_and_sends_no_datagrams() {
    use mote_browser::{BrowserCommand, BrowserHost, BrowserResult};
    use mote_testkit::browser_fixture::BrowserDatagramFixture;
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
    let datagrams = BrowserDatagramFixture::start().unwrap();
    let route = format!("/webrtc/{}", datagrams.port());
    let data = tempfile::tempdir().unwrap();
    for generation in 0..2 {
        let config = LaunchConfig::new(
            data.path(),
            "webrtc",
            Path::new(&std::env::var("MOTE_CHROME").unwrap()),
        )
        .unwrap();
        let policy = BrowserPolicy::new(ApprovedOrigins::new([fixture.base_url()]).unwrap())
            .with_read_paths([route.clone(), "/read".into()]);
        let browser = BrowserHost::launch(config, policy).unwrap();
        let session = browser.session();
        for navigation in 0..2 {
            session
                .command(BrowserCommand::Navigate {
                    url: fixture.url(&route),
                })
                .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
            assert_eq!(
                datagrams.count(),
                0,
                "WebRTC datagrams escaped in context {generation}, navigation {navigation}"
            );
            let BrowserResult::Snapshot(snapshot) = session
                .command(BrowserCommand::Snapshot {
                    interactive_only: false,
                    token_cap: 2000,
                })
                .unwrap()
            else {
                panic!("snapshot expected")
            };
            assert!(
                snapshot.text.contains("inline blocked"),
                "first inline script retained WebRTC: {}",
                snapshot.text
            );
            assert!(snapshot.text.contains("replacement rejected"));
            assert!(
                snapshot.text.contains("blank frame blocked"),
                "fresh realm retained WebRTC: {}",
                snapshot.text
            );
            session
                .command(BrowserCommand::Navigate {
                    url: fixture.url("/read"),
                })
                .unwrap();
        }
    }
}
