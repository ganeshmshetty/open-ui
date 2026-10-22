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
    ) -> Vec<NodeId> {
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
            if let Some(code) = code {
                let mut at = ctx.clone();
                at.path = Some(path);
                at.stmt = ctx.budget_stmt.clone().unwrap_or_else(|| ctx.stmt.clone());
                self.limit(code, &at);
                return Vec::new();
            }
        }
        // Collapse aliases iteratively, retaining every ancestor for per-instantiation cycle detection.
        if let Expr::Name { name } = e {
            if ctx.locals.contains(name) {
                let mut at = ctx.clone();
                at.path = Some(path);
                self.emit(Code::SlotKind, &at);
                return Vec::new();
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
                self.pending_plans.push(SlotPlan {
                    base: path,
                    expression,
                    accepts: ctx.accepts.clone(),
                });
                return Vec::new();
            }
            let mut next = name.clone();
            let mut added = Vec::new();
            let mut at = ctx.clone();
            at.path = Some(path.clone());
            let result = loop {
                if !self.active.insert(next.clone()) {
                    self.emit(Code::Cycle, &at);
                    break Vec::new();
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
                    break Vec::new();
                };
                if at.budget_stmt.is_none() {
                    at.budget_stmt = Some(at.stmt.clone());
                }
                at.stmt = s.name;
                if s.kind == StatementKind::Query {
                    self.emit(Code::SlotKind, &at);
                    break Vec::new();
                }
                if self.kinds.get(&at.stmt) == Some(&Kind::Value) {
                    let null_list = matches!(&s.value, Expr::Null { .. })
                        || matches!(&s.value,Expr::List{items}if items.iter().all(|e|matches!(e,Expr::Null{..})));
                    if !null_list {
                        if self.cache.get(&at.stmt).is_some_and(|v| !v.valid) {
                            break Vec::new();
                        }
                        self.emit(Code::SlotKind, &at);
                        break Vec::new();
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
            Expr::Null { .. } => Vec::new(),
            Expr::List { items } => {
                if path.0 == "root" && place.is_none() && !self.checking_orphan {
                    self.emit(Code::SlotKind, &at);
                    return Vec::new();
                }
                if items.len() > self.limits.max_list_len {
                    self.limit(Code::ListLen, &at)
                }
                let mut children = Vec::new();
                for (i, e) in items.iter().take(self.limits.max_list_len).enumerate() {
                    let child = NodePath(format!("{}#{i}", path.0));
                    if matches!(e, Expr::List { .. }) {
                        let mut at = at.clone();
                        at.path = Some(child);
                        self.emit(Code::SlotKind, &at);
                        continue;
                    }
                    let before = self.limited.len();
                    children.extend(self.walk_expr(e, &at, child, depth, place));
                    if self.limited.len() > before
                        && self
                            .limited
                            .iter()
                            .any(|(s, c)| s == &at.stmt && matches!(c, Code::Nodes | Code::IrDepth))
                    {
                        break;
                    }
                }
                children
            }
            Expr::Call { name, .. } if ["each", "if", "coalesce"].contains(&name.as_str()) => {
                self.plan_nodes(e, &at, path, depth, place);
                Vec::new()
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
                    return Vec::new();
                };
                let context_allowed = def.allowed_in.is_empty()
                    || place.is_some_and(|p| def.allowed_in.iter().any(|v| v == p))
                    || (self.checking_orphan && place.is_none());
                let kind_allowed = ctx.accepts.is_empty() || ctx.accepts.contains(&def.name);
                if !context_allowed || !kind_allowed {
                    self.emit(Code::SlotKind, &at);
                    return Vec::new();
                }
                if let Some(node) = self.component(e, (id, &def), args, &at, path, depth) {
                    vec![node]
                } else {
                    Vec::new()
                }
            }
            _ => {
                self.emit(Code::SlotKind, &at);
                Vec::new()
            }
        }
    }
    fn bind_args(
        &mut self,
        def: &ComponentDef,
        args: &[Argument],
        ctx: &Context,
    ) -> BTreeMap<usize, Expr> {
        let mut bound = BTreeMap::new();
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
    fn component(
        &mut self,
        source: &Expr,
        definition: (ComponentId, &ComponentDef),
        args: &[Argument],
        ctx: &Context,
        path: NodePath,
        depth: usize,
    ) -> Option<NodeId> {
        let (id, def) = definition;
        let node_id = self.ids.allocate(&path);
        let mut local = ctx.clone();
        local.budget_stmt = None;
        if def.name == "Form" {
            local.form_scope = Some(node_id);
        }
        let ctx = &local;
        let mut bound = self.bind_args(def, args, ctx);
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
                    self.prop(e, arg, &at)
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
            if let std::collections::btree_map::Entry::Vacant(e) = bound.entry(i) {
                if let Some(v) = &arg.default {
                    if arg.required {
                        self.recovery(Code::MissingRequired, Recovery::Defaulted, ctx);
                    }
                    e.insert(v.to_expr());
                } else if arg.required {
                    self.emit(Code::MissingRequired, ctx);
                    failed = true;
                }
            }
        }
        if failed {
            return None;
        }
        let before = self.allocated;
        self.allocated += 1;
        for (i, arg) in def
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.ty.slot().is_some())
        {
            let Some(expr) = bound.get(&i) else {
                slots.push(IrSlot {
                    name: arg.name.clone(),
                    children: Vec::new(),
                    plans: Vec::new(),
                });
                continue;
            };
            let children = self.slot(
                expr,
                ctx,
                &path,
                arg,
                depth + 1,
                &format!("{}.{}", def.name, arg.name),
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
        self.ir.nodes.insert(node_id, node);
        Some(node_id)
    }
    fn literal_prop(&self, mut v: Value, arg: &ArgDef) -> Prop {
        if matches!(arg.ty, T::Float)
            && let Value::Int(i) = v
        {
            v = Value::Float(i as f64)
        }
        if let T::Named { name } = &arg.ty
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
            && let T::Named { name } = &arg.ty
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
    fn prop(&mut self, e: &Expr, arg: &ArgDef, ctx: &Context) -> Option<Prop> {
        if let T::Binding { of } = &arg.ty {
            if let Expr::State { name } = e {
                if let Some(state) = self.ir.states.get(name).cloned() {
                    let infer = state.default == Value::Null
                        || matches!(&state.default,Value::List(v)if v.is_empty());
                    if infer
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
                    return None;
                }
            }
            self.emit(Code::BindTarget, ctx);
            return None;
        }
        if matches!(arg.ty, T::Action) {
            let before = self.diagnostics.len();
            let (valid, normalized) = self.normalize_action(e, ctx);
            if valid {
                let steps = self.action_steps(&normalized);
                if steps.is_empty() {
                    return None;
                }
                let id = ActionPlanId(self.ir.actions.len());
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
                self.ir.actions.push(ActionPlan {
                    steps,
                    origin,
                    submit_scope,
                });
                return Some(Prop::Static(PropValue::Action(id)));
            }
            if self.diagnostics.len() == before {
                self.bad_prop(Code::SchemaType, arg, ctx);
            }
            return None;
        }
        let checked = self.check_value(e, ctx);
        if !checked.valid {
            return arg.default.clone().map(|v| self.literal_prop(v, arg));
        }
        let Some(mut v) = checked.value else {
            return Some(self.dynamic(e));
        };
        if matches!(arg.ty, T::Url) && matches!(v, Value::Str(_)) && !self.valid_url(&v) {
            self.emit(Code::TaintedUrl, ctx);
            return arg.default.clone().map(|v| self.literal_prop(v, arg));
        }
        if let Some(cap) = ctx.list_cap
            && let Value::List(items) = &mut v
            && items.len() > cap.min(self.limits.max_list_len)
        {
            std::sync::Arc::make_mut(items).truncate(cap.min(self.limits.max_list_len));
            self.limit(Code::ListLen, ctx);
        }
        // Range checks happen before token membership checks, and preserve the literal numeric kind.
        let range = if let T::Named { name } = &arg.ty {
            match self.lib.types().get(name) {
                Some(TypeDef::Token { min, max, .. }) => Some((*min as f64, *max as f64)),
                _ => None,
            }
        } else {
            arg.min.zip(arg.max)
        };
        if (matches!(arg.ty, T::Int)
            || matches!(&arg.ty,T::Named{name}if matches!(self.lib.types().get(name),Some(TypeDef::Token{..}))))
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
            let code = if let T::Named { name } = &arg.ty {
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
        place: &str,
    ) -> Vec<NodeId> {
        let name = &arg.name;
        let Some(accepts) = arg.ty.slot() else {
            return Vec::new();
        };
        let expressions = if let Expr::List { items } = e {
            items.as_slice()
        } else {
            std::slice::from_ref(e)
        };
        if expressions.len() > self.limits.max_list_len {
            self.limit(Code::ListLen, ctx)
        }
        let mut repeats: BTreeMap<String, usize> = BTreeMap::new();
        let mut children = Vec::new();
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
            at.accepts = if let Accepts::Only(v) = accepts {
                v.clone()
            } else {
                Vec::new()
            };
            let before = self.diagnostics.len();
            let pending = std::mem::take(&mut self.pending_plans);
            let ids = self.walk_expr(e, &at, path, depth, Some(place));
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
                    children.push(id)
                } else {
                    let mut c = at.clone();
                    c.path = self.ir.nodes.get(&id).map(|n| n.path.clone());
                    self.emit(Code::SlotKind, &c);
                    self.remove_subtree(id)
                }
            }
            plans.extend(new_plans.into_iter().map(|mut p| {
                if let Some(v) = only {
                    p.accepts = v.clone()
                }
                p
            }));
            if self.diagnostics[before..].iter().any(|d| {
                matches!(d.code, Code::Nodes | Code::IrDepth)
                    && d.stmt.as_deref() == Some(&ctx.stmt)
            }) {
                break;
            }
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
        let expression = self.ir.intern(e);
        self.pending_plans.push(SlotPlan {
            base: path,
            expression,
            accepts: Vec::new(),
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
                    .get(id.0)
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
                Some(Prop::Static(PropValue::Value(v))) => display(v),
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
            n.props
                .values()
                .any(|p| matches!(p, Prop::Dynamic(_) | Prop::Static(PropValue::Binding(_))))
                || n.slots.iter().any(|s| !s.plans.is_empty())
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
                    let expression = ExprId(self.ir.expressions.len());
                    self.ir.expressions.push(source.clone());
                    self.ir.fallbacks.insert(
                        expression.0,
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
                            if let T::Binding { of } = &a.ty
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
