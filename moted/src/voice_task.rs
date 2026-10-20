use super::hrns_native::{NativeTurnOutcome, VoiceSource, VoiceTurnOutcome};
use super::voice::VoiceOwnerTurn;
use super::Daemon;
use mote_hrns::Scope;
use serde_json::json;
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

fn route(daemon: &Daemon, turn: VoiceOwnerTurn) {
    let source = VoiceSource {
        session_id: turn.session_id,
        utterance_id: turn.utterance_id,
        start_ms: turn.start_ms,
        end_ms: turn.end_ms,
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
    let outcome = daemon.native.lock().unwrap().enqueue_owner_turn(
        scope,
        turn.text,
        source,
    );
    match outcome {
        Ok(VoiceTurnOutcome::Duplicate { task_id }) => {
            let _ = daemon.log.append(
                "main",
                "voice.task.duplicate",
                &json!({"task_id":task_id}),
            );
            daemon
                .voice
                .owner_turn_finished(turn.session_id, turn.utterance_id, false);
        }
        Ok(VoiceTurnOutcome::Enqueued(outcome)) => {
            let (task_id, status, waiting) = match outcome {
                NativeTurnOutcome::Complete { task_id, .. } => (task_id, "complete", false),
                NativeTurnOutcome::WaitingForApproval { task_id, .. } => {
                    (task_id, "waiting_for_approval", true)
                }
                NativeTurnOutcome::WaitingForInput { task_id, .. } => {
                    (task_id, "waiting_for_input", true)
                }
                NativeTurnOutcome::Blocked { task_id } => (task_id, "blocked", true),
            };
            let _ = daemon.log.append(
                "main",
                "voice.task",
                &json!({"task_id":task_id,"status":status}),
            );
            daemon
                .voice
                .owner_turn_finished(turn.session_id, turn.utterance_id, waiting);
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
