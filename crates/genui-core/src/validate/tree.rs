use super::*;
use crate::r#lib::{Accepts, ArgDef, ComponentDef, TypeDef};

impl Validator<'_> {
    pub(super) fn walk_expr(
        &mut self,
        e: &Expr,
        ctx: &Context,
        path: NodePath,
        depth: usize,
        place: Option<&str>,
    ) -> crate::SharedVector<NodeId> {
        if self.tasks.is_some() && !self.checking_orphan && !self.checking_template {
            self.walk_retained(e, ctx, path, depth, place)
        } else {
            self.walk_expr_inner(e, ctx, path, depth, place)
        }
    }
    pub(super) fn walk_expr_inner(
        &mut self,
        e: &Expr,
        ctx: &Context,
        path: NodePath,
        depth: usize,
        place: Option<&str>,
    ) -> crate::SharedVector<NodeId> {
        self.direct_limit = false;
        // A bare name is alias resolution; the referenced definition is charged
        // when its expression is walked. Counting both would bill one instance twice.
        if !matches!(e, Expr::Name { .. }) {
            self.work.instances_examined += 1;
        }
        if !self.checking_orphan
            && !self.checking_template
            && matches!(e,Expr::Call{name,..}if self.lib.component(name).is_some())
        {
            let code = if depth > self.limits.max_ir_depth {
                Some(Code::IrDepth)
            } else if !self.checking_template && self.allocated >= self.limits.max_nodes {
                Some(Code::Nodes)
            } else {
                None
            };
            if depth <= self.limits.max_ir_depth
                && let Some(machine) = &mut self.tasks
            {
                machine.charge(self.allocated, self.allocated < self.limits.max_nodes);
            }
            if let Some(code) = code {
                let mut at = ctx.clone();
                at.path = Some(path);
                at.stmt = ctx.budget_stmt.clone().unwrap_or_else(|| ctx.stmt.clone());
                self.limit(code, &at);
                self.direct_limit = true;
                return Default::default();
            }
        }
        // Collapse aliases iteratively, retaining every ancestor for per-instantiation cycle detection.
        if let Expr::Name { name } = e {
            if ctx.locals.contains(name) {
                let mut at = ctx.clone();
                at.path = Some(path);
                self.emit(Code::SlotKind, &at);
                return Default::default();
            }
            if (self.checking_orphan || self.checking_template)
                && self.kinds.get(name) == Some(&Kind::Node)
            {
                if !self.checking_orphan
                    && let Some(s) = self.table.get(name).cloned()
                    && self
                        .template_seen
                        .insert((name.clone(), ctx.locals.iter().cloned().collect()))
                {
                    let mut at = ctx.clone();
                    if at.budget_stmt.is_none() {
                        at.budget_stmt = Some(at.stmt.clone());
                    }
                    at.stmt = s.name;
                    self.template_queue.push_back(TemplateCheck {
                        expression: s.value,
                        ctx: at,
                        path: path.clone(),
                        place: place.map(str::to_owned),
                    });
                }
                let expression = self.ir.intern(e);
                self.pending_plans.push_back(SlotPlan {
                    base: path,
                    expression,
                    accepts: ctx.accepts.clone(),
                });
                return Default::default();
            }
            let mut next = name.clone();
            let mut added = Vec::new();
            let mut at = ctx.clone();
            at.path = Some(path.clone());
            let result = loop {
                if !self.active.insert(next.clone()) {
                    self.emit(Code::Cycle, &at);
                    break Default::default();
                }
                added.push(next.clone());
                let Some(s) = self.table.get(&next).cloned() else {
                    self.recovery(
                        Code::Unresolved,
                        if place.is_some() {
                            Recovery::ChildOmitted
                        } else {
                            Recovery::ValueNulled
                        },
                        &at,
                    );
                    break Default::default();
                };
                if at.budget_stmt.is_none() {
                    at.budget_stmt = Some(at.stmt.clone());
                }
                at.stmt = s.name;
                if s.kind == StatementKind::Query {
                    self.emit(Code::SlotKind, &at);
                    break Default::default();
                }
                if self.kinds.get(&at.stmt) == Some(&Kind::Value) {
                    let null_list = matches!(&s.value, Expr::Null { .. })
                        || matches!(&s.value,Expr::List{items}if items.iter().all(|e|matches!(e,Expr::Null{..})));
                    if !null_list {
                        if self.cache.get(&at.stmt).is_some_and(|v| !v.valid) {
                            break Default::default();
                        }
                        self.emit(Code::SlotKind, &at);
                        break Default::default();
                    }
                }
                if let Expr::Name { name } = s.value {
                    next = name;
                    continue;
                }
                break self.walk_expr(&s.value, &at, path.clone(), depth, place);
            };
            for n in added {
                self.active.remove(&n);
            }
            return result;
        }
        let mut at = ctx.clone();
        at.path = Some(path.clone());
        match e {
            Expr::Null { .. } => Default::default(),
            Expr::List { items } => {
                if path.0 == "root" && place.is_none() && !self.checking_orphan {
                    self.emit(Code::SlotKind, &at);
                    return Default::default();
                }
                if items.len() > self.limits.max_list_len {
                    self.limit(Code::ListLen, &at)
                }
                if self.tasks.is_some() && !self.checking_template && !self.checking_orphan {
                    return self.retained_sequence("$list", items, ctx, &path, depth, place, None);
                }
                let mut children = crate::SharedVector::default();
                for (i, e) in items.iter().take(self.limits.max_list_len).enumerate() {
                    let child = NodePath(format!("{}#{i}", path.0));
                    if matches!(e, Expr::List { .. }) {
                        let mut at = at.clone();
                        at.path = Some(child);
                        self.emit(Code::SlotKind, &at);
                        continue;
                    }
                    let mut item_at = at.clone();
                    if let Some(last) = item_at.order.last_mut() {
                        last.2 = i;
                    }
                    if let Some(last) = item_at.allocation_order.last_mut() {
                        *last = i as u64;
                    }
                    children.extend(self.walk_expr(e, &item_at, child, depth, place));
                    if self.direct_limit {
                        break;
                    }
                }
                children
            }
            Expr::Call { name, .. } if ["each", "if", "coalesce"].contains(&name.as_str()) => {
                self.plan_nodes(e, &at, path, depth, place);
                Default::default()
            }
            Expr::Call { name, args } => {
                let Some((id, def)) = self.lib.component(name).map(|(id, c)| (id, c.clone()))
                else {
                    if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                        self.emit(Code::UnknownComponent, &at);
                    } else if self.classify(e) == Kind::Value {
                        let checked = self.check_value(e, &at);
                        if checked.valid {
                            self.emit(Code::SlotKind, &at);
                        }
                    } else {
                        self.emit(Code::SlotKind, &at);
                    }
                    return Default::default();
                };
                let context_allowed = def.allowed_in.is_empty()
                    || place.is_some_and(|p| def.allowed_in.iter().any(|v| v == p))
                    || (self.checking_orphan && place.is_none());
                let kind_allowed = ctx.accepts.is_empty() || ctx.accepts.contains(&def.name);
                if !context_allowed || !kind_allowed {
                    self.emit(Code::SlotKind, &at);
                    return Default::default();
                }
                let node = self.component(e, (id, &def), args, &at, path, depth);
                // A descendant limit belongs to its own slot, including when
                // min_items drops this component and rolls back its allocation.
                self.direct_limit = false;
                if let Some(node) = node {
                    let allowed = ctx.accepts.is_empty()
                        || self
                            .ir
                            .nodes
                            .get(&node)
                            .and_then(|node| self.lib.components().get(node.kind.0 as usize))
                            .is_some_and(|component| ctx.accepts.contains(&component.name));
                    if !allowed {
                        self.emit(Code::SlotKind, &at);
                        self.remove_subtree(node);
                        return Default::default();
                    }
                    std::iter::once(node).collect()
                } else {
                    Default::default()
                }
            }
            _ => {
                self.emit(Code::SlotKind, &at);
                Default::default()
            }
        }
    }
    fn bind_args(
        &mut self,
        def: &ComponentDef,
        args: &[Argument],
        ctx: &Context,
    ) -> crate::SharedMap<usize, Expr> {
        let mut bound = crate::SharedMap::new();
        let mut named = BTreeSet::new();
        // Named required arguments reserve their position. Positional arguments fill the remainder.
        for a in args.iter().filter(|a| a.name.is_some()) {
            let name = a.name.as_deref().unwrap_or("");
            if !named.insert(name) {
                self.emit(Code::DuplicateArg, ctx);
                continue;
            }
            let Some((i, param)) = def.args.iter().enumerate().find(|(_, p)| p.name == name) else {
                self.emit(Code::UnknownArg, ctx);
                continue;
            };
            if param.required {
                self.emit(Code::RequiredByName, ctx);
            }
            bound.insert(i, a.value.clone());
        }
        for a in args.iter().filter(|a| a.name.is_none()) {
            if let Some((i, _)) = def
                .args
                .iter()
                .enumerate()
                .find(|(i, p)| p.required && !bound.contains_key(i))
            {
                bound.insert(i, a.value.clone());
            } else {
                self.emit(Code::ExtraPositional, ctx);
            }
        }
        bound
    }
    /// A clean concrete subtree can retain checked props and IDs. The canonical
    /// walk still charges its node count/depth at its new position; a previously
    /// truncated subtree cannot be reused because its omitted suffix may now fit.
    fn reuse_subtree(
        &mut self,
        id: ComponentId,
        ctx: &Context,
        path: &NodePath,
        depth: usize,
    ) -> Option<NodeId> {
        if self.checking_orphan || self.checking_template || self.tasks.is_some() {
            return None;
        }
        let reuse = self.reuse.as_ref()?;
        if reuse.dirty.contains(&ctx.stmt) {
            return None;
        }
        let old_id = self.ids.allocate(path);
        let root = reuse.previous.ir.nodes.get(&old_id)?;
        if root.path != *path
            || root.kind != id
            || root.origin != ctx.stmt
            || root.status != NodeStatus::Sealed
        {
            return None;
        }
        let under =
            |p: &str| p == path.0 || p.strip_prefix(&path.0).is_some_and(|s| s.starts_with('/'));
        if reuse.previous.diagnostics.iter().any(|d| {
            matches!(d.code, Code::Nodes | Code::IrDepth) && d.path.as_deref().is_some_and(under)
        }) {
            return None;
        }
        let mut stack = vec![(old_id, depth)];
        let mut nodes = Vec::new();
        while let Some((id, d)) = stack.pop() {
            self.work.instances_examined += 1;
            let n = reuse.previous.ir.nodes.get(&id)?;
            if d > self.limits.max_ir_depth
                || reuse.dirty.contains(&n.origin)
                || n.status != NodeStatus::Sealed
                || n.props.values().any(|p| {
                    !matches!(
                        p,
                        Prop::Static(PropValue::Value(_) | PropValue::Token { .. })
                    )
                })
                || n.slots.iter().any(|s| !s.plans.is_empty())
            {
                return None;
            }
            for s in &n.slots {
                for id in &s.children {
                    stack.push((*id, d + 1));
                }
            }
            nodes.push(n.clone());
            if nodes.len() > self.limits.max_nodes.saturating_sub(self.allocated) {
                return None;
            }
        }
        let mut ids = BTreeMap::new();
        for n in &nodes {
            ids.insert(n.id, self.ids.allocate(&n.path));
        }
        self.allocated += nodes.len();
        reuse.count.set(reuse.count.get() + nodes.len());
        self.diagnostics.extend(
            reuse
                .previous
                .diagnostics
                .iter()
                .filter(|d| d.span.is_none() && d.path.as_deref().is_some_and(under))
                .cloned()
                .map(|mut d| {
                    d.provisional = false;
                    d
                }),
        );
        for mut n in nodes {
            n.id = *ids.get(&n.id)?;
            for s in &mut n.slots {
                for id in &mut s.children {
                    *id = *ids.get(id)?;
                }
            }
            self.ir.nodes.insert(n.id, n);
        }
        Some(old_id)
    }
    pub(super) fn component(
        &mut self,
        source: &Expr,
        definition: (ComponentId, &ComponentDef),
        args: &[Argument],
        ctx: &Context,
        path: NodePath,
        depth: usize,
    ) -> Option<NodeId> {
        let start = self.diagnostics.len();
        let result = self.component_inner(source, definition, args, ctx, path.clone(), depth);
        if result.is_none() && !self.checking_orphan && !self.checking_template {
            let mut local = ctx.clone();
            local.budget_stmt = None;
            let end = self.diagnostics.len();
            // Binding itself only emits diagnostics. Capture its structural
            // result without emitting the same recovery a second time.
            let bound = self
                .retained_instance
                .as_ref()
                .map(|check| check.bound.clone())
                .unwrap_or_else(|| self.bind_args(definition.1, args, &local));
            self.diagnostics.truncate(end);
            let mut reads = BTreeSet::new();
            for (index, arg) in definition.1.args.iter().enumerate() {
                if arg.ty.slot().is_none()
                    && let Some(expression) = bound.get(&index)
                {
                    crate::session::dependencies(expression, &mut reads);
                }
            }
            let id = self.ids.allocate(&path);
            self.record_instance(
                id,
                InstanceCheck {
                    source: self.retained_instance.as_ref().map_or_else(
                        || std::sync::Arc::new(source.clone()),
                        |instance| instance.source.clone(),
                    ),
                    ctx: local,
                    path,
                    depth,
                    bound,
                    reads,
                    diagnostics: self.diagnostics[start..].to_vec(),
                },
            );
        }
        result
    }
    fn component_inner(
        &mut self,
        source: &Expr,
        definition: (ComponentId, &ComponentDef),
        args: &[Argument],
        ctx: &Context,
        path: NodePath,
        depth: usize,
    ) -> Option<NodeId> {
        let (id, def) = definition;
        let location = self.tasks.as_ref().map(|machine| machine.location.clone());
        if let Some(id) = self.reuse_subtree(id, ctx, &path, depth) {
            return Some(id);
        }
        let node_id = self.ids.allocate(&path);
        let mut local = ctx.clone();
        local.budget_stmt = None;
        if def.name == "Form" {
            local.form_scope = Some(node_id);
        }
        let ctx = &local;
        let local_diag_start = self.diagnostics.len();
        let retained = self.retained_instance.clone();
        let mut bound = if let Some(instance) = &retained {
            for diagnostic in &instance.diagnostics {
                if matches!(
                    diagnostic.code,
                    Code::DuplicateArg
                        | Code::UnknownArg
                        | Code::RequiredByName
                        | Code::ExtraPositional
                ) {
                    self.replay(diagnostic.clone());
                }
            }
            instance.bound.clone()
        } else {
            self.bind_args(def, args, ctx)
        };
        let mut props = BTreeMap::new();
        let mut slots = Vec::new();
        let mut failed = false;
        for (i, arg) in def
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.ty.slot().is_none())
        {
            let value = bound.get(&i);
            let prop = if let Some(e) = value {
                {
                    let mut at = ctx.clone();
                    if def.name == "Facts" && arg.name == "items" {
                        at.list_cap = Some(50);
                    }
                    self.prop(e, arg, &at, i)
                }
            } else if let Some(v) = &arg.default {
                if arg.required {
                    self.recovery(Code::MissingRequired, Recovery::Defaulted, ctx);
                }
                Some(self.literal_prop(v.clone(), arg))
            } else if arg.required {
                self.emit(Code::MissingRequired, ctx);
                failed = true;
                None
            } else {
                None
            };
            if let Some(p) = prop {
                props.insert(PropId(i as u16), p);
            } else if arg.required {
                failed = true
            }
        }
        for (i, arg) in def
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.ty.slot().is_some())
        {
            if !bound.contains_key(&i) {
                if let Some(v) = &arg.default {
                    if arg.required {
                        self.recovery(Code::MissingRequired, Recovery::Defaulted, ctx);
                    }
                    bound.insert(i, v.to_expr());
                } else if arg.required {
                    self.emit(Code::MissingRequired, ctx);
                    failed = true;
                }
            }
        }
        if failed {
            return None;
        }
        if self.retained_slots.is_some() && self.retained_instance.is_none() {
            let previous = self.instances.get(&node_id)?;
            for (index, _) in def
                .args
                .iter()
                .enumerate()
                .filter(|(_, a)| a.ty.slot().is_some())
            {
                match (previous.bound.get(&index), bound.get(&index)) {
                    (Some(a), Some(b)) if a == b && a.recovery_metadata_eq(b) => {}
                    (None, None) => {}
                    _ => return None,
                }
            }
        }
        let own_before_slots = self.diagnostics[local_diag_start..].to_vec();
        let before = self.allocated;
        self.allocated += 1;
        let retained_slots = self.retained_slots.take();
        for (i, arg) in def
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.ty.slot().is_some())
        {
            if let Some(retained) = &retained_slots
                && let Some(slot) = retained.iter().find(|slot| slot.name == arg.name)
            {
                slots.push(slot.clone());
                continue;
            }
            let Some(expr) = bound.get(&i) else {
                slots.push(IrSlot {
                    name: arg.name.clone(),
                    children: Default::default(),
                    plans: Default::default(),
                });
                continue;
            };
            let children = self.slot(
                expr,
                ctx,
                &path,
                arg,
                depth + 1,
                (&format!("{}.{}", def.name, arg.name), i),
            );
            let plans = std::mem::take(&mut self.pending_plans);
            if arg.min_items.is_some_and(|min| children.len() < min) && plans.is_empty() {
                self.emit(Code::MinItems, ctx);
                failed = true
            }
            slots.push(IrSlot {
                name: arg.name.clone(),
                children,
                plans,
            });
        }
        let after_slots = self.diagnostics.len();
        if failed {
            for s in slots {
                for id in s.children {
                    self.remove_subtree(id)
                }
            }
            self.allocated = before;
            return None;
        }
        if def.name == "Tab"
            && let (Some(label), Some(value)) = (
                def.args.iter().position(|a| a.name == "label"),
                def.args.iter().position(|a| a.name == "value"),
            )
            && !props.contains_key(&PropId(value as u16))
            && let Some(p) = props.get(&PropId(label as u16)).cloned()
        {
            props.insert(PropId(value as u16), p);
        }
        if def.name == "Facts"
            && let Some((i, arg)) = def.args.iter().enumerate().find(|(_, a)| a.name == "items")
            && let Some(Prop::Static(PropValue::Value(Value::List(items)))) =
                props.get_mut(&PropId(i as u16))
        {
            if items.iter().any(|v|!matches!(v,Value::Map(m)if m.iter().any(|(k,v)|k=="label"&&v.string().is_some())&&m.iter().any(|(k,_)|k=="value"))){self.bad_prop(Code::SchemaType,arg,ctx);self.allocated=before;for slot in slots{for id in slot.children{self.remove_subtree(id)}}return None}
            for item in std::sync::Arc::make_mut(items) {
                if let Value::Map(m) = item {
                    std::sync::Arc::make_mut(m).retain(|(k, _)| k == "label" || k == "value");
                }
            }
            if items.len() > 50 {
                std::sync::Arc::make_mut(items).truncate(50);
                self.limit(Code::ListLen, ctx);
            }
        }
        if def.name == "Chart" {
            self.chart_summary(def, &mut props, ctx)
        }
        let node_id = self.ids.allocate(&path);
        let mut node = Node {
            id: node_id,
            path,
            kind: id,
            role: def.role,
            props,
            slots,
            status: NodeStatus::Sealed,
            origin: ctx.stmt.clone(),
        };
        if def
            .requires
            .iter()
            .any(|r| !self.options.capabilities.contains(r))
        {
            self.fallback(source, def, &mut node, ctx);
        }
        if !self.checking_template && !self.checking_orphan {
            let mut diagnostics = own_before_slots;
            diagnostics.extend_from_slice(&self.diagnostics[after_slots..]);
            let mut reads = BTreeSet::new();
            for (index, _arg) in def
                .args
                .iter()
                .enumerate()
                .filter(|(_, arg)| arg.ty.slot().is_none())
            {
                if let Some(e) = bound.get(&index) {
                    crate::session::dependencies(e, &mut reads);
                }
            }
            self.record_instance(
                node_id,
                InstanceCheck {
                    source: self.retained_instance.as_ref().map_or_else(
                        || std::sync::Arc::new(source.clone()),
                        |instance| instance.source.clone(),
                    ),
                    ctx: ctx.clone(),
                    path: node.path.clone(),
                    depth,
                    bound,
                    reads,
                    diagnostics,
                },
            );
        }
        if !self.checking_orphan
            && !self.checking_template
            && let Some(machine) = &mut self.tasks
        {
            machine
                .graph
                .insert(node.clone(), self.allocated.saturating_sub(before));
            if let Some(location) = location {
                machine.node_location(node_id, location);
            }
        }
        if self.ir.nodes.get(&node_id) != Some(&node) {
            self.ir.nodes.insert(node_id, node);
        }
        Some(node_id)
    }
    fn literal_prop(&self, mut v: Value, arg: &ArgDef) -> Prop {
        if v == Value::Null {
            return Prop::Static(PropValue::Value(v));
        }
        if matches!(arg.ty.non_null(), T::Float)
            && let Value::Int(i) = v
        {
            v = Value::Float(i as f64)
        }
        if let T::Named { name } = arg.ty.non_null()
            && let Some(TypeDef::Token { scale, .. }) = self.lib.types().get(name)
        {
            return Prop::Static(PropValue::Token {
                scale: scale.clone(),
                value: v,
            });
        }
        Prop::Static(PropValue::Value(v))
    }
    fn bad_prop(&mut self, code: Code, arg: &ArgDef, ctx: &Context) -> Option<Prop> {
        let recovery = if arg.default.is_some() {
            Recovery::Defaulted
        } else if arg.required {
            Recovery::NodeDropped
        } else {
            Recovery::ArgOmitted
        };
        self.recovery(code, recovery, ctx);
        if code == Code::Enum
            && let T::Named { name } = arg.ty.non_null()
            && let Some(TypeDef::Enum { values }) = self.lib.types().get(name)
            && let Some(d) = self.diagnostics.last_mut()
        {
            d.hint = Some(
                values
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join("|"),
            );
        }
        arg.default.clone().map(|v| self.literal_prop(v, arg))
    }
    fn recovered_prop(&self, arg: &ArgDef) -> Option<Prop> {
        arg.default
            .clone()
            .or_else(|| matches!(arg.ty, T::Optional { .. }).then_some(Value::Null))
            .map(|v| self.literal_prop(v, arg))
    }
    fn prop_owner(ctx: &Context, arg: &ArgDef) -> String {
        let argument = arg.name.replace('%', "%25").replace('|', "%7C");
        format!(
            "{}|arg:{argument}",
            ctx.path
                .as_ref()
                .map_or(ctx.stmt.as_str(), |path| path.0.as_str())
        )
    }
    fn prop(&mut self, e: &Expr, arg: &ArgDef, ctx: &Context, index: usize) -> Option<Prop> {
        let mut order = ctx.allocation_order.clone();
        order.extend([0, index as u64]);
        let scope = self.begin_owner(Self::prop_owner(ctx, arg), order);
        let result = self.prop_inner(e, arg, ctx);
        self.ir.end_owner(scope);
        result
    }
    fn prop_inner(&mut self, e: &Expr, arg: &ArgDef, ctx: &Context) -> Option<Prop> {
        if let Expr::Name { name } = e
            && !ctx.locals.contains(name)
            && let Some(value) = self.cache.get(name)
        {
            if !value.valid {
                return self.recovered_prop(arg);
            }
            if matches!(arg.ty, T::Optional { .. }) && value.value == Some(Value::Null) {
                return Some(self.literal_prop(Value::Null, arg));
            }
        }
        if matches!(arg.ty, T::Optional { .. }) && matches!(e, Expr::Null { .. }) {
            return Some(Prop::Static(PropValue::Value(Value::Null)));
        }
        if let T::Binding { of } = arg.ty.non_null() {
            if let Expr::State { name } = e {
                if let Some(state) = self.ir.states.get(name).cloned() {
                    let infer = state.default == Value::Null
                        || matches!(&state.default,Value::List(v)if v.is_empty());
                    if infer
                        && self.sealed_states.is_none()
                        && (matches!(state.ty, T::Any)
                            || matches!((&state.ty,of.as_ref()),(T::List{of:t},T::List{..})if matches!(t.as_ref(),T::Any)))
                        && let Some(s) = self.ir.states.get_mut(name)
                    {
                        s.ty = *of.clone();
                    }
                    let valid = if infer {
                        self.ir
                            .states
                            .get(name)
                            .is_some_and(|s| type_compatible(of, &s.ty))
                    } else {
                        self.lib.accepts(of, &state.default)
                    };
                    if valid {
                        return Some(Prop::Static(PropValue::Binding(name.clone())));
                    }
                } else {
                    self.emit(Code::Undeclared, ctx);
                    return self.recovered_prop(arg);
                }
            }
            self.emit(Code::BindTarget, ctx);
            return self.recovered_prop(arg);
        }
        if matches!(arg.ty.non_null(), T::Action) {
            let before = self.diagnostics.len();
            let (valid, normalized) = self.normalize_action(e, ctx);
            if valid {
                let steps = self.action_steps(&normalized);
                if steps.is_empty() {
                    return self.recovered_prop(arg);
                }
                let id = ActionPlanId(Self::prop_owner(ctx, arg).into());
                let origin = ctx
                    .path
                    .as_ref()
                    .map(|p| self.ids.allocate(p))
                    .unwrap_or(NodeId(0));
                let submit_scope = if arg.name == "on_submit" {
                    ctx.form_scope
                } else {
                    None
                };
                let id = self.ir.intern_action(
                    id,
                    ActionPlan {
                        steps,
                        origin,
                        submit_scope,
                    },
                );
                return Some(Prop::Static(PropValue::Action(id)));
            }
            if self.diagnostics.len() == before {
                return self.bad_prop(Code::SchemaType, arg, ctx);
            }
            return self.recovered_prop(arg);
        }
        let checked = self.check_value(e, ctx);
        if !checked.valid {
            return self.recovered_prop(arg);
        }
        let Some(mut v) = checked.value else {
            return Some(self.dynamic(e));
        };
        if matches!(arg.ty.non_null(), T::Url) && matches!(v, Value::Str(_)) && !self.valid_url(&v)
        {
            self.emit(Code::TaintedUrl, ctx);
            return self.recovered_prop(arg);
        }
        if let Some(cap) = ctx.list_cap
            && let Value::List(items) = &mut v
            && items.len() > cap.min(self.limits.max_list_len)
        {
            std::sync::Arc::make_mut(items).truncate(cap.min(self.limits.max_list_len));
            self.limit(Code::ListLen, ctx);
        }
        // Range checks happen before token membership checks, and preserve the literal numeric kind.
        let range = if let T::Named { name } = arg.ty.non_null() {
            match self.lib.types().get(name) {
                Some(TypeDef::Token { min, max, .. }) => Some((*min as f64, *max as f64)),
                _ => None,
            }
        } else {
            arg.min.zip(arg.max)
        };
        if (matches!(arg.ty.non_null(), T::Int)
            || matches!(arg.ty.non_null(),T::Named{name}if matches!(self.lib.types().get(name),Some(TypeDef::Token{..}))))
            && !matches!(v, Value::Int(_))
        {
            return self.bad_prop(Code::SchemaType, arg, ctx);
        }
        if let Some((min, max)) = range
            && let Some(n) = v.number()
            && (n < min || n > max)
        {
            self.emit(Code::Range, ctx);
            v = if matches!(v, Value::Int(_)) {
                Value::Int(n.clamp(min, max) as i64)
            } else {
                Value::Float(n.clamp(min, max))
            }
        }
        if !self.lib.accepts(&arg.ty, &v) {
            let code = if let T::Named { name } = arg.ty.non_null() {
                if matches!(self.lib.types().get(name), Some(TypeDef::Enum { .. })) {
                    Code::Enum
                } else {
                    Code::SchemaType
                }
            } else {
                Code::SchemaType
            };
            return self.bad_prop(code, arg, ctx);
        }
        Some(self.literal_prop(v, arg))
    }
    fn slot(
        &mut self,
        e: &Expr,
        ctx: &Context,
        parent: &NodePath,
        arg: &ArgDef,
        depth: usize,
        placement: (&str, usize),
    ) -> crate::SharedVector<NodeId> {
        let (place, slot_index) = placement;
        let name = &arg.name;
        let Some(accepts) = arg.ty.slot() else {
            return Default::default();
        };
        let expressions = if let Expr::List { items } = e {
            items.as_slice()
        } else {
            std::slice::from_ref(e)
        };
        if expressions.len() > self.limits.max_list_len {
            self.limit(Code::ListLen, ctx)
        }
        if self.tasks.is_some() && !self.checking_template && !self.checking_orphan {
            return self.retained_sequence(
                name,
                expressions,
                ctx,
                parent,
                depth,
                Some(place),
                Some((arg, slot_index)),
            );
        }
        let mut entries = crate::SharedMap::new();
        let mut sizes = Vec::new();
        let mut repeats: BTreeMap<String, usize> = BTreeMap::new();
        let mut children = crate::SharedVector::default();
        let mut plans = Vec::new();
        for (i, e) in expressions
            .iter()
            .take(self.limits.max_list_len)
            .enumerate()
        {
            let base = match e {
                Expr::Name { name } => {
                    let k = repeats.entry(name.clone()).or_default();
                    *k += 1;
                    if *k == 1 {
                        format!("@{name}")
                    } else {
                        format!("@{name}~{k}")
                    }
                }
                Expr::Call { name, .. } if name == "each" => format!("e{i}"),
                _ => format!("#{i}"),
            };
            let path = parent.child(name, &base);
            let mut at = ctx.clone();
            at.path = Some(path.clone());
            at.order.push((name.clone(), i, 0));
            at.allocation_order
                .extend([1, slot_index as u64, i as u64, 0]);
            at.accepts = if let Accepts::Only(v) = accepts {
                v.clone()
            } else {
                Vec::new()
            };
            let before = self.diagnostics.len();
            let before_children = children.len();
            let pending = std::mem::take(&mut self.pending_plans);
            let ids = self.walk_expr(e, &at, path.clone(), depth, Some(place));
            let new_plans = std::mem::replace(&mut self.pending_plans, pending);
            let only = if let Accepts::Only(v) = accepts {
                Some(v)
            } else {
                None
            };
            for id in ids {
                let allowed = only.is_none_or(|allowed| {
                    self.ir
                        .nodes
                        .get(&id)
                        .and_then(|n| self.lib.components().get(n.kind.0 as usize))
                        .is_some_and(|c| allowed.contains(&c.name))
                });
                if allowed {
                    children.push_back(id)
                } else {
                    let mut c = at.clone();
                    c.path = self.ir.nodes.get(&id).map(|n| n.path.clone());
                    self.emit(Code::SlotKind, &c);
                    self.remove_subtree(id)
                }
            }
            let planned = !new_plans.is_empty();
            plans.extend(new_plans.into_iter().map(|mut p| {
                if let Some(v) = only {
                    p.accepts = v.clone()
                }
                p
            }));
            if !self.checking_orphan && !self.checking_template {
                let mut reads = BTreeSet::new();
                crate::session::dependencies(e, &mut reads);
                let output: crate::SharedVector<_> =
                    children.iter().skip(before_children).copied().collect();
                sizes.push(output.len());
                entries.insert(
                    i,
                    SlotItem {
                        source: std::sync::Arc::new(e.clone()),
                        ctx: at.clone(),
                        path,
                        output,
                        diagnostics: self.diagnostics[before..].to_vec(),
                        reads,
                        planned,
                    },
                );
            }
            if self.direct_limit {
                break;
            }
        }
        if !self.checking_orphan && !self.checking_template {
            let key = SlotKey {
                parent: self.ids.allocate(parent),
                name: name.clone(),
            };
            self.record_slot(
                key,
                SlotCheck {
                    parent_path: parent.clone(),
                    arg: arg.clone(),
                    depth,
                    place: place.into(),
                    items: entries,
                    counts: PrefixCounts::new(&sizes),
                },
            );
        }
        self.pending_plans.extend(plans);
        children
    }
    fn plan_nodes(
        &mut self,
        e: &Expr,
        ctx: &Context,
        path: NodePath,
        depth: usize,
        place: Option<&str>,
    ) {
        let Expr::Call { name, args } = e else { return };
        let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
        let valid = if name == "each" {
            positional.len() == 2
                && args.iter().filter(|a| a.name.is_some()).count() <= 1
                && args
                    .iter()
                    .filter(|a| a.name.is_some())
                    .all(|a| a.name.as_deref() == Some("key"))
        } else if name == "if" {
            args.len() == 3 && positional.len() == 3
        } else {
            args.len() >= 2 && args.len() == positional.len()
        };
        if !valid {
            self.emit(Code::ExprType, ctx);
            return;
        }
        if name == "each" {
            let collection = self.check_value(&positional[0].value, ctx);
            if !collection.valid {
                return;
            }
            if !matches!(collection.ty, T::Any | T::Data { .. } | T::List { .. }) {
                self.emit(Code::ExprType, ctx);
                return;
            }
            let Expr::Lambda { params, body } = &positional[1].value else {
                self.emit(Code::ExprType, ctx);
                return;
            };
            if !(1..=2).contains(&params.len()) {
                self.emit(Code::ExprType, ctx);
                return;
            }
            let mut local = ctx.clone();
            local.locals.extend(params.iter().cloned());
            self.check_template(body, &local, path.clone(), depth, place);
            if let Some(key) = args.iter().find(|a| a.name.as_deref() == Some("key")) {
                match &key.value {
                    Expr::String { .. } => {}
                    Expr::Lambda { params, body } if (1..=2).contains(&params.len()) => {
                        let mut local = ctx.clone();
                        local.locals.extend(params.iter().cloned());
                        self.check_value(body, &local);
                    }
                    _ => {
                        self.emit(Code::ExprType, ctx);
                        return;
                    }
                }
            }
        } else {
            if name == "if" {
                let c = self.check_value(&positional[0].value, ctx);
                if !c.valid {
                    return;
                }
                if !matches!(c.ty, T::Bool | T::Any) {
                    self.emit(Code::ExprType, ctx);
                    return;
                }
            }
            for a in positional.iter().skip(usize::from(name == "if")) {
                self.check_template(&a.value, ctx, path.clone(), depth, place);
            }
        }
        let mut order = ctx.allocation_order.clone();
        order.push(0);
        let scope = self.begin_owner(format!("{}|$plan", path.0), order);
        let expression = self.ir.intern(e);
        self.ir.end_owner(scope);
        self.pending_plans.push_back(SlotPlan {
            base: path,
            expression,
            accepts: ctx.accepts.clone(),
        });
    }
    fn check_template(
        &mut self,
        e: &Expr,
        ctx: &Context,
        path: NodePath,
        depth: usize,
        place: Option<&str>,
    ) {
        let concrete_ids = std::mem::take(&mut self.ids);
        let mut keep = std::mem::take(&mut self.ir);
        self.ir.states = keep.states.clone();
        self.ir.queries = keep.queries.clone();
        let allocated = self.allocated;
        let old = self.checking_template;
        let pending = std::mem::take(&mut self.pending_plans);
        self.checking_template = true;
        if !old {
            self.template_queue.clear();
            self.template_seen.clear();
        }
        self.walk_expr(e, ctx, path, depth, place);
        if !old {
            while let Some(job) = self.template_queue.pop_front() {
                self.walk_expr(&job.expression, &job.ctx, job.path, 1, job.place.as_deref());
            }
        }
        if !self.checking_orphan {
            keep.states = std::mem::take(&mut self.ir.states);
        }
        self.ir = keep;
        self.ids = concrete_ids;
        self.allocated = allocated;
        self.checking_template = old;
        self.pending_plans = pending;
    }
    fn chart_summary(
        &mut self,
        def: &ComponentDef,
        props: &mut BTreeMap<PropId, Prop>,
        ctx: &Context,
    ) {
        let mut order = ctx.allocation_order.clone();
        order.extend([2, 0]);
        let scope = self.begin_owner(
            format!(
                "{}|$summary",
                ctx.path
                    .as_ref()
                    .map_or(ctx.stmt.as_str(), |p| p.0.as_str())
            ),
            order,
        );
        self.chart_summary_inner(def, props, ctx);
        self.ir.end_owner(scope);
    }
    fn chart_summary_inner(
        &mut self,
        def: &ComponentDef,
        props: &mut BTreeMap<PropId, Prop>,
        ctx: &Context,
    ) {
        let Some(summary) = def.args.iter().position(|a| a.name == "summary") else {
            return;
        };
        if props.contains_key(&PropId(summary as u16)) {
            return;
        }
        let find = |name: &str| {
            def.args
                .iter()
                .position(|a| a.name == name)
                .and_then(|i| props.get(&PropId(i as u16)))
        };
        let kind_prop = find("kind").cloned();
        let data_prop = find("data").cloned();
        let kind = match &kind_prop {
            Some(Prop::Static(PropValue::Value(v))) => {
                v.string().map(|s| if s == "line" { "Line" } else { "Bar" })
            }
            _ => None,
        };
        let count = match &data_prop {
            Some(Prop::Static(PropValue::Value(Value::List(v)))) => Some(v.len()),
            _ => None,
        };
        let prop = if let (Some(kind), Some(count)) = (kind, count) {
            Prop::Static(PropValue::Value(Value::from(format!(
                "{kind} chart, {count} points"
            ))))
        } else {
            let expression = |prop: &Option<Prop>| match prop {
                Some(Prop::Dynamic(id)) => self
                    .ir
                    .expressions
                    .get(id)
                    .cloned()
                    .unwrap_or_else(Expr::null),
                Some(Prop::Static(PropValue::Value(v))) => v.to_expr(),
                _ => Expr::null(),
            };
            let call = |name: &str, values: Vec<Expr>| Expr::Call {
                name: name.into(),
                args: values
                    .into_iter()
                    .map(|value| Argument { name: None, value })
                    .collect(),
            };
            let kind_expr = if let Some(kind) = kind {
                Expr::String { value: kind.into() }
            } else {
                call(
                    "if",
                    vec![
                        Expr::Binary {
                            op: crate::syntax::ast::BinaryOp::Eq,
                            left: Box::new(expression(&kind_prop)),
                            right: Box::new(Expr::String {
                                value: "line".into(),
                            }),
                        },
                        Expr::String {
                            value: "Line".into(),
                        },
                        Expr::String {
                            value: "Bar".into(),
                        },
                    ],
                )
            };
            let e = call(
                "fmt",
                vec![
                    Expr::String {
                        value: "{} chart, {} points".into(),
                    },
                    kind_expr,
                    call("len", vec![expression(&data_prop)]),
                ],
            );
            self.dynamic(&e)
        };
        props.insert(PropId(summary as u16), prop);
        self.emit(Code::A11yMissing, ctx);
    }
    fn fallback(&mut self, source: &Expr, def: &ComponentDef, node: &mut Node, ctx: &Context) {
        let target = if self.options.capabilities.contains("markdown") {
            "Markdown"
        } else {
            "Text"
        };
        let Some((id, replacement)) = self.lib.component(target) else {
            return;
        };
        let mut text = def.to_text.clone();
        let mut dynamic = false;
        for (i, a) in def.args.iter().enumerate() {
            let value = node.props.get(&PropId(i as u16));
            let replacement = match value {
                Some(Prop::Static(PropValue::Value(v))) => {
                    dynamic |= matches!(
                        v,
                        Value::Float(_) | Value::List(_) | Value::Map(_) | Value::Ref(_)
                    );
                    display(v)
                }
                Some(Prop::Static(PropValue::Token { value, .. })) => display(value),
                None => String::new(),
                _ => {
                    dynamic = true;
                    String::new()
                }
            };
            if a.ty.slot().is_none() {
                text = text.replace(&format!("{{{}}}", a.name), &replacement);
            }
        }
        for s in &node.slots {
            let text_children = s
                .children
                .iter()
                .map(|id| self.node_text(*id))
                .collect::<Vec<_>>()
                .join(if def.name == "Row" { " · " } else { "\n" });
            text = text.replace(&format!("{{{}}}", s.name), &text_children);
            dynamic |= !s.plans.is_empty();
        }
        text = text.replace("{value}", "").replace("{values}", "");
        let descendants = self.descendants(node);
        dynamic |= descendants.iter().any(|n| {
            n.props.values().any(|p| {
                matches!(
                    p,
                    Prop::Dynamic(_)
                        | Prop::Static(PropValue::Binding(_))
                        | Prop::Static(PropValue::Value(
                            Value::Float(_) | Value::List(_) | Value::Map(_) | Value::Ref(_)
                        ))
                )
            }) || n.slots.iter().any(|s| !s.plans.is_empty())
        });
        let mut props = BTreeMap::new();
        for (i, a) in replacement.args.iter().enumerate() {
            if let Some(v) = &a.default {
                props.insert(PropId(i as u16), self.literal_prop(v.clone(), a));
            }
        }
        if let Some(i) = replacement.args.iter().position(|a| a.required) {
            props.insert(
                PropId(i as u16),
                if dynamic {
                    let mut order = ctx.allocation_order.clone();
                    order.extend([2, 1]);
                    let scope = self.begin_owner(format!("{}|$fallback", node.path.0), order);
                    let expression = self.ir.expressions.intern_unique(source);
                    self.ir.end_owner(scope);
                    self.ir.fallbacks.insert(
                        expression.clone(),
                        FallbackPlan {
                            original: node.clone(),
                            descendants: descendants.clone(),
                            template: def.to_text.clone(),
                        },
                    );
                    Prop::Dynamic(expression)
                } else {
                    Prop::Static(PropValue::Value(Value::from(text)))
                },
            );
        }
        for s in &node.slots {
            for id in &s.children {
                self.remove_subtree(*id)
            }
        }
        node.slots.clear();
        node.kind = id;
        node.role = replacement.role;
        node.props = props;
        self.emit(Code::CapFallback, ctx);
    }
    fn descendants(&self, node: &Node) -> Vec<Node> {
        let mut pending: Vec<_> = node
            .slots
            .iter()
            .flat_map(|s| s.children.iter().copied())
            .collect();
        let mut out = Vec::new();
        while let Some(id) = pending.pop() {
            if let Some(n) = self.ir.nodes.get(&id) {
                pending.extend(n.slots.iter().flat_map(|s| s.children.iter().copied()));
                out.push(n.clone());
            }
        }
        out
    }
    fn node_text(&self, id: NodeId) -> String {
        let Some(n) = self.ir.nodes.get(&id) else {
            return String::new();
        };
        let Some(c) = self.lib.components().get(n.kind.0 as usize) else {
            return String::new();
        };
        let mut text = c.to_text.clone();
        for (i, a) in c.args.iter().enumerate() {
            let v = match n.props.get(&PropId(i as u16)) {
                Some(Prop::Static(PropValue::Value(v))) => display(v),
                _ => String::new(),
            };
            text = text.replace(&format!("{{{}}}", a.name), &v)
        }
        for s in &n.slots {
            text = text.replace(
                &format!("{{{}}}", s.name),
                &s.children
                    .iter()
                    .map(|id| self.node_text(*id))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        text
    }
}
fn display(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Str(s) => s.text.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => v.plain_json().to_string(),
    }
}
fn type_compatible(a: &T, b: &T) -> bool {
    a == b || matches!(a, T::Any) || matches!((a, b), (T::Number, T::Int | T::Float))
}

