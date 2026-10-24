//! Host-facing data types shared by prompts, messages and events (SPEC §§7.5,7.7,9).
use crate::r#lib::TypeExpr;
use serde::{Deserialize, Serialize};

/// A type sketch such as `list of {id: str, name: str}` (SPEC §7.5).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DataShape(pub String);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolArgKind {
    #[default]
    Plain,
    FreeText,
    Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolArg {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeExpr,
    #[serde(default)]
    pub kind: ToolArgKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub args: Vec<ToolArg>,
    #[serde(default)]
    pub result_shape: DataShape,
    #[serde(default)]
    pub auto_run: bool,
}

/// A reference id from the per-turn catalog (SPEC §7.7).
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RefId(pub String);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefEntry {
    pub id: RefId,
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub shape: DataShape,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefCatalog {
    #[serde(default)]
    pub entries: Vec<RefEntry>,
}

impl RefCatalog {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
