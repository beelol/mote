use mote_hrns::*;

#[test]
fn both_harnesses_follow_the_same_guarded_task_lifecycle() {
    for harness in [HarnessKind::Opencode, HarnessKind::Mote] {
        let mut task = Task {
            id: TaskId::new("t").unwrap(),
            scope: Scope::new("owner", "personal").unwrap(),
            outcome: "Research".into(),
            state: TaskState::Queued,
            harness,
        };
        for next in [
            TaskState::Running,
            TaskState::WaitingForApproval,
            TaskState::Running,
            TaskState::WaitingForInput,
            TaskState::Running,
            TaskState::Blocked,
            TaskState::Running,
            TaskState::Complete,
        ] {
            task.transition(task.state, next).unwrap();
            assert_eq!(task.state, next);
        }
        assert!(task
            .transition(TaskState::Complete, TaskState::Running)
            .is_err());
        assert_eq!(task.state, TaskState::Complete);
    }
}

#[test]
fn stale_updates_and_terminal_replays_cannot_change_state() {
    for terminal in [TaskState::Complete, TaskState::Failed, TaskState::Cancelled] {
        let mut task = Task {
            id: TaskId::new("t").unwrap(),
            scope: Scope::new("owner", "work").unwrap(),
            outcome: "Research".into(),
            state: TaskState::Running,
            harness: HarnessKind::Mote,
        };
        assert!(task.transition(TaskState::Queued, terminal).is_err());
        assert_eq!(task.state, TaskState::Running);
        task.transition(TaskState::Running, terminal).unwrap();
        assert!(task.transition(terminal, terminal).is_err());
        assert!(task.transition(terminal, TaskState::Queued).is_err());
    }
}

#[test]
fn approvals_cannot_skip_ready_and_committed_actions_never_restart() {
    let mut action = Action {
        id: ActionId::new("a").unwrap(),
        task_id: TaskId::new("t").unwrap(),
        state: ActionState::Proposed,
        name: "send".into(),
        arguments: Default::default(),
    };
    assert!(action
        .transition(ActionState::Proposed, ActionState::Running)
        .is_err());
    for next in [
        ActionState::AwaitingApproval,
        ActionState::Ready,
        ActionState::Running,
        ActionState::Committed,
    ] {
        action.transition(action.state, next).unwrap();
    }
    for next in [
        ActionState::Ready,
        ActionState::Running,
        ActionState::Committed,
    ] {
        assert!(action.transition(ActionState::Committed, next).is_err());
    }
}

#[test]
fn denied_failed_and_cancelled_actions_are_terminal() {
    for (from, terminal) in [
        (ActionState::AwaitingApproval, ActionState::Denied),
        (ActionState::Running, ActionState::Failed),
        (ActionState::Ready, ActionState::Cancelled),
    ] {
        let mut action = Action {
            id: ActionId::new("a").unwrap(),
            task_id: TaskId::new("t").unwrap(),
            state: from,
            name: "send".into(),
            arguments: Default::default(),
        };
        action.transition(from, terminal).unwrap();
        assert!(action.transition(terminal, ActionState::Ready).is_err());
    }
}
