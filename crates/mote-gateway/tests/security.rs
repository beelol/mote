use mote_gateway::{
    ClientCredential, DeviceStore, GatewayConfig, HostIdentity, NoiseSession, PairingOffer,
    PendingHandshake,
};
use mote_protocol::{ClientFrame, HostFrame, HostId, RequestId};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    time::Instant,
};

fn pair(config: &GatewayConfig, host: &HostIdentity, store: &DeviceStore) -> ClientCredential {
    let now = Instant::now();
    let mut offer = PairingOffer::open(config, host, now, &[]).unwrap();
    let credential = ClientCredential::from_pairing_code(config, &offer.code()).unwrap();
    let (client, first) = credential.begin_pairing(config, "iPad", 1).unwrap();
    let pending = PendingHandshake::pairing(config, host, &mut offer, &first, now).unwrap();
    let (_, second) = pending.confirm(store, true, now).unwrap();
    client.finish(&second).unwrap();
    credential
}

fn connect(
    config: &GatewayConfig,
    host: &HostIdentity,
    store: &DeviceStore,
    credential: &ClientCredential,
    counter: i64,
) -> (NoiseSession, NoiseSession) {
    let (client, first) = credential.begin_session(config, counter).unwrap();
    let (server, second) = PendingHandshake::session(config, host, store, &first).unwrap();
    (client.finish(&second).unwrap(), server)
}

#[test]
fn identity_is_persistent_private_and_symlink_safe() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("host");
    let host = HostIdentity::load_or_create(&data).unwrap();
    assert_eq!(
        HostIdentity::load_or_create(&data).unwrap().host_id(),
        host.host_id()
    );
    assert_eq!(host.host_id().as_str().len(), 64);
    assert_eq!(host.host_id(), &HostId::from_public_key(host.public_key()));
    assert_eq!(
        std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(data.join("host.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let fake = dir.path().join("fake");
    symlink(&data, &fake).unwrap();
    assert!(HostIdentity::load_or_create(&fake).is_err());
    std::fs::remove_file(data.join("host.key")).unwrap();
    symlink(dir.path().join("missing"), data.join("host.key")).unwrap();
    assert!(HostIdentity::load_or_create(&data).is_err());
}

#[test]
fn replay_and_revocation_survive_store_restart() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let path = config.data_dir.join("devices.db");
    let store = DeviceStore::open(&path, host.host_id()).unwrap();
    let client = pair(&config, &host, &store);
    let (_, first) = client.begin_session(&config, 2).unwrap();
    PendingHandshake::session(&config, &host, &store, &first).unwrap();
    assert!(PendingHandshake::session(&config, &host, &store, &first).is_err());
    drop(store);
    let store = DeviceStore::open(&path, host.host_id()).unwrap();
    assert!(PendingHandshake::session(&config, &host, &store, &first).is_err());
    store.revoke(client.public_key()).unwrap();
    drop(store);
    let store = DeviceStore::open(&path, host.host_id()).unwrap();
    let (_, first) = client.begin_session(&config, 3).unwrap();
    assert!(PendingHandshake::session(&config, &host, &store, &first).is_err());
}

#[test]
fn wrong_host_and_unknown_client_fail_before_transport() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let other = HostIdentity::load_or_create(&dir.path().join("other")).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let mut offer = PairingOffer::open(&config, &host, Instant::now(), &[]).unwrap();
    let unknown = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let (_, first) = unknown.begin_session(&config, 1).unwrap();
    assert!(PendingHandshake::session(&config, &host, &store, &first).is_err());
    let (_, first) = unknown.begin_pairing(&config, "iPad", 1).unwrap();
    assert!(
        PendingHandshake::pairing(&config, &other, &mut offer, &first, Instant::now()).is_err()
    );
    let client = pair(&config, &host, &store);
    let (_, first) = client.begin_session(&config, 2).unwrap();
    assert!(PendingHandshake::session(&config, &other, &store, &first).is_err());
    let other_store =
        DeviceStore::open(&config.data_dir.join("devices.db"), other.host_id()).unwrap();
    assert!(PendingHandshake::session(&config, &host, &other_store, &first).is_err());
}

