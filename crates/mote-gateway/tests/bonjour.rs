use anyhow::Result;
use mote_gateway::discovery::{
    advertise, discover, Advertisement, Advertiser, DiscoveryBrowser, DiscoveryRecord,
    EndpointCandidate, SERVICE_TYPE,
};
use mote_gateway::{ClientCredential, GatewayClient, GatewayConfig, GatewayServer};
use mote_ipc::{Dispatch, RpcError};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

struct Fixture;
impl Dispatch for Fixture {
    fn call(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        unreachable!()
    }
}
struct FakeBrowser(Vec<DiscoveryRecord>);
impl DiscoveryBrowser for FakeBrowser {
    fn browse(&self, _: Duration) -> Result<Vec<DiscoveryRecord>> {
        Ok(self.0.clone())
    }
}
struct FakeAdvertiser(Arc<AtomicBool>);
struct Running(Arc<AtomicBool>);
impl Advertisement for Running {}
impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl Advertiser for FakeAdvertiser {
    fn start(&self, _: &DiscoveryRecord) -> Result<Box<dyn Advertisement>> {
        self.0.store(true, Ordering::SeqCst);
        Ok(Box::new(Running(self.0.clone())))
    }
}
fn fixture() -> (tempfile::TempDir, GatewayConfig, GatewayServer) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let server = GatewayServer::bind_loopback(config.clone(), Arc::new(Fixture)).unwrap();
    (dir, config, server)
}
fn record(server: &GatewayServer) -> DiscoveryRecord {
    DiscoveryRecord::new(server.local_addr(), 1, server.public_key()).unwrap()
}
fn pair(server: &GatewayServer, config: &GatewayConfig) -> ClientCredential {
    let credential =
        ClientCredential::from_pairing_code(config, &server.open_pairing().unwrap()).unwrap();
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| {
            GatewayClient::pair(
                server.local_addr(),
                config,
                &credential,
                "Discovery fixture",
                1,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(device) = server.pending_devices().first() {
                server.confirm_pairing(&device.client_id, true).unwrap();
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(pending.join().unwrap().unwrap());
    });
    credential
}

#[test]
fn txt_contains_only_protocol_major_and_public_fingerprint() {
    let endpoint = "127.0.0.1:47810".parse().unwrap();
    let record = DiscoveryRecord::new(endpoint, 1, &[0; 32]).unwrap();
    assert_eq!(SERVICE_TYPE, "_mote._tcp");
    assert_eq!(
        record.txt(),
        vec![
            "protocol_major=1",
            "fingerprint=66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925"
        ]
    );
    assert_eq!(
        DiscoveryRecord::from_txt(endpoint, &record.txt()).unwrap(),
        record
    );
    for txt in [
        vec!["protocol_major=1".into()],
        vec![
            "protocol_major=1".into(),
            format!("fingerprint={}", "a".repeat(64)),
            "host_id=private".into(),
        ],
        vec!["protocol_major=1".into(), "fingerprint=bad".into()],
    ] {
        assert!(DiscoveryRecord::from_txt(endpoint, &txt).is_err());
    }
}

#[test]
fn fake_browser_hint_connects_only_as_the_pinned_host() {
    let (_dir, config, server) = fixture();
    let credential = pair(&server, &config);
    let mut saved: SocketAddr = "127.0.0.1:9".parse().unwrap();
    let hints = discover(
        true,
        &FakeBrowser(vec![record(&server)]),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].endpoint, server.local_addr());
    let candidate = EndpointCandidate::new(hints[0].clone(), &credential).unwrap();
    let client = candidate
        .connect(&config, &credential, 2, &mut saved)
        .unwrap();
    assert_eq!(client.host_id(), server.host_id());
    assert_eq!(saved, server.local_addr());
}

#[test]
fn gateway_owns_advertisement_and_discovery_disable_preserves_listener() {
    for enabled in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let mut config = GatewayConfig::new(dir.path().join("data"));
        config.bind = "127.0.0.1:0".parse().unwrap();
        config.discovery_enabled = enabled;
        let mut server = GatewayServer::bind_loopback(config, Arc::new(Fixture)).unwrap();
        let running = Arc::new(AtomicBool::new(false));
        server
            .start_discovery(&FakeAdvertiser(running.clone()))
            .unwrap();
        assert_eq!(running.load(Ordering::SeqCst), enabled);
        assert!(server.local_addr().ip().is_loopback());
        drop(server);
        assert!(!running.load(Ordering::SeqCst));
    }
}

