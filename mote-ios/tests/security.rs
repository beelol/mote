use mote_gateway::{ClientCredential, GatewayConfig, HostIdentity, PairingOffer};
use mote_ios::keychain::StoredCredential;
use std::time::Instant;

#[test]
fn credentials_cannot_be_restored_under_a_colliding_host_name() {
    let root = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(root.path().join("host"));
    let identity = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let other = HostIdentity::load_or_create(&root.path().join("other")).unwrap();
    let offer = PairingOffer::open(&config, &identity, Instant::now(), &[]).unwrap();
    let client = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let stored = StoredCredential::paired(&client);
    assert!(stored.restore(other.host_id()).is_err());
    assert_eq!(
        stored.restore(identity.host_id()).unwrap().public_key(),
        client.public_key()
    );
}

#[test]
fn serialized_session_counter_never_restarts_after_relaunch() {
    let root = tempfile::tempdir().unwrap();
    let config = GatewayConfig::new(root.path().join("host"));
    let identity = HostIdentity::load_or_create(&config.data_dir).unwrap();
    let offer = PairingOffer::open(&config, &identity, Instant::now(), &[]).unwrap();
    let client = ClientCredential::from_pairing_code(&config, &offer.code()).unwrap();
    let mut stored = StoredCredential::paired(&client);
    assert_eq!(stored.reserve_counter().unwrap(), 2);
    let mut relaunched: StoredCredential =
        serde_json::from_slice(&serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(relaunched.reserve_counter().unwrap(), 3);
}
