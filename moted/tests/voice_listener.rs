use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn run(commands: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mote-voice-listener"))
        .args(["--input", "fixture", "--model", "fixture-model.bin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start fixture listener");
    {
        let stdin = child.stdin.as_mut().expect("listener stdin");
        for command in commands {
            writeln!(stdin, "{command}").expect("write fixture command");
        }
        writeln!(stdin, "{}", json!({"cmd": "quit"})).expect("stop listener");
    }
    let output = child.wait_with_output().expect("finish fixture listener");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf-8 listener output")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL event"))
        .collect()
}

fn long_fixture(seconds: f32) -> Vec<Value> {
    vec![
        json!({
            "cmd": "script",
            "lines": [{"start_ms": 0, "end_ms": (seconds * 1000.0) as u64, "text": "keep the complete long request together"}]
        }),
        json!({"cmd": "simulate", "speechlike": seconds, "gain": 0.3}),
        json!({"cmd": "feed_end"}),
    ]
}

#[test]
fn held_audio_is_evicted_at_thirty_seconds() {
    let events = run(&long_fixture(70.0));
    let held = events
        .iter()
        .find(|event| event["type"] == "end")
        .and_then(|event| event["held_max_ms"].as_u64())
        .expect("end event reports held audio");
    assert!((25_000..=30_000).contains(&held), "held {held} ms");
    assert!(
        events.iter().any(|event| event["type"] == "utterance"),
        "the segmented utterance still completes"
    );
}

#[test]
fn utterance_terminates_at_ninety_seconds() {
    let events = run(&long_fixture(91.0));
    let utterance = events
        .iter()
        .find(|event| event["type"] == "utterance")
        .expect("long utterance terminates");
    let start = utterance["start_ms"].as_u64().expect("start time");
    let end = utterance["end_ms"].as_u64().expect("end time");
    assert!(end.saturating_sub(start) <= 90_000, "{start}..{end}");
}

#[test]
fn malformed_commands_fail_closed_and_the_listener_continues() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mote-voice-listener"))
        .args(["--input", "fixture", "--model", "fixture-model.bin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start fixture listener");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "not json").unwrap();
        writeln!(stdin, "{}", json!({"cmd": "feed", "pcm": "%%%"})).unwrap();
        writeln!(stdin, "{}", json!({"cmd": "quit"})).unwrap();
    }
    let output = child.wait_with_output().expect("listener output");
    assert!(output.status.success());
    let events: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["type"] == "error")
            .count(),
        2
    );
}

#[test]
fn jsonl_events_never_contain_pcm() {
    let events = run(&long_fixture(2.0));
    fn bounded(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.len() <= 16 && values.iter().all(bounded),
            Value::Object(fields) => {
                !fields.contains_key("pcm")
                    && !fields.contains_key("samples")
                    && fields.values().all(bounded)
            }
            Value::String(text) => text.len() <= 4_096,
            _ => true,
        }
    }
    assert!(
        events.iter().all(bounded),
        "event could carry audio: {events:?}"
    );
}

#[test]
fn listener_source_has_no_outbound_network_implementation() {
    let source = include_str!("../src/bin/mote-voice-listener.rs");
    for forbidden in [
        "TcpStream",
        "UdpSocket",
        "reqwest",
        "ureq",
        "hyper::",
        "std::net",
    ] {
        assert!(!source.contains(forbidden), "found {forbidden}");
    }
}
