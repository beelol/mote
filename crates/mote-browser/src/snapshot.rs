use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeRef(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserNode {
    pub reference: NodeRef,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserSnapshot {
    pub url: String,
    pub nodes: Vec<BrowserNode>,
    pub text: String,
    pub truncated: bool,
    pub page_state_hash: String,
}

impl BrowserSnapshot {
    pub fn untrusted_text(&self) -> String {
        format!(
            "<untrusted_browser_page>\n{}\n</untrusted_browser_page>",
            self.text
        )
    }
}

pub struct SnapshotMap {
    session: u128,
    generation: u64,
    nodes: HashMap<NodeRef, i64>,
    current: bool,
}

impl Default for SnapshotMap {
    fn default() -> Self {
        Self {
            session: rand::random(),
            generation: 0,
            nodes: HashMap::new(),
            current: false,
        }
    }
}

impl SnapshotMap {
    pub fn begin_document(&mut self) {
        self.generation += 1;
        self.nodes.clear();
        self.current = false;
    }

    pub fn reference(&mut self, backend_id: i64) -> NodeRef {
        let reference = NodeRef(format!(
            "{:032x}:{}:{backend_id}",
            self.session, self.generation
        ));
        self.nodes.insert(reference.clone(), backend_id);
        self.current = true;
        reference
    }

    pub(crate) fn retain_snapshot(&mut self, references: &[NodeRef]) {
        self.nodes
            .retain(|reference, _| references.contains(reference));
        self.current = !self.nodes.is_empty();
    }

    pub fn resolve(&self, reference: &NodeRef) -> Result<i64> {
        if !self.current {
            bail!("a current snapshot is required");
        }
        self.nodes
            .get(reference)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("stale or unknown node reference"))
    }
}
