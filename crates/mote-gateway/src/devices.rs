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
}