#[test]
fn encrypted_frames_are_ordered_bound_and_poison_on_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let credential = pair(&config, &host, &store);
    let (mut client, mut server) = connect(&config, &host, &store, &credential, 2);
    let call = ClientFrame::Call {
        request_id: RequestId::new("1").unwrap(),
        method: "state".into(),
        params: serde_json::json!({}),
    };
    let frames = client.write_client(&call).unwrap();
    assert!(!frames[0].windows(5).any(|b| b == b"state"));
    assert_eq!(server.read_client(&frames[0]).unwrap(), Some(call.clone()));
    assert!(server.read_client(&frames[0]).is_err());
    assert!(server
        .write_host(&HostFrame::Result {
            request_id: RequestId::new("1").unwrap(),
            result: serde_json::json!({})
        })
        .is_err());
    let (mut client, mut server) = connect(&config, &host, &store, &credential, 3);
    let first = client.write_client(&call).unwrap();
    let second = client.write_client(&call).unwrap();
    assert!(server.read_client(&second[0]).is_err());
    assert!(server.read_client(&first[0]).is_err());
    let (mut client, mut server) = connect(&config, &host, &store, &credential, 4);
    let mut frames = client.write_client(&call).unwrap();
    frames[0][3] ^= 1;
    assert!(server.read_client(&frames[0]).is_err());
    assert!(server.read_client(&[]).is_err());
    let (_, mut server) = connect(&config, &host, &store, &credential, 5);
    let other = HostId::from_public_key(b"other");
    let wrong = HostFrame::Acknowledgement {
        host_id: other.clone(),
        request_id: RequestId::new("1").unwrap(),
        cursor: mote_protocol::Cursor::new(other, 0).unwrap(),
    };
    assert!(server.write_host(&wrong).is_err());
}

#[test]
fn credentials_restore_and_keys_never_enter_frames() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let client = pair(&config, &host, &store);
    let restored = ClientCredential::restore(
        *client.host_public_key(),
        *client.private_key_for_secure_storage(),
    )
    .unwrap();
    assert_eq!(restored.public_key(), client.public_key());
    let (pending, first) = restored.begin_session(&config, 2).unwrap();
    let (mut server, second) = PendingHandshake::session(&config, &host, &store, &first).unwrap();
    let mut client_session = pending.finish(&second).unwrap();
    let host_secret = std::fs::read(config.data_dir.join("host.key")).unwrap();
    let result = HostFrame::Result {
        request_id: RequestId::new("one").unwrap(),
        result: serde_json::json!({"ok":true}),
    };
    let frames = server.write_host(&result).unwrap();
    assert_eq!(client_session.read_host(&frames[0]).unwrap(), Some(result));
    for bytes in [first, second, frames.concat()] {
        assert!(!bytes
            .windows(32)
            .any(|b| b == host_secret || b == client.private_key_for_secure_storage()));
    }
    assert!(restored.begin_pairing(&config, "iPad", 3).is_err());
}

#[test]
fn simultaneous_key_creation_has_one_complete_identity() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("host");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let (data, barrier) = (data.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                HostIdentity::load_or_create(&data)
                    .unwrap()
                    .host_id()
                    .clone()
            })
        })
        .collect();
    let ids: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(ids.iter().all(|id| id == &ids[0]));
    assert_eq!(std::fs::read_dir(data).unwrap().count(), 1);
}

#[test]
fn session_role_errors_poison_transport() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let credential = pair(&config, &host, &store);
    let (mut client, mut server) = connect(&config, &host, &store, &credential, 2);
    assert!(server.read_host(b"anything").is_err());
    let call = ClientFrame::Call {
        request_id: RequestId::new("1").unwrap(),
        method: "state".into(),
        params: serde_json::json!({}),
    };
    let frames = client.write_client(&call).unwrap();
    assert!(server.read_client(&frames[0]).is_err());
}

