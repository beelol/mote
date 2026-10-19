use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

pub const MAX_DELEGATE_REPO_BYTES: usize = 4 * 1024;
pub const MAX_DELEGATE_TITLE_BYTES: usize = 256;
pub const MAX_DELEGATE_PROMPT_BYTES: usize = 256 * 1024;
pub const MAX_DELEGATE_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_DELEGATE_EVENTS_PER_POLL: usize = 1_000;
pub const MAX_DELEGATE_CHANGED_PATHS: usize = 512;

const MAX_DELEGATE_ID_BYTES: usize = 256;
const MAX_PERMISSION_MESSAGE_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegatedTask {
    pub repo: String,
    pub title: String,
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationId {
    task_id: String,
    run_id: String,
}

impl DelegationId {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor(u64);

impl Cursor {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedEvent {
    cursor: Cursor,
    kind: DelegatedEventKind,
}

impl DelegatedEvent {
    pub const fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub const fn kind(&self) -> &DelegatedEventKind {
        &self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DelegatedEventKind {
    Progress {
        status: String,
        message: String,
    },
    Permission {
        request_id: String,
        tool: String,
        input: Value,
    },
    Completed {
        summary: String,
        changed_paths: Vec<String>,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionDecision {
    pub request_id: String,
    pub allow: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DelegateError {
    Unavailable,
    InvalidTask,
    InvalidPayload,
}

impl std::fmt::Display for DelegateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Overseer is unavailable",
            Self::InvalidTask => "delegated coding task is invalid",
            Self::InvalidPayload => "Overseer returned an invalid payload",
        })
    }
}

impl std::error::Error for DelegateError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskCreateRequest {
    repo: String,
    harness: &'static str,
    prompt: String,
    title: String,
    workspace_mode: &'static str,
    permission_mode: &'static str,
}

impl TaskCreateRequest {
    pub const fn harness(&self) -> &str {
        self.harness
    }

    pub const fn workspace_mode(&self) -> &str {
        self.workspace_mode
    }

    pub const fn permission_mode(&self) -> &str {
        self.permission_mode
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionReply {
    pub task_id: String,
    pub run_id: String,
    pub workspace_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverseerTransportEventKind {
    Progress {
        status: String,
        message: String,
    },
    Permission {
        request_id: String,
        tool: String,
        input: Value,
    },
    Completed {
        summary: String,
        changed_paths: Vec<String>,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverseerTransportEvent {
    cursor: Cursor,
    run_id: String,
    kind: OverseerTransportEventKind,
}

impl OverseerTransportEvent {
    pub fn progress(
        cursor: u64,
        run_id: impl Into<String>,
        status: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            cursor: Cursor::new(cursor),
            run_id: run_id.into(),
            kind: OverseerTransportEventKind::Progress {
                status: status.into(),
                message: message.into(),
            },
        }
    }

    pub fn permission(
        cursor: u64,
        run_id: impl Into<String>,
        request_id: impl Into<String>,
        tool: impl Into<String>,
        input: Value,
    ) -> Self {
        Self {
            cursor: Cursor::new(cursor),
            run_id: run_id.into(),
            kind: OverseerTransportEventKind::Permission {
                request_id: request_id.into(),
                tool: tool.into(),
                input,
            },
        }
    }

    pub fn completed(
        cursor: u64,
        run_id: impl Into<String>,
        summary: impl Into<String>,
        changed_paths: Vec<String>,
    ) -> Self {
        Self {
            cursor: Cursor::new(cursor),
            run_id: run_id.into(),
            kind: OverseerTransportEventKind::Completed {
                summary: summary.into(),
                changed_paths,
            },
        }
    }

    pub fn failed(cursor: u64, run_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            cursor: Cursor::new(cursor),
            run_id: run_id.into(),
            kind: OverseerTransportEventKind::Failed {
                message: message.into(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverseerTransportError {
    Unavailable,
    Protocol,
}

pub trait OverseerTransport: Send {
    fn submit(
        &mut self,
        request: TaskCreateRequest,
    ) -> Result<SubmissionReply, OverseerTransportError>;

    fn events_after(
        &mut self,
        run_id: &str,
        cursor: Cursor,
    ) -> Result<Vec<OverseerTransportEvent>, OverseerTransportError>;

    fn answer_permission(
        &mut self,
        run_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), OverseerTransportError>;
}

pub struct OverseerDelegate<T> {
    transport: T,
}

impl<T: OverseerTransport> OverseerDelegate<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn owns_worktrees(&self) -> bool {
        false
    }

    pub fn commits_mote_actions(&self) -> bool {
        false
    }

    pub const fn transport(&self) -> &T {
        &self.transport
    }

    pub fn submit(&mut self, task: DelegatedTask) -> Result<DelegationId, DelegateError> {
        validate_task(&task)?;
        let request = TaskCreateRequest {
            repo: task.repo,
            harness: "claude",
            prompt: task.prompt,
            title: task.title,
            workspace_mode: "worktree",
            permission_mode: "acceptEdits",
        };
        let reply = self
            .transport
            .submit(request)
            .map_err(map_transport_error)?;
        if !valid_id(&reply.task_id) || !valid_id(&reply.run_id) || !valid_id(&reply.workspace_id) {
            return Err(DelegateError::InvalidPayload);
        }
        Ok(DelegationId {
            task_id: reply.task_id,
            run_id: reply.run_id,
        })
    }

    pub fn poll(
        &mut self,
        id: &DelegationId,
        cursor: Cursor,
    ) -> Result<Vec<DelegatedEvent>, DelegateError> {
        let events = self
            .transport
            .events_after(id.run_id(), cursor)
            .map_err(map_transport_error)?;
        if events.len() > MAX_DELEGATE_EVENTS_PER_POLL {
            return Err(DelegateError::InvalidPayload);
        }

        let mut previous = cursor;
        let mut normalized = Vec::with_capacity(events.len());
        for event in events {
            if event.run_id != id.run_id || event.cursor <= previous {
                return Err(DelegateError::InvalidPayload);
            }
            previous = event.cursor;
            normalized.push(normalize_event(event)?);
        }
        Ok(normalized)
    }

    pub fn answer_permission(
        &mut self,
        id: &DelegationId,
        decision: PermissionDecision,
    ) -> Result<(), DelegateError> {
        if !valid_id(&decision.request_id) || decision.message.len() > MAX_PERMISSION_MESSAGE_BYTES
        {
            return Err(DelegateError::InvalidPayload);
        }
        self.transport
            .answer_permission(id.run_id(), decision)
            .map_err(map_transport_error)
    }
}

fn validate_task(task: &DelegatedTask) -> Result<(), DelegateError> {
    if !task.repo.starts_with('/')
        || !valid_text(&task.repo, MAX_DELEGATE_REPO_BYTES)
        || !valid_text(&task.title, MAX_DELEGATE_TITLE_BYTES)
        || !valid_text(&task.prompt, MAX_DELEGATE_PROMPT_BYTES)
    {
        return Err(DelegateError::InvalidTask);
    }
    Ok(())
}

fn normalize_event(event: OverseerTransportEvent) -> Result<DelegatedEvent, DelegateError> {
    let kind = match event.kind {
        OverseerTransportEventKind::Progress { status, message } => {
            validate_event_text(&status)?;
            validate_event_text(&message)?;
            DelegatedEventKind::Progress { status, message }
        }
        OverseerTransportEventKind::Permission {
            request_id,
            tool,
            input,
        } => {
            if !valid_id(&request_id)
                || !valid_text(&tool, MAX_DELEGATE_TITLE_BYTES)
                || serde_json::to_vec(&input)
                    .map_err(|_| DelegateError::InvalidPayload)?
                    .len()
                    > MAX_DELEGATE_EVENT_BYTES
            {
                return Err(DelegateError::InvalidPayload);
            }
            DelegatedEventKind::Permission {
                request_id,
                tool,
                input,
            }
        }
        OverseerTransportEventKind::Completed {
            summary,
            changed_paths,
        } => {
            validate_event_text(&summary)?;
            if changed_paths.len() > MAX_DELEGATE_CHANGED_PATHS
                || changed_paths
                    .iter()
                    .any(|path| !valid_text(path, MAX_DELEGATE_REPO_BYTES))
            {
                return Err(DelegateError::InvalidPayload);
            }
            DelegatedEventKind::Completed {
                summary,
                changed_paths,
            }
        }
        OverseerTransportEventKind::Failed { message } => {
            validate_event_text(&message)?;
            DelegatedEventKind::Failed { message }
        }
    };
    Ok(DelegatedEvent {
        cursor: event.cursor,
        kind,
    })
}

fn validate_event_text(text: &str) -> Result<(), DelegateError> {
    if valid_text(text, MAX_DELEGATE_EVENT_BYTES) {
        Ok(())
    } else {
        Err(DelegateError::InvalidPayload)
    }
}

fn valid_id(value: &str) -> bool {
    valid_text(value, MAX_DELEGATE_ID_BYTES)
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

fn map_transport_error(error: OverseerTransportError) -> DelegateError {
    match error {
        OverseerTransportError::Unavailable => DelegateError::Unavailable,
        OverseerTransportError::Protocol => DelegateError::InvalidPayload,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverseerTransportCall {
    Submit(TaskCreateRequest),
    Poll {
        run_id: String,
        cursor: Cursor,
    },
    Permission {
        run_id: String,
        decision: PermissionDecision,
    },
}

impl OverseerTransportCall {
    pub const fn as_submit(&self) -> Option<&TaskCreateRequest> {
        match self {
            Self::Submit(request) => Some(request),
            _ => None,
        }
    }

    pub const fn cursor(&self) -> Option<Cursor> {
        match self {
            Self::Poll { cursor, .. } => Some(*cursor),
            _ => None,
        }
    }

    pub const fn permission_allow(&self) -> Option<bool> {
        match self {
            Self::Permission { decision, .. } => Some(decision.allow),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct FakeOverseerTransport {
    submissions: VecDeque<Result<SubmissionReply, OverseerTransportError>>,
    event_batches: VecDeque<Result<Vec<OverseerTransportEvent>, OverseerTransportError>>,
    permission_results: VecDeque<Result<(), OverseerTransportError>>,
    calls: Vec<OverseerTransportCall>,
}

impl FakeOverseerTransport {
    pub fn queue_submission(&mut self, result: Result<SubmissionReply, OverseerTransportError>) {
        self.submissions.push_back(result);
    }

    pub fn queue_events(
        &mut self,
        result: Result<Vec<OverseerTransportEvent>, OverseerTransportError>,
    ) {
        self.event_batches.push_back(result);
    }

    pub fn queue_permission(&mut self, result: Result<(), OverseerTransportError>) {
        self.permission_results.push_back(result);
    }

    pub fn calls(&self) -> &[OverseerTransportCall] {
        &self.calls
    }
}

impl OverseerTransport for FakeOverseerTransport {
    fn submit(
        &mut self,
        request: TaskCreateRequest,
    ) -> Result<SubmissionReply, OverseerTransportError> {
        self.calls.push(OverseerTransportCall::Submit(request));
        self.submissions
            .pop_front()
            .unwrap_or(Err(OverseerTransportError::Unavailable))
    }

    fn events_after(
        &mut self,
        run_id: &str,
        cursor: Cursor,
    ) -> Result<Vec<OverseerTransportEvent>, OverseerTransportError> {
        self.calls.push(OverseerTransportCall::Poll {
            run_id: run_id.into(),
            cursor,
        });
        self.event_batches
            .pop_front()
            .unwrap_or(Err(OverseerTransportError::Unavailable))
    }

    fn answer_permission(
        &mut self,
        run_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), OverseerTransportError> {
        self.calls.push(OverseerTransportCall::Permission {
            run_id: run_id.into(),
            decision,
        });
        self.permission_results
            .pop_front()
            .unwrap_or(Err(OverseerTransportError::Unavailable))
    }
}
