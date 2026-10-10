use matrix_lang::{parse_surface, CandidateStatus, CandidateStream, DiagnosticCode};
use std::{fs, path::Path};

fn blocks(markdown: &str) -> Vec<&str> {
    markdown
        .split("```\n")
        .enumerate()
        .filter_map(|(index, block)| (index % 2 == 1).then_some(block.trim()))
        .collect()
}

#[test]
fn documented_candidates_remain_incomplete_at_every_extendible_byte() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../matrix");
    let mut sources = Vec::new();
    let examples = fs::read_to_string(root.join("examples.md")).unwrap();
    for source in blocks(&examples) {
        if !source.starts_with('{') && !source.starts_with("PullRequest {") {
            sources.push(source.to_owned());
        }
    }
    for entry in fs::read_dir(root.join("recipes")).unwrap() {
        let markdown = fs::read_to_string(entry.unwrap().path()).unwrap();
        for source in blocks(&markdown) {
            if !source.starts_with("PullRequest {") {
                sources.push(source.to_owned());
            }
        }
    }
    for entry in fs::read_dir(root.join("conformance/v0")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "matrix")
        {
            sources.push(fs::read_to_string(path).unwrap());
        }
    }
    for source in sources {
        let surface = if source.starts_with("screen ") || source.starts_with("screen{") {
            source
        } else {
            format!("screen {{ {source} }}")
        };
        assert!(parse_surface(&surface).is_ok(), "{surface}");
        for cut in 0..surface.len() {
            let mut stream = CandidateStream::new();
            let status = stream.push(&surface.as_bytes()[..cut]);
            let prefix_parses = std::str::from_utf8(&surface.as_bytes()[..cut])
                .ok()
                .is_some_and(|prefix| parse_surface(prefix).is_ok());
            assert!(
                if prefix_parses {
                    matches!(status, CandidateStatus::Complete(_))
                } else {
                    matches!(status, CandidateStatus::Incomplete)
                },
                "cut={cut} in {surface}: {status:?}"
            );
        }
    }
}

#[test]
fn every_utf8_byte_prefix_is_noninteractive_until_it_parses() {
    for source in [
        "screen { text \"é 🌙\\n\" button \"Go\" -> Act{x: value} }",
        "screen { text \"\\uD83C\\uDF19\" row nowrap center between { field \"N\" count kind number min -1.2e3 max 0 } }",
        "screen { tabs { tab \"a\" { icon \"warning\" } } }",
    ] {
        for cut in 0..=source.len() {
            let mut stream = CandidateStream::new();
            let status = stream.push(&source.as_bytes()[..cut]);
            let parses = std::str::from_utf8(&source.as_bytes()[..cut])
                .ok()
                .is_some_and(|prefix| parse_surface(prefix).is_ok());
            if parses {
                assert!(matches!(status, CandidateStatus::Complete(_)), "cut={cut}: {status:?}");
            } else {
                assert!(matches!(status, CandidateStatus::Incomplete), "cut={cut}: {status:?}");
            }
            if cut < source.len() {
                let status = stream.push(&source.as_bytes()[cut..]);
                assert!(matches!(status, CandidateStatus::Complete(_)), "cut={cut}: {status:?}");
            }
        }
    }
}

#[test]
fn rejects_invalid_syntax_without_returning_an_ast() {
    let mut stream = CandidateStream::new();
    assert!(matches!(
        stream.push(b"screen { text "),
        CandidateStatus::Incomplete
    ));
    match stream.push(b"# }") {
        CandidateStatus::Rejected(errors) => {
            assert_eq!(errors[0].code, DiagnosticCode::CommentNotAllowed)
        }
        other => panic!("expected rejection, got {other:?}"),
    }
}

#[test]
fn rejects_over_limit_before_buffering_it() {
    let mut stream = CandidateStream::new();
    assert!(matches!(
        stream.push(&vec![b' '; 256 * 1024]),
        CandidateStatus::Incomplete
    ));
    match stream.push(b"x") {
        CandidateStatus::Rejected(errors) => {
            assert_eq!(errors[0].code, DiagnosticCode::SourceTooLong)
        }
        other => panic!("expected rejection, got {other:?}"),
    }
    assert!(matches!(stream.push(b""), CandidateStatus::Rejected(_)));
}

#[test]
fn trailing_tokens_revoke_a_previously_complete_candidate() {
    let mut stream = CandidateStream::new();
    assert!(matches!(
        stream.push(b"screen {}"),
        CandidateStatus::Complete(_)
    ));
    assert!(matches!(
        stream.push(b" garbage"),
        CandidateStatus::Rejected(_)
    ));
}

#[test]
fn malformed_utf8_rejects_and_split_utf8_remains_incomplete() {
    let mut stream = CandidateStream::new();
    assert!(matches!(
        stream.push(b"screen { text \"\xc3"),
        CandidateStatus::Incomplete
    ));
    assert!(matches!(
        stream.push(b"\xa9\" }"),
        CandidateStatus::Complete(_)
    ));
    let mut bad = CandidateStream::new();
    assert!(matches!(
        bad.push(b"screen { text \"\xff"),
        CandidateStatus::Rejected(_)
    ));
}

#[test]
fn impossible_negative_text_bound_rejects_at_end_of_buffer() {
    let mut stream = CandidateStream::new();
    let status = stream.push(b"screen { field \"Name\" name min -1");
    assert!(matches!(status, CandidateStatus::Rejected(_)), "{status:?}");
}

#[test]
fn signed_decimal_ranges_remain_extendible_at_every_byte() {
    for control in ["slider", "stepper", "field"] {
        for (min, max) in [
            ("-2", "-10e-1"),
            ("-2.5", "-100.25e-2"),
            ("-2e-20", "-10E-21"),
            ("-20", "-1e+1"),
            ("-0.2", "-0.10"),
        ] {
            let kind = if control == "field" { " kind number" } else { "" };
            let source = format!("screen {{ {control} \"X\" n{kind} min {min} max {max} }}");
            assert!(parse_surface(&source).is_ok(), "{source}");
            for cut in 0..source.len() {
                let mut stream = CandidateStream::new();
                let status = stream.push(&source.as_bytes()[..cut]);
                assert!(matches!(status, CandidateStatus::Incomplete), "{source}, cut {cut}: {status:?}");
                let status = stream.push(&source.as_bytes()[cut..]);
                assert!(matches!(status, CandidateStatus::Complete(_)), "{source}, cut {cut}: {status:?}");
            }
            let mut stream = CandidateStream::new();
            for (index, byte) in source.bytes().enumerate() {
                let status = stream.push(&[byte]);
                assert!(
                    if index + 1 == source.len() {
                        matches!(status, CandidateStatus::Complete(_))
                    } else {
                        matches!(status, CandidateStatus::Incomplete)
                    },
                    "{source}, byte {index}: {status:?}"
                );
            }
        }
    }
}

#[test]
fn impossible_negative_numeric_constraints_reject_at_end_of_buffer() {
    for source in [
        "screen { field \"X\" n max -1",
        "screen { field \"X\" n min -2 max -10",
        "screen { slider \"X\" n min 0 max -1",
        "screen { slider \"X\" n min -2 max -10e1",
        "screen { slider \"X\" n min -2 max -1 step -1",
        "screen { stepper \"X\" n step -1",
    ] {
        let status = CandidateStream::new().push(source.as_bytes());
        assert!(matches!(status, CandidateStatus::Rejected(_)), "{source}: {status:?}");
    }
}
