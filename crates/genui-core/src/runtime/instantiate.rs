//! Runtime instantiation of data-dependent node plans (§§2.10,5.5,2.11,4.4).
//!
//! Static validation retains `SlotPlan`s for `each`, node-producing `if` and
//! `coalesce`. This module walks the validated tree in canonical order, evaluates
//! those plans against the current runtime values, applies the §4.4 recovery
//! ladder and returns an expanded IR for the wire tree. The static IR is never
//! mutated: patches and replay observe only this expanded projection.
use super::*;
use crate::ir::{
    ActionPlan, ActionPlanId, IrSlot, Node, NodePath, NodeStatus, Prop, PropId, SlotPlan, Step,
};
use crate::r#lib::{Accepts, ArgDef, ComponentDef, T, TypeDef};

pub(super) struct Inputs<'a> {
    pub ir: &'a Ir,
    pub library: &'a Library,
    pub limits: &'a Limits,
    pub state: &'a StateStore,
    pub values: &'a BTreeMap<String, Value>,
    pub query_results: &'a BTreeMap<String, Value>,
    pub ref_values: &'a BTreeMap<String, Value>,
    pub failed_refs: &'a BTreeSet<String>,
    pub catalog: &'a BTreeSet<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub queries: BTreeMap<String, Value>,
}

type Locals = BTreeMap<String, Value>;

enum SlotEntry {
    Child(NodeId),
    Plan(SlotPlan),
}

pub(super) struct Builder<'a> {
    input: Inputs<'a>,
    out: Ir,
    paths: BTreeMap<String, NodeId>,
    allocator: crate::ir::IdAllocator,
    resolved: Resolved,
    used: usize,
    stop_slot: bool,
    active: BTreeSet<String>,
}

impl<'a> Builder<'a> {
    pub(super) fn build(input: Inputs<'a>) -> (Ir, Resolved) {
        let out = input.ir.clone();
        let mut paths = BTreeMap::new();
        let mut allocator = crate::ir::IdAllocator::default();
        let mut sorted: Vec<_> = out.nodes.values().collect();
        sorted.sort_by(|a, b| a.path.0.cmp(&b.path.0));
        for node in sorted {
            allocator.allocate(&node.path);
            paths.insert(node.path.0.clone(), node.id);
        }
        let mut builder = Self {
            out,
            paths,
            allocator,
            resolved: Resolved::default(),
            used: 0,
            stop_slot: false,
            active: BTreeSet::new(),
            input,
        };
        let root = if let Some(plan) = builder.input.ir.root_plan.clone() {
            let mut nodes = builder.build_plan(&plan, 1, &Locals::new(), "root");
            if nodes.len() > 1 {
                for id in nodes.split_off(1) {
                    builder.remove_subtree(id);
                }
            }
            nodes.first().copied()
        } else {
            builder
                .input
                .ir
                .root
                .and_then(|id| builder.build_static(id, 1, &Locals::new()))
        };
        builder.out.root = root;
        let mut resolved = builder.resolved;
        let mut diagnostics = builder.input.diagnostics;
        sort_diagnostics(&mut diagnostics);
        resolved.diagnostics = diagnostics;
        resolved.ir = builder.out;
        (resolved.ir.clone(), resolved)
    }

    fn slot_entries(&self, slot: &IrSlot) -> Vec<SlotEntry> {
        let mut entries = Vec::new();
        for child in slot.children.iter() {
            let index = self.out.nodes.get(child).map_or(0, |node| node.index);
            entries.push((index, SlotEntry::Child(*child)));
        }
        for plan in slot.plans.iter() {
            entries.push((plan.index, SlotEntry::Plan(plan.clone())));
        }
        entries.sort_by_key(|(index, _)| *index);
        entries.into_iter().map(|(_, entry)| entry).collect()
    }

    fn id_for(&mut self, path: &NodePath) -> NodeId {
        if let Some(id) = self.paths.get(&path.0) {
            *id
        } else {
            let id = self.allocator.allocate(path);
            self.paths.insert(path.0.clone(), id);
            id
        }
    }

    fn emit(&mut self, code: Code, stmt: &str, path: &NodePath) {
        push_diagnostic(
            &mut self.input.diagnostics,
            code,
            Some(stmt.to_owned()),
            Some(path.0.clone()),
        );
    }

