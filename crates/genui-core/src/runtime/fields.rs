//! Runtime field status and submit gating (§§2.14,7.3).
//!
//! A field is a node with a `bind` prop. Its scope is the nearest enclosing
//! `Form`, or the page scope when there is none. Checks run in the fixed order
//! required, format, length, range; the first failure is the field's error.
use crate::{
    ir::{Ir, NodeId, NodePath, Prop, PropValue},
    r#lib::Library,
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldError {
    Required,
    Format,
    TooShort,
    TooLong,
    TooSmall,
    TooLarge,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldStatus {
    pub error: Option<FieldError>,
    pub touched: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub node: NodeId,
    pub path: NodePath,
    pub scope: Option<NodeId>,
    pub form_path: Option<NodePath>,
    pub bind: String,
    pub required: bool,
    pub kind: String,
    pub min_len: Option<i64>,
    pub max_len: Option<i64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldRegistry {
    fields: Vec<Field>,
}

impl FieldRegistry {
    pub fn build(ir: &Ir, library: &Library, resolved: &crate::runtime::Resolved) -> Self {
        let parents = parents(ir);
        let mut fields = Vec::new();
        let mut nodes: Vec<_> = ir.nodes.values().collect();
        nodes.sort_by(|a, b| a.path.0.cmp(&b.path.0));
        for node in nodes {
            let Some(component) = library.components().get(node.kind.0 as usize) else {
                continue;
            };
            let Some(bind) = arg_value(node, component, "bind", resolved) else {
                continue;
            };
            let PropValue::Binding(bind) = bind else {
                continue;
            };
            let scope = nearest_form(ir, library, &parents, node.id);
            let form_path = scope.and_then(|id| ir.nodes.get(&id).map(|n| n.path.clone()));
            fields.push(Field {
                node: node.id,
                path: node.path.clone(),
                scope,
                form_path,
                bind,
                required: matches!(
                    arg_value(node, component, "required", resolved),
                    Some(PropValue::Value(Value::Bool(true)))
                ),
                kind: arg_string(node, component, "kind", resolved)
                    .unwrap_or_else(|| "text".into()),
                min_len: arg_int(node, component, "min_len", resolved),
                max_len: arg_int(node, component, "max_len", resolved),
                min: arg_number(node, component, "min", resolved),
                max: arg_number(node, component, "max", resolved),
            });
        }
        Self { fields }
    }
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
    /// All fields in submit scope. `None` is the page scope.
    pub fn scope(&self, scope: Option<NodeId>) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(move |f| f.scope == scope)
    }
    pub fn field(&self, node: &NodePath) -> Option<&Field> {
        self.fields.iter().find(|f| f.path == *node)
    }
    /// Validate every registered field for display, regardless of scope.
    pub fn all(
        &self,
        state: &crate::runtime::StateStore,
        touched: &BTreeSet<String>,
    ) -> BTreeMap<String, FieldStatus> {
        let mut out = BTreeMap::new();
        for field in &self.fields {
            out.insert(
                field.path.0.clone(),
                FieldStatus {
                    error: check(field, state),
                    touched: touched.contains(&field.path.0),
                },
            );
        }
        out
    }
    /// Validate one scope. Returns every field's status in that scope.
    pub fn validate<'a>(
        &'a self,
        scope: Option<NodeId>,
        state: &crate::runtime::StateStore,
        touched: &BTreeSet<String>,
    ) -> BTreeMap<&'a str, FieldStatus> {
        let mut out = BTreeMap::new();
        for field in self.scope(scope) {
            out.insert(
                field.path.0.as_str(),
                FieldStatus {
                    error: check(field, state),
                    touched: touched.contains(&field.path.0),
                },
            );
        }
        out
    }
}

fn parents(ir: &Ir) -> BTreeMap<NodeId, NodeId> {
    let mut parents = BTreeMap::new();
    for node in ir.nodes.values() {
        for slot in &node.slots {
            for child in slot.children.iter() {
                parents.insert(*child, node.id);
            }
        }
    }
    parents
}

fn nearest_form(
    ir: &Ir,
    library: &Library,
    parents: &BTreeMap<NodeId, NodeId>,
    mut id: NodeId,
) -> Option<NodeId> {
    while let Some(parent) = parents.get(&id).copied() {
        if ir
            .nodes
            .get(&parent)
            .and_then(|n| library.components().get(n.kind.0 as usize))
            .is_some_and(|c| c.name == "Form")
        {
            return Some(parent);
        }
        id = parent;
    }
    None
}

/// Read a field prop whether the program wrote it as a literal or an evaluated
/// expression (§§2.14,5.5,7.3).
fn arg_value(
    node: &crate::ir::Node,
    component: &crate::r#lib::ComponentDef,
    name: &str,
    resolved: &crate::runtime::Resolved,
) -> Option<PropValue> {
    let index = component.args.iter().position(|arg| arg.name == name)?;
    let index = u16::try_from(index).ok()?;
    let prop = crate::ir::PropId(index);
    match node.props.get(&prop) {
        Some(Prop::Static(value)) => Some(value.clone()),
        Some(Prop::Dynamic(_)) => resolved.prop(node.id, prop).cloned(),
        None => None,
    }
}
fn arg_int(
    node: &crate::ir::Node,
    component: &crate::r#lib::ComponentDef,
    name: &str,
    resolved: &crate::runtime::Resolved,
) -> Option<i64> {
    match arg_value(node, component, name, resolved) {
        Some(PropValue::Value(Value::Int(value))) => Some(value),
        _ => None,
    }
}
fn arg_number(
    node: &crate::ir::Node,
    component: &crate::r#lib::ComponentDef,
    name: &str,
    resolved: &crate::runtime::Resolved,
) -> Option<f64> {
    match arg_value(node, component, name, resolved) {
        Some(PropValue::Value(value)) => value.number(),
        _ => None,
    }
}
fn arg_string(
    node: &crate::ir::Node,
    component: &crate::r#lib::ComponentDef,
    name: &str,
    resolved: &crate::runtime::Resolved,
) -> Option<String> {
    match arg_value(node, component, name, resolved) {
        Some(PropValue::Value(Value::Str(text))) => Some(text.text.to_string()),
        _ => None,
    }
}

