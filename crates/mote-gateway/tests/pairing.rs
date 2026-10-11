use mote_gateway::{
    ClientCredential, DeviceStore, GatewayConfig, HostIdentity, PairingOffer, PendingHandshake,
};
use std::time::{Duration, Instant};

#[test]
fn pairing_pins_identity_and_requires_owner_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let now = Instant::now();
    let mut offer = PairingOffer::open(&config, &host, now, &["127.0.0.1".into()]).unwrap();
    let code = offer.code();
    assert!(code.starts_with("MOTE1-"));
    let client = ClientCredential::from_pairing_code(&config, &code).unwrap();
    assert_eq!(client.host_id(), host.host_id());
    let (pending_client, first) = client.begin_pairing(&config, "iPad", 1).unwrap();
    let pending = PendingHandshake::pairing(&config, &host, &mut offer, &first, now).unwrap();
    assert!(PendingHandshake::pairing(&config, &host, &mut offer, &first, now).is_err());
    assert!(!store.is_active(client.public_key()).unwrap());
    let (_, second) = pending.confirm(&store, true, now).unwrap();
    pending_client.finish(&second).unwrap();
    assert!(store.is_active(client.public_key()).unwrap());
    let mut offer = PairingOffer::open(&config, &host, now, &[]).unwrap();
    let denied = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let (_, first) = denied.begin_pairing(&config, "denied", 1).unwrap();
    let pending = PendingHandshake::pairing(&config, &host, &mut offer, &first, now).unwrap();
    assert!(pending.confirm(&store, false, now).is_err());
    assert!(!store.is_active(denied.public_key()).unwrap());
}

#[test]
fn expiry_and_failure_budget_close_the_offer() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("host"));
    config.pairing_window = Duration::from_millis(10);
    config.confirm_timeout = Duration::from_millis(5);
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let store = DeviceStore::open(&config.data_dir.join("devices.db"), host.host_id()).unwrap();
    let now = Instant::now();
    let mut offer = PairingOffer::open(&config, &host, now, &[]).unwrap();
    let client = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let (_, first) = client.begin_pairing(&config, "iPad", 1).unwrap();
    assert!(PendingHandshake::pairing(
        &config,
        &host,
        &mut offer,
        &first,
        now + config.pairing_window
    )
    .is_err());
    let mut offer = PairingOffer::open(&config, &host, now, &[]).unwrap();
    let client = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let (_, first) = client.begin_pairing(&config, "iPad", 1).unwrap();
    let pending = PendingHandshake::pairing(&config, &host, &mut offer, &first, now).unwrap();
    assert!(pending
        .confirm(&store, true, now + config.confirm_timeout)
        .is_err());
    for _ in 0..5 {
        assert!(PendingHandshake::pairing(&config, &host, &mut offer, b"bad", now).is_err());
    }
    let mut offer = PairingOffer::open(&config, &host, now, &[]).unwrap();
    let client = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    for _ in 0..5 {
        assert!(PendingHandshake::pairing(&config, &host, &mut offer, b"bad", now).is_err());
    }
    let (_, first) = client.begin_pairing(&config, "iPad", 2).unwrap();
    assert!(PendingHandshake::pairing(&config, &host, &mut offer, &first, now).is_err());
}

#[test]
fn pairing_code_parser_refuses_malformed_or_unsupported_codes() {
    let dir = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(dir.path().join("host"));
    let host = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let offer = PairingOffer::open(&config, &host, Instant::now(), &[]).unwrap();
    let code = offer.code();
    for bad in [
        code.replace("MOTE1", "OVSR1"),
        format!("{code}A"),
        "MOTE1-AAAA".into(),
        code[..code.len() - 1].into(),
    ] {
        assert!(
            ClientCredential::from_pairing_code(&config, &bad).is_err(),
            "{bad}"
        );
    }
}