    fn limit(&mut self, code: Code, stmt: &str, path: &NodePath) {
        self.emit(code, stmt, path);
        self.stop_slot = true;
    }

    /// Evaluate one value expression for a prop or producer with its own fuel
    /// budget (§2.7). Diagnostics belong to the current prop.
    fn eval(
        &mut self,
        expression: &Expr,
        locals: &Locals,
        stmt: &str,
        path: &NodePath,
    ) -> (Value, Vec<Diagnostic>, bool) {
        let mut diagnostics = Vec::new();
        let context = Context {
            stmt: stmt.to_owned(),
            path: Some(path.0.clone()),
        };
        let mut evaluator = Evaluator::new(
            self.input.ir,
            self.input.library,
            self.input.limits,
            self.input.state,
            self.input.values,
            self.input.query_results,
            self.input.ref_values,
            self.input.failed_refs,
            self.input.catalog,
            &mut self.input.queries,
            &mut diagnostics,
        );
        let value = evaluator.eval(expression, locals, &context);
        let failed = evaluator.failed;
        drop(evaluator);
        (value, diagnostics, failed)
    }

    fn build_static(&mut self, id: NodeId, depth: usize, locals: &Locals) -> Option<NodeId> {
        let mut node = self.out.nodes.get(&id).cloned()?;
        if depth > self.input.limits.max_ir_depth {
            self.limit(Code::IrDepth, &node.origin, &node.path);
            return None;
        }
        if self.used >= self.input.limits.max_nodes {
            self.limit(Code::Nodes, &node.origin, &node.path);
            return None;
        }
        self.used += 1;
        let mut created = vec![id];
        let mut drop_node = false;
        let props: Vec<_> = node.props.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (prop_id, prop) in props {
            let Prop::Dynamic(expression) = prop else {
                continue;
            };
            let Some(component) = self
                .input
                .library
                .components()
                .get(node.kind.0 as usize)
                .cloned()
            else {
                continue;
            };
            let Some(arg) = component.args.get(prop_id.0 as usize).cloned() else {
                continue;
            };
            let Some(expression) = self.input.ir.expressions.get(&expression).cloned() else {
                continue;
            };
            let (value, diagnostics, failed) =
                self.eval(&expression, locals, &node.origin, &node.path);
            self.input.diagnostics.extend(diagnostics);
            let origin = node.origin.clone();
            let node_path = node.path.clone();
            match self.recover_prop(&arg, value, failed, &origin, &node_path, &mut node, prop_id) {
                None => {
                    drop_node = true;
                    break;
                }
                Some(None) => {}
                Some(Some(value)) => {
                    self.resolved.set(id, prop_id, PropValue::Value(value));
                }
            }
        }
        if !drop_node {
            let slots = node.slots.clone();
            for (slot_index, slot) in slots.iter().enumerate() {
                let mut entries = self.slot_entries(slot);
                let mut children = crate::SharedVector::default();
                self.stop_slot = false;
                for entry in entries.drain(..) {
                    match entry {
                        SlotEntry::Child(child) => {
                            if let Some(child) = self.build_static(child, depth + 1, locals) {
                                children.push_back(child);
                            }
                        }
                        SlotEntry::Plan(plan) => {
                            for child in self.build_plan(&plan, depth + 1, locals, &node.origin) {
                                children.push_back(child);
                            }
                        }
                    }
                    if self.stop_slot {
                        break;
                    }
                }
                // A dropped child already produced its own diagnostic.
                let created_here: Vec<_> = children.iter().copied().collect();
                created.extend(created_here);
                if let Some(min) = slot_min(component_arg(self.input.library, &node, slot_index))
                    && children.len() < min
                {
                    self.emit(Code::MinItems, &node.origin, &node.path);
                    drop_node = true;
                }
                if drop_node {
                    break;
                }
                node.slots[slot_index].children = children;
                node.slots[slot_index].plans = Default::default();
            }
        }
        if drop_node {
            let count = created.len();
            for id in created {
                self.remove_subtree_keep(id);
            }
            self.used = self.used.saturating_sub(count);
            return None;
        }
        if self.out.nodes.get(&id) != Some(&node) {
            self.out.nodes.insert(id, node);
        }
        Some(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn recover_prop(
        &mut self,
        arg: &ArgDef,
        value: Value,
        failed: bool,
        stmt: &str,
        path: &NodePath,
        node: &mut Node,
        prop_id: PropId,
    ) -> Option<Option<Value>> {
        let mut replaced = value;
        let wrong_type = !failed
            && replaced != Value::Null
            && !matches!(replaced, Value::Ref(_))
            && !self.input.library.accepts(&arg.ty, &replaced);
        if wrong_type {
            push_diagnostic(
                &mut self.input.diagnostics,
                Code::ExprType,
                Some(stmt.to_owned()),
                Some(path.0.clone()),
            );
        }
        let url_rejected = !failed
            && !wrong_type
            && matches!(arg.ty.non_null(), T::Url)
            && matches!(&replaced, Value::Str(s) if !self.url_ok(s));
        if url_rejected {
            push_diagnostic(
                &mut self.input.diagnostics,
                Code::TaintedUrl,
                Some(stmt.to_owned()),
                Some(path.0.clone()),
            );
        }
        let bad = failed || wrong_type || url_rejected;
        if bad {
            if let Some(default) = &arg.default {
                replaced = default.clone();
            } else if matches!(arg.ty, T::Optional { .. }) {
                node.props.remove(&prop_id);
                return Some(None);
            } else {
                return None;
            }
        } else if replaced == Value::Null && !matches!(arg.ty, T::Optional { .. }) {
            if let Some(default) = &arg.default {
                replaced = default.clone();
            } else {
                push_diagnostic(
                    &mut self.input.diagnostics,
                    Code::MissingRequired,
                    Some(stmt.to_owned()),
                    Some(path.0.clone()),
                );
                return None;
            }
        }
        Some(Some(replaced))
    }

    fn url_ok(&self, value: &PStr) -> bool {
        matches!(value.prov, Provenance::Host | Provenance::Tool)
    }

    fn build_plan(
        &mut self,
        plan: &SlotPlan,
        depth: usize,
        locals: &Locals,
        stmt: &str,
    ) -> Vec<NodeId> {
        let Some(expression) = self.input.ir.expressions.get(&plan.expression).cloned() else {
            return Vec::new();
        };
        self.build_expr(
            &expression,
            plan.base.clone(),
            depth,
            locals,
            &plan.accepts,
            stmt,
            plan.index,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_expr(
        &mut self,
        expression: &Expr,
        path: NodePath,
        depth: usize,
        locals: &Locals,
        accepts: &[String],
        stmt: &str,
        index: usize,
    ) -> Vec<NodeId> {
        match expression {
            Expr::Null { .. } => Vec::new(),
            Expr::List { items } => {
                if items.len() > self.input.limits.max_list_len {
                    self.limit(Code::ListLen, stmt, &path);
                }
                let mut out = Vec::new();
                for (i, item) in items
                    .iter()
                    .take(self.input.limits.max_list_len)
                    .enumerate()
                {
                    if matches!(item, Expr::List { .. }) {
                        self.emit(Code::SlotKind, stmt, &path);
                        continue;
                    }
                    let child = NodePath(format!("{}#{i}", path.0));
                    out.extend(self.build_expr(item, child, depth, locals, accepts, stmt, index));
                    if self.stop_slot {
                        break;
                    }
                }
                out
            }
            Expr::Name { name } => {
                if locals.contains_key(name) {
                    self.emit(Code::SlotKind, stmt, &path);
                    return Vec::new();
                }
                let Some(template) = self.input.ir.templates.get(name).cloned() else {
                    self.emit(Code::Unresolved, stmt, &path);
                    return Vec::new();
                };
                if !self.active.insert(name.clone()) {
                    self.emit(Code::Cycle, stmt, &path);
                    return Vec::new();
                }
                let nodes = self.build_expr(&template, path, depth, locals, accepts, stmt, index);
                self.active.remove(name);
                nodes
            }
            Expr::Call { name, args } => match name.as_str() {
                "each" => self.build_each(args, path, depth, locals, accepts, stmt, index),
                "if" => self.build_if(args, path, depth, locals, accepts, stmt, index),
                "coalesce" => {
                    for argument in args {
                        if matches!(argument.value, Expr::Null { .. }) {
                            continue;
                        }
                        let nodes = self.build_expr(
                            &argument.value,
                            path.clone(),
                            depth,
                            locals,
                            accepts,
                            stmt,
                            index,
                        );
                        if !nodes.is_empty() {
                            return nodes;
                        }
                    }
                    Vec::new()
                }
                _ => self.build_component(name, args, path, depth, locals, accepts, stmt, index),
            },
            _ => {
                self.emit(Code::SlotKind, stmt, &path);
                Vec::new()
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_if(
        &mut self,
        args: &[Argument],
        path: NodePath,
        depth: usize,
        locals: &Locals,
        accepts: &[String],
        stmt: &str,
        index: usize,
    ) -> Vec<NodeId> {
        let Some(condition) = args.first() else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        let (value, diagnostics, failed) = self.eval(&condition.value, locals, stmt, &path);
        self.input.diagnostics.extend(diagnostics);
        if failed {
            return Vec::new();
        }
        let Value::Bool(condition) = value else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        let Some(branch) = args.get(if condition { 1 } else { 2 }) else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        self.build_expr(&branch.value, path, depth, locals, accepts, stmt, index)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_each(
        &mut self,
        args: &[Argument],
        path: NodePath,
        depth: usize,
        locals: &Locals,
        accepts: &[String],
        stmt: &str,
        index: usize,
    ) -> Vec<NodeId> {
        let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
        let (Some(collection), Some(body)) = (positional.first(), positional.get(1)) else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        let Expr::Lambda { params, body } = &body.value else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        if !(1..=2).contains(&params.len()) {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        }
        let (value, diagnostics, failed) = self.eval(&collection.value, locals, stmt, &path);
        self.input.diagnostics.extend(diagnostics);
        if failed {
            return Vec::new();
        }
        let Value::List(items) = value else {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        };
        let cap = self
            .input
            .limits
            .each_max_items
            .min(self.input.limits.max_list_len);
        if items.len() > cap {
            self.emit(Code::ListLen, stmt, &path);
        }
        let key = args
            .iter()
            .find(|a| a.name.as_deref() == Some("key"))
            .map(|a| a.value.clone());
        if let Some(Expr::Lambda { params, .. }) = &key
            && !(1..=2).contains(&params.len())
        {
            self.emit(Code::ExprType, stmt, &path);
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for (element_index, item) in items.iter().take(cap).enumerate() {
            let mut element = locals.clone();
            element.insert(params[0].clone(), item.clone());
            if let Some(second) = params.get(1) {
                element.insert(
                    second.clone(),
                    Value::Int(i64::try_from(element_index).unwrap_or(i64::MAX)),
                );
            }
            let key_value = match &key {
                None => None,
                Some(Expr::Lambda {
                    params: key_params,
                    body,
                }) => {
                    let mut key_locals = locals.clone();
                    key_locals.insert(key_params[0].clone(), item.clone());
                    if let Some(second) = key_params.get(1) {
                        key_locals.insert(
                            second.clone(),
                            Value::Int(i64::try_from(element_index).unwrap_or(i64::MAX)),
                        );
                    }
                    let (value, diagnostics, failed) = self.eval(body, &key_locals, stmt, &path);
                    self.input.diagnostics.extend(diagnostics);
                    if failed {
                        continue;
                    }
                    Some(value)
                }
                Some(Expr::String { value: field }) => match item {
                    Value::Map(entries) => Some(
                        entries
                            .iter()
                            .find(|(k, _)| k.as_str() == field.as_str())
                            .map_or(Value::Null, |(_, v)| v.clone()),
                    ),
                    _ => Some(Value::Null),
                },
                Some(_) => {
                    self.emit(Code::ExprType, stmt, &path);
                    return Vec::new();
                }
            };
            let (child_path, key_code) = path.element(element_index, key_value.as_ref(), &mut seen);
            if let Some(code) = key_code {
                self.emit(code, stmt, &child_path);
            }
            let nodes = self.build_expr(body, child_path, depth, &element, accepts, stmt, index);
            out.extend(nodes);
            if self.stop_slot {
                break;
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn build_component(
        &mut self,
        name: &str,
        args: &[Argument],
        path: NodePath,
        depth: usize,
        locals: &Locals,
        accepts: &[String],
        stmt: &str,
        index: usize,
    ) -> Vec<NodeId> {
        let Some((kind, def)) = self
            .input
            .library
            .component(name)
            .map(|(id, def)| (id, def.clone()))
        else {
            self.emit(Code::SlotKind, stmt, &path);
            return Vec::new();
        };
        if !accepts.is_empty() && !accepts.contains(&def.name) {
            self.emit(Code::SlotKind, stmt, &path);
            return Vec::new();
        }
        if depth > self.input.limits.max_ir_depth {
            self.limit(Code::IrDepth, stmt, &path);
            return Vec::new();
        }
        if self.used >= self.input.limits.max_nodes {
            self.limit(Code::Nodes, stmt, &path);
            return Vec::new();
        }
        let node_id = self.id_for(&path);
        let bound = bind_args(&def, args);
        let mut props = BTreeMap::new();
        let mut failed = false;
        for (i, arg) in def.args.iter().enumerate() {
            if arg.ty.slot().is_some() {
                continue;
            }
            if let Some(expression) = bound.get(&i) {
                match build_arg(self, expression, arg, locals, stmt, &path, &def, i) {
                    Some(prop) => {
                        props.insert(PropId(i as u16), prop);
                    }
                    None => {
                        if matches!(arg.ty, T::Optional { .. }) {
                            continue;
                        }
                        failed = true;
                        break;
                    }
                }
            } else if let Some(default) = &arg.default {
                props.insert(
                    PropId(i as u16),
                    literal_prop(self.input.library, default, arg),
                );
            } else if arg.required {
                self.emit(Code::MissingRequired, stmt, &path);
                failed = true;
                break;
            }
        }
        if failed {
            return Vec::new();
        }
        self.used += 1;
        let mut created = vec![node_id];
        let mut slots = Vec::with_capacity(def.args.len());
        for (slot_index, arg) in def.args.iter().enumerate() {
            if arg.ty.slot().is_none() {
                continue;
            }
            let mut children = crate::SharedVector::default();
            if let Some(expression) = bound.get(&slot_index) {
                children = self.build_slot(expression, &path, arg, depth + 1, locals, stmt);
            } else if let Some(default) = &arg.default {
                children = self.build_slot(&default.to_expr(), &path, arg, depth + 1, locals, stmt);
            } else if arg.required {
                self.emit(Code::MissingRequired, stmt, &path);
                failed = true;
                break;
            }
            let created_here: Vec<_> = children.iter().copied().collect();
            created.extend(created_here);
            if let Some(min) = arg.min_items
                && children.len() < min
            {
                self.emit(Code::MinItems, stmt, &path);
                failed = true;
                break;
            }
            slots.push(IrSlot {
                name: arg.name.clone(),
                children,
                plans: Default::default(),
            });
        }
        if failed {
            let count = created.len();
            for id in created {
                self.remove_subtree_keep(id);
            }
            self.used = self.used.saturating_sub(count);
            return Vec::new();
        }
        let role = def.role;
        let node = Node {
            id: node_id,
            path,
            kind,
            role,
            props,
            slots,
            status: NodeStatus::Sealed,
            origin: stmt.to_owned(),
            index,
        };
        if self.out.nodes.get(&node_id) != Some(&node) {
            self.out.nodes.insert(node_id, node);
        }
        vec![node_id]
    }

    #[allow(clippy::too_many_arguments)]
    fn build_slot(
        &mut self,
        expression: &Expr,
        parent: &NodePath,
        arg: &ArgDef,
        depth: usize,
        locals: &Locals,
        stmt: &str,
    ) -> crate::SharedVector<NodeId> {
        let expressions: Vec<&Expr> = if let Expr::List { items } = expression {
            items.iter().collect()
        } else {
            vec![expression]
        };
        if expressions.len() > self.input.limits.max_list_len {
            self.limit(Code::ListLen, stmt, parent);
        }
        let accepts = accepts_of(arg);
        let cap = self.input.limits.max_list_len;
        let mut repeats: BTreeMap<String, usize> = BTreeMap::new();
        let mut children = crate::SharedVector::default();
        self.stop_slot = false;
        for (i, element) in expressions.iter().take(cap).enumerate() {
            let base = match element {
                Expr::Name { name } => {
                    let count = repeats.entry(name.clone()).or_default();
                    *count += 1;
                    if *count == 1 {
                        format!("@{name}")
                    } else {
                        format!("@{name}~{count}")
                    }
                }
                Expr::Call { name, .. } if name == "each" => format!("e{i}"),
                _ => format!("#{i}"),
            };
            if matches!(element, Expr::Null { .. }) {
                continue;
            }
            let path = parent.child(&arg.name, &base);
            if matches!(element, Expr::List { .. }) {
                self.emit(Code::SlotKind, stmt, &path);
                continue;
            }
            for child in self.build_expr(element, path, depth, locals, &accepts, stmt, i) {
                children.push_back(child);
            }
            if self.stop_slot {
                break;
            }
        }
        children
    }

    fn remove_subtree(&mut self, id: NodeId) {
        self.remove_subtree_keep(id);
        self.used = self.used.saturating_sub(1);
    }
    /// Remove the node and every node that is no longer reachable through the
    /// runtime projection. The caller owns budget accounting.
    fn remove_subtree_keep(&mut self, id: NodeId) {
        let Some(node) = self.out.nodes.remove(&id) else {
            return;
        };
        for slot in node.slots {
            for child in slot.children {
                self.remove_subtree_keep(child);
            }
        }
    }
}

fn accepts_of(arg: &ArgDef) -> Vec<String> {
    match arg.ty.slot() {
        Some(Accepts::Only(values)) => values.clone(),
        _ => Vec::new(),
    }
}
fn slot_min(arg: Option<&ArgDef>) -> Option<usize> {
    arg.and_then(|arg| arg.min_items)
}
fn component_arg<'a>(library: &'a Library, node: &Node, slot: usize) -> Option<&'a ArgDef> {
    library
        .components()
        .get(node.kind.0 as usize)
        .and_then(|component| component.args.get(slot))
}

fn bind_args(def: &ComponentDef, args: &[Argument]) -> BTreeMap<usize, Expr> {
    let mut bound = BTreeMap::new();
    for argument in args.iter().filter(|a| a.name.is_some()) {
        let Some(name) = argument.name.as_deref() else {
            continue;
        };
        if let Some((index, _)) = def.args.iter().enumerate().find(|(_, p)| p.name == name)
            && !bound.contains_key(&index)
        {
            bound.insert(index, argument.value.clone());
        }
    }
    for argument in args.iter().filter(|a| a.name.is_none()) {
        if let Some((index, _)) = def
            .args
            .iter()
            .enumerate()
            .find(|(i, p)| p.required && !bound.contains_key(i))
        {
            bound.insert(index, argument.value.clone());
        }
    }
    bound
}

fn literal_prop(library: &Library, value: &Value, arg: &ArgDef) -> Prop {
    let mut value = value.clone();
    if value == Value::Null {
        return Prop::Static(PropValue::Value(value));
    }
    if matches!(arg.ty.non_null(), T::Float)
        && let Value::Int(i) = value
    {
        value = Value::Float(i as f64);
    }
    if let T::Named { name } = arg.ty.non_null()
        && let Some(TypeDef::Token { scale, .. }) = library.types().get(name)
    {
        return Prop::Static(PropValue::Token {
            scale: scale.clone(),
            value,
        });
    }
    Prop::Static(PropValue::Value(value))
}

/// Build typed action steps for a node instantiated from a plan. Static
/// template checks discard their action arena, so this mirrors §2.13 over the
/// already-validated expression and interns step arguments into the runtime IR.
fn runtime_action_steps(
    builder: &mut Builder<'_>,
    expression: &Expr,
    visiting: &mut BTreeSet<String>,
) -> Vec<Step> {
    match expression {
        Expr::List { items } => items
            .iter()
            .flat_map(|item| runtime_action_steps(builder, item, visiting))
            .collect(),
        Expr::Name { name } => {
            let Some(entry) = builder.input.ir.values.get(name) else {
                return Vec::new();
            };
            if entry.kind != crate::ir::Kind::Action || !visiting.insert(name.clone()) {
                return Vec::new();
            }
            let body = entry.expression.clone();
            let steps = runtime_action_steps(builder, &body, visiting);
            visiting.remove(name);
            steps
        }
        Expr::Call { name, args } => {
            let argument = |index: usize| args.get(index).map(|a| &a.value);
            let step = match name.as_str() {
                "set" => match (argument(0), argument(1)) {
                    (Some(Expr::State { name }), Some(value)) => Some(Step::Set {
                        state: name.clone(),
                        value: builder.out.intern(value),
                    }),
                    _ => None,
                },
                "restore" => Some(Step::Restore {
                    states: args
                        .iter()
                        .filter_map(|a| match &a.value {
                            Expr::State { name } => Some(name.clone()),
                            _ => None,
                        })
                        .collect(),
                }),
                "refresh" => match argument(0) {
                    Some(Expr::Name { name }) => Some(Step::Refresh {
                        query: name.clone(),
                    }),
                    _ => None,
                },
                "say" => argument(0).map(|text| Step::Say {
                    text: builder.out.intern(text),
                    ctx: argument(1).map(|ctx| builder.out.intern(ctx)),
                }),
                "open" => argument(0).map(|url| Step::Open {
                    url: builder.out.intern(url),
                }),
                "emit" => match argument(0) {
                    Some(Expr::String { value }) => Some(Step::Emit {
                        name: value.clone(),
                        params: args
                            .iter()
                            .skip(1)
                            .map(|a| builder.out.intern(&a.value))
                            .collect(),
                    }),
                    _ => None,
                },
                _ => builder.input.library.action(name).map(|_| Step::Emit {
                    name: name.clone(),
                    params: args.iter().map(|a| builder.out.intern(&a.value)).collect(),
                }),
            };
            step.into_iter().collect()
        }
        _ => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_arg(
    builder: &mut Builder<'_>,
    expression: &Expr,
    arg: &ArgDef,
    locals: &Locals,
    stmt: &str,
    path: &NodePath,
    _def: &ComponentDef,
    _index: usize,
) -> Option<Prop> {
    if let T::Binding { .. } = arg.ty.non_null() {
        if let Expr::State { name } = expression {
            return Some(Prop::Static(PropValue::Binding(name.clone())));
        }
        return None;
    }
    if matches!(arg.ty.non_null(), T::Action) {
        let escaped = arg.name.replace('%', "%25").replace('|', "%7C");
        let suffix = format!("|arg:{escaped}");
        let static_template = builder
            .input
            .ir
            .actions
            .ids()
            .filter_map(|id| {
                let key: &str = id.0.as_ref();
                let prefix = key.strip_suffix(&suffix)?;
                (path.0.starts_with(prefix)
                    && (path.0 == prefix || path.0.as_bytes().get(prefix.len()) == Some(&b':')))
                .then_some((prefix.len(), id.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .and_then(|(_, id)| builder.input.ir.actions.get(&id).cloned());
        let node_id = builder.id_for(path);
        let captured: crate::SharedMap<String, Value> =
            locals.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let id = ActionPlanId(format!("{}|arg:{escaped}", path.0).into());
        let plan = if let Some(template) = static_template {
            ActionPlan {
                steps: template.steps,
                origin: node_id,
                submit_scope: template.submit_scope,
                locals: captured,
            }
        } else {
            let mut visiting = BTreeSet::new();
            let steps = runtime_action_steps(builder, expression, &mut visiting);
            if steps.is_empty() {
                return None;
            }
            ActionPlan {
                steps,
                origin: node_id,
                submit_scope: None,
                locals: captured,
            }
        };
        builder.out.actions.insert(id.clone(), plan);
        return Some(Prop::Static(PropValue::Action(id)));
    }
    let (value, diagnostics, failed) = builder.eval(expression, locals, stmt, path);
    builder.input.diagnostics.extend(diagnostics);
    let mut value = value;
    if !failed
        && value != Value::Null
        && !matches!(value, Value::Ref(_))
        && !builder.input.library.accepts(&arg.ty, &value)
    {
        push_diagnostic(
            &mut builder.input.diagnostics,
            Code::ExprType,
            Some(stmt.to_owned()),
            Some(path.0.clone()),
        );
        value = Value::Null;
    }
    if failed || value == Value::Null {
        if let Some(default) = &arg.default {
            return Some(literal_prop(builder.input.library, default, arg));
        }
        if matches!(arg.ty, T::Optional { .. }) {
            return None;
        }
        if !failed {
            push_diagnostic(
                &mut builder.input.diagnostics,
                Code::MissingRequired,
                Some(stmt.to_owned()),
                Some(path.0.clone()),
            );
        }
        return None;
    }
    Some(literal_prop(builder.input.library, &value, arg))
}
