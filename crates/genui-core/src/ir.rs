//! Validated IR and structural identity (§5). Hashes are caches; path strings are identity.
use crate::{
    diag::Code,
    syntax::ast::{Expr, StatementKind},
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Group,
    Region,
    Heading,
    Paragraph,
    Code,
    Separator,
    Image,
    Figure,
    Status,
    Alert,
    Button,
    TextInput,
    MultiLineInput,
    Checkbox,
    CheckboxGroup,
    RadioGroup,
    Switch,
    Select,
    Form,
    List,
    ListItem,
    Table,
    TabList,
    Tab,
    TabPanel,
    Chart,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ComponentId(pub u16);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PropId(pub u16);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NodeId(pub u64);
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NodePath(pub String);
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExprId {
    pub hash: u64,
    pub key: std::sync::Arc<str>,
}
impl std::fmt::Debug for ExprId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExprId").field(&self.hash).finish()
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ActionPlanId(pub std::sync::Arc<str>);
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Static(PropValue),
    Dynamic(ExprId),
}
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Value(Value),
    Token { scale: String, value: Value },
    Binding(String),
    Action(ActionPlanId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Partial,
    Sealed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub path: NodePath,
    pub kind: ComponentId,
    pub role: Role,
    pub props: BTreeMap<PropId, Prop>,
    pub slots: Vec<IrSlot>,
    pub status: NodeStatus,
    pub origin: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct IrSlot {
    pub name: String,
    pub children: crate::SharedVector<NodeId>,
    pub plans: crate::SharedVector<SlotPlan>,
}
/// A structurally checked expression awaiting M4 data evaluation, with its source identity intact.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotPlan {
    pub base: NodePath,
    pub expression: ExprId,
    pub accepts: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActionPlan {
    pub steps: Vec<Step>,
    pub origin: NodeId,
    pub submit_scope: Option<NodeId>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Set { state: String, value: ExprId },
    Restore { states: Vec<String> },
    Refresh { query: String },
    Say { text: ExprId, ctx: Option<ExprId> },
    Open { url: ExprId },
    Emit { name: String, params: Vec<ExprId> },
}
#[derive(Clone, Debug, PartialEq)]
pub struct FallbackPlan {
    pub original: Node,
    pub descendants: Vec<Node>,
    pub template: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Node,
    Action,
    Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ValueEntry {
    pub kind: Kind,
    pub source_kind: StatementKind,
    pub expression: Expr,
    pub value: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StateDef {
    pub default: Value,
    pub ty: crate::r#lib::TypeExpr,
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueryDef {
    pub tool: String,
    pub args: Expr,
    pub default: Expr,
    pub allowed: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ir {
    pub templates: crate::SharedMap<String, Expr>,
    pub root: Option<NodeId>,
    pub nodes: crate::SharedMap<NodeId, Node>,
    pub expressions: crate::ExpressionArena,
    pub actions: crate::ActionArena,
    pub fallbacks: crate::SharedMap<ExprId, FallbackPlan>,
    pub values: crate::SharedMap<String, ValueEntry>,
    pub states: crate::SharedMap<String, StateDef>,
    pub queries: crate::SharedMap<String, QueryDef>,
    pub root_plan: Option<SlotPlan>,
}
impl Ir {
    pub fn intern(&mut self, e: &Expr) -> ExprId {
        self.expressions.intern(e)
    }
    pub(crate) fn begin_owner(&mut self, key: String, order: Vec<u64>) -> crate::arena::Scope {
        self.expressions.enter(key, order)
    }
    pub(crate) fn end_owner(&mut self, scope: crate::arena::Scope) {
        let (removed, action) = self.expressions.exit(scope);
        for id in removed {
            self.fallbacks.remove(&id);
        }
        if let Some(owner) = action {
            self.actions.remove(&ActionPlanId(owner.into()));
        }
    }
    pub(crate) fn intern_action(&mut self, id: ActionPlanId, plan: ActionPlan) -> ActionPlanId {
        self.expressions.mark_action();
        self.actions.insert(id.clone(), plan);
        id
    }
    pub(crate) fn drop_action(&mut self, id: &ActionPlanId) {
        self.actions.remove(id);
        for expr in self.expressions.remove_owner(&id.0) {
            self.fallbacks.remove(&expr);
        }
    }
    pub fn expression_indices(&self) -> BTreeMap<ExprId, usize> {
        self.expressions.canonical_indices()
    }
    pub fn canonical_tree(&self, lib: &crate::r#lib::Library) -> serde_json::Value {
        fn tree(
            ir: &Ir,
            id: NodeId,
            lib: &crate::r#lib::Library,
            indices: &BTreeMap<ExprId, usize>,
        ) -> serde_json::Value {
            use serde_json::{Value as J, json};
            let Some(n) = ir.nodes.get(&id) else {
                return J::Null;
            };
            let Some(c) = lib.components().get(n.kind.0 as usize) else {
                return J::Null;
            };
            let props: serde_json::Map<_, _> = n
                .props
                .iter()
                .filter_map(|(id, p)| {
                    Some((
                        c.args.get(id.0 as usize)?.name.clone(),
                        match p {
                            Prop::Dynamic(i) => json!({"dynamic":indices.get(i)?}),
                            Prop::Static(p) => match p {
                                PropValue::Value(v) => v.ir_json(),
                                PropValue::Token { scale, value } => {
                                    json!({"tok":[scale,value.plain_json()]})
                                }
                                PropValue::Binding(b) => json!({"bind":format!("${b}")}),
                                PropValue::Action(i) => json!({"action":i.0.as_ref()}),
                            },
                        },
                    ))
                })
                .collect();
            let children: serde_json::Map<_, _> = n
                .slots
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        J::Array(
                            s.children
                                .iter()
                                .map(|id| tree(ir, *id, lib, indices))
                                .collect(),
                        ),
                    )
                })
                .collect();
            json!({"id":n.path.0,"kind":c.name,"role":n.role,"props":props,"children":children})
        }
        self.root.map_or(serde_json::Value::Null, |id| {
            tree(self, id, lib, &self.expressions.canonical_indices())
        })
    }
}

pub fn escape_key(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        match c {
            '/' => out.push_str("%2F"),
            ':' => out.push_str("%3A"),
            '%' => out.push_str("%25"),
            '~' => out.push_str("%7E"),
            '#' => out.push_str("%23"),
            _ => out.push(c),
        }
    }
    out
}
impl NodePath {
    pub fn root() -> Self {
        Self("root".into())
    }
    pub fn child(&self, slot: &str, base: &str) -> Self {
        Self(format!("{}/{slot}:{base}", self.0))
    }
    pub fn element(
        &self,
        index: usize,
        key: Option<&Value>,
        seen: &mut BTreeSet<String>,
    ) -> (Self, Option<Code>) {
        let typed = match key {
            Some(Value::Str(s)) => Some(format!("s{}", escape_key(&s.text))),
            Some(Value::Int(i)) => Some(format!("n{i}")),
            _ => None,
        };
        match typed {
            Some(k) if seen.insert(k.clone()) => (Self(format!("{}:{k}", self.0)), None),
            Some(_) => (
                Self(format!("{}#{index}", self.0)),
                Some(Code::DuplicateKey),
            ),
            None => (
                Self(format!("{}#{index}", self.0)),
                key.map(|_| Code::BadKey),
            ),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct IdAllocator {
    paths: crate::SharedMap<NodeId, NodePath>,
}
impl IdAllocator {
    pub fn allocate(&mut self, path: &NodePath) -> NodeId {
        self.allocate_with(path, xxhash_rust::xxh3::xxh3_64)
    }
    /// The initial hash is injectable to prove collision behavior without searching for xxh3 collisions.
    pub fn allocate_with(&mut self, path: &NodePath, initial: impl FnOnce(&[u8]) -> u64) -> NodeId {
        let mut hash = NodeId(initial(path.0.as_bytes()));
        let mut salt = 0u64;
        loop {
            match self.paths.get(&hash) {
                Some(existing) if existing != path => {
                    salt = salt.wrapping_add(1);
                    hash = NodeId(xxhash_rust::xxh3::xxh3_64_with_seed(
                        path.0.as_bytes(),
                        salt,
                    ));
                }
                _ => {
                    self.paths.insert(hash, path.clone());
                    return hash;
                }
            }
        }
    }
    pub fn path(&self, id: NodeId) -> Option<&NodePath> {
        self.paths.get(&id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_compares_strings_and_rehashes() {
        let mut a = IdAllocator::default();
        let x = NodePath("root/a:#0".into());
        let y = NodePath("root/a:#1".into());
        let i = a.allocate_with(&x, |_| 7);
        let j = a.allocate_with(&y, |_| 7);
        assert_ne!(i, j);
        assert_eq!(a.path(i), Some(&x));
        assert_eq!(a.path(j), Some(&y));
        assert_eq!(a.allocate_with(&x, |_| 7), i);
    }
    #[test]
    fn expression_debug_identity_names_the_hash_not_the_body() {
        // SPEC §5.5: ExprId is an opaque structural identity. Its debug form is
        // used as an internal key, so it must carry the hash and stay O(1).
        let id = ExprId {
            hash: 0xfeed,
            key: "a very long expression body".into(),
        };
        assert_eq!(format!("{id:?}"), "ExprId(65261)");
    }
    #[test]
    fn typed_keys_escape_only_reserved_characters() {
        let p = NodePath::root().child("items", "e0");
        let mut seen = BTreeSet::new();
        assert_eq!(
            p.element(0, Some(&Value::from("/:%~#é")), &mut seen).0.0,
            "root/items:e0:s%2F%3A%25%7E%23é"
        );
        assert_ne!(
            p.element(1, Some(&Value::Int(1)), &mut seen).0,
            p.element(2, Some(&Value::from("1")), &mut seen).0
        );
    }
}
