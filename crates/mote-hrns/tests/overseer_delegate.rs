use mote_hrns::{
    Cursor, DelegateError, DelegatedEventKind, DelegatedTask, FakeOverseerTransport,
    OverseerDelegate, OverseerTransportError, OverseerTransportEvent, PermissionDecision,
    SubmissionReply, MAX_DELEGATE_PROMPT_BYTES, MAX_DELEGATE_TITLE_BYTES,
};
use serde_json::json;

fn task() -> DelegatedTask {
    DelegatedTask {
        repo: "/Users/owner/projects/mote".into(),
        title: "Fix context ordering".into(),
        prompt: "Fix the context ordering bug and run its focused tests.".into(),
    }
}

fn submitted() -> SubmissionReply {
    SubmissionReply {
        task_id: "task-1".into(),
        run_id: "run-1".into(),
        workspace_id: "workspace-1".into(),
    }
}

#[test]
fn submit_is_bounded_and_gives_overseer_worktree_ownership() {
    let mut transport = FakeOverseerTransport::default();
    transport.queue_submission(Ok(submitted()));
    let mut delegate = OverseerDelegate::new(transport);

    let id = delegate.submit(task()).unwrap();

    assert_eq!(id.task_id(), "task-1");
    assert_eq!(id.run_id(), "run-1");
    let calls = delegate.transport().calls();
    let request = calls[0].as_submit().unwrap();
    assert_eq!(request.workspace_mode(), "worktree");
    assert_eq!(request.permission_mode(), "acceptEdits");
    assert_eq!(request.harness(), "claude");
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "repo": "/Users/owner/projects/mote",
            "harness": "claude",
            "prompt": "Fix the context ordering bug and run its focused tests.",
            "title": "Fix context ordering",
            "workspace_mode": "worktree",
            "permission_mode": "acceptEdits"
        })
    );
}

#[test]
fn poll_returns_strictly_new_cursor_ordered_progress_and_completion() {
    let mut transport = FakeOverseerTransport::default();
    transport.queue_submission(Ok(submitted()));
    transport.queue_events(Ok(vec![
        OverseerTransportEvent::progress(4, "run-1", "running", "Reading context.rs"),
        OverseerTransportEvent::completed(
            5,
            "run-1",
            "Fixed context ordering",
            vec!["crates/mote-hrns/src/context.rs".into()],
        ),
    ]));
    let mut delegate = OverseerDelegate::new(transport);
    let id = delegate.submit(task()).unwrap();

    let events = delegate.poll(&id, Cursor::new(3)).unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].cursor(), Cursor::new(4));
    assert_eq!(
        events[0].kind(),
        &DelegatedEventKind::Progress {
            status: "running".into(),
            message: "Reading context.rs".into(),
        }
    );
    assert_eq!(events[1].cursor(), Cursor::new(5));
    assert_eq!(
        events[1].kind(),
        &DelegatedEventKind::Completed {
            summary: "Fixed context ordering".into(),
            changed_paths: vec!["crates/mote-hrns/src/context.rs".into()],
        }
    );
    assert_eq!(
        delegate.transport().calls()[1].cursor(),
        Some(Cursor::new(3))
    );
}

#[test]
fn permission_is_normalized_and_answered_only_through_overseer() {
    let mut transport = FakeOverseerTransport::default();
    transport.queue_submission(Ok(submitted()));
    transport.queue_events(Ok(vec![OverseerTransportEvent::permission(
        8,
        "run-1",
        "permission-1",
        "Bash",
        json!({"command":"cargo test -p mote-hrns"}),
    )]));
    transport.queue_permission(Ok(()));
    let mut delegate = OverseerDelegate::new(transport);
    let id = delegate.submit(task()).unwrap();

    let events = delegate.poll(&id, Cursor::new(7)).unwrap();
    assert_eq!(
        events[0].kind(),
        &DelegatedEventKind::Permission {
            request_id: "permission-1".into(),
            tool: "Bash".into(),
            input: json!({"command":"cargo test -p mote-hrns"}),
        }
    );
    delegate
        .answer_permission(
            &id,
            PermissionDecision {
                request_id: "permission-1".into(),
                allow: false,
                message: "Not needed".into(),
            },
        )
        .unwrap();
    assert_eq!(delegate.transport().calls().len(), 3);
    assert_eq!(
        delegate.transport().calls()[2].permission_allow(),
        Some(false)
    );
}

#[test]
fn unavailable_transport_remains_visible_as_an_error() {
    let mut transport = FakeOverseerTransport::default();
    transport.queue_submission(Err(OverseerTransportError::Unavailable));
    let mut delegate = OverseerDelegate::new(transport);

    assert_eq!(delegate.submit(task()), Err(DelegateError::Unavailable));
}

#[test]
fn oversized_and_malformed_payloads_fail_closed() {
    let mut delegate = OverseerDelegate::new(FakeOverseerTransport::default());
    let mut oversized_prompt = task();
    oversized_prompt.prompt = "x".repeat(MAX_DELEGATE_PROMPT_BYTES + 1);
    assert_eq!(
        delegate.submit(oversized_prompt),
        Err(DelegateError::InvalidTask)
    );

    let mut oversized_title = task();
    oversized_title.title = "x".repeat(MAX_DELEGATE_TITLE_BYTES + 1);
    assert_eq!(
        delegate.submit(oversized_title),
        Err(DelegateError::InvalidTask)
    );

    let mut transport = FakeOverseerTransport::default();
    transport.queue_submission(Ok(submitted()));
    transport.queue_events(Ok(vec![OverseerTransportEvent::progress(
        2,
        "another-run",
        "running",
        "wrong run",
    )]));
    let mut delegate = OverseerDelegate::new(transport);
    let id = delegate.submit(task()).unwrap();
    assert_eq!(
        delegate.poll(&id, Cursor::new(1)),
        Err(DelegateError::InvalidPayload)
    );
}

#[test]
fn delegate_contract_has_no_action_or_worktree_authority() {
    fn normalized_event_has_no_authority(kind: &DelegatedEventKind) {
        match kind {
            DelegatedEventKind::Progress { .. }
            | DelegatedEventKind::Permission { .. }
            | DelegatedEventKind::Completed { .. }
            | DelegatedEventKind::Failed { .. } => {}
        }
    }

    normalized_event_has_no_authority(&DelegatedEventKind::Failed {
        message: "compile failed".into(),
    });
    let delegate = OverseerDelegate::new(FakeOverseerTransport::default());
    assert!(!delegate.owns_worktrees());
    assert!(!delegate.commits_mote_actions());
}