impl Validator<'_> {
    /// Infer pending state types in a heap-based canonical structural pre-order.
    /// Templates are checked, never evaluated; revisiting the same template/environment adds no bindings.
    pub(super) fn infer_binding_defaults(&mut self) {
        let Some(root) = self.table.get("root").map(|s| s.value.clone()) else {
            return;
        };
        if !self
            .ir
            .states
            .values()
            .any(|s| s.default == Value::Null || matches!(&s.default,Value::List(v)if v.is_empty()))
        {
            return;
        }
        let mut jobs = vec![(root, BTreeSet::<String>::new())];
        let mut seen = BTreeSet::new();
        while let Some((e, locals)) = jobs.pop() {
            self.work.instances_examined += 1;
            match e {
                Expr::Name { name } if !locals.contains(&name) => {
                    if self.kinds.get(&name) == Some(&Kind::Node)
                        && seen.insert((name.clone(), locals.clone()))
                        && let Some(s) = self.table.get(&name)
                    {
                        jobs.push((s.value.clone(), locals));
                    }
                }
                Expr::List { items } => {
                    for e in items.into_iter().rev() {
                        jobs.push((e, locals.clone()));
                    }
                }
                Expr::Call { name, args } => {
                    if let Some((_, component)) = self.lib.component(&name) {
                        let mut bound = BTreeMap::new();
                        for a in args.iter().filter(|a| a.name.is_some()) {
                            if let Some(i) = component
                                .args
                                .iter()
                                .position(|p| Some(p.name.as_str()) == a.name.as_deref())
                            {
                                bound.entry(i).or_insert_with(|| a.value.clone());
                            }
                        }
                        for a in args.iter().filter(|a| a.name.is_none()) {
                            if let Some(i) = component
                                .args
                                .iter()
                                .enumerate()
                                .find(|(i, p)| p.required && !bound.contains_key(i))
                                .map(|(i, _)| i)
                            {
                                bound.insert(i, a.value.clone());
                            }
                        }
                        for (i, a) in component.args.iter().enumerate() {
                            if let T::Binding { of } = a.ty.non_null()
                                && let Some(Expr::State { name }) = bound.get(&i)
                                && let Some(state) = self.ir.states.get_mut(name)
                            {
                                let pending_null =
                                    state.default == Value::Null && matches!(state.ty, T::Any);
                                let pending_list = matches!(&state.default,Value::List(v)if v.is_empty())
                                    && matches!((&state.ty,of.as_ref()),(T::List{of:t},T::List{..})if matches!(t.as_ref(),T::Any));
                                if pending_null || pending_list {
                                    state.ty = *of.clone();
                                }
                            }
                        }
                        for (i, a) in component.args.iter().enumerate().rev() {
                            if a.ty.slot().is_some()
                                && let Some(e) = bound.remove(&i)
                            {
                                jobs.push((e, locals.clone()));
                            }
                        }
                    } else if name == "each" {
                        if let Some(Argument {
                            value: Expr::Lambda { params, body },
                            ..
                        }) = args.get(1)
                        {
                            let mut local = locals;
                            local.extend(params.iter().cloned());
                            jobs.push((*body.clone(), local));
                        }
                    } else if name == "if" || name == "coalesce" {
                        for a in args.into_iter().skip(usize::from(name == "if")).rev() {
                            jobs.push((a.value, locals.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#lib::{ComponentDef, GroupDef, Role};

    fn library() -> Library {
        Library::builder("tree", "0.1.0")
            .group(GroupDef::new("G", 0, "Retained slot fixtures."))
            .component(
                ComponentDef::new("Leaf", Role::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"x\")"),
            )
            .component(
                ComponentDef::new("Frame", Role::Group)
                    .group("G")
                    .doc("A frame with an optional header.")
                    .to_text("{children}")
                    .req("children", T::nodes())
                    .opt_none("header", T::nodes())
                    .example("Frame([])"),
            )
            .component(
                ComponentDef::new("Show", Role::Paragraph)
                    .group("G")
                    .doc("A text binding sink.")
                    .req("value", T::binding(T::text())),
            )
            .component(
                ComponentDef::new("ShowList", Role::Paragraph)
                    .group("G")
                    .doc("A list binding sink.")
                    .req("items", T::binding(T::list(T::text()))),
            )
            .component(
                ComponentDef::new("Rich", Role::Group)
                    .group("G")
                    .doc("A capability-gated container.")
                    .to_text("Rich {title}: {children}")
                    .requires("markdown")
                    .req("title", T::text())
                    .req("children", T::nodes()),
            )
            .component(
                ComponentDef::new("Text", Role::Paragraph)
                    .group("G")
                    .doc("A text fallback.")
                    .to_text("{text}")
                    .req("text", T::text()),
            )
            .component(
                ComponentDef::new("Markdown", Role::Paragraph)
                    .group("G")
                    .doc("A markdown fallback.")
                    .to_text("{text}")
                    .req("text", T::text()),
            )
            .build()
            .unwrap()
    }

    fn call(source: &str) -> (Expr, Vec<Argument>) {
        let parsed = parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default());
        let statement = parsed
            .program
            .statements
            .into_iter()
            .next()
            .expect("one statement");
        let Expr::Call { args, .. } = statement.value.clone() else {
            panic!("expected a component call");
        };
        (statement.value, args)
    }

    fn retained_validator<'a>(
        lib: &'a Library,
        options: &'a Options,
        path: &NodePath,
        old: Expr,
        old_args: &[Argument],
    ) -> (Validator<'a>, crate::ir::NodeId) {
        let mut validator = Validator::new(&Program::default(), Vec::new(), lib, options);
        let id = validator.ids.allocate(path);
        let mut bound = crate::SharedMap::default();
        bound.insert(0, old_args[0].value.clone());
        validator.instances.insert(
            id,
            InstanceCheck {
                source: std::sync::Arc::new(old),
                ctx: Context::at("root", Some(path.clone())),
                path: path.clone(),
                depth: 1,
                bound,
                reads: BTreeSet::new(),
                diagnostics: Vec::new(),
            },
        );
        validator.retained_slots = Some(Vec::new());
        (validator, id)
    }

    #[test]
    fn changed_retained_slot_bounds_reject_reuse() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let (old, old_args) = call("root=Frame([Leaf(\"a\")])");
        let (new, new_args) = call("root=Frame([Leaf(\"b\")])");
        let (mut validator, _) = retained_validator(&lib, &options, &path, old, &old_args);
        let (kind, def) = lib.component("Frame").expect("Frame");
        let ctx = Context::at("root", Some(path.clone()));
        assert!(
            validator
                .component(&new, (kind, def), &new_args, &ctx, path, 1)
                .is_none(),
            "a changed slot expression must not reuse retained children"
        );
    }

    #[test]
    fn unchanged_and_absent_retained_slot_bounds_keep_the_instance() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let (old, old_args) = call("root=Frame([Leaf(\"a\")])");
        let (new, new_args) = call("root=Frame([Leaf(\"a\")])");
        let (mut validator, _) = retained_validator(&lib, &options, &path, old, &old_args);
        let (kind, def) = lib.component("Frame").expect("Frame");
        let ctx = Context::at("root", Some(path.clone()));
        assert!(
            validator
                .component(&new, (kind, def), &new_args, &ctx, path, 1)
                .is_some(),
            "equal present and absent slot bounds must preserve the instance"
        );
    }

    // --- retained subtree reuse (SPEC §3.6) -------------------------------

    const NESTED: &str = "root=Frame([child])\nchild=Leaf(\"a\")\n";

    fn validate_source(lib: &Library, options: &Options, source: &str) -> Validation {
        validate(source.as_bytes(), lib, options)
    }

    fn root_id(lib: &Library, options: &Options, path: &NodePath) -> crate::ir::NodeId {
        let mut probe = Validator::new(&Program::default(), Vec::new(), lib, options);
        probe.ids.allocate(path)
    }

    fn reuse(
        lib: &Library,
        options: &Options,
        previous: &Validation,
        dirty: &BTreeSet<String>,
        path: &NodePath,
        depth: usize,
    ) -> (Option<crate::ir::NodeId>, usize, u64) {
        let count = std::cell::Cell::new(0usize);
        let mut validator = Validator::new(&Program::default(), Vec::new(), lib, options);
        validator.reuse = Some(Reuse {
            previous,
            dirty,
            count: &count,
        });
        let (id, _) = lib.component("Frame").expect("Frame");
        let ctx = Context::at("root", Some(path.clone()));
        let out = validator.reuse_subtree(id, &ctx, path, depth);
        (out, count.get(), validator.work.instances_examined)
    }

    #[test]
    fn clean_retained_subtree_is_reused_and_charged_exactly() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let previous = validate_source(&lib, &options, NESTED);
        assert_eq!(previous.ir.nodes.len(), 2);
        let (out, count, examined) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
        assert!(out.is_some(), "a clean sealed subtree must be reused");
        assert_eq!(count, 2, "both retained nodes must be counted as reused");
        assert_eq!(examined, 2, "each reused node is examined exactly once");
        let mut probe = Validator::new(&Program::default(), Vec::new(), &lib, &options);
        let probe_count = std::cell::Cell::new(0usize);
        let probe_dirty = BTreeSet::new();
        probe.reuse = Some(Reuse {
            previous: &previous,
            dirty: &probe_dirty,
            count: &probe_count,
        });
        let (id, _) = lib.component("Frame").expect("Frame");
        let ctx = Context::at("root", Some(path.clone()));
        assert!(probe.reuse_subtree(id, &ctx, &path, 1).is_some());
        assert_eq!(probe.allocated, 2, "reuse must charge the node budget");
    }

    #[test]
    fn reuse_is_disabled_for_templates_orphans_and_retained_tasks() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let previous = validate_source(&lib, &options, NESTED);
        let dirty = BTreeSet::new();
        for (orphan, template, tasks) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, false),
            (false, true, true),
        ] {
            let count = std::cell::Cell::new(0usize);
            let mut validator = Validator::new(&Program::default(), Vec::new(), &lib, &options);
            validator.checking_orphan = orphan;
            validator.checking_template = template;
            if tasks {
                validator.tasks = Some(crate::validate::tasks::Machine::default());
            }
            validator.reuse = Some(Reuse {
                previous: &previous,
                dirty: &dirty,
                count: &count,
            });
            let (id, _) = lib.component("Frame").expect("Frame");
            let ctx = Context::at("root", Some(path.clone()));
            assert!(
                validator.reuse_subtree(id, &ctx, &path, 1).is_none(),
                "orphan={orphan} template={template} tasks={tasks} must not reuse"
            );
            assert_eq!(count.get(), 0);
        }
    }

    #[test]
    fn reuse_requires_matching_root_identity_and_sealed_status() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let old_id = root_id(&lib, &options, &path);
        for edit in ["path", "kind", "origin", "status"] {
            let mut previous = validate_source(&lib, &options, NESTED);
            let node = previous.ir.nodes.get_mut(&old_id).expect("retained root");
            match edit {
                "path" => node.path = NodePath("elsewhere".into()),
                "kind" => node.kind = ComponentId(999),
                "origin" => node.origin = "elsewhere".into(),
                "status" => node.status = NodeStatus::Partial,
                _ => unreachable!(),
            }
            let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
            assert!(out.is_none(), "a mismatched {edit} must reject reuse");
        }
    }

    #[test]
    fn reuse_requires_a_clean_path_prefix_and_no_capacity_diagnostics() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        for (target, accepted) in [
            ("root", false),
            ("root/children:@child", false),
            ("other", true),
            ("rooted", true),
        ] {
            let mut previous = validate_source(&lib, &options, NESTED);
            let mut diagnostic = Diagnostic::new(Code::Nodes, Some("root".into()), None);
            diagnostic.path = Some(target.into());
            previous.diagnostics.extend_raw([diagnostic]);
            let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
            assert_eq!(
                out.is_some(),
                accepted,
                "diagnostic at {target} must {} reuse",
                if accepted { "allow" } else { "reject" }
            );
        }
        let mut previous = validate_source(&lib, &options, NESTED);
        let mut unrelated = Diagnostic::new(Code::DuplicateArg, Some("root".into()), None);
        unrelated.path = Some("root".into());
        previous.diagnostics.extend_raw([unrelated]);
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
        assert!(
            out.is_some(),
            "diagnostics outside the node/depth budgets must not block reuse"
        );
    }

    #[test]
    fn reuse_charges_depth_from_the_new_position_and_rejects_overflow() {
        let lib = library();
        let mut options = Options::default();
        let max = options.limits.max_ir_depth;
        let path = NodePath("root".into());
        let previous = validate_source(&lib, &options, NESTED);
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, max - 1);
        assert!(
            out.is_some(),
            "a subtree whose deepest child lands on the ceiling still fits"
        );
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, max);
        assert!(
            out.is_none(),
            "a child beyond the depth ceiling must be rejected"
        );
        options.limits.max_ir_depth = 1;
        let previous = validate_source(&lib, &options, NESTED);
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
        assert!(
            out.is_none(),
            "a nested child beyond the reduced depth ceiling must be rejected"
        );
    }

    #[test]
    fn reuse_rejects_dirty_or_unsealed_descendants() {
        let lib = library();
        let options = Options::default();
        let path = NodePath("root".into());
        let dirty: BTreeSet<String> = ["child".to_string()].into_iter().collect();
        let previous = validate_source(&lib, &options, NESTED);
        let (out, _, _) = reuse(&lib, &options, &previous, &dirty, &path, 1);
        assert!(out.is_none(), "a dirty descendant origin must reject reuse");

        let child = {
            let mut probe = Validator::new(&Program::default(), Vec::new(), &lib, &options);
            probe.ids.allocate(&NodePath("root/children:@child".into()))
        };
        for edit in ["status", "props", "plans"] {
            let mut previous = validate_source(&lib, &options, NESTED);
            let node = previous.ir.nodes.get_mut(&child).expect("retained child");
            match edit {
                "status" => node.status = NodeStatus::Partial,
                "props" => {
                    node.props.insert(
                        PropId(0),
                        Prop::Static(PropValue::Binding("violated".into())),
                    );
                }
                "plans" => node.slots.push(crate::ir::IrSlot {
                    name: "children".into(),
                    children: Default::default(),
                    plans: vec![crate::ir::SlotPlan {
                        base: node.path.clone(),
                        expression: ExprId {
                            hash: 0,
                            key: "unused".into(),
                        },
                        accepts: Vec::new(),
                    }]
                    .into(),
                }),
                _ => unreachable!(),
            }
            let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
            assert!(
                out.is_none(),
                "a descendant with changed {edit} must not reuse"
            );
        }
    }

    #[test]
    fn reuse_respects_the_new_node_budget_exactly() {
        let lib = library();
        let path = NodePath("root".into());
        let mut options = Options::default();
        let previous = validate_source(&lib, &options, NESTED);
        options.limits.max_nodes = 2;
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
        assert!(out.is_some(), "a subtree that exactly fits must reuse");
        options.limits.max_nodes = 1;
        let (out, _, _) = reuse(&lib, &options, &previous, &BTreeSet::new(), &path, 1);
        assert!(
            out.is_none(),
            "a subtree above the node ceiling must reject reuse"
        );
    }

    // --- live prop inference, templates and fallback (SPEC §§2.10, 7.1, 8.6) ---

    fn state(default: Value) -> crate::ir::StateDef {
        crate::ir::StateDef {
            ty: T::infer(&default),
            default,
        }
    }

    #[test]
    fn binding_props_widen_ambiguous_state_types() {
        let lib = library();
        let options = Options::default();
        let (_, def) = lib.component("Show").expect("Show");
        let (_, args) = call("root=Show(value:$s)");
        let expr = args[0].value.clone();
        let ctx = Context::at("root", None);

        let mut validator = Validator::new(&Program::default(), Vec::new(), &lib, &options);
        validator.ir.states.insert("s".into(), state(Value::Null));
        let prop = validator.prop(&expr, &def.args[0], &ctx, 0);
        assert!(
            matches!(prop, Some(Prop::Static(PropValue::Binding(ref name))) if name == "s"),
            "a null state must widen to the binding target: {prop:?}"
        );
        assert_eq!(validator.ir.states["s"].ty, T::Text);

        let mut validator = Validator::new(&Program::default(), Vec::new(), &lib, &options);
        validator
            .ir
            .states
            .insert("s".into(), state(Value::List(Vec::new().into())));
        let (_, list_def) = lib.component("ShowList").expect("ShowList");
        let prop = validator.prop(&expr, &list_def.args[0], &ctx, 0);
        assert!(
            matches!(prop, Some(Prop::Static(PropValue::Binding(ref name))) if name == "s"),
            "an empty-list state must widen to the list binding target: {prop:?}"
        );
        assert_eq!(validator.ir.states["s"].ty, T::list(T::text()));
    }

    #[test]
    fn template_checks_publish_state_widening_but_orphan_checks_do_not() {
        let lib = library();
        let options = Options::default();
        let (_, args) = call("root=Show(value:$s)");
        let body = args[0].value.clone();
        let body = Expr::Call {
            name: "Show".into(),
            args: vec![crate::syntax::ast::Argument {
                name: Some("value".into()),
                value: body,
            }],
        };
        for (orphan, expected) in [(false, T::Text), (true, T::Any)] {
            let mut validator = Validator::new(&Program::default(), Vec::new(), &lib, &options);
            validator.ir.states.insert("s".into(), state(Value::Null));
            validator.checking_orphan = orphan;
            validator.check_template(
                &body,
                &Context::at("root", None),
                NodePath("root".into()),
                1,
                None,
            );
            assert_eq!(
                validator.ir.states["s"].ty, expected,
                "orphan={orphan} state publication"
            );
        }
    }

    #[test]
    fn fallback_stays_static_when_descendants_only_have_empty_slots() {
        let lib = library();
        let mut options = Options::default();
        options.capabilities.remove("markdown");
        let result = validate(
            b"root=Rich(\"Title\", [Frame([Leaf(\"a\")])])\n",
            &lib,
            &options,
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == Code::CapFallback),
            "the capability fallback must run"
        );
        assert!(
            result.ir.fallbacks.is_empty(),
            "a descendant with no dynamic slot plans must keep the fallback static"
        );
        assert!(
            result.ir.nodes.values().any(|node| node.props.values().any(
                |prop| matches!(prop, Prop::Static(PropValue::Value(Value::Str(s)))
                    if s.text.as_ref().starts_with("Rich Title:"))
            )),
            "the fallback must render a static text value"
        );
    }

    #[test]
    fn display_uses_canonical_scalar_text() {
        assert_eq!(display(&Value::Float(1.0)), "1");
        assert_eq!(display(&Value::Float(1.5)), "1.5");
        assert_eq!(display(&Value::Int(-3)), "-3");
        assert_eq!(display(&Value::Bool(true)), "true");
    }

    #[test]
    fn numeric_types_are_compatible_in_both_directions() {
        assert!(type_compatible(&T::Number, &T::Int));
        assert!(type_compatible(&T::Number, &T::Float));
        assert!(type_compatible(&T::Any, &T::Text));
        assert!(!type_compatible(&T::Text, &T::Int));
    }

    // --- failed state inference (SPEC §2.10) ------------------------------

    fn infer(
        lib: &Library,
        source: &str,
        states: &[(&str, Value)],
    ) -> crate::SharedMap<String, crate::ir::StateDef> {
        let options = Options::default();
        let parsed = parse_batch(source.as_bytes(), options.limits.clone().syntax);
        let mut validator = Validator::new(&parsed.program, parsed.diagnostics, lib, &options);
        for statement in &parsed.program.statements {
            let kind = validator.classify(&statement.value);
            validator
                .table
                .insert(statement.name.clone(), statement.clone());
            validator.kinds.insert(statement.name.clone(), kind);
        }
        for (name, default) in states {
            validator
                .ir
                .states
                .insert((*name).into(), state(default.clone()));
        }
        validator.infer_binding_defaults();
        validator.ir.states.clone()
    }

    #[test]
    fn inference_follows_nodes_but_skips_values_and_shadowed_locals() {
        let lib = library();
        let states = infer(
            &lib,
            "root=Frame([child])\nchild=Show(value:$s)\n",
            &[("s", Value::Null)],
        );
        assert_eq!(states["s"].ty, T::Text);

        let states = infer(
            &lib,
            "root=Frame([alias])\nalias=Show(value:$s)\n",
            &[("s", Value::Null)],
        );
        assert_eq!(states["s"].ty, T::Text);

        let options = Options::default();
        let parsed = parse_batch(
            b"root=Frame([each([1], |child| Frame([child]))])\nchild=Show(value:$s)\n",
            options.limits.clone().syntax,
        );
        let mut validator = Validator::new(&parsed.program, parsed.diagnostics, &lib, &options);
        for statement in &parsed.program.statements {
            let kind = validator.classify(&statement.value);
            validator
                .table
                .insert(statement.name.clone(), statement.clone());
            validator.kinds.insert(statement.name.clone(), kind);
        }
        // A value statement named like a shadowed local must not be traversed.
        validator.kinds.insert("child".into(), Kind::Value);
        validator.ir.states.insert("s".into(), state(Value::Null));
        validator.infer_binding_defaults();
        assert_eq!(validator.ir.states["s"].ty, T::Any);
    }

    #[test]
    fn inference_reads_named_bindings_and_empty_list_targets() {
        let lib = library();
        let states = infer(&lib, "root=Show(value:$n)\n", &[("n", Value::Null)]);
        assert_eq!(states["n"].ty, T::Text);

        let states = infer(
            &lib,
            "root=ShowList(items:$xs)\n",
            &[("xs", Value::List(Vec::new().into()))],
        );
        assert_eq!(states["xs"].ty, T::list(T::text()));
    }

    #[test]
    fn inference_descends_into_each_if_and_coalesce_templates() {
        let lib = library();
        let states = infer(
            &lib,
            "root=Frame([each([1], |x| Show(value:$e))])\n",
            &[("e", Value::Null)],
        );
        assert_eq!(states["e"].ty, T::Text);

        let states = infer(
            &lib,
            "root=Frame([if(true, Show(value:$i), Leaf(\"x\"))])\n",
            &[("i", Value::Null)],
        );
        assert_eq!(states["i"].ty, T::Text);

        let states = infer(
            &lib,
            "root=Frame([coalesce(Show(value:$c), Leaf(\"y\"))])\n",
            &[("c", Value::Null)],
        );
        assert_eq!(states["c"].ty, T::Text);

        let states = infer(
            &lib,
            "root=Frame([if(Show(value:$v), Leaf(\"a\"), Leaf(\"b\"))])\n",
            &[("v", Value::Null)],
        );
        assert_eq!(
            states["v"].ty,
            T::Any,
            "the condition of if is not a binding position"
        );
    }
}