#[test]
fn incompatible_or_cross_host_candidate_keeps_the_saved_endpoint() {
    let (_dir, config, server) = fixture();
    let credential = pair(&server, &config);
    let mut incompatible = record(&server);
    incompatible.protocol_major = 2;
    assert!(EndpointCandidate::new(incompatible, &credential).is_err());
    let candidate = EndpointCandidate::new(record(&server), &credential).unwrap();
    let wrong = ClientCredential::restore([5; 32], [6; 32]).unwrap();
    let mut saved = "127.0.0.1:9".parse().unwrap();
    assert!(candidate.connect(&config, &wrong, 2, &mut saved).is_err());
    assert_eq!(saved, "127.0.0.1:9".parse().unwrap());
}

#[test]
fn moved_port_is_saved_only_after_pinned_noise_proof() {
    let (_dir, mut config, server) = fixture();
    let credential = pair(&server, &config);
    let mut saved = server.local_addr();
    let old = saved;
    let key = *server.public_key();
    drop(server);
    let occupied = std::net::TcpListener::bind(old).unwrap();
    config.bind = "127.0.0.1:0".parse().unwrap();
    let moved = GatewayServer::bind_loopback(config.clone(), Arc::new(Fixture)).unwrap();
    assert_ne!(moved.local_addr(), old);
    assert_eq!(moved.public_key(), &key);
    let candidate = EndpointCandidate::new(record(&moved), &credential).unwrap();
    assert_eq!(saved, old);
    let client = candidate
        .connect(&config, &credential, 2, &mut saved)
        .unwrap();
    assert_eq!(saved, moved.local_addr());
    assert_eq!(client.host_id(), credential.host_id());
    drop(occupied);
}

#[test]
fn impersonator_cannot_move_known_host() {
    let (_dir, config, server) = fixture();
    let credential = pair(&server, &config);
    let (_impostor_dir, _, impostor) = fixture();
    let mut saved = server.local_addr();
    assert!(EndpointCandidate::new(record(&impostor), &credential).is_err());
    let forged =
        DiscoveryRecord::new(impostor.local_addr(), 1, credential.host_public_key()).unwrap();
    let candidate = EndpointCandidate::new(forged, &credential).unwrap();
    assert!(candidate
        .connect(&config, &credential, 2, &mut saved)
        .is_err());
    assert_eq!(saved, server.local_addr());
}

#[test]
fn advertiser_shutdown_releases_registration() {
    let (_dir, _, server) = fixture();
    let running = Arc::new(AtomicBool::new(false));
    let registration = advertise(true, &FakeAdvertiser(running.clone()), &record(&server)).unwrap();
    assert!(running.load(Ordering::SeqCst));
    drop(registration);
    assert!(!running.load(Ordering::SeqCst));
}

#[test]
fn discovery_disabled_skips_advertising_and_browsing() {
    struct Unavailable;
    impl Advertiser for Unavailable {
        fn start(&self, _: &DiscoveryRecord) -> Result<Box<dyn Advertisement>> {
            panic!("disabled advertiser invoked")
        }
    }
    impl DiscoveryBrowser for Unavailable {
        fn browse(&self, _: Duration) -> Result<Vec<DiscoveryRecord>> {
            panic!("disabled browser invoked")
        }
    }
    let (_dir, _, server) = fixture();
    assert!(advertise(false, &Unavailable, &record(&server))
        .unwrap()
        .is_none());
    assert!(discover(false, &Unavailable, Duration::from_secs(1))
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and the Mac Bonjour stack"]
fn live_mac_stack_discovers_and_removes_loopback_registration() {
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    use mote_gateway::discovery::{DnsSdAdvertiser, DnsSdBrowser};
    let (_dir, _, server) = fixture();
    let expected = record(&server);
    let registration = advertise(true, &DnsSdAdvertiser, &expected).unwrap();
    let records = discover(true, &DnsSdBrowser, Duration::from_secs(2)).unwrap();
    assert!(records.contains(&expected), "records: {records:?}");
    println!(
        "Mac Bonjour discovered {} with TXT {:?}",
        expected.endpoint,
        expected.txt()
    );
    drop(registration);
    let records = discover(true, &DnsSdBrowser, Duration::from_secs(2)).unwrap();
    assert!(
        !records.contains(&expected),
        "registration survived shutdown: {records:?}"
    );
    println!("Mac Bonjour removed the registration after advertiser shutdown");
}
