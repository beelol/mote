use mote_memory::{MemoryBackend, MemoryError, MemoryScope};
use mote_memory_tencent::{MemoryCoreClient, TencentConfig, TencentMemory, UreqMemoryCoreClient};
use serde_json::json;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use url::Url;

fn make(url: &str) -> Result<TencentConfig, MemoryError> {
    TencentConfig::new(
        Url::parse(url).unwrap(),
        "default",
        "team",
        "mote-test",
        PathBuf::from("state.json"),
        Duration::from_secs(1),
    )
}

#[test]
fn accepts_literal_loopback_and_canonicalizes_localhost() {
    assert_eq!(
        make("http://127.0.0.1:8420").unwrap().base_url().host_str(),
        Some("127.0.0.1")
    );
    assert!(make("http://[::1]:8420").is_ok());
    assert!(make("http://localhost:8420")
        .unwrap()
        .base_url()
        .host_str()
        .is_some());
}

#[test]
fn refuses_non_loopback_and_non_origin_urls() {
    for url in [
        "http://192.0.2.1:8420",
        "https://127.0.0.1:8420",
        "http://user@127.0.0.1:8420",
        "http://127.0.0.1:8420/path",
        "http://127.0.0.1:8420?query=yes",
        "http://127.0.0.1:8420#fragment",
    ] {
        assert!(
            matches!(make(url), Err(MemoryError::Refused { .. })),
            "{url}"
        );
    }
}

#[test]
fn refuses_unbounded_timeout_and_invalid_names() {
    let url = Url::parse("http://127.0.0.1:8420").unwrap();
    assert!(TencentConfig::new(
        url.clone(),
        "default",
        "team",
        "mote-test",
        PathBuf::from("state.json"),
        Duration::from_secs(31),
    )
    .is_err());
    assert!(TencentConfig::new(
        url,
        "default",
        "team",
        "bad prefix",
        PathBuf::from("state.json"),
        Duration::from_secs(1),
    )
    .is_err());
}

#[test]
fn transport_refuses_redirects_and_malformed_json() {
    let redirect_target = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    redirect_target.set_nonblocking(true).unwrap();
    let location = redirect_target.local_addr().unwrap();
    let redirect = serve_once(
        format!("HTTP/1.1 302 Found\r\nLocation: http://{location}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes(),
        Duration::ZERO,
    );
    let mut client = UreqMemoryCoreClient::new(redirect, "default", Duration::from_secs(1));
    assert!(client.post("v3/conversation/search", &json!({})).is_err());
    assert_eq!(
        redirect_target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    let malformed = serve_once(
        b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nnot-json".to_vec(),
        Duration::ZERO,
    );
    let mut client = UreqMemoryCoreClient::new(malformed, "default", Duration::from_secs(1));
    assert!(matches!(
        client.post("v3/conversation/search", &json!({})),
        Err(MemoryError::Backend { .. })
    ));
}

#[test]
fn transport_accepts_a_bounded_success_envelope() {
    let expected = json!({
        "code": 0,
        "message": "ok",
        "request_id": "fixture",
        "data": {"accepted_ids": ["server-generated-id"]}
    });
    let endpoint = serve_once(
        http_ok(&serde_json::to_vec(&expected).unwrap()),
        Duration::ZERO,
    );
    let mut client = UreqMemoryCoreClient::new(endpoint, "default", Duration::from_secs(1));
    assert_eq!(
        client
            .post("v3/conversation/add", &json!({"messages": []}))
            .unwrap(),
        expected
    );
}

#[test]
fn transport_enforces_request_and_response_size_bounds() {
    let mut client = UreqMemoryCoreClient::new(
        Url::parse("http://127.0.0.1:9/").unwrap(),
        "default",
        Duration::from_secs(1),
    );
    assert!(matches!(
        client.post(
            "v3/conversation/add",
            &json!({"content": "x".repeat(1024 * 1024)})
        ),
        Err(MemoryError::Refused { .. })
    ));

    let oversized = vec![b'x'; 2 * 1024 * 1024 + 1];
    let endpoint = serve_once(http_ok(&oversized), Duration::ZERO);
    let mut client = UreqMemoryCoreClient::new(endpoint, "default", Duration::from_secs(1));
    assert!(matches!(
        client.post("v3/conversation/search", &json!({})),
        Err(MemoryError::Refused { .. })
    ));
}

#[test]
fn loopback_search_refuses_mismatched_present_scope() {
    let envelope = json!({
        "code": 0,
        "message": "ok",
        "request_id": "fixture",
        "data": {"messages": [{
            "id": "server-id",
            "content": "fixture",
            "score": 1.0,
            "team_id": "wrong-team"
        }]}
    });
    let endpoint = serve_once(
        http_ok(&serde_json::to_vec(&envelope).unwrap()),
        Duration::ZERO,
    );
    let directory = TempDir::new().unwrap();
    let config = TencentConfig::new(
        endpoint,
        "default",
        "fixture-team",
        "mote-test",
        directory.path().join("state.json"),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut backend = TencentMemory::connect(config).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    assert_eq!(
        backend.retrieve(&scope, "fixture", 1).unwrap_err(),
        MemoryError::refused("Tencent returned mismatched team_id")
    );
}

#[test]
fn transport_enforces_timeout() {
    let endpoint = serve_once(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
        Duration::from_millis(150),
    );
    let mut client = UreqMemoryCoreClient::new(endpoint, "default", Duration::from_millis(20));
    assert!(matches!(
        client.post("v3/conversation/search", &json!({})),
        Err(MemoryError::Unavailable { .. })
    ));
}

fn serve_once(response: Vec<u8>, delay: Duration) -> Url {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut content_length = 0_usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or_default() == 0 || line == "\r\n" {
                break;
            }
            if let Some(value) = line
                .to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
            {
                content_length = value;
            }
        }
        let mut body = vec![0_u8; content_length];
        let _ = reader.read_exact(&mut body);
        drop(reader);
        thread::sleep(delay);
        let _ = std::io::Write::write_all(&mut stream, &response);
    });
    Url::parse(&format!("http://{address}/")).unwrap()
}

fn http_ok(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
