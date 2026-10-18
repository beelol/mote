use mote_harness::{ServerFactory, ThinkEvent};
use mote_hrns::adapters::OpenCodeAdapter;
use mote_hrns::{HarnessKind, HarnessRequest, ModelStep, Scope, Task, TaskId, TaskState};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;

const FLAG: &str = "MOTE_HRNS_OPENCODE_COMPAT";

pub fn enabled() -> bool {
    enabled_value(std::env::var_os(FLAG).as_deref())
}

fn enabled_value(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

pub fn run(
    factory: Arc<dyn ServerFactory>,
    model: &str,
    thread: &str,
    system: &str,
    turn: &str,
    timeout: Duration,
) -> Result<(Vec<ThinkEvent>, Duration), String> {
    let mut adapter = OpenCodeAdapter::new(factory, model)?.with_timeout(timeout);
    let request = HarnessRequest {
        task: Task {
            id: TaskId::new(format!("compat-{thread}"))?,
            scope: Scope::new("owner", "personal")?,
            outcome: turn.into(),
            state: TaskState::Running,
            harness: HarnessKind::Opencode,
        },
        context: system.into(),
        checkpoint: None,
    };
    let (reply, first_token) = adapter
        .step_with_timing(request)
        .map_err(|error| error.to_string())?;
    match reply.step {
        ModelStep::Complete { text, .. } => Ok((
            vec![ThinkEvent::Text { text }, ThinkEvent::Done { ok: true }],
            first_token,
        )),
        ModelStep::Tool { .. } => Err("Mote held the OpenCode tool proposal".into()),
        ModelStep::Message { .. } | ModelStep::WaitForInput { .. } => {
            Err("OpenCode compatibility step did not complete the turn".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_harness::OpenCodeSession;
    use std::sync::Mutex;

    struct Session {
        events: Mutex<Option<Vec<ThinkEvent>>>,
    }

    impl OpenCodeSession for Session {
        fn create_session(&self) -> Result<String, String> {
            Ok("session-1".into())
        }

        fn run_step(
            &self,
            _session: &str,
            _model: &str,
            _system: Option<&str>,
            _prompt: &str,
            _timeout: Duration,
        ) -> Result<(Vec<ThinkEvent>, Duration), String> {
            Ok((
                self.events.lock().unwrap().take().unwrap(),
                Duration::ZERO,
            ))
        }

        fn run_turn(
            &self,
            session: &str,
            model: &str,
            system: Option<&str>,
            prompt: &str,
            timeout: Duration,
        ) -> Result<(Vec<ThinkEvent>, Duration), String> {
            self.run_step(session, model, system, prompt, timeout)
        }
    }

    struct Factory {
        events: Mutex<Option<Vec<ThinkEvent>>>,
    }

    impl ServerFactory for Factory {
        fn start(&self) -> Result<Arc<dyn OpenCodeSession>, String> {
            Ok(Arc::new(Session {
                events: Mutex::new(self.events.lock().unwrap().take()),
            }))
        }
    }

    fn factory(events: Vec<ThinkEvent>) -> Arc<dyn ServerFactory> {
        Arc::new(Factory {
            events: Mutex::new(Some(events)),
        })
    }

    #[test]
    fn flag_is_explicit_and_off_by_default() {
        assert!(!enabled_value(None));
        assert!(!enabled_value(Some(OsStr::new("true"))));
        assert!(enabled_value(Some(OsStr::new("1"))));
    }

    #[test]
    fn completed_adapter_step_preserves_the_existing_think_event_shape() {
        let (events, _) = run(
            factory(vec![
                ThinkEvent::Text {
                    text: "answer".into(),
                },
                ThinkEvent::Done { ok: true },
            ]),
            "ollama/qwen3-coder:30b-64k",
            "main",
            "system",
            "turn",
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(
            events,
            vec![
                ThinkEvent::Text {
                    text: "answer".into(),
                },
                ThinkEvent::Done { ok: true },
            ]
        );
    }

    #[test]
    fn tool_proposal_is_held_instead_of_executed() {
        let error = run(
            factory(vec![ThinkEvent::ToolCall {
                name: "moted_memory_search".into(),
                id: Some("call-1".into()),
                input: serde_json::json!({"query":"red"}),
            }]),
            "ollama/qwen3-coder:30b-64k",
            "main",
            "system",
            "turn",
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error, "Mote held the OpenCode tool proposal");
    }
}
