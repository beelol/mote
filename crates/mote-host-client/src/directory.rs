use crate::{require_host, ConnectionState, HostSessionState};
use mote_protocol::{
    ClientFrame, Event, HostError, HostErrorCode, HostId, HostSnapshot, RequestId, VersionRange,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

pub trait CredentialStore {
    type Credential;

    fn load(&self, host_id: &HostId) -> io::Result<Option<Self::Credential>>;
    fn save(&mut self, host_id: &HostId, credential: &Self::Credential) -> io::Result<()>;
    fn delete(&mut self, host_id: &HostId) -> io::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairedHost {
    host_id: HostId,
    public_key: [u8; 32],
    name: String,
    location: Option<String>,
    versions: VersionRange,
}

#[derive(Deserialize)]
#[serde(remote = "PairedHost", deny_unknown_fields)]
struct HostFields {
    host_id: HostId,
    public_key: [u8; 32],
    name: String,
    location: Option<String>,
    versions: VersionRange,
}

impl<'de> Deserialize<'de> for PairedHost {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let host = HostFields::deserialize(deserializer)?;
        host.validate().map_err(serde::de::Error::custom)?;
        Ok(host)
    }
}

impl PairedHost {
    pub fn new(
        public_key: [u8; 32],
        name: impl Into<String>,
        location: Option<String>,
        versions: VersionRange,
    ) -> Result<Self, HostError> {
        let host = Self {
            host_id: HostId::from_public_key(&public_key),
            public_key,
            name: name.into(),
            location,
            versions,
        };
        host.validate()?;
        Ok(host)
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }
    pub fn versions(&self) -> VersionRange {
        self.versions
    }

    fn validate(&self) -> Result<(), HostError> {
        require_host(&self.host_id, &HostId::from_public_key(&self.public_key))?;
        if self.name.trim().is_empty() {
            return Err(HostError::new(
                HostErrorCode::InvalidParams,
                "host name must be nonempty",
            ));
        }
        VersionRange::new(self.versions.min, self.versions.max)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostEntry {
    host: PairedHost,
    state: HostSessionState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedDirectory")]
pub struct HostDirectory {
    hosts: BTreeMap<HostId, HostEntry>,
    selected_host: Option<HostId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedDirectory {
    hosts: BTreeMap<HostId, HostEntry>,
    selected_host: Option<HostId>,
}

impl TryFrom<UncheckedDirectory> for HostDirectory {
    type Error = HostError;

    fn try_from(value: UncheckedDirectory) -> Result<Self, Self::Error> {
        for (id, entry) in &value.hosts {
            require_host(id, entry.host.host_id())?;
            entry.host.validate()?;
            entry.state.validate(id)?;
        }
        let directory = Self {
            hosts: value.hosts,
            selected_host: value.selected_host,
        };
        if let Some(id) = &directory.selected_host {
            directory.host(id)?;
        }
        Ok(directory)
    }
}

impl HostDirectory {
    pub fn hosts(&self) -> impl Iterator<Item = &PairedHost> {
        self.hosts.values().map(|entry| &entry.host)
    }
    pub(crate) fn snapshot_frame(
        &mut self,
        host: &HostId,
        frame: mote_protocol::HostFrame,
    ) -> Result<Option<mote_protocol::HostFrame>, HostError> {
        self.state_mut(host)?.snapshot_frame(host, frame)
    }
    pub fn register(&mut self, host: PairedHost) -> Result<(), HostError> {
        host.validate()?;
        if self.hosts.contains_key(host.host_id()) {
            return Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "host is already registered",
            ));
        }
        let state = HostSessionState::new(host.host_id());
        self.hosts
            .insert(host.host_id().clone(), HostEntry { host, state });
        Ok(())
    }

    pub fn host(&self, host_id: &HostId) -> Result<&PairedHost, HostError> {
        self.hosts
            .get(host_id)
            .map(|entry| &entry.host)
            .ok_or_else(unpaired_host)
    }

    pub fn state(&self, host_id: &HostId) -> Result<&HostSessionState, HostError> {
        self.hosts
            .get(host_id)
            .map(|entry| &entry.state)
            .ok_or_else(unpaired_host)
    }

    pub fn selected_host(&self) -> Option<&HostId> {
        self.selected_host.as_ref()
    }

    pub fn select_host(&mut self, host_id: &HostId) -> Result<(), HostError> {
        self.host(host_id)?;
        self.selected_host = Some(host_id.clone());
        Ok(())
    }

    pub fn select_thread(
        &mut self,
        host_id: &HostId,
        thread: Option<String>,
    ) -> Result<(), HostError> {
        self.state_mut(host_id)?.select_thread(host_id, thread)
    }

    pub fn set_connection(
        &mut self,
        host_id: &HostId,
        state: ConnectionState,
    ) -> Result<(), HostError> {
        self.state_mut(host_id)?.set_connection(host_id, state)
    }

    pub fn install_snapshot(
        &mut self,
        host_id: &HostId,
        snapshot: HostSnapshot,
    ) -> Result<(), HostError> {
        self.state_mut(host_id)?.install_snapshot(host_id, snapshot)
    }

    pub fn apply_event(
        &mut self,
        host_id: &HostId,
        source_host: &HostId,
        event: Event,
    ) -> Result<bool, HostError> {
        self.state_mut(host_id)?
            .apply_event(host_id, source_host, event)
    }

    pub fn enqueue(&mut self, host_id: &HostId, frame: ClientFrame) -> Result<(), HostError> {
        self.state_mut(host_id)?
            .outbox_mut(host_id)?
            .enqueue(host_id, frame)
    }

    pub fn acknowledge(
        &mut self,
        host_id: &HostId,
        request_id: &RequestId,
    ) -> Result<bool, HostError> {
        self.state_mut(host_id)?
            .outbox_mut(host_id)?
            .acknowledge(host_id, request_id)
    }

    fn state_mut(&mut self, host_id: &HostId) -> Result<&mut HostSessionState, HostError> {
        self.hosts
            .get_mut(host_id)
            .map(|entry| &mut entry.state)
            .ok_or_else(unpaired_host)
    }
}

fn unpaired_host() -> HostError {
    HostError::new(
        HostErrorCode::Unauthorized,
        "host is not paired in this directory",
    )
}
