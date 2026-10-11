use crate::{Cursor, HostError, HostErrorCode, HostFrame, HostId, HostSnapshot, RequestId};
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotChunk {
    pub transfer_id: RequestId,
    pub request_id: Option<RequestId>,
    pub cursor: Cursor,
    pub index: u64,
    pub total_bytes: u64,
    pub data: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotAssembler {
    pending: Option<PendingSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingSnapshot {
    transfer_id: RequestId,
    request_id: Option<RequestId>,
    cursor: Cursor,
    next_index: u64,
    total_bytes: u64,
    data: String,
}

impl SnapshotAssembler {
    pub fn accept(
        &mut self,
        host: &HostId,
        frame: HostFrame,
    ) -> Result<Option<HostFrame>, HostError> {
        match frame {
            HostFrame::SnapshotChunk { chunk } => self.push(host, chunk),
            _ if self.pending.take().is_some() => Err(invalid_chunk()),
            frame => Ok(Some(frame)),
        }
    }

    pub fn push(
        &mut self,
        host: &HostId,
        chunk: SnapshotChunk,
    ) -> Result<Option<HostFrame>, HostError> {
        let pending = self.pending.take();
        if chunk.cursor.host_id != *host {
            return Err(HostError::new(
                HostErrorCode::IdentityChanged,
                "snapshot chunk belongs to another host",
            ));
        }
        if chunk.data.is_empty()
            || chunk.data.len() > SNAPSHOT_CHUNK_BYTES
            || chunk.total_bytes == 0
        {
            return Err(invalid_chunk());
        }
        let mut pending = match pending {
            Some(pending) => pending,
            None if chunk.index == 0 => PendingSnapshot {
                transfer_id: chunk.transfer_id.clone(),
                request_id: chunk.request_id.clone(),
                cursor: chunk.cursor.clone(),
                next_index: 0,
                total_bytes: chunk.total_bytes,
                data: String::new(),
            },
            None => return Err(invalid_chunk()),
        };
        if pending.transfer_id != chunk.transfer_id
            || pending.request_id != chunk.request_id
            || pending.cursor != chunk.cursor
            || pending.next_index != chunk.index
            || pending.total_bytes != chunk.total_bytes
            || (pending.data.len() as u64)
                .checked_add(chunk.data.len() as u64)
                .is_none_or(|size| size > pending.total_bytes)
        {
            return Err(invalid_chunk());
        }
        pending.data.push_str(&chunk.data);
        pending.next_index += 1;
        if pending.data.len() as u64 == pending.total_bytes {
            let snapshot: HostSnapshot =
                serde_json::from_str(&pending.data).map_err(|_| invalid_chunk())?;
            if snapshot.cursor != pending.cursor || snapshot.host_id != *host {
                return Err(invalid_chunk());
            }
            return Ok(Some(match pending.request_id {
                Some(request_id) => HostFrame::Result {
                    request_id,
                    result: serde_json::to_value(snapshot).map_err(|_| invalid_chunk())?,
                },
                None => HostFrame::Snapshot { snapshot },
            }));
        }
        self.pending = Some(pending);
        Ok(None)
    }

    pub fn reset(&mut self) {
        self.pending = None;
    }
}

fn invalid_chunk() -> HostError {
    HostError::new(HostErrorCode::InvalidRequest, "invalid snapshot transfer")
}
