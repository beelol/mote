use crate::{require_host, ConnectionState, Outbox};
use mote_protocol::{
    CommittedSurface, Cursor, Event, HostError, HostErrorCode, HostId, HostSnapshot,
    PendingApproval, RequestId, ThreadSnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalState {
    pub approval: PendingApproval,
    pub status: ApprovalStatus,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalEnvelope {
    host_id: HostId,
    approval: PendingApproval,
    status: ApprovalStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostSessionState {
    cursor: Cursor,
    selected_thread: Option<String>,
    threads: BTreeMap<String, ThreadSnapshot>,
    surfaces: BTreeMap<String, BTreeMap<i64, CommittedSurface>>,
    pending_approvals: BTreeMap<RequestId, PendingApproval>,
    approval_states: BTreeMap<RequestId, ApprovalState>,
    outbox: Outbox,
    #[serde(skip)]
    connection: ConnectionState,
    #[serde(skip)]
    snapshot_transfer: mote_protocol::SnapshotAssembler,
}

#[derive(Deserialize)]
#[serde(remote = "HostSessionState", deny_unknown_fields)]
struct SessionFields {
    cursor: Cursor,
    selected_thread: Option<String>,
    threads: BTreeMap<String, ThreadSnapshot>,
    surfaces: BTreeMap<String, BTreeMap<i64, CommittedSurface>>,
    pending_approvals: BTreeMap<RequestId, PendingApproval>,
    #[serde(default)]
    approval_states: BTreeMap<RequestId, ApprovalState>,
    outbox: Outbox,
    #[serde(skip)]
    connection: ConnectionState,
    #[serde(skip)]
    snapshot_transfer: mote_protocol::SnapshotAssembler,
}

impl<'de> Deserialize<'de> for HostSessionState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut state = SessionFields::deserialize(deserializer)?;
        for (id, approval) in &state.pending_approvals {
            state
                .approval_states
                .entry(id.clone())
                .or_insert_with(|| ApprovalState {
                    approval: approval.clone(),
                    status: ApprovalStatus::Pending,
                });
        }
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
            approval_states: BTreeMap::new(),
            outbox: Outbox::new(host_id),
            connection: ConnectionState::Offline,
            snapshot_transfer: Default::default(),
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

    pub fn approval_states(&self) -> &BTreeMap<RequestId, ApprovalState> {
        &self.approval_states
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
        self.snapshot_transfer.reset();
        Ok(())
    }

    pub(crate) fn snapshot_frame(
        &mut self,
        host: &HostId,
        frame: mote_protocol::HostFrame,
    ) -> Result<Option<mote_protocol::HostFrame>, HostError> {
        self.snapshot_transfer.accept(host, frame)
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
        if snapshot.cursor.seq < self.cursor.seq {
            return Err(HostError::new(
                HostErrorCode::InvalidCursor,
                "snapshot precedes committed cursor",
            ));
        }
        let mut next = self.clone();
        next.snapshot_transfer.reset();
        next.cursor = snapshot.cursor;
        next.threads.clear();
        next.surfaces.clear();
        next.pending_approvals.clear();
        next.approval_states
            .retain(|_, state| state.status != ApprovalStatus::Pending);
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
            if self
                .pending_approvals
                .get(&approval.approval_id)
                .is_some_and(|previous| previous != &approval)
            {
                return Err(invalid_approval("approval authority changed"));
            }
            if next.approval_states.contains_key(&approval.approval_id) {
                return Err(invalid_approval("terminal approval cannot become pending"));
            }
            next.approval_states.insert(
                approval.approval_id.clone(),
                ApprovalState {
                    approval: approval.clone(),
                    status: ApprovalStatus::Pending,
                },
            );
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
        if matches!(event.kind.as_str(), "approval.created" | "approval.updated") {
            let envelope: ApprovalEnvelope = serde_json::from_value(event.payload.clone())
                .map_err(|_| invalid_approval("invalid approval envelope"))?;
            require_host(host_id, &envelope.host_id)?;
            if event.scope_id != envelope.approval.thread {
                return Err(invalid_approval("approval thread mismatch"));
            }
            if event.kind == "approval.created" && envelope.status != ApprovalStatus::Pending {
                return Err(invalid_approval("created approval must be pending"));
            }
            let previous_state = self.approval_states.get(&envelope.approval.approval_id);
            if previous_state.is_some_and(|state| {
                state.status != ApprovalStatus::Pending && state.status != envelope.status
            }) {
                return Err(invalid_approval("terminal approval status cannot change"));
            }
            let previous = previous_state
                .map(|state| &state.approval)
                .or_else(|| self.pending_approvals.get(&envelope.approval.approval_id));
            if let Some(previous) = previous {
                if previous != &envelope.approval {
                    return Err(invalid_approval("approval authority changed"));
                }
            }
            self.approval_states.insert(
                envelope.approval.approval_id.clone(),
                ApprovalState {
                    approval: envelope.approval.clone(),
                    status: envelope.status,
                },
            );
            if envelope.status == ApprovalStatus::Pending {
                self.pending_approvals
                    .insert(envelope.approval.approval_id.clone(), envelope.approval);
            } else {
                self.pending_approvals
                    .remove(&envelope.approval.approval_id);
            }
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
                    if self
                        .approval_states
                        .get(&id)
                        .is_some_and(|state| state.status == ApprovalStatus::Pending)
                    {
                        self.approval_states.remove(&id);
                    }
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
        for (id, state) in &self.approval_states {
            if id != &state.approval.approval_id
                || (state.status == ApprovalStatus::Pending
                    && self.pending_approvals.get(id) != Some(&state.approval))
                || (state.status != ApprovalStatus::Pending
                    && self.pending_approvals.contains_key(id))
            {
                return Err(invalid_approval("approval state mismatch"));
            }
        }
        Ok(())
    }
}

fn invalid_state(_: serde_json::Error) -> HostError {
    HostError::new(HostErrorCode::InvalidRequest, "invalid host snapshot")
}

fn invalid_approval(message: &str) -> HostError {
    HostError::new(HostErrorCode::InvalidApproval, message)
}
