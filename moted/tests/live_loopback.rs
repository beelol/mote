use mote_ipc::Dispatch;
use mote_protocol::{ClientFrame, HostFrame, HostSnapshot, RequestId};
use mote_testkit::ScriptedServer;
use serde_json::json;

#[test]
fn external_test_pairs_real_daemon_and_receives_chat_and_matrix() {
    let models = ScriptedServer::start().unwrap();
    models.set_chat_chunks(vec![json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string()]);
    let mut host = moted::test_fixture::start(&models).unwrap();
    let mut client = host
        .pair(&host.open_pairing().unwrap(), "External iPad")
        .unwrap();
    let turn = ClientFrame::Call {
        request_id: RequestId::new("turn").unwrap(),
        method: "turn".into(),
        params: json!({"thread":"main","message":"hello"}),
    };
    assert!(
        matches!(client.exchange(&turn).unwrap(), HostFrame::Result { result, .. } if result["status"] == "done")
    );
    host.daemon()
        .call(
            "matrix.render",
            json!({"thread":"main","source":"screen { text \"External fixture\" }"}),
        )
        .unwrap();
    let events = host.events_after(0).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "talk.done" && event.payload["ledger_id"].is_number()));
    assert!(events.iter().any(|event| event.kind == "ui.surface"));
    let snapshot = ClientFrame::Call {
        request_id: RequestId::new("snapshot").unwrap(),
        method: "snapshot".into(),
        params: json!({}),
    };
    let HostFrame::Result { result, .. } = client.exchange(&snapshot).unwrap() else {
        panic!("snapshot result");
    };
    let snapshot: HostSnapshot = serde_json::from_value(result).unwrap();
    assert_eq!(snapshot.host_id, *host.host_id());
    assert_eq!(snapshot.threads[0].messages[0].text, "hello");
    assert_eq!(snapshot.threads[0].messages.len(), 2);
    assert_eq!(snapshot.surfaces.len(), 1);
    assert_eq!(snapshot.surfaces[0].tree["version"], 1);
    assert!(matches!(
        client
            .exchange(&ClientFrame::Call {
                request_id: RequestId::new("forbidden").unwrap(),
                method: "config.set".into(),
                params: json!({"key":"models.local_only","value":false}),
            })
            .unwrap(),
        HostFrame::Error { .. }
    ));
    client.force_disconnect();
    host.rotate_endpoint().unwrap();
    host.reconnect(&mut client).unwrap();
    assert!(matches!(
        client.exchange(&turn).unwrap(),
        HostFrame::Result { .. }
    ));
    assert_eq!(
        host.events_after(0)
            .unwrap()
            .iter()
            .filter(|event| event.kind == "turn.user")
            .count(),
        1
    );
}
