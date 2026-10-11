use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn mcp_exposes_browser_tools_without_raw_execution_or_approval() {
    let data = std::env::temp_dir().join(format!("mote-browser-mcp-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_moted"))
        .arg("mcp")
        .env("MOTE_HOME", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<_> = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for tool in [
        "open",
        "snapshot",
        "text",
        "find",
        "click",
        "type",
        "select",
        "back",
        "scroll",
        "screenshot",
        "fetch",
    ] {
        assert!(
            names.contains(&format!("browser.{tool}").as_str()),
            "missing browser.{tool}"
        );
    }
    assert!(!names.iter().any(|name| name.contains("approve")
        || name.contains("evaluate")
        || name.contains("cdp")
        || name.contains("configure")));
    std::fs::remove_dir_all(data).unwrap();
}

#[cfg(feature = "test-fixture")]
mod live {
    use super::*;
    use mote_ipc::Dispatch;
    use mote_protocol::{
        ApprovalDecision, ApprovalVerdict, ClientFrame, Cursor, HostFrame, PendingApproval,
        RequestId,
    };
    static LIVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
    fn paired_browser_approval_is_inert_until_one_exact_decision() {
        let _guard = LIVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
        let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
        let models = mote_testkit::ScriptedServer::start().unwrap();
        let host = moted::test_fixture::start(&models).unwrap();
        let mut ipad = host
            .pair(&host.open_pairing().unwrap(), "Browser approval iPad")
            .unwrap();
        let mut other = host
            .pair(&host.open_pairing().unwrap(), "Undelivered device")
            .unwrap();
        host.daemon().call("browser.configure", json!({"thread":"main","executable":std::env::var("MOTE_CHROME").unwrap(),"origins":[fixture.base_url()],"read_paths":["/read","/injection","/navigation","/favicon.ico"]})).unwrap();
        let opened = host
            .daemon()
            .call(
                "browser.open",
                json!({"thread":"main","url":fixture.url("/read")}),
            )
            .unwrap();
        let nodes = opened["snapshot"]["nodes"].as_array().unwrap();
        let button =
            nodes.iter().find(|node| node["role"] == "button").unwrap()["reference"].clone();
        let field =
            nodes.iter().find(|node| node["role"] == "textbox").unwrap()["reference"].clone();
        host.daemon()
            .call(
                "browser.type",
                json!({"thread":"main","reference":field,"value":"Fixture owner"}),
            )
            .unwrap();
        let cursor = host.events_after(0).unwrap().last().unwrap().seq;
        ipad.send(&ClientFrame::Subscribe {
            request_id: RequestId::new("approval-stream").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(host.host_id().clone(), cursor).unwrap(),
        })
        .unwrap();
        host.daemon()
            .call("browser.click", json!({"thread":"main","reference":button}))
            .unwrap();
        assert_eq!(fixture.count("submit"), 0);
        let record: PendingApproval = loop {
            let HostFrame::Event { event, .. } = ipad.receive().unwrap() else {
                panic!("streamed card")
            };
            if event.kind == "approval.created" {
                assert_eq!(event.payload["host_id"], json!(host.host_id()));
                assert_eq!(event.payload["status"], "pending");
                break serde_json::from_value(event.payload["approval"].clone()).unwrap();
            }
        };
        host.reconnect(&mut ipad).unwrap();
        let decision = ApprovalDecision {
            host_id: host.host_id().clone(),
            thread: "main".into(),
            approval_id: record.approval_id.clone(),
            action_hash: record.action_hash.clone(),
            expires_at: record.expires_at,
            decision: ApprovalVerdict::Approve,
        };
        assert!(matches!(
            other
                .exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new("undelivered").unwrap(),
                    decision: decision.clone()
                })
                .unwrap(),
            HostFrame::Error { .. }
        ));
        assert_eq!(fixture.count("submit"), 0);
        for (name, altered) in [
            ("wrong-thread", {
                let mut changed = decision.clone();
                changed.thread = "foreign".into();
                changed
            }),
            ("wrong-host", {
                let mut changed = decision.clone();
                changed.host_id = mote_protocol::HostId::from_public_key(b"foreign");
                changed
            }),
            ("wrong-action", {
                let mut changed = decision.clone();
                changed.action_hash = mote_protocol::ActionDigest::new("0".repeat(64)).unwrap();
                changed
            }),
        ] {
            assert!(matches!(
                ipad.exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new(name).unwrap(),
                    decision: altered
                }),
                Ok(HostFrame::Error { .. }) | Err(_)
            ));
            host.reconnect(&mut ipad).unwrap();
        }
        assert_eq!(fixture.count("submit"), 0);
        assert!(matches!(
            ipad.exchange(&ClientFrame::ApprovalDecision {
                request_id: RequestId::new("approve").unwrap(),
                decision: decision.clone()
            })
            .unwrap(),
            HostFrame::Result { .. }
        ));
        assert_eq!(fixture.count("submit"), 1);
        let _ = ipad
            .exchange(&ClientFrame::ApprovalDecision {
                request_id: RequestId::new("repeat").unwrap(),
                decision,
            })
            .unwrap();
        assert_eq!(fixture.count("submit"), 1);
        let events = host.events_after(0).unwrap();
        assert!(events.iter().any(|event| event.kind == "browser_step"));
        let result = events
            .iter()
            .rev()
            .find(|event| event.kind == "ui.surface")
            .unwrap();
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&result.payload).unwrap()).unwrap();
        assert!(result
            .payload
            .to_string()
            .contains("Approved request completed"));
        assert!(result.payload.to_string().contains("POST"));
        assert!(result.payload.to_string().contains("HTTP 200"));
        assert!(result.payload.to_string().contains(&fixture.base_url()));
        let database = rusqlite::Connection::open(host.root().join("data/mote.db")).unwrap();
        let ledger: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM tool_ledger WHERE action LIKE 'browser.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            ledger,
            events
                .iter()
                .filter(|event| event.kind == "browser_step")
                .count() as i64
        );
        let artifacts: Vec<_> = std::fs::read_dir(host.root().join("data/browser-evidence"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(
            artifacts
                .iter()
                .filter(|path| path.extension().is_some_and(|value| value == "png"))
                .count(),
            2
        );
        let har = artifacts
            .iter()
            .find(|path| path.extension().is_some_and(|value| value == "har"))
            .unwrap();
        let har = std::fs::read_to_string(har).unwrap();
        assert!(!har.contains("Fixture owner"));
        assert!(har.contains("[redacted]"));
        assert!(
            events
                .iter()
                .any(|event| event.kind == "approval.updated"
                    && event.payload["status"] == "approved")
        );
        for verdict in [ApprovalVerdict::Approve, ApprovalVerdict::Deny] {
            let approved = verdict == ApprovalVerdict::Approve;
            let target = host.root().join(if approved {
                "approved.txt"
            } else {
                "denied.txt"
            });
            let id = host
                .daemon()
                .call(
                    "file.write",
                    json!({"thread":"main","path":target,"content":"legacy contents"}),
                )
                .unwrap()["id"]
                .as_i64()
                .unwrap();
            let HostFrame::Result { result, .. } = ipad
                .exchange(&ClientFrame::Call {
                    request_id: RequestId::new(format!("file-snapshot-{id}")).unwrap(),
                    method: "snapshot".into(),
                    params: json!({}),
                })
                .unwrap()
            else {
                panic!("file snapshot")
            };
            let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(result).unwrap();
            let record = snapshot
                .pending_approvals
                .iter()
                .find(|record| record.approval_id.as_str() == id.to_string())
                .unwrap();
            assert_eq!(record.kind, "file_write");
            let decision = ApprovalDecision {
                host_id: host.host_id().clone(),
                thread: "main".into(),
                approval_id: record.approval_id.clone(),
                action_hash: record.action_hash.clone(),
                expires_at: record.expires_at,
                decision: verdict,
            };
            assert!(matches!(
                ipad.exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new(format!("file-decision-{id}")).unwrap(),
                    decision
                })
                .unwrap(),
                HostFrame::Result { .. }
            ));
            assert_eq!(target.exists(), approved);
            if approved {
                assert_eq!(std::fs::read_to_string(target).unwrap(), "legacy contents");
            }
        }
    }

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
    fn generated_but_undelivered_snapshot_cannot_authorize_its_copied_card() {
        let _guard = LIVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
        let models = mote_testkit::ScriptedServer::start().unwrap();
        let host = moted::test_fixture::start(&models).unwrap();
        let mut ipad = host
            .pair(&host.open_pairing().unwrap(), "Undelivered snapshot")
            .unwrap();
        host.daemon().call("browser.configure",json!({"thread":"main","executable":std::env::var("MOTE_CHROME").unwrap(),"origins":[fixture.base_url()],"read_paths":["/read","/favicon.ico"]})).unwrap();
        host.daemon()
            .call(
                "browser.open",
                json!({"thread":"main","url":fixture.url("/read")}),
            )
            .unwrap();
        host.daemon()
            .call(
                "browser.fetch",
                json!({"thread":"main","url":fixture.url("/effect/undelivered"),"method":"POST"}),
            )
            .unwrap();
        let db = rusqlite::Connection::open(host.root().join("data/mote.db")).unwrap();
        let public: Vec<u8> = db
            .query_row(
                "SELECT public_key FROM gateway_devices WHERE name='Undelivered snapshot'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        use sha2::Digest;
        let device = format!("{:x}", sha2::Sha256::digest(public));
        let actor = mote_ipc::Actor::PairedDevice {
            host_id: host.host_id().clone(),
            device_id: device,
        };
        let generated = host
            .daemon()
            .call_with_context(&actor, "snapshot", json!({}))
            .unwrap();
        let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(generated).unwrap();
        let card = &snapshot.pending_approvals[0];
        let decision = ApprovalDecision {
            host_id: host.host_id().clone(),
            thread: card.thread.clone(),
            approval_id: card.approval_id.clone(),
            action_hash: card.action_hash.clone(),
            expires_at: card.expires_at,
            decision: ApprovalVerdict::Approve,
        };
        assert!(matches!(
            ipad.exchange(&ClientFrame::ApprovalDecision {
                request_id: RequestId::new("copied-card").unwrap(),
                decision
            })
            .unwrap(),
            HostFrame::Error { .. }
        ));
        assert_eq!(fixture.count("undelivered"), 0);
    }

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
    fn persistence_failures_preserve_accounting_before_and_after_release() {
        let _guard = LIVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        for (failure, effects, statuses) in [
            ("ledger", 0, vec![]),
            ("step", 0, vec!["failed"]),
            ("complete", 1, vec!["indeterminate"]),
            ("event", 1, vec!["indeterminate"]),
        ] {
            let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
            let models = mote_testkit::ScriptedServer::start().unwrap();
            let host = moted::test_fixture::start(&models).unwrap();
            let mut ipad = host
                .pair(&host.open_pairing().unwrap(), "Accounting failure")
                .unwrap();
            host.daemon().call("browser.configure",json!({"thread":"main","executable":std::env::var("MOTE_CHROME").unwrap(),"origins":[fixture.base_url()],"read_paths":["/read","/favicon.ico"]})).unwrap();
            host.daemon()
                .call(
                    "browser.open",
                    json!({"thread":"main","url":fixture.url("/read")}),
                )
                .unwrap();
            host.daemon().call("browser.fetch",json!({"thread":"main","url":fixture.url("/effect/accounting"),"method":"POST"})).unwrap();
            let HostFrame::Result { result, .. } = ipad
                .exchange(&ClientFrame::Call {
                    request_id: RequestId::new("snapshot").unwrap(),
                    method: "snapshot".into(),
                    params: json!({}),
                })
                .unwrap()
            else {
                panic!("snapshot")
            };
            let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(result).unwrap();
            let card = &snapshot.pending_approvals[0];
            let decision = ApprovalDecision {
                host_id: host.host_id().clone(),
                thread: card.thread.clone(),
                approval_id: card.approval_id.clone(),
                action_hash: card.action_hash.clone(),
                expires_at: card.expires_at,
                decision: ApprovalVerdict::Approve,
            };
            let db = rusqlite::Connection::open(host.root().join("data/mote.db")).unwrap();
            db.execute_batch(match failure {
                "ledger" => "CREATE TRIGGER fail_accounting BEFORE INSERT ON tool_ledger WHEN NEW.action='browser.approved_request' BEGIN SELECT RAISE(FAIL,'injected ledger failure'); END;",
                "step" => "CREATE TRIGGER fail_accounting BEFORE INSERT ON events WHEN NEW.kind='browser_step' BEGIN SELECT RAISE(FAIL,'injected step failure'); END;",
                "complete" => "CREATE TRIGGER fail_terminal BEFORE UPDATE ON approvals WHEN NEW.status='approved' BEGIN SELECT RAISE(FAIL,'injected completion failure'); END;",
                _ => "CREATE TRIGGER fail_terminal BEFORE INSERT ON events WHEN NEW.kind='approval.updated' BEGIN SELECT RAISE(FAIL,'injected event failure'); END;",
            }).unwrap();
            let public: Vec<u8> = db
                .query_row(
                    "SELECT public_key FROM gateway_devices WHERE name='Accounting failure'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            use sha2::Digest;
            let actor = mote_ipc::Actor::PairedDevice {
                host_id: host.host_id().clone(),
                device_id: format!("{:x}", sha2::Sha256::digest(public)),
            };
            let release_error = host
                .daemon()
                .call_with_context(
                    &actor,
                    "approval.decision",
                    serde_json::to_value(&decision).unwrap(),
                )
                .unwrap_err();
            assert_eq!(fixture.count("accounting"), effects, "{failure}");
            let attempts: Vec<String> = db
                .prepare("SELECT status FROM tool_ledger WHERE action='browser.approved_request'")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let approval_status: String = db
                .query_row(
                    "SELECT status FROM approvals WHERE id=?1",
                    [card.approval_id.as_str()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                attempts, statuses,
                "{failure}: {release_error:?}; approval={approval_status}"
            );
            if effects == 0 {
                assert!(
                    host.events_after(0).unwrap().iter().any(|event| event.kind
                        == "approval.updated"
                        && event.payload["approval"]["approval_id"] == card.approval_id.as_str()
                        && event.payload["status"] == "failed"),
                    "failed attempt must terminate its delivered card: {failure}: {release_error:?}; approval={approval_status}"
                );
            }
            assert_eq!(
                host.events_after(0)
                    .unwrap()
                    .iter()
                    .any(|event| event.kind == "browser_step"
                        && event.payload["action"] == "browser.approved_request"),
                effects > 0
            );
            let _ = ipad
                .exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new("repeated-release").unwrap(),
                    decision,
                })
                .unwrap();
            assert_eq!(fixture.count("accounting"), effects, "{failure}");
            host.daemon().call("browser.close", json!({})).unwrap();
        }
    }

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
    fn denial_expiry_navigation_page_change_and_restart_cannot_release() {
        let _guard = LIVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
        let models = mote_testkit::ScriptedServer::start().unwrap();
        let host = moted::test_fixture::start(&models).unwrap();
        let mut ipad = host
            .pair(&host.open_pairing().unwrap(), "Invalidation iPad")
            .unwrap();
        let configure = json!({"thread":"main","executable":std::env::var("MOTE_CHROME").unwrap(),"origins":[fixture.base_url()],"read_paths":["/read","/navigation","/injection","/favicon.ico"]});
        host.daemon()
            .call("browser.configure", configure.clone())
            .unwrap();
        for mode in ["denied", "expiry", "navigation", "page-change", "restart"] {
            let opened = host
                .daemon()
                .call(
                    "browser.open",
                    json!({"thread":"main","url":fixture.url("/read")}),
                )
                .unwrap();
            host.daemon().call("browser.fetch",json!({"thread":"main","url":fixture.url(&format!("/effect/{mode}")),"method":"POST","body":"secret=private-body"})).unwrap();
            let HostFrame::Result { result, .. } = ipad
                .exchange(&ClientFrame::Call {
                    request_id: RequestId::new(format!("snapshot-{mode}")).unwrap(),
                    method: "snapshot".into(),
                    params: json!({}),
                })
                .unwrap()
            else {
                panic!("snapshot")
            };
            let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(result).unwrap();
            let record = snapshot
                .pending_approvals
                .iter()
                .find(|record| record.kind == "browser")
                .unwrap();
            assert_eq!(record.payload["site"], fixture.base_url());
            assert!(record.payload["effect"].is_string());
            assert!(!serde_json::to_string(&record.payload)
                .unwrap()
                .contains("private-body"));
            let decision = ApprovalDecision {
                host_id: host.host_id().clone(),
                thread: "main".into(),
                approval_id: record.approval_id.clone(),
                action_hash: record.action_hash.clone(),
                expires_at: record.expires_at,
                decision: if mode == "denied" {
                    ApprovalVerdict::Deny
                } else {
                    ApprovalVerdict::Approve
                },
            };
            match mode {
                "expiry" => {
                    let db = rusqlite::Connection::open(host.root().join("data/mote.db")).unwrap();
                    db.execute(
                        "UPDATE approvals SET expires_at=0 WHERE id=?1",
                        [record.approval_id.as_str()],
                    )
                    .unwrap();
                }
                "navigation" => {
                    host.daemon()
                        .call(
                            "browser.open",
                            json!({"thread":"main","url":fixture.url("/navigation")}),
                        )
                        .unwrap();
                }
                "page-change" => {
                    let field = opened["snapshot"]["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|node| node["role"] == "textbox")
                        .unwrap()["reference"]
                        .clone();
                    host.daemon()
                        .call(
                            "browser.type",
                            json!({"thread":"main","reference":field,"value":"changed"}),
                        )
                        .unwrap();
                }
                "restart" => {
                    host.daemon().call("browser.close", json!({})).unwrap();
                    host.daemon()
                        .call("browser.configure", configure.clone())
                        .unwrap();
                }
                _ => {}
            }
            let result = ipad
                .exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new(format!("decision-{mode}")).unwrap(),
                    decision,
                })
                .unwrap();
            assert!(
                !matches!(result,HostFrame::Result { result,.. } if result["status"]=="approved"),
                "{mode}"
            );
            assert_eq!(fixture.count(mode), 0, "{mode}");
        }
    }

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and pinned MOTE_CHROME"]
    fn page_instructions_are_inert_and_all_observed_triggers_need_cards() {
        let _guard = LIVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let fixture = mote_testkit::browser_fixture::BrowserFixture::start().unwrap();
        let models = mote_testkit::ScriptedServer::start().unwrap();
        let host = moted::test_fixture::start(&models).unwrap();
        let mut ipad = host
            .pair(&host.open_pairing().unwrap(), "Trigger iPad")
            .unwrap();
        let effects = [
            "submit", "purchase", "message", "download", "oauth", "nonget",
        ];
        let mut reads = vec![
            "/read".to_string(),
            "/injection".to_string(),
            "/favicon.ico".to_string(),
        ];
        reads.extend(effects.iter().map(|effect| format!("/trigger/{effect}")));
        host.daemon().call("browser.configure",json!({"thread":"main","executable":std::env::var("MOTE_CHROME").unwrap(),"origins":[fixture.base_url()],"read_paths":reads})).unwrap();
        let page = host
            .daemon()
            .call(
                "browser.open",
                json!({"thread":"main","url":fixture.url("/injection")}),
            )
            .unwrap();
        assert!(page["snapshot"]["text"]
            .as_str()
            .unwrap()
            .contains("<untrusted_browser_page>"));
        assert!(page.to_string().contains("Ignore previous instructions"));
        assert_eq!(fixture.count("purchase"), 0);
        assert!(!host
            .events_after(0)
            .unwrap()
            .iter()
            .any(|event| event.kind == "approval.created"));
        assert!(host
            .daemon()
            .call(
                "browser.open",
                json!({"thread":"main","url":"http://127.0.0.1:1/forbidden"})
            )
            .is_err());
        for effect in effects {
            host.daemon()
                .call(
                    "browser.open",
                    json!({"thread":"main","url":fixture.url(&format!("/trigger/{effect}"))}),
                )
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let record = loop {
                let HostFrame::Result { result, .. } = ipad
                    .exchange(&ClientFrame::Call {
                        request_id: RequestId::new(format!(
                            "snapshot-{effect}-{}",
                            host.events_after(0).unwrap().len()
                        ))
                        .unwrap(),
                        method: "snapshot".into(),
                        params: json!({}),
                    })
                    .unwrap()
                else {
                    panic!("snapshot")
                };
                let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(result).unwrap();
                if let Some(record) = snapshot
                    .pending_approvals
                    .into_iter()
                    .find(|record| record.kind == "browser")
                {
                    break record;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "{effect} card missing"
                );
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            assert_eq!(fixture.count(effect), 0);
            let decision = ApprovalDecision {
                host_id: host.host_id().clone(),
                thread: "main".into(),
                approval_id: record.approval_id,
                action_hash: record.action_hash,
                expires_at: record.expires_at,
                decision: ApprovalVerdict::Deny,
            };
            let result = ipad
                .exchange(&ClientFrame::ApprovalDecision {
                    request_id: RequestId::new(format!("deny-{effect}")).unwrap(),
                    decision,
                })
                .unwrap();
            assert!(matches!(result,HostFrame::Result { result,.. } if result["status"]=="denied"));
            assert_eq!(fixture.count(effect), 0);
        }
    }
}
