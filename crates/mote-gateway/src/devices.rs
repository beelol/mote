use anyhow::{ensure, Result};
use mote_protocol::HostId;
use rusqlite::{params, Connection};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

pub struct DeviceStore {
    conn: Connection,
    host_id: HostId,
    run_id: Option<String>,
}

pub(crate) enum Claim {
    New,
    Recorded(mote_protocol::HostFrame),
    Unfinished(mote_protocol::RequestState),
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
        let has_owner: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('remote_requests') WHERE name='run_id')",
            [],
            |row| row.get(0),
        )?;
        if !has_owner {
            conn.execute_batch("ALTER TABLE remote_requests ADD COLUMN run_id TEXT;")?;
        }
        Ok(Self {
            conn,
            host_id: host_id.clone(),
            run_id: None,
        })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }

    pub(crate) fn begin_run(&mut self) -> Result<()> {
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce)
            .map_err(|error| anyhow::anyhow!("gateway run identity: {error}"))?;
        self.conn.execute(
            "UPDATE remote_requests SET run_id=NULL WHERE host_id=?1 AND outcome IS NULL",
            [self.host_id.as_str()],
        )?;
        self.run_id = Some(crate::noise::hex(&nonce));
        Ok(())
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

    pub(crate) fn claim(&self, device: &str, request: &str, hash: &str) -> Result<Claim> {
        let run_id = self
            .run_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("gateway run not started"))?;
        let changed = self.conn.execute("INSERT OR IGNORE INTO remote_requests(host_id,device_id,request_id,request_hash,run_id) VALUES(?1,?2,?3,?4,?5)", params![self.host_id.as_str(), device, request, hash, run_id])?;
        if changed == 1 {
            return Ok(Claim::New);
        }
        let (stored_hash, outcome, owner): (String, Option<String>, Option<String>) = self.conn.query_row("SELECT request_hash,outcome,run_id FROM remote_requests WHERE host_id=?1 AND device_id=?2 AND request_id=?3", params![self.host_id.as_str(), device, request], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        ensure!(
            stored_hash == hash,
            "request ID reused for different intent"
        );
        match outcome {
            Some(outcome) => Ok(Claim::Recorded(serde_json::from_str(&outcome)?)),
            None => Ok(Claim::Unfinished(if owner.as_ref() == Some(run_id) {
                mote_protocol::RequestState::Pending
            } else {
                mote_protocol::RequestState::Indeterminate
            })),
        }
    }

    pub(crate) fn finish_request(
        &self,
        device: &str,
        request: &str,
        outcome: &mote_protocol::HostFrame,
    ) -> Result<()> {
        let changed = self.conn.execute("UPDATE remote_requests SET outcome=?4 WHERE host_id=?1 AND device_id=?2 AND request_id=?3 AND outcome IS NULL AND run_id=?5", params![self.host_id.as_str(), device, request, serde_json::to_string(outcome)?, self.run_id])?;
        ensure!(
            changed == 1,
            "request belongs to an interrupted gateway run"
        );
        Ok(())
    }

    pub(crate) fn active_device(&self, device: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT public_key FROM gateway_devices WHERE host_id=?1 AND revoked=0")?;
        let keys = stmt.query_map([self.host_id.as_str()], |row| row.get::<_, Vec<u8>>(0))?;
        use sha2::Digest;
        for key in keys {
            if crate::noise::hex(&sha2::Sha256::digest(key?)) == device {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
