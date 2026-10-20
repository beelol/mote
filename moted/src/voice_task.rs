use super::hrns_native::{NativeTurnOutcome, VoiceSource, VoiceTurnOutcome};
use super::voice::{VoiceAccountingEvent, VoiceOwnerTurn};
use super::Daemon;
use mote_context::ToolLedgerEntry;
use mote_hrns::Scope;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{mpsc, Weak};

pub fn spawn(daemon: Weak<Daemon>, turns: mpsc::Receiver<VoiceOwnerTurn>) {
    std::thread::spawn(move || {
        while let Ok(turn) = turns.recv() {
            let Some(daemon) = daemon.upgrade() else {
                break;
            };
            route(&daemon, turn);
        }
    });
}

pub fn spawn_accounting(daemon: Weak<Daemon>, operations: mpsc::Receiver<VoiceAccountingEvent>) {
    std::thread::spawn(move || {
        let mut speech_rows = HashMap::new();
        while let Ok(operation) = operations.recv() {
            let Some(daemon) = daemon.upgrade() else {
                break;
            };
            let operation_kind = match &operation {
                VoiceAccountingEvent::Recognized { .. } => "stt",
                VoiceAccountingEvent::SpeechStarted { .. }
                | VoiceAccountingEvent::SpeechFinished { .. } => "tts",
            };
            let status = match &operation {
                VoiceAccountingEvent::SpeechFinished { status, .. } => status.as_str(),
                _ => "ok",
            };
            let result = account_operation(
                &daemon.store.lock().unwrap(),
                daemon.host_id.as_str(),
                &mut speech_rows,
                operation,
            );
            let _ = daemon.log.append(
                "voice",
                "voice.accounting",
                &json!({
                    "operation":operation_kind,
                    "status":if result.is_ok() { status } else { "failed" },
                }),
            );
        }
    });
}

