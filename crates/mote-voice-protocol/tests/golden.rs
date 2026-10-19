use mote_voice_protocol::{
    decode_command, decode_line, encode_line, Command, Event, Frame, InputMode, ProtocolError,
    Source,
};

#[test]
fn golden_frames_decode_and_round_trip() {
    for line in include_str!("fixtures/overseer.jsonl").lines() {
        let frame = decode_line(line).expect("golden frame must decode");
        let encoded = match &frame {
            Frame::Command(command) => encode_line(command).expect("command must encode"),
            Frame::Event(event) => encode_line(event).expect("event must encode"),
        };
        assert_eq!(
            decode_line(&encoded).expect("encoded frame must decode"),
            frame
        );
    }
}

#[test]
fn production_refuses_every_fixture_command() {
    for line in [
        r#"{"cmd":"feed","pcm":"AA=="}"#,
        r#"{"cmd":"feed_end"}"#,
        r#"{"cmd":"simulate","speech":"hello"}"#,
        r#"{"cmd":"script","lines":[{"start_ms":0,"end_ms":10,"text":"hello"}]}"#,
    ] {
        assert!(matches!(
            decode_command(line, InputMode::Production),
            Err(ProtocolError::FixtureCommandInProduction)
        ));
        assert!(decode_command(line, InputMode::Fixture).is_ok());
    }
}

#[test]
fn production_accepts_control_commands() {
    assert_eq!(
        decode_command(r#"{"cmd":"suppress","ms":250}"#, InputMode::Production).unwrap(),
        Command::Suppress { ms: 250 }
    );
}

#[test]
fn rejects_unknown_fields_malformed_json_and_multiple_frames() {
    assert!(decode_line(r#"{"type":"resumed","t_ms":1,"extra":true}"#).is_err());
    assert!(decode_line("not json").is_err());
    assert!(matches!(
        decode_line("{\"type\":\"resumed\",\"t_ms\":1}\n{\"cmd\":\"quit\"}"),
        Err(ProtocolError::MultipleLines)
    ));
}

#[test]
fn events_contain_only_metadata_and_text() {
    let event = Event::Level {
        source: Source::Agent,
        value: 0.25,
        t_ms: 10,
    };
    let encoded = encode_line(&event).unwrap();
    assert!(!encoded.contains("pcm"));
    assert_eq!(decode_line(&encoded).unwrap(), Frame::Event(event));
}
