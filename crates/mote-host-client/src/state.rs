use crate::{require_host, ConnectionState, Outbox};
use mote_protocol::{
    CommittedSurface, Cursor, Event, HostError, HostErrorCode, HostId, HostSnapshot,
    PendingApproval, RequestId, ThreadSnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostSessionState {
    cursor: Cursor,
    selected_thread: Option<String>,
    threads: BTreeMap<String, ThreadSnapshot>,
    surfaces: BTreeMap<String, BTreeMap<i64, CommittedSurface>>,
    pending_approvals: BTreeMap<RequestId, PendingApproval>,
    outbox: Outbox,
    #[serde(skip)]
    connection: ConnectionState,
}

#[derive(Deserialize)]
#[serde(remote = "HostSessionState", deny_unknown_fields)]
struct SessionFields {
    cursor: Cursor,
    selected_thread: Option<String>,
    threads: BTreeMap<String, ThreadSnapshot>,
    surfaces: BTreeMap<String, BTreeMap<i64, CommittedSurface>>,
    pending_approvals: BTreeMap<RequestId, PendingApproval>,
    outbox: Outbox,
    #[serde(skip)]
    connection: ConnectionState,
}

impl<'de> Deserialize<'de> for HostSessionState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = SessionFields::deserialize(deserializer)?;
        state
            .validate(&state.cursor.host_id)
            .map_err(serde::de::Error::custom)?;
        Ok(state)
    }
}

impl HostSessionState {
    pub(crate) fn new(host_id: &HostId) -> Self {
        Self {
            cursor: Cursor::new(host_id.clone(), 0).expect("zero is a valid cursor"),
            selected_thread: None,
            threads: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            pending_approvals: BTreeMap::new(),
            outbox: Outbox::new(host_id),
            connection: ConnectionState::Offline,
        }
    }

    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    pub fn selected_thread(&self) -> Option<&str> {
        self.selected_thread.as_deref()
    }

    pub fn threads(&self) -> &BTreeMap<String, ThreadSnapshot> {
        &self.threads
    }

    pub fn surfaces(&self) -> &BTreeMap<String, BTreeMap<i64, CommittedSurface>> {
        &self.surfaces
    }

    pub fn pending_approvals(&self) -> &BTreeMap<RequestId, PendingApproval> {
        &self.pending_approvals
    }

    pub fn outbox(&self) -> &Outbox {
        &self.outbox
    }

    pub fn connection(&self) -> &ConnectionState {
        &self.connection
    }

    pub(crate) fn select_thread(
        &mut self,
        host_id: &HostId,
        thread: Option<String>,
    ) -> Result<(), HostError> {
        require_host(&self.cursor.host_id, host_id)?;
        if thread.as_ref().is_some_and(|thread| thread.is_empty()) {
            return Err(HostError::new(
                HostErrorCode::InvalidParams,
                "thread must be nonempty",
            ));
        }
        self.selected_thread = thread;
        Ok(())
    }

    pub(crate) fn set_connection(
        &mut self,
        host_id: &HostId,
        connection: ConnectionState,
    ) -> Result<(), HostError> {
        require_host(&self.cursor.host_id, host_id)?;
        self.connection = connection;
        Ok(())
    }

