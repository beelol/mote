use mote_hrns::HarnessKind;

pub type DaemonHarnessRuntime = HarnessRuntime<fn(HarnessKind) -> bool>;

pub fn daemon_runtime(configured: HarnessKind) -> Result<DaemonHarnessRuntime, SwitchError> {
    HarnessRuntime::new(configured, daemon_harness_available)
}

fn daemon_harness_available(harness: HarnessKind) -> bool {
    matches!(harness, HarnessKind::Opencode)
}

pub fn harness_name(harness: HarnessKind) -> &'static str {
    match harness {
        HarnessKind::Opencode => "opencode",
        HarnessKind::Mote => "mote",
    }
}

pub fn configured_harness(value: Option<&serde_json::Value>) -> Result<HarnessKind, SwitchError> {
    match value.and_then(serde_json::Value::as_str) {
        None | Some("opencode") => Ok(HarnessKind::Opencode),
        Some("mote") => Ok(HarnessKind::Mote),
        Some(_) => Err(SwitchError::InvalidConfiguration),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimePhase {
    Checkpoint,
    ModelStep,
    ToolExecution,
    WaitingForApproval,
    ApprovalCommit,
    StoreTransaction,
}

impl RuntimePhase {
    fn can_switch(self) -> bool {
        matches!(self, Self::Checkpoint)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessSelection {
    pub configured: HarnessKind,
    pub active: HarnessKind,
    pub pending: Option<HarnessKind>,
    pub phase: RuntimePhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchStatus {
    Unchanged,
    Applied(HarnessKind),
    Pending(HarnessKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchError {
    InvalidConfiguration,
    Unavailable(HarnessKind),
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfiguration => formatter.write_str("configured harness is invalid"),
            Self::Unavailable(harness) => write!(formatter, "{harness:?} harness is unavailable"),
        }
    }
}

impl std::error::Error for SwitchError {}

pub struct HarnessRuntime<A> {
    selection: HarnessSelection,
    available: A,
}

impl<A> HarnessRuntime<A>
where
    A: Fn(HarnessKind) -> bool,
{
    pub fn new(configured: HarnessKind, available: A) -> Result<Self, SwitchError> {
        if !available(configured) {
            return Err(SwitchError::Unavailable(configured));
        }
        Ok(Self {
            selection: HarnessSelection {
                configured,
                active: configured,
                pending: None,
                phase: RuntimePhase::Checkpoint,
            },
            available,
        })
    }

    pub fn selection(&self) -> HarnessSelection {
        self.selection
    }

    pub fn validate_harness(&self, harness: HarnessKind) -> Result<(), SwitchError> {
        if !(self.available)(harness) {
            return Err(SwitchError::Unavailable(harness));
        }
        Ok(())
    }

    pub fn enter(&mut self, phase: RuntimePhase) {
        self.selection.phase = phase;
    }

    pub fn request_harness(&mut self, harness: HarnessKind) -> Result<SwitchStatus, SwitchError> {
        self.validate_harness(harness)?;
        self.selection.configured = harness;
        if harness == self.selection.active {
            self.selection.pending = None;
            return Ok(SwitchStatus::Unchanged);
        }
        if self.selection.phase.can_switch() {
            self.selection.active = harness;
            self.selection.pending = None;
            return Ok(SwitchStatus::Applied(harness));
        }
        self.selection.pending = Some(harness);
        Ok(SwitchStatus::Pending(harness))
    }

    pub fn checkpoint(&mut self) -> Result<SwitchStatus, SwitchError> {
        self.selection.phase = RuntimePhase::Checkpoint;
        let Some(harness) = self.selection.pending else {
            return Ok(SwitchStatus::Unchanged);
        };
        if !(self.available)(harness) {
            return Err(SwitchError::Unavailable(harness));
        }
        self.selection.pending = None;
        self.selection.active = harness;
        Ok(SwitchStatus::Applied(harness))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> HarnessRuntime<impl Fn(HarnessKind) -> bool> {
        HarnessRuntime::new(HarnessKind::Opencode, |_| true).unwrap()
    }

    #[test]
    fn absent_configuration_resolves_to_opencode_without_mutating_defaults() {
        assert_eq!(configured_harness(None).unwrap(), HarnessKind::Opencode);
        assert_eq!(
            configured_harness(Some(&serde_json::json!("opencode"))).unwrap(),
            HarnessKind::Opencode
        );
        assert_eq!(
            configured_harness(Some(&serde_json::json!("mote"))).unwrap(),
            HarnessKind::Mote
        );
        assert_eq!(
            configured_harness(Some(&serde_json::json!("other"))),
            Err(SwitchError::InvalidConfiguration)
        );
    }

    #[test]
    fn idle_switch_applies_at_the_current_checkpoint() {
        let mut runtime = runtime();
        assert_eq!(
            runtime.request_harness(HarnessKind::Mote).unwrap(),
            SwitchStatus::Applied(HarnessKind::Mote)
        );
        assert_eq!(runtime.selection().active, HarnessKind::Mote);
        assert_eq!(runtime.selection().pending, None);
    }

    #[test]
    fn in_flight_phases_defer_until_the_next_checkpoint() {
        for phase in [
            RuntimePhase::ModelStep,
            RuntimePhase::ToolExecution,
            RuntimePhase::WaitingForApproval,
            RuntimePhase::ApprovalCommit,
            RuntimePhase::StoreTransaction,
        ] {
            let mut runtime = runtime();
            runtime.enter(phase);
            assert_eq!(
                runtime.request_harness(HarnessKind::Mote).unwrap(),
                SwitchStatus::Pending(HarnessKind::Mote)
            );
            assert_eq!(runtime.selection().active, HarnessKind::Opencode);
            assert_eq!(
                runtime.checkpoint().unwrap(),
                SwitchStatus::Applied(HarnessKind::Mote)
            );
            assert_eq!(runtime.selection().active, HarnessKind::Mote);
        }
    }

    #[test]
    fn reverting_a_pending_switch_keeps_the_active_harness() {
        let mut runtime = runtime();
        runtime.enter(RuntimePhase::ModelStep);
        runtime.request_harness(HarnessKind::Mote).unwrap();
        assert_eq!(
            runtime.request_harness(HarnessKind::Opencode).unwrap(),
            SwitchStatus::Unchanged
        );
        assert_eq!(runtime.selection().pending, None);
        assert_eq!(runtime.checkpoint().unwrap(), SwitchStatus::Unchanged);
        assert_eq!(runtime.selection().active, HarnessKind::Opencode);
    }

    #[test]
    fn unavailable_target_never_changes_configured_active_or_pending() {
        let mut runtime =
            HarnessRuntime::new(HarnessKind::Opencode, |kind| kind == HarnessKind::Opencode)
                .unwrap();
        runtime.enter(RuntimePhase::ModelStep);
        assert_eq!(
            runtime.request_harness(HarnessKind::Mote),
            Err(SwitchError::Unavailable(HarnessKind::Mote))
        );
        assert_eq!(
            runtime.selection(),
            HarnessSelection {
                configured: HarnessKind::Opencode,
                active: HarnessKind::Opencode,
                pending: None,
                phase: RuntimePhase::ModelStep,
            }
        );
    }

    #[test]
    fn unavailable_startup_fails_instead_of_falling_back() {
        assert!(matches!(
            HarnessRuntime::new(HarnessKind::Mote, |kind| kind == HarnessKind::Opencode),
            Err(SwitchError::Unavailable(HarnessKind::Mote))
        ));
    }

    #[test]
    fn target_is_checked_again_when_the_checkpoint_is_reached() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let mote_available = Arc::new(AtomicBool::new(true));
        let availability = mote_available.clone();
        let mut runtime = HarnessRuntime::new(HarnessKind::Opencode, move |kind| {
            kind == HarnessKind::Opencode || availability.load(Ordering::Relaxed)
        })
        .unwrap();
        runtime.enter(RuntimePhase::ModelStep);
        runtime.request_harness(HarnessKind::Mote).unwrap();
        mote_available.store(false, Ordering::Relaxed);

        assert_eq!(
            runtime.checkpoint(),
            Err(SwitchError::Unavailable(HarnessKind::Mote))
        );
        assert_eq!(runtime.selection().active, HarnessKind::Opencode);
        assert_eq!(runtime.selection().pending, Some(HarnessKind::Mote));
    }
}
