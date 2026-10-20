use mote_memory::MemoryError;
use serde_json::Value;
use std::io::Read;
use std::time::Duration;
use url::Url;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

pub trait MemoryCoreClient: Send {
    fn post(&mut self, path: &str, body: &Value) -> Result<Value, MemoryError>;
}

pub struct UreqMemoryCoreClient {
    base_url: Url,
    service_id: String,
    agent: ureq::Agent,
}

impl UreqMemoryCoreClient {
    pub fn new(base_url: Url, service_id: impl Into<String>, timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .try_proxy_from_env(false)
            .timeout(timeout)
            .build();
        Self {
            base_url,
            service_id: service_id.into(),
            agent,
        }
    }
}

impl MemoryCoreClient for UreqMemoryCoreClient {
    fn post(&mut self, path: &str, body: &Value) -> Result<Value, MemoryError> {
        let bytes = serde_json::to_vec(body)
            .map_err(|error| MemoryError::malformed(format!("request JSON: {error}")))?;
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(MemoryError::refused("Tencent request exceeds 1 MiB"));
        }
        let url = self
            .base_url
            .join(path)
            .map_err(|error| MemoryError::malformed(format!("Tencent request path: {error}")))?;
        let response = self
            .agent
            .post(url.as_str())
            .set("content-type", "application/json")
            .set("x-tdai-service-id", &self.service_id)
            .send_bytes(&bytes)
            .map_err(map_ureq_error)?;
        let mut bounded = response.into_reader().take(MAX_RESPONSE_BYTES + 1);
        let mut response_bytes = Vec::new();
        bounded
            .read_to_end(&mut response_bytes)
            .map_err(|error| MemoryError::unavailable(format!("Tencent response read: {error}")))?;
        if response_bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(MemoryError::refused("Tencent response exceeds 2 MiB"));
        }
        serde_json::from_slice(&response_bytes)
            .map_err(|error| MemoryError::backend(format!("malformed Tencent JSON: {error}")))
    }
}

fn map_ureq_error(error: ureq::Error) -> MemoryError {
    match error {
        ureq::Error::Status(status, _) if (300..400).contains(&status) => {
            MemoryError::refused("Tencent redirect refused")
        }
        ureq::Error::Status(status, _) => {
            MemoryError::backend(format!("Tencent HTTP status {status}"))
        }
        ureq::Error::Transport(error) if error.kind() == ureq::ErrorKind::TooManyRedirects => {
            MemoryError::refused("Tencent redirect refused")
        }
        ureq::Error::Transport(error) => {
            MemoryError::unavailable(format!("Tencent transport: {error}"))
        }
    }
}
