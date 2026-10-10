use crate::resolve::{FieldIdentity, NodeIdentity};
use crate::types::{DisplayFormat, Type};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingDefinition {
    pub ty: Type,
    pub display: DisplayFormat,
}

impl From<Type> for BindingDefinition {
    fn from(ty: Type) -> Self {
        Self {
            ty,
            display: DisplayFormat::Canonical,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDefinition {
    pub binding: BindingDefinition,
    pub initial: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordDefinition {
    pub fields: BTreeMap<String, BindingDefinition>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnumDefinition {
    pub members: Vec<String>,
    pub labels: BTreeMap<String, String>,
}

impl EnumDefinition {
    pub fn new(members: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            members: members.into_iter().map(Into::into).collect(),
            labels: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionContract {
    pub parameters: BTreeMap<String, Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub version: u32,
    pub records: BTreeMap<String, RecordDefinition>,
    pub enums: BTreeMap<String, EnumDefinition>,
    pub values: BTreeMap<String, BindingDefinition>,
    pub fields: BTreeMap<String, FieldDefinition>,
    pub actions: BTreeMap<String, ActionContract>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            records: BTreeMap::new(),
            enums: BTreeMap::new(),
            values: BTreeMap::new(),
            fields: BTreeMap::new(),
            actions: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BindingSnapshot {
    pub manifest_version: u32,
    pub values: BTreeMap<String, Value>,
    pub fields: BTreeMap<String, Value>,
    pub row_fields: BTreeMap<FieldIdentity, Value>,
    pub selections: BTreeMap<NodeIdentity, Vec<usize>>,
}

impl Default for BindingSnapshot {
    fn default() -> Self {
        Self {
            manifest_version: MANIFEST_VERSION,
            values: BTreeMap::new(),
            fields: BTreeMap::new(),
            row_fields: BTreeMap::new(),
            selections: BTreeMap::new(),
        }
    }
}
