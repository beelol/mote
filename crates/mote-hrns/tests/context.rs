use mote_hrns::{
    Compaction, CompactionEngine, CompactionError, CompactionRequest, Compactor, ContextEntry,
    ContextError, ContextRequest, ContextSectionKind, ContextTurn, LedgeredCompaction,
    LocalCompactionClient, Scope, ScopedMemory, SkillContext, TaskContextBuilder, TurnRole,
};

fn entry(source: &str, text: &str) -> ContextEntry {
    ContextEntry::new(source, text)
}

fn turn(role: TurnRole, source: &str, text: &str) -> ContextTurn {
    ContextTurn {
        role,
        entry: entry(source, text),
    }
}

fn request() -> ContextRequest {
    let scope = Scope::new("owner", "personal").unwrap();
    ContextRequest {
        scope: scope.clone(),
        token_budget: 128,
        compaction_budget: 24,
        charter: entry("house.md", "Protect the owner."),
        pinned_instructions: vec![entry("pin:1", "Never submit without approval.")],
        task_state: entry("task:t1", "Compare the supplied notes."),
        memories: vec![ScopedMemory {
            scope,
            entry: entry("memory:sqlite:1:1", "The owner prefers short answers."),
        }],
        committed_actions: vec![entry("action:a1", "Read notes.txt")],
        approvals: vec![entry("approval:a2", "Sending mail is awaiting approval")],
        turns: vec![
            turn(TurnRole::Owner, "turn:1", "Start with the local sources."),
            turn(TurnRole::Assistant, "turn:2", "I found two relevant notes."),
            turn(TurnRole::Owner, "turn:3", "Compare them and cite both."),
        ],
        last_compaction: None,
        skills: vec![
            SkillContext {
                entry: entry("skill:files", "Read local files"),
                selected: true,
                visible: true,
            },
            SkillContext {
                entry: entry("skill:hidden", "Hidden administrative capability"),
                selected: true,
                visible: false,
            },
            SkillContext {
                entry: entry("skill:unused", "Unselected capability"),
                selected: false,
                visible: true,
            },
        ],
    }
}

#[test]
fn deterministic_pack_preserves_order_protected_content_and_visible_skills() {
    let mut builder = TaskContextBuilder::new();
    let first = builder.build(request()).unwrap();
    let second = builder.build(request()).unwrap();

    assert_eq!(first, second);
    assert!(first.token_estimate <= 128);
    assert_eq!(
        first
            .sections
            .iter()
            .map(|section| section.kind)
            .collect::<Vec<_>>(),
        [
            ContextSectionKind::Charter,
            ContextSectionKind::PinnedInstructions,
            ContextSectionKind::TaskState,
            ContextSectionKind::ScopedMemories,
            ContextSectionKind::CommittedActions,
            ContextSectionKind::Approvals,
            ContextSectionKind::NewestTurns,
            ContextSectionKind::CoveredCompaction,
            ContextSectionKind::SelectedSkills,
        ]
    );
    assert_eq!(
        first.entries(ContextSectionKind::Charter)[0].text,
        "Protect the owner."
    );
    assert_eq!(
        first.entries(ContextSectionKind::PinnedInstructions)[0].text,
        "Never submit without approval."
    );
    assert_eq!(
        first
            .entries(ContextSectionKind::NewestTurns)
            .last()
            .unwrap()
            .source_ref,
        "turn:3"
    );
    assert_eq!(
        first
            .entries(ContextSectionKind::SelectedSkills)
            .iter()
            .map(|item| item.source_ref.as_str())
            .collect::<Vec<_>>(),
        ["skill:files"]
    );

    let ledger = first.ledger_pack("think");
    assert_eq!(ledger.parts.len(), 5);
    assert_eq!(ledger.token_estimate(), first.token_estimate);
    assert!(ledger.parts.iter().all(|part| part.hash.len() == 64));
}

#[test]
fn mismatched_realm_or_account_fails_closed() {
    for scope in [
        Scope::new("owner", "work").unwrap(),
        Scope::new("someone-else", "personal").unwrap(),
    ] {
        let mut input = request();
        input.memories.push(ScopedMemory {
            scope,
            entry: entry("memory:decoy", "Secret from another scope"),
        });
        assert_eq!(
            TaskContextBuilder::new().build(input),
            Err(ContextError::ScopeMismatch)
        );
    }
}

struct RawClient {
    response: Result<LedgeredCompaction, CompactionError>,
}

impl LocalCompactionClient for RawClient {
    fn complete(
        &mut self,
        _request: &CompactionRequest,
    ) -> Result<LedgeredCompaction, CompactionError> {
        self.response.clone()
    }
}

#[test]
fn structured_compactor_rejects_malformed_unledgered_and_incomplete_output() {
    let turns = vec![turn(TurnRole::Owner, "turn:1", "old")];
    let cases = [
        LedgeredCompaction {
            output: "not json".into(),
            ledger_id: 7,
        },
        LedgeredCompaction {
            output: r#"{"summary":"old","covered_sources":["turn:1"]}"#.into(),
            ledger_id: 0,
        },
        LedgeredCompaction {
            output: r#"{"summary":"old","covered_sources":[]}"#.into(),
            ledger_id: 7,
        },
    ];
    for response in cases {
        let mut compactor = Compactor::new(RawClient {
            response: Ok(response),
        });
        assert!(compactor
            .compact(CompactionRequest {
                turns: turns.clone(),
                max_tokens: 8
            })
            .is_err());
    }
}

struct FailingCompactor;

impl CompactionEngine for FailingCompactor {
    fn compact(&mut self, _request: CompactionRequest) -> Result<Compaction, CompactionError> {
        Err(CompactionError::Unavailable)
    }
}

#[test]
fn failed_compaction_retains_last_valid_source_covered_compaction() {
    let mut input = request();
    input.token_budget = 70;
    input.turns[0].entry.text = "older owner context ".repeat(20);
    input.turns[1].entry.text = "older assistant context ".repeat(20);
    input.last_compaction = Some(Compaction {
        summary: "Local sources requested; two notes found.".into(),
        covered_sources: vec!["turn:1".into(), "turn:2".into()],
        ledger_id: 41,
    });

    let pack = TaskContextBuilder::with_compactor(FailingCompactor)
        .build(input)
        .unwrap();

    assert!(pack.retained_previous_compaction);
    assert_eq!(pack.compaction_ledger_id, Some(41));
    assert_eq!(
        pack.entries(ContextSectionKind::CoveredCompaction)[0].source_ref,
        "compaction:ledger:41"
    );
    assert_eq!(
        pack.entries(ContextSectionKind::NewestTurns)
            .last()
            .unwrap()
            .source_ref,
        "turn:3"
    );
}

#[test]
fn long_history_without_complete_source_coverage_fails_closed() {
    let mut input = request();
    input.token_budget = 40;
    input.turns[0].entry.text = "older owner context ".repeat(20);
    input.turns[1].entry.text = "older assistant context ".repeat(20);
    input.last_compaction = Some(Compaction {
        summary: "Only one old turn.".into(),
        covered_sources: vec!["turn:1".into()],
        ledger_id: 9,
    });

    assert_eq!(
        TaskContextBuilder::new().build(input),
        Err(ContextError::InvalidCompaction)
    );
}

#[test]
fn protected_content_is_never_truncated_to_force_a_pack() {
    let mut input = request();
    input.token_budget = 3;
    assert_eq!(
        TaskContextBuilder::new().build(input),
        Err(ContextError::ProtectedContentExceedsBudget)
    );
}
