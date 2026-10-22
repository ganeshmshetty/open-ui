//! Additive, data-driven library definitions and canonical Library Spec (§8).
pub use crate::ir::Role;
use crate::{ir::ComponentId, value::Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TypeExpr {
    Any,
    Text,
    Int,
    Float,
    Number,
    Bool,
    Scalar,
    Named { name: String },
    List { of: Box<TypeExpr> },
    Map { of: Box<TypeExpr> },
    Node { accepts: Accepts },
    Data { of: Box<TypeExpr> },
    Binding { of: Box<TypeExpr> },
    Action,
    Url,
    Options,
    Optional { of: Box<TypeExpr> },
}
pub type T = TypeExpr;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Accepts {
    Any(String),
    Only(Vec<String>),
}
impl T {
    pub fn text() -> Self {
        Self::Text
    }
    pub fn int() -> Self {
        Self::Int
    }
    pub fn float() -> Self {
        Self::Float
    }
    pub fn number() -> Self {
        Self::Number
    }
    pub fn bool() -> Self {
        Self::Bool
    }
    pub fn scalar() -> Self {
        Self::Scalar
    }
    pub fn enum_(s: &str) -> Self {
        Self::Named { name: s.into() }
    }
    pub fn action() -> Self {
        Self::Action
    }
    pub fn url() -> Self {
        Self::Url
    }
    pub fn options() -> Self {
        Self::Options
    }
    pub fn list(t: Self) -> Self {
        Self::List { of: Box::new(t) }
    }
    pub fn map(t: Self) -> Self {
        Self::Map { of: Box::new(t) }
    }
    pub fn data() -> Self {
        Self::Data {
            of: Box::new(Self::map(Self::Any)),
        }
    }
    pub fn binding(t: Self) -> Self {
        Self::Binding { of: Box::new(t) }
    }
    pub fn optional(t: Self) -> Self {
        Self::Optional { of: Box::new(t) }
    }
    pub fn node() -> Self {
        Self::Node {
            accepts: Accepts::Any("any".into()),
        }
    }
    pub fn nodes() -> Self {
        Self::list(Self::node())
    }
    pub fn nodes_of(names: &[&str]) -> Self {
        Self::list(Self::Node {
            accepts: Accepts::Only(names.iter().map(|s| (*s).into()).collect()),
        })
    }
    /// Nullable wrappers preserve the underlying prop/slot kind.
    pub fn non_null(&self) -> &Self {
        let mut ty = self;
        while let Self::Optional { of } = ty {
            ty = of;
        }
        ty
    }
    pub fn slot(&self) -> Option<&Accepts> {
        match self.non_null() {
            Self::Node { accepts } => Some(accepts),
            Self::List { of } => match of.non_null() {
                Self::Node { accepts } => Some(accepts),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn infer(v: &Value) -> Self {
        match v {
            Value::Null => Self::Any,
            Value::Bool(_) => Self::Bool,
            Value::Int(_) => Self::Int,
            Value::Float(_) => Self::Float,
            Value::Str(_) => Self::Text,
            Value::List(v) => Self::list(v.first().map_or(Self::Any, Self::infer)),
            Value::Map(_) => Self::map(Self::Any),
            Value::Ref(_) => Self::data(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TypeDef {
    Enum { values: Vec<String> },
    Token { scale: String, min: i64, max: i64 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub position: Option<usize>,
    pub required: bool,
    #[serde(rename = "type")]
    pub ty: T,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "read_default"
    )]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub min_items: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDef {
    pub name: String,
    pub group: String,
    pub role: Role,
    pub doc: String,
    pub args: Vec<ArgDef>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub to_text: String,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_in: Vec<String>,
}
impl ComponentDef {
    pub fn new(name: &str, role: Role) -> Self {
        Self {
            name: name.into(),
            role,
            group: String::new(),
            doc: String::new(),
            args: Vec::new(),
            requires: Vec::new(),
            to_text: String::new(),
            examples: Vec::new(),
            allowed_in: Vec::new(),
        }
    }
    pub fn group(mut self, s: &str) -> Self {
        self.group = s.into();
        self
    }
    pub fn doc(mut self, s: &str) -> Self {
        self.doc = s.into();
        self
    }
    pub fn to_text(mut self, s: &str) -> Self {
        self.to_text = s.into();
        self
    }
    pub fn example(mut self, s: &str) -> Self {
        self.examples.push(s.into());
        self
    }
    pub fn requires(mut self, s: &str) -> Self {
        self.requires.push(s.into());
        self
    }
    pub fn only_in(mut self, s: &str) -> Self {
        self.allowed_in.push(s.into());
        self
    }
    fn arg(mut self, name: &str, ty: T, required: bool, default: Option<Value>) -> Self {
        let position = required.then(|| self.args.iter().filter(|a| a.required).count());
        self.args.push(ArgDef {
            name: name.into(),
            ty,
            required,
            position,
            default,
            min: None,
            max: None,
            min_items: None,
        });
        self
    }
    pub fn req(self, n: &str, t: T) -> Self {
        self.arg(n, t, true, None)
    }
    pub fn req_default(self, n: &str, t: T, v: impl Into<Value>) -> Self {
        self.arg(n, t, true, Some(v.into()))
    }
    pub fn opt(self, n: &str, t: T, v: impl Into<Value>) -> Self {
        self.arg(n, t, false, Some(v.into()))
    }
    pub fn opt_none(self, n: &str, t: T) -> Self {
        self.arg(n, t, false, None)
    }
    pub fn range(mut self, min: f64, max: f64) -> Self {
        if let Some(a) = self.args.last_mut() {
            a.min = Some(min);
            a.max = Some(max)
        }
        self
    }
    pub fn min_items(mut self, min: usize) -> Self {
        if let Some(a) = self.args.last_mut() {
            a.min_items = Some(min)
        }
        self
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDef {
    pub name: String,
    pub order: u32,
    pub notes: String,
}
impl GroupDef {
    pub fn new(name: &str, order: u32, notes: &str) -> Self {
        Self {
            name: name.into(),
            order,
            notes: notes.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<T>,
    pub result: T,
    pub fuel: u64,
    #[serde(skip)]
    pub implementation: Option<fn(&[Value]) -> Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    pub name: String,
    pub params: Vec<T>,
    pub doc: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySpec {
    pub spec_version: u32,
    pub id: String,
    pub version: String,
    pub language_major: u32,
    pub groups: Vec<GroupDef>,
    pub types: BTreeMap<String, TypeDef>,
    pub components: Vec<ComponentDef>,
    pub functions: Vec<FunctionDef>,
    pub actions: Vec<ActionDef>,
    pub formats: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
}
#[derive(Clone, Debug)]
pub struct Library {
    spec: LibrarySpec,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LibraryBuilder {
    pub id: String,
    pub version: String,
    pub groups: Vec<GroupDef>,
    pub types: BTreeMap<String, TypeDef>,
    pub components: Vec<ComponentDef>,
    pub functions: Vec<FunctionDef>,
    pub actions: Vec<ActionDef>,
    pub formats: Vec<String>,
    #[serde(skip)]
    base: Option<LibrarySpec>,
    #[serde(skip)]
    errors: Vec<String>,
}
impl Default for LibraryBuilder {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            version: "0.1.0".into(),
            groups: Vec::new(),
            types: BTreeMap::new(),
            components: Vec::new(),
            functions: Vec::new(),
            actions: Vec::new(),
            formats: Vec::new(),
            base: None,
            errors: Vec::new(),
        }
    }
}
pub const BUILTINS: &[&str] = &[
    "if", "coalesce", "len", "first", "last", "each", "map", "filter", "sort_by", "sum", "avg",
    "min", "max", "round", "floor", "ceil", "abs", "fixed", "fmt", "text", "upper", "lower",
    "contains", "join", "ref",
];
pub const ACTIONS: &[&str] = &["set", "restore", "refresh", "say", "open", "emit"];
pub const CAPABILITIES: &[&str] = &[
    "charts",
    "images",
    "markdown",
    "code_highlight",
    "text_input",
];
impl Library {
    pub fn builder(id: &str, version: &str) -> LibraryBuilder {
        LibraryBuilder {
            id: id.into(),
            version: version.into(),
            ..LibraryBuilder::default()
        }
    }
    pub fn build(builder: LibraryBuilder) -> Result<Self, Vec<String>> {
        builder.build()
    }
    pub fn extend(&self) -> LibraryBuilder {
        LibraryBuilder {
            id: self.spec.id.clone(),
            version: self.spec.version.clone(),
            groups: self.spec.groups.clone(),
            types: self.spec.types.clone(),
            components: self.spec.components.clone(),
            functions: self.spec.functions.clone(),
            actions: self.spec.actions.clone(),
            formats: self.spec.formats.clone(),
            base: Some(self.spec.clone()),
            errors: Vec::new(),
        }
    }
    pub fn spec(&self) -> &LibrarySpec {
        &self.spec
    }
    pub fn components(&self) -> &[ComponentDef] {
        &self.spec.components
    }
    pub fn types(&self) -> &BTreeMap<String, TypeDef> {
        &self.spec.types
    }
    pub fn component(&self, name: &str) -> Option<(ComponentId, &ComponentDef)> {
        self.spec
            .components
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == name)
            .and_then(|(i, c)| Some((ComponentId(u16::try_from(i).ok()?), c)))
    }
    pub fn function(&self, name: &str) -> Option<&FunctionDef> {
        self.spec.functions.iter().find(|f| f.name == name)
    }
    pub fn action(&self, name: &str) -> Option<&ActionDef> {
        self.spec.actions.iter().find(|a| a.name == name)
    }
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        crate::canonical::serialize(&self.spec)
    }
    pub fn hash(&self) -> &str {
        &self.spec.hash
    }
    /// Literal-only type checking. Unknown dynamic values are checked by M4 instead.
    pub fn accepts(&self, t: &T, v: &Value) -> bool {
        if !v.is_finite() {
            return false;
        }
        match (t, v) {
            (T::Any, _) => true,
            (T::Optional { .. }, Value::Null) => true,
            (T::Optional { of }, v) => self.accepts(of, v),
            (T::Text | T::Url, Value::Str(_))
            | (T::Int | T::Float | T::Number | T::Scalar, Value::Int(_))
            | (T::Float | T::Number | T::Scalar, Value::Float(_))
            | (T::Bool, Value::Bool(_))
            | (T::Scalar, Value::Str(_)) => true,
            (T::Named { name }, v) => match self.types().get(name) {
                Some(TypeDef::Enum { values }) => {
                    v.string().is_some_and(|s| values.iter().any(|v| v == s))
                }
                Some(TypeDef::Token { min, max, .. }) => matches!(v,Value::Int(i)if i>=min&&i<=max),
                None => false,
            },
            (T::List { of }, Value::List(v)) => v.iter().all(|v| self.accepts(of, v)),
            (T::Map { of }, Value::Map(v)) => v.iter().all(|(_, v)| self.accepts(of, v)),
            (T::Data { .. }, Value::Ref(_)) => true,
            (T::Data { of }, Value::List(v)) => v.iter().all(|v| self.accepts(of, v)),
            (T::Options, Value::List(v)) => v.iter().all(|v| match v {
                Value::Str(_) => true,
                Value::Map(m) => ["label", "value"]
                    .iter()
                    .all(|k| m.iter().any(|(n, v)| n == k && v.string().is_some())),
                _ => false,
            }),
            _ => false,
        }
    }
}
impl LibraryBuilder {
    pub fn group(mut self, g: GroupDef) -> Self {
        self.groups.push(g);
        self
    }
    pub fn type_(mut self, n: &str, t: TypeDef) -> Self {
        if self.types.insert(n.into(), t).is_some() {
            self.errors.push(format!("duplicate type {n}"));
        }
        self
    }
    pub fn component(mut self, c: ComponentDef) -> Self {
        self.components.push(c);
        self
    }
    pub fn function(mut self, f: FunctionDef) -> Self {
        self.functions.push(f);
        self
    }
    pub fn action(mut self, a: ActionDef) -> Self {
        self.actions.push(a);
        self
    }
    pub fn build(self) -> Result<Library, Vec<String>> {
        let mut errors = self.errors.clone();
        if let Some(base) = &self.base {
            if !self.components.starts_with(&base.components) {
                errors.push("extension cannot modify base components".into())
            }
            if !self.groups.starts_with(&base.groups) {
                errors.push("extension cannot modify base groups".into())
            }
            if base.types.iter().any(|(n, t)| self.types.get(n) != Some(t)) {
                errors.push("extension cannot modify base types".into())
            }
            if !self.formats.starts_with(&base.formats) {
                errors.push("extension cannot modify base formats".into())
            }
            let old_functions = serde_json::to_value(&base.functions).ok();
            let prefix = self
                .functions
                .get(..base.functions.len())
                .and_then(|v| serde_json::to_value(v).ok());
            if old_functions != prefix {
                errors.push("extension cannot modify base functions".into())
            }
            if base.functions.iter().zip(&self.functions).any(|(a, b)| {
                match (a.implementation, b.implementation) {
                    (Some(a), Some(b)) => !std::ptr::fn_addr_eq(a, b),
                    (None, None) => false,
                    _ => true,
                }
            }) {
                errors.push("extension cannot modify base implementations".into())
            }
            if serde_json::to_value(&base.actions).ok()
                != self
                    .actions
                    .get(..base.actions.len())
                    .and_then(|v| serde_json::to_value(v).ok())
            {
                errors.push("extension cannot modify base actions".into())
            }
        }
        if !errors.is_empty() {
            errors.sort();
            errors.dedup();
            return Err(errors);
        }
        let mut names: BTreeSet<String> = BUILTINS
            .iter()
            .chain(ACTIONS)
            .map(|n| (*n).into())
            .collect();
        for n in self
            .components
            .iter()
            .map(|c| &c.name)
            .chain(self.functions.iter().map(|f| &f.name))
            .chain(self.actions.iter().map(|a| &a.name))
        {
            if !names.insert(n.clone()) {
                errors.push(format!("duplicate name {n}"))
            }
        }
        if self.components.len() > u16::MAX as usize {
            errors.push("too many components".into())
        }
        let mut group_names = BTreeSet::new();
        for g in &self.groups {
            if !group_names.insert(&g.name) {
                errors.push(format!("duplicate group {}", g.name))
            }
            for word in g
                .notes
                .split('`')
                .enumerate()
                .filter_map(|(i, s)| (i % 2 == 1).then_some(s))
            {
                if !self.components.iter().any(|c| c.name == word) {
                    errors.push(format!("{}: unknown component in notes {word}", g.name))
                }
            }
        }
        fn check_type(t: &T, at: &str, b: &LibraryBuilder, e: &mut Vec<String>) {
            match t {
                T::Named { name } if !b.types.contains_key(name) => {
                    e.push(format!("{at}: unknown type {name}"))
                }
                T::List { of }
                | T::Map { of }
                | T::Optional { of }
                | T::Binding { of }
                | T::Data { of } => check_type(of, at, b, e),
                T::Node {
                    accepts: Accepts::Only(v),
                } => {
                    for n in v {
                        if !b.components.iter().any(|c| &c.name == n) {
                            e.push(format!("{at}: unknown component type {n}"))
                        }
                    }
                }
                T::Node {
                    accepts: Accepts::Any(s),
                } if s != "any" => e.push(format!("{at}: invalid node acceptance")),
                _ => {}
            }
        }
        for c in &self.components {
            if c.doc.trim().is_empty() {
                errors.push(format!("{}: missing doc", c.name))
            }
            if !group_names.contains(&c.group) {
                errors.push(format!("{}: unknown group {}", c.name, c.group))
            }
            if c.args.len() > u16::MAX as usize {
                errors.push(format!("{}: too many arguments", c.name))
            }
            let mut optional = false;
            let mut arg_names = BTreeSet::new();
            let mut position = 0;
            for a in &c.args {
                if a.required && optional {
                    errors.push(format!("{}: required argument after optional", c.name))
                }
                optional |= !a.required;
                if !arg_names.insert(&a.name) {
                    errors.push(format!("{}: duplicate argument {}", c.name, a.name))
                }
                if a.required {
                    if a.position != Some(position) {
                        errors.push(format!("{}.{}: invalid position", c.name, a.name))
                    }
                    position += 1;
                } else if a.position.is_some() {
                    errors.push(format!(
                        "{}.{}: optional argument has position",
                        c.name, a.name
                    ))
                }
                check_type(&a.ty, &format!("{}.{}", c.name, a.name), &self, &mut errors);
                if a.min
                    .zip(a.max)
                    .is_some_and(|(a, b)| !a.is_finite() || !b.is_finite() || a > b)
                {
                    errors.push(format!("{}.{}: invalid range", c.name, a.name))
                }
            }
            for r in &c.requires {
                if !CAPABILITIES.contains(&r.as_str()) {
                    errors.push(format!("{}: unknown capability {r}", c.name))
                }
            }
            for place in &c.allowed_in {
                let mut p = place.split('.');
                let target = p.next().unwrap_or("");
                let slot = p.next().unwrap_or("");
                if !self.components.iter().any(|c| {
                    c.name == target
                        && c.args
                            .iter()
                            .any(|a| a.name == slot && a.ty.slot().is_some())
                }) {
                    errors.push(format!("{}: invalid structural context {place}", c.name))
                }
            }
        }
        for (n, t) in &self.types {
            match t {
                TypeDef::Enum { values }
                    if values.is_empty()
                        || values.iter().collect::<BTreeSet<_>>().len() != values.len() =>
                {
                    errors.push(format!("{n}: invalid enum"))
                }
                TypeDef::Token { min, max, .. } if min > max => {
                    errors.push(format!("{n}: invalid token range"))
                }
                _ => {}
            }
        }
        for f in &self.functions {
            for t in f.params.iter().chain(std::iter::once(&f.result)) {
                check_type(t, &f.name, &self, &mut errors)
            }
            if f.fuel == 0 {
                errors.push(format!("{}: fuel must be positive", f.name))
            }
        }
        for a in &self.actions {
            for t in &a.params {
                check_type(t, &a.name, &self, &mut errors)
            }
        }
        if self.components.iter().any(|c| {
            c.requires
                .iter()
                .any(|r| CAPABILITIES.contains(&r.as_str()))
        }) && !["Text", "Markdown"].iter().all(|name| {
            self.components.iter().any(|c| {
                c.name == *name
                    && c.args.iter().filter(|a| a.required).count() == 1
                    && c.args
                        .iter()
                        .find(|a| a.required)
                        .is_some_and(|a| a.ty == T::Text)
                    && if *name == "Text" {
                        c.requires.is_empty()
                    } else {
                        c.requires.iter().all(|r| r == "markdown")
                    }
            })
        }) {
            errors.push("capability fallbacks require Text and Markdown".into());
        }
        if !errors.is_empty() {
            errors.sort();
            errors.dedup();
            return Err(errors);
        }
        let mut lib = Library {
            spec: LibrarySpec {
                spec_version: 1,
                id: self.id,
                version: self.version,
                language_major: 1,
                groups: self.groups,
                types: self.types,
                components: self.components,
                functions: self.functions,
                actions: self.actions,
                formats: self.formats,
                hash: String::new(),
            },
        };
        lib.spec.groups.sort_by_key(|g| g.order);
        for c in lib.components() {
            for a in &c.args {
                if let Some(v) = &a.default
                    && (!lib.accepts(&a.ty, v)
                        || v.number().is_some_and(|v| {
                            a.min.is_some_and(|m| v < m) || a.max.is_some_and(|m| v > m)
                        }))
                {
                    errors.push(format!("{}.{}: invalid default", c.name, a.name))
                }
            }
            for (i, example) in c.examples.iter().enumerate() {
                let parsed = crate::syntax::parse_batch(
                    example.as_bytes(),
                    crate::limits::SyntaxLimits::default(),
                );
                let source = if parsed.program.statements.is_empty() {
                    format!("root={example}")
                } else {
                    example.clone()
                };
                let options = crate::validate::Options {
                    url_hosts: ["example.com".into()].into(),
                    ..Default::default()
                };
                let result = crate::validate::validate(source.as_bytes(), &lib, &options);
                if result
                    .diagnostics
                    .iter()
                    .any(|d| d.severity != crate::diag::Severity::Info)
                {
                    errors.push(format!("{}: invalid example {i}", c.name))
                }
            }
        }
        if !errors.is_empty() {
            errors.sort();
            return Err(errors);
        }
        match lib.canonical_json() {
            Ok(canonical) => {
                lib.spec.hash =
                    format!("{:032x}", xxhash_rust::xxh3::xxh3_128(canonical.as_bytes()))
            }
            Err(e) => return Err(vec![format!("library serialization: {e}")]),
        }
        Ok(lib)
    }
}

fn read_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}
