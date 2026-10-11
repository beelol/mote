use mote_host_client::{ConnectionState, HostDirectory, PairedHost};
use mote_protocol::{
    ChatMessage, Cursor, HostFrame, HostSnapshot, HostStatus, MessageRole, MessageStatus,
    RequestId, SnapshotChunk, ThreadSnapshot, VersionRange,
};

fn fixture() -> (
    HostDirectory,
    mote_protocol::HostId,
    HostSnapshot,
    Vec<HostFrame>,
) {
    let paired = PairedHost::new([31; 32], "Home", None, VersionRange::current()).unwrap();
    let host = paired.host_id().clone();
    let mut directory = HostDirectory::default();
    directory.register(paired).unwrap();
    directory.select_host(&host).unwrap();
    let snapshot = HostSnapshot {
        host_id: host.clone(),
        cursor: Cursor::new(host.clone(), 20).unwrap(),
        threads: vec![ThreadSnapshot {
            thread: "A".into(),
            messages: vec![ChatMessage {
                id: "10".into(),
                role: MessageRole::User,
                text: "x".repeat(150_000),
                status: MessageStatus::Read,
            }],
        }],
        surfaces: vec![],
        pending_approvals: vec![],
        status: HostStatus::Online,
    };
    let data = serde_json::to_string(&snapshot).unwrap();
    let chunks = data
        .as_bytes()
        .chunks(60_000)
        .enumerate()
        .map(|(index, bytes)| HostFrame::SnapshotChunk {
            chunk: SnapshotChunk {
                transfer_id: RequestId::new("snapshot-1").unwrap(),
                request_id: None,
                cursor: snapshot.cursor.clone(),
                index: index as u64,
                total_bytes: data.len() as u64,
                data: String::from_utf8(bytes.to_vec()).unwrap(),
            },
        })
        .collect();
    (directory, host, snapshot, chunks)
}

#[test]
fn snapshot_chunks_install_only_after_complete_and_do_not_persist_partial_state() {
    let (mut directory, host, snapshot, chunks) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.json");
    directory
        .receive_frame_durable(&path, &host, chunks[0].clone())
        .unwrap();
    assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
    assert!(directory.state(&host).unwrap().threads().is_empty());
    let mut reopened = HostDirectory::load(&path).unwrap();
    assert!(reopened.receive_frame(&host, chunks[1].clone()).is_err());
    for chunk in &chunks[1..chunks.len() - 1] {
        directory
            .receive_frame_durable(&path, &host, chunk.clone())
            .unwrap();
        assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
    }
    directory
        .receive_frame_durable(&path, &host, chunks.last().unwrap().clone())
        .unwrap();
    let reopened = HostDirectory::load(&path).unwrap();
    assert_eq!(reopened.state(&host).unwrap().cursor(), &snapshot.cursor);
    assert_eq!(
        reopened.state(&host).unwrap().threads()["A"],
        snapshot.threads[0]
    );
    assert_eq!(
        directory.state(&host).unwrap().connection(),
        &ConnectionState::Online
    );
}

#[test]
fn interrupted_out_of_order_or_changed_snapshot_cannot_replace_visible_state() {
    for fault in [
        "order",
        "cursor",
        "host",
        "length",
        "transfer",
        "interleaved",
    ] {
        let (mut directory, host, _, chunks) = fixture();
        directory.receive_frame(&host, chunks[0].clone()).unwrap();
        let HostFrame::SnapshotChunk { mut chunk } = chunks[1].clone() else {
            unreachable!()
        };
        let frame = match fault {
            "order" => {
                chunk.index += 1;
                HostFrame::SnapshotChunk { chunk }
            }
            "cursor" => {
                chunk.cursor.seq += 1;
                HostFrame::SnapshotChunk { chunk }
            }
            "host" => {
                chunk.cursor.host_id = mote_protocol::HostId::from_public_key(&[9; 32]);
                HostFrame::SnapshotChunk { chunk }
            }
            "length" => {
                chunk.total_bytes += 1;
                HostFrame::SnapshotChunk { chunk }
            }
            "transfer" => {
                chunk.transfer_id = RequestId::new("different").unwrap();
                HostFrame::SnapshotChunk { chunk }
            }
            _ => HostFrame::Event {
                host_id: host.clone(),
                event: mote_protocol::Event {
                    seq: 30,
                    ts: 0,
                    scope_id: "A".into(),
                    kind: "turn.user".into(),
                    payload: serde_json::json!({"text":"interleaved"}),
                },
            },
        };
        assert!(
            directory.receive_frame(&host, frame).is_err(),
            "accepted {fault}"
        );
        assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
        assert!(directory.state(&host).unwrap().threads().is_empty());
        directory
            .set_connection(&host, ConnectionState::Offline)
            .unwrap();
        assert!(directory
            .receive_frame(&host, chunks.last().unwrap().clone())
            .is_err());
        for chunk in chunks {
            directory.receive_frame(&host, chunk).unwrap();
        }
        assert_eq!(directory.state(&host).unwrap().cursor().seq, 20);
    }
}