/// The §2.14 check order; the first failing check is the field's error.
pub fn check(field: &Field, state: &crate::runtime::StateStore) -> Option<FieldError> {
    let value = state.value(&field.bind).cloned().unwrap_or(Value::Null);
    let empty = matches!(&value, Value::Null)
        || matches!(&value, Value::Str(text) if text.text.is_empty())
        || matches!(&value, Value::List(items) if items.is_empty());
    if empty {
        return field.required.then_some(FieldError::Required);
    }
    if !format_ok(&field.kind, &value) {
        return Some(FieldError::Format);
    }
    let length = match &value {
        Value::Str(text) => Some(text.text.chars().count()),
        Value::List(items) => Some(items.len()),
        _ => None,
    };
    if let Some(length) = length {
        if let Some(min) = field.min_len
            && length < usize::try_from(min.max(0)).unwrap_or(usize::MAX)
        {
            return Some(FieldError::TooShort);
        }
        if let Some(max) = field.max_len
            && length > usize::try_from(max.max(0)).unwrap_or(usize::MAX)
        {
            return Some(FieldError::TooLong);
        }
    }
    if let Some(number) = numeric(&value) {
        if let Some(min) = field.min
            && number < min
        {
            return Some(FieldError::TooSmall);
        }
        if let Some(max) = field.max
            && number > max
        {
            return Some(FieldError::TooLarge);
        }
    }
    None
}

fn numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Int(value) => Some(*value as f64),
        Value::Float(value) => Some(*value),
        Value::Str(text) => text.text.parse::<f64>().ok().filter(|v| v.is_finite()),
        _ => None,
    }
}

/// Fixed named formats from §2.14. The model never supplies a pattern.
pub fn format_ok(kind: &str, value: &Value) -> bool {
    let Value::Str(text) = value else {
        return !matches!(kind, "email" | "url" | "number" | "integer");
    };
    let text = text.text.as_ref();
    match kind {
        "email" => {
            let mut parts = text.split('@');
            let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next())
            else {
                return false;
            };
            !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !text.chars().any(char::is_whitespace)
        }
        "url" => {
            let rest = text
                .strip_prefix("https://")
                .or_else(|| text.strip_prefix("http://"));
            rest.is_some_and(|rest| {
                !rest.is_empty() && !rest.starts_with('/') && !text.chars().any(char::is_whitespace)
            })
        }
        "number" => text.parse::<f64>().is_ok_and(f64::is_finite),
        "integer" => text.parse::<i64>().is_ok(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::StateStore;
    use crate::value::PStr;
    use std::sync::Arc;

    fn state(name: &str, value: Value) -> StateStore {
        let mut store = StateStore::default();
        store.insert_test(name, value);
        store
    }
    fn field(kind: &str, required: bool) -> Field {
        Field {
            node: NodeId(0),
            path: NodePath("root".into()),
            scope: None,
            form_path: None,
            bind: "v".into(),
            required,
            kind: kind.into(),
            min_len: None,
            max_len: None,
            min: None,
            max: None,
        }
    }
    fn text(value: &str) -> Value {
        Value::Str(PStr {
            text: Arc::from(value),
            prov: crate::value::Provenance::User,
        })
    }

    #[test]
    fn checks_run_in_the_fixed_order() {
        let f = Field {
            min_len: Some(6),
            ..field("email", true)
        };
        assert_eq!(
            check(&f, &state("v", text("bad"))),
            Some(FieldError::Format)
        );
        assert_eq!(
            check(&f, &state("v", text("a@b"))),
            Some(FieldError::Format)
        );
        assert_eq!(
            check(&f, &state("v", text("a@b.c"))),
            Some(FieldError::TooShort)
        );
        let f = Field {
            max_len: Some(2),
            ..field("text", false)
        };
        assert_eq!(
            check(&f, &state("v", text("abc"))),
            Some(FieldError::TooLong)
        );
        let f = Field {
            min: Some(2.0),
            max: Some(4.0),
            ..field("number", false)
        };
        assert_eq!(
            check(&f, &state("v", text("1"))),
            Some(FieldError::TooSmall)
        );
        assert_eq!(
            check(&f, &state("v", text("9"))),
            Some(FieldError::TooLarge)
        );
        assert_eq!(check(&f, &state("v", text("3"))), None);
    }

    #[test]
    fn empty_values_only_fail_required() {
        assert_eq!(check(&field("text", false), &state("v", text(""))), None);
        assert_eq!(
            check(&field("text", true), &state("v", text(""))),
            Some(FieldError::Required)
        );
        assert_eq!(check(&field("text", false), &state("v", Value::Null)), None);
    }
}