    pub(crate) fn install_snapshot(
        &mut self,
        host_id: &HostId,
        snapshot: HostSnapshot,
    ) -> Result<(), HostError> {
        require_host(&self.cursor.host_id, host_id)?;
        require_host(host_id, &snapshot.host_id)?;
        require_host(host_id, &snapshot.cursor.host_id)?;
        let snapshot: HostSnapshot =
            serde_json::from_value(serde_json::to_value(snapshot).map_err(invalid_state)?)
                .map_err(invalid_state)?;
        let mut next = self.clone();
        next.cursor = snapshot.cursor;
        next.threads.clear();
        next.surfaces.clear();
        next.pending_approvals.clear();
        for thread in snapshot.threads {
            if next.threads.insert(thread.thread.clone(), thread).is_some() {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "duplicate snapshot thread",
                ));
            }
        }
        for surface in snapshot.surfaces {
            if surface.surface_seq > next.cursor.seq {
                return Err(HostError::new(
                    HostErrorCode::InvalidCursor,
                    "surface exceeds snapshot cursor",
                ));
            }
            if next
                .surfaces
                .entry(surface.thread.clone())
                .or_default()
                .insert(surface.surface_seq, surface)
                .is_some()
            {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "duplicate snapshot surface",
                ));
            }
        }
        for approval in snapshot.pending_approvals {
            if next
                .pending_approvals
                .insert(approval.approval_id.clone(), approval)
                .is_some()
            {
                return Err(HostError::new(
                    HostErrorCode::InvalidApproval,
                    "duplicate snapshot approval",
                ));
            }
        }
        if next
            .selected_thread
            .as_ref()
            .is_some_and(|thread| !next.threads.contains_key(thread))
        {
            next.selected_thread = None;
        }
        *self = next;
        Ok(())
    }

    pub(crate) fn apply_event(
        &mut self,
        host_id: &HostId,
        source_host: &HostId,
        event: Event,
    ) -> Result<bool, HostError> {
        require_host(&self.cursor.host_id, host_id)?;
        require_host(host_id, source_host)?;
        let cursor = Cursor::new(host_id.clone(), event.seq)?;
        if cursor.seq <= self.cursor.seq {
            return Ok(false);
        }
        if event.kind == "resync" {
            return Err(HostError::new(
                HostErrorCode::InvalidCursor,
                "authoritative snapshot required",
            ));
        }
        if mote_protocol::ThreadProjection::handles(&event.kind) {
            let mut projection = match self.threads.get(&event.scope_id) {
                Some(thread) => mote_protocol::ThreadProjection::resume(thread.clone()),
                None => mote_protocol::ThreadProjection::new(event.scope_id.clone()),
            };
            projection.apply(&event);
            self.threads
                .insert(event.scope_id.clone(), projection.thread.clone());
            if let Some(surface) = &projection.surface {
                self.surfaces
                    .entry(event.scope_id.clone())
                    .or_default()
                    .insert(surface.surface_seq, surface.clone());
            }
        }
        if event.kind == "approval.decided" {
            if let Some(id) = event.payload["approval_id"].as_str() {
                if let Ok(id) = RequestId::new(id) {
                    self.pending_approvals.remove(&id);
                }
            }
        }
        self.cursor = cursor;
        Ok(true)
    }

    pub(crate) fn outbox_mut(&mut self, host_id: &HostId) -> Result<&mut Outbox, HostError> {
        require_host(&self.cursor.host_id, host_id)?;
        Ok(&mut self.outbox)
    }

    pub(crate) fn validate(&self, host_id: &HostId) -> Result<(), HostError> {
        require_host(host_id, &self.cursor.host_id)?;
        self.outbox.validate(host_id)?;
        if self
            .selected_thread
            .as_ref()
            .is_some_and(|thread| thread.is_empty())
        {
            return Err(HostError::new(
                HostErrorCode::InvalidParams,
                "thread must be nonempty",
            ));
        }
        for (id, thread) in &self.threads {
            if id != &thread.thread {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "thread key mismatch",
                ));
            }
        }
        for (thread, surfaces) in &self.surfaces {
            for (seq, surface) in surfaces {
                if thread != &surface.thread || *seq != surface.surface_seq {
                    return Err(HostError::new(
                        HostErrorCode::InvalidRequest,
                        "surface key mismatch",
                    ));
                }
            }
        }
        for (id, approval) in &self.pending_approvals {
            if id != &approval.approval_id {
                return Err(HostError::new(
                    HostErrorCode::InvalidApproval,
                    "approval key mismatch",
                ));
            }
        }
        Ok(())
    }
}

fn invalid_state(_: serde_json::Error) -> HostError {
    HostError::new(HostErrorCode::InvalidRequest, "invalid host snapshot")
}
