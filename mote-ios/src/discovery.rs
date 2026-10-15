use anyhow::{ensure, Result};
use mote_gateway::discovery::{DiscoveryBrowser, DiscoveryRecord};
use std::time::Duration;

pub struct SimulatorBrowser;

impl DiscoveryBrowser for SimulatorBrowser {
    fn browse(&self, timeout: Duration) -> Result<Vec<DiscoveryRecord>> {
        ensure!(
            !timeout.is_zero() && timeout <= Duration::from_secs(30),
            "invalid discovery timeout"
        );
        #[cfg(target_os = "ios")]
        {
            let mut output = vec![0u8; 65536];
            let count = unsafe {
                mote_ios_discover(timeout.as_secs_f64(), output.as_mut_ptr(), output.len())
            };
            ensure!(
                count >= 0 && count as usize <= output.len(),
                "Simulator Bonjour browse failed: {count}"
            );
            decode_records(&output[..count as usize])
        }
        #[cfg(not(target_os = "ios"))]
        {
            mote_gateway::discovery::DnsSdBrowser.browse(timeout)
        }
    }
}

pub fn decode_records(bytes: &[u8]) -> Result<Vec<DiscoveryRecord>> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        endpoint: std::net::SocketAddr,
        txt: Vec<String>,
    }
    let records: Vec<Record> = serde_json::from_slice(bytes)?;
    ensure!(records.len() <= 16, "too many discovery records");
    records
        .into_iter()
        .map(|record| {
            ensure!(
                mote_gateway::net::is_local_range(record.endpoint.ip()),
                "discovery address is not local"
            );
            DiscoveryRecord::from_txt(record.endpoint, &record.txt)
        })
        .collect()
}

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_discover(timeout: f64, output: *mut u8, capacity: usize) -> isize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_ipad_accepts_private_discovery_and_rejects_public_addresses() {
        let fingerprint = "a".repeat(64);
        let record = |endpoint: &str| {
            serde_json::to_vec(&serde_json::json!([{
                "endpoint": endpoint,
                "txt": ["protocol_major=1", format!("fingerprint={fingerprint}")]
            }]))
            .unwrap()
        };
        assert_eq!(
            decode_records(&record("192.168.1.20:47810"))
                .unwrap()
                .first()
                .unwrap()
                .endpoint,
            "192.168.1.20:47810".parse().unwrap()
        );
        assert!(decode_records(&record("8.8.8.8:47810")).is_err());
    }
}
