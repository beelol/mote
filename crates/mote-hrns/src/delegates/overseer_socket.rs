use super::{
    Cursor, OverseerTransport, OverseerTransportError, OverseerTransportEvent, PermissionDecision,
    SubmissionReply, TaskCreateRequest, MAX_DELEGATE_EVENTS_PER_POLL,
};
use mote_ipc::overseer::{Client, Error as ClientError};
use serde_json::{json, Value};

pub struct SocketOverseerTransport {
    client: Client,
}

impl SocketOverseerTransport {
    pub fn local(version: impl Into<String>) -> Self {
        Self::new(Client::new(mote_paths::overseer_socket_path(), version))
    }

    pub const fn new(client: Client) -> Self {
        Self { client }
    }

    pub const fn client(&self) -> &Client {
        &self.client
    }
}

impl OverseerTransport for SocketOverseerTransport {
    fn submit(
        &mut self,
        request: TaskCreateRequest,
    ) -> Result<SubmissionReply, OverseerTransportError> {
        let params = serde_json::to_value(request).map_err(|_| OverseerTransportError::Protocol)?;
        let value = self
            .client
            .connect()
            .map_err(map_client_error)?
            .request("task.create", params)
            .map_err(map_client_error)?;
        if value
            .get("launch_error")
            .is_some_and(|error| !error.is_null())
        {
            return Err(OverseerTransportError::Protocol);
        }
        Ok(SubmissionReply {
            task_id: required_string(&value, "/task/id")?,
            run_id: required_string(&value, "/run/id")?,
            workspace_id: required_string(&value, "/workspace/id")?,
        })
    }

    fn events_after(
        &mut self,
        run_id: &str,
        cursor: Cursor,
    ) -> Result<Vec<OverseerTransportEvent>, OverseerTransportError> {
        self.client
            .connect()
            .map_err(map_client_error)?
            .replay(run_id, cursor.value(), MAX_DELEGATE_EVENTS_PER_POLL)
            .map_err(map_client_error)?
            .into_iter()
            .map(|event| map_event(run_id, event))
            .collect()
    }

    fn answer_permission(
        &mut self,
        run_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), OverseerTransportError> {
        self.client
            .connect()
            .map_err(map_client_error)?
            .request(
                "run.permission",
                json!({
                    "run_id": run_id,
                    "request_id": decision.request_id,
                    "allow": decision.allow,
                    "message": decision.message,
                }),
            )
            .map_err(map_client_error)?;
        Ok(())
    }
}

fn map_event(
    expected_run_id: &str,
    event: Value,
) -> Result<OverseerTransportEvent, OverseerTransportError> {
    let cursor = event
        .get("seq")
        .and_then(Value::as_u64)
        .filter(|cursor| *cursor > 0)
        .ok_or(OverseerTransportError::Protocol)?;
    let run_id = event
        .get("run_id")
        .and_then(Value::as_str)
        .filter(|run_id| *run_id == expected_run_id)
        .ok_or(OverseerTransportError::Protocol)?;
    let kind = event
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(OverseerTransportError::Protocol)?;
    let payload = event
        .get("payload")
        .ok_or(OverseerTransportError::Protocol)?;

    Ok(match kind {
        "permission" => OverseerTransportEvent::permission(
            cursor,
            run_id,
            required_string(payload, "/request_id")?,
            required_string(payload, "/tool")?,
            payload.get("input").cloned().unwrap_or(Value::Null),
        ),
        "turn_done" if payload.get("ok").and_then(Value::as_bool) == Some(true) => {
            OverseerTransportEvent::completed(
                cursor,
                run_id,
                optional_string(payload, "/summary")
                    .unwrap_or_else(|| "Coding task completed".into()),
                Vec::new(),
            )
        }
        "turn_done" | "error" => OverseerTransportEvent::failed(
            cursor,
            run_id,
            optional_string(payload, "/message")
                .or_else(|| optional_string(payload, "/summary"))
                .unwrap_or_else(|| format!("Overseer reported {kind}")),
        ),
        "status" => {
            let status = required_string(payload, "/status")?;
            let message = optional_string(payload, "/why").unwrap_or_else(|| status.clone());
            OverseerTransportEvent::progress(cursor, run_id, status, message)
        }
        "output" => OverseerTransportEvent::progress(
            cursor,
            run_id,
            "running",
            required_string(payload, "/text")?,
        ),
        "tool" => OverseerTransportEvent::progress(
            cursor,
            run_id,
            "running",
            optional_string(payload, "/summary")
                .or_else(|| optional_string(payload, "/name"))
                .unwrap_or_else(|| "Tool running".into()),
        ),
        _ => OverseerTransportEvent::progress(
            cursor,
            run_id,
            kind,
            format!("Overseer event: {kind}"),
        ),
    })
}

fn required_string(value: &Value, pointer: &str) -> Result<String, OverseerTransportError> {
    optional_string(value, pointer).ok_or(OverseerTransportError::Protocol)
}

fn optional_string(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn map_client_error(error: ClientError) -> OverseerTransportError {
    match error {
        ClientError::Unavailable => OverseerTransportError::Unavailable,
        ClientError::Protocol
        | ClientError::Rejected
        | ClientError::Resync
        | ClientError::LimitExceeded => OverseerTransportError::Protocol,
    }
}
