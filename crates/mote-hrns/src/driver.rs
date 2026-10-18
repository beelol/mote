use crate::{
    action_request_hash, Action, ActionError, ActionExecutor, ActionId, ActionReceipt, ActionState,
    Checkpoint, Harness, HarnessError, HarnessKind, HarnessRequest, HrnsStore, ModelStep,
    ReceiptStatus, Scope, Task, TaskId, TaskState,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub struct HarnessAdapters {
    pub opencode: Box<dyn Harness>,
    pub mote: Box<dyn Harness>,
}

impl HarnessAdapters {
    fn selected(&mut self, kind: HarnessKind) -> &mut dyn Harness {
        match kind {
            HarnessKind::Opencode => self.opencode.as_mut(),
            HarnessKind::Mote => self.mote.as_mut(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTask {
    pub id: TaskId,
    pub scope: Scope,
    pub outcome: String,
    pub harness: HarnessKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverOutcome {
    Idle,
    Started(Task),
    Message {
        task_id: TaskId,
        text: String,
    },
    Proposed(Action),
    AwaitingApproval {
        task: Task,
        action_id: ActionId,
    },
    WaitingForInput {
        task: Task,
        question: String,
    },
    Blocked(Task),
    Resumed(Task),
    Claimed(Action),
    Committed {
        action: Action,
        receipt: ActionReceipt,
    },
    Completed {
        task: Task,
        text: String,
        sources: Vec<String>,
    },
    Cancelled(Task),
    Recovered {
        checkpoints: usize,
    },
}

#[derive(Debug)]
pub enum DriverError {
    Store(rusqlite::Error),
    Harness(HarnessError),
    Action(ActionError),
    InvalidReceipt,
    ActionInFlight,
    CorruptState,
    UnknownTask,
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "task store: {error}"),
            Self::Harness(error) => write!(formatter, "harness: {error}"),
            Self::Action(error) => write!(formatter, "action: {error}"),
            Self::InvalidReceipt => formatter.write_str("executor returned an invalid receipt"),
            Self::ActionInFlight => formatter.write_str("cannot cancel an executing action"),
            Self::CorruptState => formatter.write_str("durable task state is incomplete"),
            Self::UnknownTask => formatter.write_str("task does not exist"),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<rusqlite::Error> for DriverError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(error)
    }
}

pub struct Driver {
    store: HrnsStore,
    adapters: HarnessAdapters,
    executor: Box<dyn ActionExecutor>,
    claimed: HashSet<ActionId>,
}

impl Driver {
    pub fn new(
        store: HrnsStore,
        adapters: HarnessAdapters,
        executor: Box<dyn ActionExecutor>,
    ) -> Self {
        Self {
            store,
            adapters,
            executor,
            claimed: HashSet::new(),
        }
    }

    pub fn store(&self) -> &HrnsStore {
        &self.store
    }

    pub fn enqueue(&self, task: NewTask) -> Result<Task, DriverError> {
        Ok(self
            .store
            .enqueue(task.id, task.scope, task.outcome, task.harness)?)
    }

    pub fn cancel(&self, id: &TaskId) -> Result<DriverOutcome, DriverError> {
        let (task, revision) = self
            .store
            .task_records()?
            .into_iter()
            .find(|(task, _)| task.id == *id)
            .ok_or(DriverError::UnknownTask)?;
        if let Some((action, action_revision)) = self.store.latest_action(&task.scope, &task.id)? {
            match action.state {
                ActionState::Running => return Err(DriverError::ActionInFlight),
                ActionState::Proposed | ActionState::AwaitingApproval | ActionState::Ready => {
                    self.store.transition_action(
                        &task.scope,
                        &action.id,
                        action_revision,
                        ActionState::Cancelled,
                    )?;
                }
                ActionState::Committed
                | ActionState::Denied
                | ActionState::Failed
                | ActionState::Cancelled => {}
            }
        }
        let cancelled =
            self.store
                .transition(&task.scope, &task.id, revision, TaskState::Cancelled)?;
        self.checkpoint(&cancelled, self.latest_action_id(&cancelled)?, None)?;
        Ok(DriverOutcome::Cancelled(cancelled))
    }

    pub fn recover(&self) -> Result<DriverOutcome, DriverError> {
        let mut checkpoints = 0;
        for (task, _) in self.store.task_records()? {
            if task.state == TaskState::Queued {
                continue;
            }
            let latest = self.store.latest_action(&task.scope, &task.id)?;
            if task.state.is_terminal()
                && latest.as_ref().is_some_and(|(action, _)| {
                    matches!(
                        action.state,
                        ActionState::Proposed
                            | ActionState::AwaitingApproval
                            | ActionState::Ready
                            | ActionState::Running
                    )
                })
            {
                return Err(DriverError::CorruptState);
            }
            let last_action = latest.map(|(action, _)| action.id);
            let current = self.store.checkpoint(&task.scope, &task.id)?;
            let stale = current.as_ref().is_none_or(|checkpoint| {
                checkpoint.state != task.state || checkpoint.last_action != last_action
            });
            if stale {
                self.checkpoint(&task, last_action, None)?;
                checkpoints += 1;
            }
        }
        Ok(DriverOutcome::Recovered { checkpoints })
    }

    pub fn step(&mut self) -> Result<DriverOutcome, DriverError> {
        let Some((task, revision)) = self.store.active_record()? else {
            let Some((task, revision)) = self.store.next_queued_record()? else {
                return Ok(DriverOutcome::Idle);
            };
            let started =
                self.store
                    .transition(&task.scope, &task.id, revision, TaskState::Running)?;
            self.checkpoint(&started, None, None)?;
            return Ok(DriverOutcome::Started(started));
        };
        self.step_active(task, revision)
    }

    fn step_active(&mut self, task: Task, revision: u64) -> Result<DriverOutcome, DriverError> {
        let latest = self.store.latest_action(&task.scope, &task.id)?;
        if let Some((action, action_revision)) = latest {
            if let Some(outcome) = self.step_action(&task, revision, action, action_revision)? {
                return Ok(outcome);
            }
        }
        if task.state != TaskState::Running {
            return self.waiting_outcome(task);
        }

        let checkpoint = self.store.checkpoint(&task.scope, &task.id)?;
        let request = HarnessRequest {
            task: task.clone(),
            context: task.outcome.clone(),
            checkpoint,
        };
        let reply = match self.adapters.selected(task.harness).step(request) {
            Ok(reply) => reply,
            Err(error) => {
                let failed =
                    self.store
                        .transition(&task.scope, &task.id, revision, TaskState::Failed)?;
                self.checkpoint(&failed, self.latest_action_id(&failed)?, None)?;
                return Err(DriverError::Harness(error));
            }
        };
        self.apply_model_step(task, revision, reply)
    }

    fn step_action(
        &mut self,
        task: &Task,
        task_revision: u64,
        action: Action,
        action_revision: u64,
    ) -> Result<Option<DriverOutcome>, DriverError> {
        match action.state {
            ActionState::Proposed => {
                let action = self.store.transition_action(
                    &task.scope,
                    &action.id,
                    action_revision,
                    ActionState::AwaitingApproval,
                )?;
                let waiting = self.store.transition(
                    &task.scope,
                    &task.id,
                    task_revision,
                    TaskState::WaitingForApproval,
                )?;
                self.checkpoint(&waiting, Some(action.id.clone()), None)?;
                Ok(Some(DriverOutcome::AwaitingApproval {
                    task: waiting,
                    action_id: action.id,
                }))
            }
            ActionState::AwaitingApproval => {
                if task.state == TaskState::Running {
                    let waiting = self.store.transition(
                        &task.scope,
                        &task.id,
                        task_revision,
                        TaskState::WaitingForApproval,
                    )?;
                    self.checkpoint(&waiting, Some(action.id.clone()), None)?;
                    Ok(Some(DriverOutcome::AwaitingApproval {
                        task: waiting,
                        action_id: action.id,
                    }))
                } else {
                    Ok(Some(DriverOutcome::AwaitingApproval {
                        task: task.clone(),
                        action_id: action.id,
                    }))
                }
            }
            ActionState::Ready => {
                if task.state == TaskState::WaitingForApproval {
                    let resumed = self.store.transition(
                        &task.scope,
                        &task.id,
                        task_revision,
                        TaskState::Running,
                    )?;
                    self.checkpoint(&resumed, Some(action.id), None)?;
                    return Ok(Some(DriverOutcome::Resumed(resumed)));
                }
                let claimed = self.store.claim_action(
                    &task.scope,
                    &action.id,
                    action_revision,
                )?;
                self.claimed.insert(claimed.id.clone());
                self.checkpoint(task, Some(claimed.id.clone()), None)?;
                Ok(Some(DriverOutcome::Claimed(claimed)))
            }
            ActionState::Running => {
                if !self.claimed.remove(&action.id) {
                    let failed = self.store.fail_action(
                        &task.scope,
                        &action.id,
                        action_revision,
                    )?;
                    self.checkpoint(task, Some(failed.id), None)?;
                    return Err(DriverError::Action(ActionError::Indeterminate));
                }
                let receipt = match self.executor.execute(&action) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        let failed = self.store.fail_action(
                            &task.scope,
                            &action.id,
                            action_revision,
                        )?;
                        self.checkpoint(task, Some(failed.id), None)?;
                        return Err(DriverError::Action(error));
                    }
                };
                if receipt.action_id != action.id
                    || receipt.status != ReceiptStatus::Committed
                    || receipt.request_hash != action_request_hash(&action)
                {
                    return Err(DriverError::InvalidReceipt);
                }
                let (committed, receipt) = self.store.commit_action(
                    &task.scope,
                    &action.id,
                    action_revision,
                    receipt,
                )?;
                self.checkpoint(task, Some(committed.id.clone()), None)?;
                Ok(Some(DriverOutcome::Committed {
                    action: committed,
                    receipt,
                }))
            }
            ActionState::Denied
            | ActionState::Failed
            | ActionState::Cancelled
            | ActionState::Committed => {
                if task.state == TaskState::WaitingForApproval {
                    let resumed = self.store.transition(
                        &task.scope,
                        &task.id,
                        task_revision,
                        TaskState::Running,
                    )?;
                    self.checkpoint(&resumed, Some(action.id), None)?;
                    Ok(Some(DriverOutcome::Resumed(resumed)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn apply_model_step(
        &self,
        task: Task,
        revision: u64,
        reply: crate::HarnessReply,
    ) -> Result<DriverOutcome, DriverError> {
        let adapter = reply.adapter_checkpoint;
        match reply.step {
            ModelStep::Message { text } => {
                let mut checkpoint = self.next_checkpoint(
                    &task,
                    self.latest_action_id(&task)?,
                    Some(ModelStep::Message { text: text.clone() }),
                )?;
                checkpoint.adapter = adapter;
                self.store
                    .record_step(&task.scope, &task.id, revision, None, checkpoint)?;
                Ok(DriverOutcome::Message {
                    task_id: task.id,
                    text,
                })
            }
            ModelStep::Tool { name, arguments } => {
                let action = self.next_action(&task, name.clone(), arguments.clone())?;
                let mut checkpoint = self.next_checkpoint(
                    &task,
                    Some(action.id.clone()),
                    Some(ModelStep::Tool { name, arguments }),
                )?;
                checkpoint.adapter = adapter;
                self.store.propose_action_with_checkpoint(
                    &task.scope,
                    action.clone(),
                    revision,
                    checkpoint,
                )?;
                Ok(DriverOutcome::Proposed(action))
            }
            ModelStep::Complete { text, sources } => {
                let mut projected = task.clone();
                projected.state = TaskState::Complete;
                let mut checkpoint = self.next_checkpoint(
                    &projected,
                    self.latest_action_id(&task)?,
                    Some(ModelStep::Complete {
                        text: text.clone(),
                        sources: sources.clone(),
                    }),
                )?;
                checkpoint.adapter = adapter;
                let (completed, _) = self.store.record_step(
                    &task.scope,
                    &task.id,
                    revision,
                    Some(TaskState::Complete),
                    checkpoint,
                )?;
                Ok(DriverOutcome::Completed {
                    task: completed,
                    text,
                    sources,
                })
            }
            ModelStep::WaitForInput { question } => {
                let mut projected = task.clone();
                projected.state = TaskState::WaitingForInput;
                let mut checkpoint = self.next_checkpoint(
                    &projected,
                    self.latest_action_id(&task)?,
                    Some(ModelStep::WaitForInput {
                        question: question.clone(),
                    }),
                )?;
                checkpoint.adapter = adapter;
                let (waiting, _) = self.store.record_step(
                    &task.scope,
                    &task.id,
                    revision,
                    Some(TaskState::WaitingForInput),
                    checkpoint,
                )?;
                Ok(DriverOutcome::WaitingForInput {
                    task: waiting,
                    question,
                })
            }
        }
    }

    fn next_action(
        &self,
        task: &Task,
        name: String,
        arguments: Map<String, Value>,
    ) -> Result<Action, DriverError> {
        let index = self.store.actions(&task.scope, &task.id)?.len() + 1;
        let id = ActionId::new(format!("{}:{index}", task.id.as_str()))
            .map_err(|_| DriverError::InvalidReceipt)?;
        Ok(Action {
            id,
            task_id: task.id.clone(),
            state: ActionState::Proposed,
            name,
            arguments,
        })
    }

    fn latest_action_id(&self, task: &Task) -> Result<Option<ActionId>, DriverError> {
        Ok(self
            .store
            .latest_action(&task.scope, &task.id)?
            .map(|(action, _)| action.id))
    }

    fn checkpoint(
        &self,
        task: &Task,
        last_action: Option<ActionId>,
        step: Option<ModelStep>,
    ) -> Result<Checkpoint, DriverError> {
        let checkpoint = self.next_checkpoint(task, last_action, step)?;
        Ok(self.store.write_checkpoint(&task.scope, checkpoint)?)
    }

    fn next_checkpoint(
        &self,
        task: &Task,
        last_action: Option<ActionId>,
        step: Option<ModelStep>,
    ) -> Result<Checkpoint, DriverError> {
        let current = self.store.checkpoint(&task.scope, &task.id)?;
        let revision = current
            .as_ref()
            .map_or(0, |checkpoint| checkpoint.revision + 1);
        let adapter = current.and_then(|checkpoint| checkpoint.adapter);
        Ok(Checkpoint {
            task_id: task.id.clone(),
            revision,
            state: task.state,
            last_action,
            step,
            adapter,
        })
    }

    fn waiting_outcome(&self, task: Task) -> Result<DriverOutcome, DriverError> {
        match task.state {
            TaskState::WaitingForApproval => {
                let action_id = self
                    .latest_action_id(&task)?
                    .ok_or(DriverError::CorruptState)?;
                Ok(DriverOutcome::AwaitingApproval { task, action_id })
            }
            TaskState::WaitingForInput => {
                let checkpoint = self
                    .store
                    .checkpoint(&task.scope, &task.id)?
                    .ok_or(DriverError::CorruptState)?;
                let Some(ModelStep::WaitForInput { question }) = checkpoint.step else {
                    return Err(DriverError::CorruptState);
                };
                Ok(DriverOutcome::WaitingForInput { task, question })
            }
            TaskState::Blocked => Ok(DriverOutcome::Blocked(task)),
            _ => Err(DriverError::CorruptState),
        }
    }
}