fn account_operation(
    store: &mote_context::ContextStore,
    host_id: &str,
    speech_rows: &mut HashMap<u64, i64>,
    operation: VoiceAccountingEvent,
) -> rusqlite::Result<()> {
    match operation {
        VoiceAccountingEvent::Recognized {
            output_tokens,
            audio_ms,
            ..
        } => {
            store.record_tool(&ToolLedgerEntry {
                host_id: host_id.into(),
                thread_id: "main".into(),
                action: "voice.stt".into(),
                input_tokens: 0,
                output_tokens,
                latency_ms: audio_ms,
                status: "ok".into(),
            })?;
        }
        VoiceAccountingEvent::SpeechStarted {
            line_id,
            input_tokens,
        } => {
            let row_id = store.record_tool(&ToolLedgerEntry {
                host_id: host_id.into(),
                thread_id: "main".into(),
                action: "voice.tts".into(),
                input_tokens,
                output_tokens: 0,
                latency_ms: 0,
                status: "pending".into(),
            })?;
            speech_rows.insert(line_id, row_id);
        }
        VoiceAccountingEvent::SpeechFinished {
            line_id,
            latency_ms,
            status,
        } => {
            let Some(row_id) = speech_rows.remove(&line_id) else {
                return Ok(());
            };
            store.complete_tool(row_id, host_id, 0, latency_ms, status.as_str())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod accounting_tests {
    use super::*;
    use crate::voice::VoiceOperationStatus;

    #[test]
    fn speech_operations_use_tool_accounting_without_model_rows_or_packs() {
        let store = mote_context::ContextStore::open_in_memory().unwrap();
        let mut speech_rows = HashMap::new();
        account_operation(
            &store,
            "host",
            &mut speech_rows,
            VoiceAccountingEvent::Recognized {
                session_id: 3,
                utterance_id: 7,
                output_tokens: 4,
                audio_ms: 850,
            },
        )
        .unwrap();
        account_operation(
            &store,
            "host",
            &mut speech_rows,
            VoiceAccountingEvent::SpeechStarted {
                line_id: 9,
                input_tokens: 6,
            },
        )
        .unwrap();
        account_operation(
            &store,
            "host",
            &mut speech_rows,
            VoiceAccountingEvent::SpeechFinished {
                line_id: 9,
                latency_ms: 1200,
                status: VoiceOperationStatus::Stopped,
            },
        )
        .unwrap();

        assert_eq!(store.count_ledger().unwrap(), 0);
        assert_eq!(store.count_packs().unwrap(), 0);
        let rows = store.tool_ledger_since(0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entry.action, "voice.tts");
        assert_eq!(rows[0].entry.input_tokens, 6);
        assert_eq!(rows[0].entry.latency_ms, 1200);
        assert_eq!(rows[0].entry.status, "stopped");
        assert_eq!(rows[1].entry.action, "voice.stt");
        assert_eq!(rows[1].entry.output_tokens, 4);
        assert_eq!(rows[1].entry.latency_ms, 850);
    }
}

fn route(daemon: &Daemon, turn: VoiceOwnerTurn) {
    let source = VoiceSource {
        session_id: turn.session_id,
        utterance_id: turn.utterance_id,
        start_ms: turn.start_ms,
        end_ms: turn.end_ms,
        played_context: turn.played_context,
    };
    let recorded = daemon.log.append(
        "main",
        "turn.user",
        &json!({
            "text":turn.text,
            "source":"voice",
            "voice_session_id":turn.session_id,
            "voice_utterance_id":turn.utterance_id,
        }),
    );
    if recorded.is_err() {
        daemon
            .voice
            .owner_turn_failed(turn.session_id, turn.utterance_id);
        return;
    }
    let scope = match Scope::new("owner", "personal") {
        Ok(scope) => scope,
        Err(_) => {
            daemon
                .voice
                .owner_turn_failed(turn.session_id, turn.utterance_id);
            return;
        }
    };
    let outcome = daemon
        .native
        .lock()
        .unwrap()
        .enqueue_owner_turn(scope, turn.text, source);
    match outcome {
        Ok(VoiceTurnOutcome::Duplicate { task_id }) => {
            let _ = daemon
                .log
                .append("main", "voice.task.duplicate", &json!({"task_id":task_id}));
            daemon
                .voice
                .owner_turn_finished(turn.session_id, turn.utterance_id, false);
        }
        Ok(VoiceTurnOutcome::Queued { task_id }) => {
            let _ = daemon.log.append(
                "main",
                "voice.task",
                &json!({"task_id":task_id,"status":"queued"}),
            );
            daemon
                .voice
                .owner_turn_finished(turn.session_id, turn.utterance_id, true);
        }
        Ok(VoiceTurnOutcome::Enqueued(NativeTurnOutcome::Complete { task_id, text, .. })) => {
            let _ = daemon.log.append(
                "main",
                "voice.task",
                &json!({"task_id":task_id,"status":"complete"}),
            );
            if daemon.voice.speak(task_id, text).is_err() {
                daemon
                    .voice
                    .owner_turn_failed(turn.session_id, turn.utterance_id);
            }
        }
        Ok(VoiceTurnOutcome::Enqueued(outcome)) => {
            let (task_id, status) = match outcome {
                NativeTurnOutcome::WaitingForApproval { task_id, .. } => {
                    (task_id, "waiting_for_approval")
                }
                NativeTurnOutcome::WaitingForInput { task_id, .. } => {
                    (task_id, "waiting_for_input")
                }
                NativeTurnOutcome::Blocked { task_id } => (task_id, "blocked"),
                NativeTurnOutcome::Complete { .. } => unreachable!(),
            };
            let _ = daemon.log.append(
                "main",
                "voice.task",
                &json!({"task_id":task_id,"status":status}),
            );
            daemon
                .voice
                .owner_turn_finished(turn.session_id, turn.utterance_id, true);
        }
        Err(_) => {
            let _ = daemon.log.append(
                "main",
                "voice.task.failed",
                &json!({"reason":"durable voice task failed"}),
            );
            daemon
                .voice
                .owner_turn_failed(turn.session_id, turn.utterance_id);
        }
    }
}