#[test]
fn wrong_secret_fails_without_consuming_a_valid_offer() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let now = Instant::now();
    let mut offer = PairingOffer::open(&config, &host, now, &[]).unwrap();
    let code = offer.code();
    let mut bytes = mote_gateway::base32::decode(code.strip_prefix("MOTE1-").unwrap()).unwrap();
    bytes[33] ^= 1;
    let wrong = ClientCredential::from_pairing_code(
        &config,
        &format!("MOTE1-{}", mote_gateway::base32::encode(&bytes)),
    )
    .unwrap();
    let (_, first) = wrong.begin_pairing(&config, "iPad", 1).unwrap();
    assert!(PendingHandshake::pairing(&config, &host, &mut offer, &first, now).is_err());
    assert!(offer.accepts_attempts(now));
    let right = ClientCredential::from_pairing_code(&config, &code).unwrap();
    let (_, first) = right.begin_pairing(&config, "iPad", 1).unwrap();
    PendingHandshake::pairing(&config, &host, &mut offer, &first, now).unwrap();
}

#[test]
fn authenticated_malformed_identity_version_and_counter_fail_closed() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let client = pair(&config, &host, &store);
    let hello = serde_json::json!({"host_id":host.host_id(),"client_id":mote_gateway::noise::hex(&Sha256::digest(client.public_key())),"protocol_major":1,"counter":2,"name":"iPad"});
    let mut variants = vec![];
    for (field, value) in [
        (
            "host_id",
            serde_json::json!(HostId::from_public_key(b"other")),
        ),
        ("client_id", serde_json::json!("wrong")),
        ("protocol_major", serde_json::json!(2)),
        ("counter", serde_json::json!(0)),
        ("counter", serde_json::json!(-1)),
        ("private_key", serde_json::json!("injected")),
    ] {
        let mut bad = hello.clone();
        bad[field] = value;
        variants.push(bad.to_string());
    }
    variants.push(format!("{hello} trailing"));
    for payload in variants {
        let mut noise = snow::Builder::new(mote_gateway::noise::PATTERN_SESSION.parse().unwrap())
            .prologue(&config.prologue)
            .unwrap()
            .local_private_key(client.private_key_for_secure_storage())
            .unwrap()
            .remote_public_key(host.public_key())
            .unwrap()
            .build_initiator()
            .unwrap();
        let mut first = [0; 4096];
        let n = noise.write_message(payload.as_bytes(), &mut first).unwrap();
        assert!(PendingHandshake::session(&config, &host, &store, &first[..n]).is_err());
    }
    connect(&config, &host, &store, &client, 2);
}

#[test]
fn large_frames_join_and_reader_size_failures_close_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let credential = pair(&config, &host, &store);
    let (mut client, mut server) = connect(&config, &host, &store, &credential, 2);
    let call = ClientFrame::Call {
        request_id: RequestId::new("large").unwrap(),
        method: "state".into(),
        params: serde_json::json!({"text":"x".repeat(200_000)}),
    };
    let frames = client.write_client(&call).unwrap();
    assert_eq!(frames.len(), 4);
    for frame in &frames[..3] {
        assert_eq!(server.read_client(frame).unwrap(), None);
    }
    assert_eq!(server.read_client(&frames[3]).unwrap(), Some(call.clone()));
    let (pending, first) = credential.begin_session(&config, 3).unwrap();
    let mut limited = config.clone();
    limited.max_message = 512;
    let (mut server, second) = PendingHandshake::session(&limited, &host, &store, &first).unwrap();
    let mut client = pending.finish(&second).unwrap();
    let frames = client.write_client(&call).unwrap();
    assert!(server.read_client(&frames[0]).is_err());
    assert!(server.read_client(&frames[1]).is_err());
    let result = HostFrame::Result {
        request_id: RequestId::new("oversize").unwrap(),
        result: serde_json::json!("x".repeat(2_000_000)),
    };
    assert!(server.write_host(&result).is_err());
}

#[test]
fn unsafe_or_malformed_persisted_keys_are_refused_without_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("host");
    HostIdentity::load_or_create(&data).unwrap();
    let path = data.join("host.key");
    let original = std::fs::read(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(HostIdentity::load_or_create(&data).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&path, b"short").unwrap();
    assert!(HostIdentity::load_or_create(&data).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"short");
}
