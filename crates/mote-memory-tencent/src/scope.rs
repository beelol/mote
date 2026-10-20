use mote_memory::{MemoryError, MemoryScope};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WireScope {
    pub team_id: String,
    pub agent_id: String,
    pub user_id: String,
}

impl WireScope {
    pub(crate) fn new(team_id: &str, prefix: &str, scope: &MemoryScope) -> Self {
        Self {
            team_id: team_id.to_owned(),
            agent_id: scoped_id(prefix, "agent", scope.realm()),
            user_id: scoped_id(prefix, "user", scope.account_id()),
        }
    }

    pub(crate) fn verify_item(&self, item: &serde_json::Value) -> Result<(), MemoryError> {
        for (field, expected) in [
            ("team_id", &self.team_id),
            ("agent_id", &self.agent_id),
            ("user_id", &self.user_id),
        ] {
            if item
                .get(field)
                .is_some_and(|actual| actual.as_str() != Some(expected))
            {
                return Err(MemoryError::refused(format!(
                    "Tencent returned mismatched {field}"
                )));
            }
        }
        Ok(())
    }
}

fn scoped_id(prefix: &str, kind: &str, value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{prefix}-{kind}-{hash:016x}")
}
