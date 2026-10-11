use crate::Daemon;
use mote_ipc::RpcError;
use mote_protocol::HostSnapshot;

pub struct HostSnapshotBuilder<'a> {
    daemon: &'a Daemon,
}

impl<'a> HostSnapshotBuilder<'a> {
    pub fn new(daemon: &'a Daemon) -> Self {
        Self { daemon }
    }

    pub fn build(&self, device: &str) -> Result<HostSnapshot, RpcError> {
        let _approvals = self.daemon.watchers.lock().unwrap();
        self.daemon
            .log
            .host_snapshot_for_device(&self.daemon.host_id, device, crate::now_ms())
            .map_err(|e| RpcError::internal(e.to_string()))
    }
}
