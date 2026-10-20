use crate::{Scope, TaskId};
use mote_context::{count_tokens, Pack, PartKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpokenLine {
    pub line_id: u64,
    pub task_id: TaskId,
    pub text: String,
    pub played_through_word: usize,
}

impl SpokenLine {
    pub fn new(line_id: u64, task_id: TaskId, text: String) -> Self {
        Self {
            line_id,
            task_id,
            text,
            played_through_word: 0,
        }
    }

    pub fn mark_played_through(&mut self, word_count: usize) {
        self.played_through_word = self.played_through_word.max(word_count).min(self.word_count());
    }

    pub fn played_text(&self) -> String {
        self.text
            .split_whitespace()
            .take(self.played_through_word)
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEntry {
    pub source_ref: String,
    pub text: String,
}

impl ContextEntry {
    pub fn new(source_ref: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            source_ref: source_ref.into(),
            text: text.into(),
        }
    }

    fn tokens(&self) -> usize {
        count_tokens(&self.text)
    }

    fn budget_tokens(&self) -> usize {
        self.tokens().saturating_add(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedMemory {
    pub scope: Scope,
    pub entry: ContextEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    Owner,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextTurn {
    pub role: TurnRole,
    pub entry: ContextEntry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillContext {
    pub entry: ContextEntry,
    pub selected: bool,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compaction {
    pub summary: String,
    pub covered_sources: Vec<String>,
    pub ledger_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionRequest {
    pub turns: Vec<ContextTurn>,
    pub max_tokens: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgeredCompaction {
    pub output: String,
    pub ledger_id: i64,
}

pub trait LocalCompactionClient {
    fn complete(
        &mut self,
        request: &CompactionRequest,
    ) -> Result<LedgeredCompaction, CompactionError>;
}

pub trait CompactionEngine {
    fn compact(&mut self, request: CompactionRequest) -> Result<Compaction, CompactionError>;
}

pub struct Compactor<C> {
    client: C,
}

impl<C> Compactor<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompactionOutput {
    summary: String,
    covered_sources: Vec<String>,
}

impl<C: LocalCompactionClient> CompactionEngine for Compactor<C> {
    fn compact(&mut self, request: CompactionRequest) -> Result<Compaction, CompactionError> {
        let response = self.client.complete(&request)?;
        if response.ledger_id <= 0 {
            return Err(CompactionError::MissingLedger);
        }
        let parsed: CompactionOutput =
            serde_json::from_str(&response.output).map_err(|_| CompactionError::MalformedOutput)?;
        let compaction = Compaction {
            summary: parsed.summary,
            covered_sources: parsed.covered_sources,
            ledger_id: response.ledger_id,
        };
        validate_compaction(&compaction, &request.turns)?;
        if count_tokens(&compaction.summary) > request.max_tokens {
            return Err(CompactionError::OverBudget);
        }
        Ok(compaction)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactionError {
    Unavailable,
    MissingLedger,
    MalformedOutput,
    InvalidCoverage,
    OverBudget,
}

impl std::fmt::Display for CompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Unavailable => "compactor unavailable",
            Self::MissingLedger => "compaction is not ledgered",
            Self::MalformedOutput => "compaction output is malformed",
            Self::InvalidCoverage => "compaction source coverage is invalid",
            Self::OverBudget => "compaction exceeds its token budget",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CompactionError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextError {
    ZeroBudget,
    MissingOwnerTurn,
    ScopeMismatch,
    ProtectedContentExceedsBudget,
    InvalidCompaction,
    InvalidPack,
    UncoveredHistory,
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::ZeroBudget => "context token budget must be greater than zero",
            Self::MissingOwnerTurn => "context has no owner turn",
            Self::ScopeMismatch => "memory scope does not match the task scope",
            Self::ProtectedContentExceedsBudget => "protected context exceeds the token budget",
            Self::InvalidCompaction => "the retained compaction is invalid",
            Self::InvalidPack => "the context pack is invalid",
            Self::UncoveredHistory => "older history cannot be omitted without source coverage",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ContextError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextRequest {
    pub scope: Scope,
    pub token_budget: usize,
    pub compaction_budget: usize,
    pub charter: ContextEntry,
    pub pinned_instructions: Vec<ContextEntry>,
    pub task_state: ContextEntry,
    pub memories: Vec<ScopedMemory>,
    pub committed_actions: Vec<ContextEntry>,
    pub approvals: Vec<ContextEntry>,
    pub turns: Vec<ContextTurn>,
    pub last_compaction: Option<Compaction>,
    pub skills: Vec<SkillContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSectionKind {
    Charter,
    PinnedInstructions,
    TaskState,
    ScopedMemories,
    CommittedActions,
    Approvals,
    NewestTurns,
    CoveredCompaction,
    SelectedSkills,
}

impl ContextSectionKind {
    pub const ORDER: [Self; 9] = [
        Self::Charter,
        Self::PinnedInstructions,
        Self::TaskState,
        Self::ScopedMemories,
        Self::CommittedActions,
        Self::Approvals,
        Self::NewestTurns,
        Self::CoveredCompaction,
        Self::SelectedSkills,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSection {
    pub kind: ContextSectionKind,
    pub entries: Vec<ContextEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPack {
    pub sections: Vec<ContextSection>,
    pub token_estimate: usize,
    pub compaction_ledger_id: Option<i64>,
    pub retained_previous_compaction: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextRenderError {
    InvalidSectionOrder,
    Serialization,
}

impl std::fmt::Display for ContextRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidSectionOrder => "context sections are incomplete or out of order",
            Self::Serialization => "context serialization failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ContextRenderError {}

impl From<ContextRenderError> for ContextError {
    fn from(_: ContextRenderError) -> Self {
        Self::InvalidPack
    }
}

#[derive(Serialize)]
struct HarnessContext<'a> {
    version: u32,
    sections: &'a [ContextSection],
}

impl ContextPack {
    pub fn entries(&self, kind: ContextSectionKind) -> &[ContextEntry] {
        self.sections
            .iter()
            .find(|section| section.kind == kind)
            .map(|section| section.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn render_harness_context(&self) -> Result<String, ContextRenderError> {
        let kinds = self.sections.iter().map(|section| section.kind);
        if self.sections.len() != ContextSectionKind::ORDER.len()
            || !kinds.eq(ContextSectionKind::ORDER)
        {
            return Err(ContextRenderError::InvalidSectionOrder);
        }

        serde_json::to_string(&HarnessContext {
            version: 1,
            sections: &self.sections,
        })
        .map_err(|_| ContextRenderError::Serialization)
    }

    pub fn ledger_pack(&self, lane: impl Into<String>) -> Pack {
        let charter = self.entries(ContextSectionKind::Charter);
        let profile = [
            self.entries(ContextSectionKind::PinnedInstructions),
            self.entries(ContextSectionKind::TaskState),
        ]
        .concat();
        let memory = self.entries(ContextSectionKind::ScopedMemories);
        let working_set = [
            self.entries(ContextSectionKind::CommittedActions),
            self.entries(ContextSectionKind::Approvals),
            self.entries(ContextSectionKind::CoveredCompaction),
            self.entries(ContextSectionKind::SelectedSkills),
        ]
        .concat();
        let turns = self.entries(ContextSectionKind::NewestTurns);
        let groups = [
            (PartKind::Charter, charter),
            (PartKind::Profile, profile.as_slice()),
            (PartKind::Memory, memory),
            (PartKind::WorkingSet, working_set.as_slice()),
            (PartKind::Turn, turns),
        ];
        let parts = groups
            .into_iter()
            .map(|(kind, entries)| {
                let text = entries
                    .iter()
                    .map(|entry| entry.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                (
                    kind,
                    entries
                        .iter()
                        .map(|entry| entry.source_ref.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    text,
                )
            })
            .collect();
        Pack::from_texts(lane, parts)
    }
}

pub struct TaskContextBuilder {
    compactor: Option<Box<dyn CompactionEngine>>,
}

impl Default for TaskContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskContextBuilder {
    pub fn new() -> Self {
        Self { compactor: None }
    }

    pub fn with_compactor(compactor: impl CompactionEngine + 'static) -> Self {
        Self {
            compactor: Some(Box::new(compactor)),
        }
    }

    pub fn build(&mut self, request: ContextRequest) -> Result<ContextPack, ContextError> {
        if request.token_budget == 0 {
            return Err(ContextError::ZeroBudget);
        }
        if request
            .memories
            .iter()
            .any(|memory| memory.scope != request.scope)
        {
            return Err(ContextError::ScopeMismatch);
        }
        let newest_owner_index = request
            .turns
            .iter()
            .rposition(|turn| turn.role == TurnRole::Owner)
            .ok_or(ContextError::MissingOwnerTurn)?;

        let protected_tokens = request.charter.budget_tokens()
            + request
                .pinned_instructions
                .iter()
                .map(ContextEntry::budget_tokens)
                .sum::<usize>()
            + request.task_state.budget_tokens()
            + request.turns[newest_owner_index].entry.budget_tokens();
        if protected_tokens > request.token_budget {
            return Err(ContextError::ProtectedContentExceedsBudget);
        }

        let older_turns: Vec<ContextTurn> = request
            .turns
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != newest_owner_index)
            .map(|(_, turn)| turn.clone())
            .collect();
        let history_tokens = protected_tokens
            + older_turns
                .iter()
                .map(|turn| turn.entry.budget_tokens())
                .sum::<usize>();
        let mut retained_previous = false;
        let mut compaction = None;
        if history_tokens > request.token_budget && !older_turns.is_empty() {
            if let Some(engine) = self.compactor.as_mut() {
                match engine.compact(CompactionRequest {
                    turns: older_turns.clone(),
                    max_tokens: request.compaction_budget,
                }) {
                    Ok(value) => compaction = Some(value),
                    Err(_) => {
                        compaction = validated_previous(request.last_compaction, &older_turns)?;
                        retained_previous = compaction.is_some();
                    }
                }
            } else {
                compaction = validated_previous(request.last_compaction, &older_turns)?;
                retained_previous = compaction.is_some();
            }
            if compaction.is_none() {
                return Err(ContextError::UncoveredHistory);
            }
        }

        let mut remaining = request.token_budget - protected_tokens;
        let compaction_entry = compaction.as_ref().and_then(|value| {
            let entry = ContextEntry::new(
                format!("compaction:ledger:{}", value.ledger_id),
                value.summary.clone(),
            );
            take_one(entry, &mut remaining)
        });
        if compaction.is_some() && compaction_entry.is_none() {
            return Err(ContextError::ProtectedContentExceedsBudget);
        }

        let mut turns = vec![request.turns[newest_owner_index].entry.clone()];
        if compaction.is_none() {
            let mut recent = request
                .turns
                .into_iter()
                .enumerate()
                .filter(|(index, _)| *index != newest_owner_index)
                .map(|(_, turn)| turn.entry)
                .rev()
                .collect::<Vec<_>>();
            let mut selected = take_while_budget(recent.drain(..), &mut remaining);
            selected.reverse();
            turns.splice(0..0, selected);
        }
        let memories = take_while_budget(
            request.memories.into_iter().map(|memory| memory.entry),
            &mut remaining,
        );
        let actions = take_while_budget(request.committed_actions, &mut remaining);
        let approvals = take_while_budget(request.approvals, &mut remaining);
        let skills = take_while_budget(
            request
                .skills
                .into_iter()
                .filter(|skill| skill.selected && skill.visible)
                .map(|skill| skill.entry),
            &mut remaining,
        );

        let compaction_ledger_id = compaction.as_ref().map(|value| value.ledger_id);
        let sections = vec![
            section(ContextSectionKind::Charter, vec![request.charter]),
            section(
                ContextSectionKind::PinnedInstructions,
                request.pinned_instructions,
            ),
            section(ContextSectionKind::TaskState, vec![request.task_state]),
            section(ContextSectionKind::ScopedMemories, memories),
            section(ContextSectionKind::CommittedActions, actions),
            section(ContextSectionKind::Approvals, approvals),
            section(ContextSectionKind::NewestTurns, turns),
            section(
                ContextSectionKind::CoveredCompaction,
                compaction_entry.into_iter().collect(),
            ),
            section(ContextSectionKind::SelectedSkills, skills),
        ];
        let mut pack = ContextPack {
            sections,
            token_estimate: 0,
            compaction_ledger_id,
            retained_previous_compaction: retained_previous,
        };
        pack.token_estimate = pack.ledger_pack("context").token_estimate();
        debug_assert!(pack.token_estimate <= request.token_budget);
        Ok(pack)
    }
}

fn section(kind: ContextSectionKind, entries: Vec<ContextEntry>) -> ContextSection {
    ContextSection { kind, entries }
}

fn take_one(entry: ContextEntry, remaining: &mut usize) -> Option<ContextEntry> {
    let tokens = entry.budget_tokens();
    if tokens > *remaining {
        return None;
    }
    *remaining -= tokens;
    Some(entry)
}

fn take_while_budget(
    entries: impl IntoIterator<Item = ContextEntry>,
    remaining: &mut usize,
) -> Vec<ContextEntry> {
    let mut selected = Vec::new();
    for entry in entries {
        if let Some(entry) = take_one(entry, remaining) {
            selected.push(entry);
        }
    }
    selected
}

fn validated_previous(
    previous: Option<Compaction>,
    turns: &[ContextTurn],
) -> Result<Option<Compaction>, ContextError> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    validate_compaction(&previous, turns).map_err(|_| ContextError::InvalidCompaction)?;
    Ok(Some(previous))
}

fn validate_compaction(
    compaction: &Compaction,
    turns: &[ContextTurn],
) -> Result<(), CompactionError> {
    if compaction.ledger_id <= 0 || compaction.summary.trim().is_empty() {
        return Err(CompactionError::MalformedOutput);
    }
    let expected: HashSet<&str> = turns
        .iter()
        .map(|turn| turn.entry.source_ref.as_str())
        .collect();
    let covered: HashSet<&str> = compaction
        .covered_sources
        .iter()
        .map(String::as_str)
        .collect();
    if expected.len() != turns.len()
        || covered.len() != compaction.covered_sources.len()
        || expected != covered
    {
        return Err(CompactionError::InvalidCoverage);
    }
    Ok(())
}
