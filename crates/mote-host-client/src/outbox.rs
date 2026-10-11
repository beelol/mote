use crate::require_host;
use mote_protocol::{ClientFrame, HostError, HostErrorCode, HostId, RequestId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{io, io::Write, path::Path};

impl crate::HostDirectory {
    pub fn load(path: &Path) -> io::Result<Self> {
        serde_json::from_reader(std::fs::File::open(path)?).map_err(io::Error::other)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec(self).map_err(io::Error::other)?)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(io::Error::other)?;
        std::fs::File::open(parent)?.sync_all()
    }

    pub fn enqueue_durable(
        &mut self,
        path: &Path,
        host: &HostId,
        frame: ClientFrame,
    ) -> io::Result<()> {
        let mut next = self.clone();
        next.enqueue(host, frame).map_err(io::Error::other)?;
        next.save(path)?;
        *self = next;
        Ok(())
    }

    pub fn receive_frame_durable(
        &mut self,
        path: &Path,
        host: &HostId,
        frame: mote_protocol::HostFrame,
    ) -> io::Result<()> {
        let mut next = self.clone();
        let result = next.receive_frame(host, frame);
        next.save(path)?;
        *self = next;
        result.map_err(io::Error::other)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outbox {
    host_id: HostId,
    requests: Vec<ClientFrame>,
}

#[derive(Deserialize)]
#[serde(remote = "Outbox", deny_unknown_fields)]
struct OutboxFields {
    host_id: HostId,
    requests: Vec<ClientFrame>,
}

impl<'de> Deserialize<'de> for Outbox {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let outbox = OutboxFields::deserialize(deserializer)?;
        outbox
            .validate(&outbox.host_id)
            .map_err(serde::de::Error::custom)?;
        Ok(outbox)
    }
}

impl Outbox {
    pub fn new(host_id: &HostId) -> Self {
        Self {
            host_id: host_id.clone(),
            requests: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, host_id: &HostId, frame: ClientFrame) -> Result<(), HostError> {
        require_host(&self.host_id, host_id)?;
        let request_id = Self::request_id(host_id, &frame)?;
        if let Some(existing) = self
            .requests
            .iter()
            .find(|existing| Self::request_id(host_id, existing).ok() == Some(request_id))
        {
            if existing != &frame {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "request ID already belongs to a different action",
                ));
            }
            return Ok(());
        }
        self.requests.push(frame);
        Ok(())
    }

    pub fn pending(&self, host_id: &HostId) -> Result<Vec<&ClientFrame>, HostError> {
        require_host(&self.host_id, host_id)?;
        Ok(self.requests.iter().collect())
    }

    pub fn acknowledge(
        &mut self,
        host_id: &HostId,
        request_id: &RequestId,
    ) -> Result<bool, HostError> {
        require_host(&self.host_id, host_id)?;
        let position = self
            .requests
            .iter()
            .position(|frame| Self::request_id(host_id, frame).ok() == Some(request_id));
        if let Some(position) = position {
            self.requests.remove(position);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn validate(&self, host_id: &HostId) -> Result<(), HostError> {
        require_host(&self.host_id, host_id)?;
        let mut ids = BTreeSet::new();
        for frame in &self.requests {
            if !ids.insert(Self::request_id(host_id, frame)?) {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "duplicate outbox request ID",
                ));
            }
        }
        Ok(())
    }

    fn request_id<'a>(
        host_id: &HostId,
        frame: &'a ClientFrame,
    ) -> Result<&'a RequestId, HostError> {
        match frame {
            ClientFrame::Call { request_id, .. } => Ok(request_id),
            ClientFrame::Subscribe {
                request_id, cursor, ..
            } => {
                require_host(host_id, &cursor.host_id)?;
                Ok(request_id)
            }
            ClientFrame::Interaction {
                request_id,
                interaction,
            } => {
                require_host(host_id, &interaction.host_id)?;
                Ok(request_id)
            }
            ClientFrame::ApprovalDecision {
                request_id,
                decision,
            } => {
                require_host(host_id, &decision.host_id)?;
                Ok(request_id)
            }
            ClientFrame::Hello { .. } => Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "handshake frames cannot enter the request outbox",
            )),
        }
    }
}
