use anyhow::{ensure, Result};
use mote_protocol::HostId;
use rusqlite::{params, Connection};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

pub struct DeviceStore {
    conn: Connection,
    host_id: HostId,
}

impl DeviceStore {
    pub fn open(path: &Path, host_id: &HostId) -> Result<Self> {
        super::pairing::secure_directory(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("missing parent"))?,
        )?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        super::pairing::validate_private_file(&file)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS gateway_devices(host_id TEXT NOT NULL, public_key BLOB NOT NULL, name TEXT NOT NULL, last_counter INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(host_id,public_key));")?;
        conn.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS remote_requests(host_id TEXT NOT NULL, device_id TEXT NOT NULL, request_id TEXT NOT NULL, request_hash TEXT NOT NULL, outcome TEXT, PRIMARY KEY(host_id,device_id,request_id));")?;
        Ok(Self {
            conn,
            host_id: host_id.clone(),
        })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }

    pub(crate) fn register(&self, key: &[u8; 32], name: &str, counter: i64) -> Result<()> {
        ensure!(
            counter > 0 && !name.is_empty() && name.len() <= 256,
            "invalid device"
        );
        self.conn.execute("INSERT INTO gateway_devices(host_id, public_key, name, last_counter) VALUES(?1,?2,?3,?4)", params![self.host_id.as_str(), key.as_slice(), name, counter])?;
        Ok(())
    }

    pub(crate) fn accept_counter(&self, key: &[u8; 32], counter: i64) -> Result<()> {
        ensure!(counter > 0, "invalid counter");
        let changed = self.conn.execute("UPDATE gateway_devices SET last_counter=?3 WHERE host_id=?1 AND public_key=?2 AND revoked=0 AND last_counter<?3", params![self.host_id.as_str(), key.as_slice(), counter])?;
        ensure!(changed == 1, "unknown, revoked or replayed device");
        Ok(())
    }

    pub fn is_active(&self, key: &[u8; 32]) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM gateway_devices WHERE host_id=?1 AND public_key=?2 AND revoked=0)", params![self.host_id.as_str(), key.as_slice()], |row| row.get(0))?)
    }

    pub fn revoke(&self, key: &[u8; 32]) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE gateway_devices SET revoked=1 WHERE host_id=?1 AND public_key=?2 AND revoked=0",
            params![self.host_id.as_str(), key.as_slice()],
        )? == 1)
    }

    pub(crate) fn claim(&self, device: &str, request: &str, hash: &str) -> Result<Option<mote_protocol::HostFrame>> {
        let changed = self.conn.execute("INSERT OR IGNORE INTO remote_requests(host_id,device_id,request_id,request_hash) VALUES(?1,?2,?3,?4)", params![self.host_id.as_str(), device, request, hash])?;
        if changed == 1 { return Ok(None); }
        let (stored_hash, outcome): (String, Option<String>) = self.conn.query_row("SELECT request_hash,outcome FROM remote_requests WHERE host_id=?1 AND device_id=?2 AND request_id=?3", params![self.host_id.as_str(), device, request], |row| Ok((row.get(0)?,row.get(1)?)))?;
        ensure!(stored_hash == hash, "request ID reused for different intent");
        Ok(Some(serde_json::from_str(&outcome.ok_or_else(|| anyhow::anyhow!("request already claimed; outcome unavailable"))?)?))
    }

    pub(crate) fn finish_request(&self, device: &str, request: &str, outcome: &mote_protocol::HostFrame) -> Result<()> {
        self.conn.execute("UPDATE remote_requests SET outcome=?4 WHERE host_id=?1 AND device_id=?2 AND request_id=?3 AND outcome IS NULL", params![self.host_id.as_str(), device, request, serde_json::to_string(outcome)?])?;
        Ok(())
    }

    pub(crate) fn active_device(&self, device: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT public_key FROM gateway_devices WHERE host_id=?1 AND revoked=0")?;
        let keys = stmt.query_map([self.host_id.as_str()], |row| row.get::<_, Vec<u8>>(0))?;
        use sha2::Digest;
        for key in keys {
            if crate::noise::hex(&sha2::Sha256::digest(key?)) == device { return Ok(true); }
        }
        Ok(false)
    }
}
