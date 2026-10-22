use super::*;
use crate::{
    r#lib::{ACTIONS, BUILTINS},
    syntax::ast::{BinaryOp, UnaryOp},
};

fn unknown(t: &T) -> bool {
    matches!(t, T::Any | T::Data { .. })
}
fn numeric(t: &T) -> bool {
    unknown(t) || matches!(t, T::Int | T::Float | T::Number)
}
fn textual(t: &T) -> bool {
    unknown(t) || matches!(t, T::Text | T::Url)
}
fn boolean(t: &T) -> bool {
    unknown(t) || matches!(t, T::Bool)
}
fn list(t: &T) -> bool {
    unknown(t) || matches!(t, T::List { .. })
}
fn scalar(t: &T) -> bool {
    numeric(t) || textual(t) || matches!(t, T::Scalar | T::Bool)
}
impl Validator<'_> {
    pub(super) fn check_value(&mut self, e: &Expr, ctx: &Context) -> Checked {
        if let Some(mut v) = Value::constant(e) {
            self.cap_value(&mut v, ctx);
            return Checked::literal(v);
        }
        match e {
            Expr::Null { origin } if origin.recovered => Checked::invalid(),
            Expr::Name { name } => {
                if ctx.locals.contains(name) {
                    return Checked::dynamic(T::Any);
                }
                if self.kinds.get(name) == Some(&Kind::Node)
                    || self.kinds.get(name) == Some(&Kind::Action)
                {
                    self.emit(Code::SlotKind, ctx);
                } else if let Some(v) = self.cache.get(name) {
                    return v.clone();
                } else if self.table.contains_key(name) {
                    return Checked::dynamic(T::Any);
                } else {
                    self.emit(Code::Unresolved, ctx);
                }
                Checked::invalid()
            }
            Expr::State { name } => match self.ir.states.get(name) {
                Some(s) => Checked::dynamic(s.ty.clone()),
                None => {
                    self.emit(Code::Undeclared, ctx);
                    Checked::invalid()
                }
            },
            // Fully literal containers returned above. References make these expression
            // plans: do not evaluate data or recursively grow cached types through aliases.
            Expr::List { items } => {
                let mut valid = true;
                if items.len() > self.limits.max_list_len {
                    self.limit(Code::ListLen, ctx)
                }
                for e in items.iter().take(self.limits.max_list_len) {
                    valid &= self.check_value(e, ctx).valid;
                }
                if valid {
                    Checked::dynamic(T::list(T::Any))
                } else {
                    Checked::invalid()
                }
            }
            Expr::Map { entries } => {
                let mut valid = true;
                for e in entries {
                    valid &= self.check_value(&e.value, ctx).valid;
                }
                if valid {
                    Checked::dynamic(T::map(T::Any))
                } else {
                    Checked::invalid()
                }
            }
            Expr::Unary { op, expr } => {
                let c = self.check_value(expr, ctx);
                if !c.valid {
                    return Checked::invalid();
                }
                if match op {
                    UnaryOp::Neg => numeric(&c.ty),
                    UnaryOp::Not => boolean(&c.ty),
                } {
                    Checked::dynamic(if *op == UnaryOp::Not { T::Bool } else { c.ty })
                } else {
                    self.emit(Code::ExprType, ctx);
                    Checked::invalid()
                }
            }
            Expr::Binary { op, left, right } => {
                let l = self.check_value(left, ctx);
                let r = self.check_value(right, ctx);
                if !l.valid || !r.valid {
                    return Checked::invalid();
                }
                let both_numbers = numeric(&l.ty) && numeric(&r.ty);
                let both_text = textual(&l.ty) && textual(&r.ty);
                let ok = match op {
                    BinaryOp::Add => both_numbers || both_text,
                    BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => both_numbers,
                    BinaryOp::And | BinaryOp::Or => boolean(&l.ty) && boolean(&r.ty),
                    BinaryOp::Eq | BinaryOp::Ne => true,
                    _ => both_numbers || both_text,
                };
                if !ok {
                    self.emit(Code::ExprType, ctx);
                    return Checked::invalid();
                }
                let ty = match op {
                    BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge => T::Bool,
                    BinaryOp::Add if matches!(l.ty, T::Text) && matches!(r.ty, T::Text) => T::Text,
                    _ => T::Number,
                };
                Checked::dynamic(ty)
            }
            Expr::Member { base, .. } => {
                let c = self.check_value(base, ctx);
                if !c.valid {
                    return Checked::invalid();
                }
                if unknown(&c.ty) || matches!(c.ty, T::Map { .. }) || c.value == Some(Value::Null) {
                    Checked::dynamic(T::Any)
                } else {
                    self.emit(Code::ExprType, ctx);
                    Checked::invalid()
                }
            }
            Expr::Index { base, index } => {
                let b = self.check_value(base, ctx);
                let i = self.check_value(index, ctx);
                if !b.valid || !i.valid {
                    return Checked::invalid();
                }
                if unknown(&b.ty)
                    || matches!(b.ty, T::List { .. }) && (unknown(&i.ty) || matches!(i.ty, T::Int))
                    || matches!(b.ty, T::Map { .. }) && textual(&i.ty)
                {
                    Checked::dynamic(T::Any)
                } else {
                    self.emit(Code::ExprType, ctx);
                    Checked::invalid()
                }
            }
            Expr::Lambda { .. } => {
                self.emit(Code::LambdaMisuse, ctx);
                Checked::invalid()
            }
            Expr::Call { name, args } => self.check_call(name, args, ctx),
            _ => Checked::invalid(), // All literal variants were handled by Value::constant.
        }
    }
    pub(super) fn cap_value(&mut self, v: &mut Value, ctx: &Context) {
        let cap = ctx
            .list_cap
            .unwrap_or(self.limits.max_list_len)
            .min(self.limits.max_list_len);
        let mut nested = ctx.clone();
        nested.list_cap = None;
        match v {
            Value::List(items) => {
                if items.len() > cap {
                    self.limit(Code::ListLen, ctx);
                    std::sync::Arc::make_mut(items).truncate(cap)
                }
                for v in std::sync::Arc::make_mut(items) {
                    self.cap_value(v, &nested)
                }
            }
            Value::Map(entries) => {
                for (_, v) in std::sync::Arc::make_mut(entries) {
                    self.cap_value(v, &nested)
                }
            }
            _ => {}
        }
    }
    fn check_call(&mut self, name: &str, args: &[Argument], ctx: &Context) -> Checked {
        if name == "ref" {
            let id = if args.len() == 1 && args[0].name.is_none() {
                if let Expr::String { value } = &args[0].value {
                    Some(value)
                } else {
                    None
                }
            } else {
                None
            };
            let Some(id) = id.filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
            }) else {
                self.emit(Code::RefInvalid, ctx);
                return Checked::invalid();
            };
            if !self.options.refs.contains(id) {
                self.emit(Code::RefUnknown, ctx);
                return Checked::invalid();
            }
            return Checked::dynamic(T::data());
        }
        if self.lib.component(name).is_some()
            || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            || name == "each"
            || ACTIONS.contains(&name)
            || self.lib.action(name).is_some()
        {
            self.emit(Code::SlotKind, ctx);
            return Checked::invalid();
        }
        if let Some(f) = self.lib.function(name).cloned() {
            if f.params.len() != args.len() || args.iter().any(|a| a.name.is_some()) {
                self.emit(Code::ExprType, ctx);
                return Checked::invalid();
            }
            let mut valid = true;
            for (a, t) in args.iter().zip(&f.params) {
                let c = self.check_value(&a.value, ctx);
                valid &= c.valid;
                if c.valid && c.value.as_ref().is_some_and(|v| !self.lib.accepts(t, v)) {
                    self.emit(Code::ExprType, ctx);
                    valid = false
                }
            }
            return if valid {
                Checked::dynamic(f.result)
            } else {
                Checked::invalid()
            };
        }
        if !BUILTINS.contains(&name) {
            self.emit(Code::UnknownFn, ctx);
            return Checked::invalid();
        }
        let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
        let named: Vec<_> = args.iter().filter(|a| a.name.is_some()).collect();
        let arity = match name {
            "if" => 3..=3,
            "coalesce" => 2..=usize::MAX,
            "fmt" => 1..=usize::MAX,
            "sum" => 1..=2,
            "fixed" | "contains" | "join" | "map" | "filter" | "sort_by" => 2..=2,
            _ => 1..=1,
        };
        let named_ok = if name == "sort_by" {
            named.len() <= 1 && named.iter().all(|a| a.name.as_deref() == Some("desc"))
        } else {
            named.is_empty()
        };
        if !arity.contains(&positional.len()) || !named_ok {
            self.emit(Code::ExprType, ctx);
            return Checked::invalid();
        }
        let mut values = Vec::new();
        let mut valid = true;
        for (index, a) in positional.iter().enumerate() {
            if let Expr::Lambda { params, body } = &a.value {
                if index == 1 && ["map", "filter", "sort_by"].contains(&name) && params.len() == 1 {
                    let mut local = ctx.clone();
                    local.locals.extend(params.iter().cloned());
                    let c = self.check_value(body, &local);
                    valid &= c.valid;
                    if name == "filter" && !boolean(&c.ty) {
                        self.emit(Code::ExprType, ctx);
                        valid = false
                    }
                    values.push(Checked::dynamic(c.ty));
                } else {
                    self.emit(Code::ExprType, ctx);
                    valid = false;
                    values.push(Checked::invalid());
                }
            } else {
                let c = self.check_value(&a.value, ctx);
                valid &= c.valid;
                values.push(c)
            }
        }
        for a in named {
            let c = self.check_value(&a.value, ctx);
            valid &= c.valid;
            if c.valid && !boolean(&c.ty) {
                self.emit(Code::ExprType, ctx);
                valid = false
            }
        }
        if !valid {
            return Checked::invalid();
        }
        let first = &values[0];
        let second = values.get(1);
        let types_ok = match name {
            "if" => boolean(&first.ty),
            "coalesce" => true,
            "len" => list(&first.ty) || textual(&first.ty) || matches!(first.ty, T::Map { .. }),
            "first" | "last" | "map" | "filter" | "sort_by" => list(&first.ty),
            "sum" | "avg" | "min" | "max" => {
                list(&first.ty) && second.is_none_or(|v| textual(&v.ty))
            }
            "round" | "floor" | "ceil" | "abs" => numeric(&first.ty),
            "fixed" => {
                numeric(&first.ty)
                    && second.is_some_and(|v| unknown(&v.ty) || matches!(v.ty, T::Int))
            }
            "text" => scalar(&first.ty),
            "upper" | "lower" => textual(&first.ty),
            "contains" => textual(&first.ty) && second.is_some_and(|v| textual(&v.ty)),
            "join" => list(&first.ty) && second.is_some_and(|v| textual(&v.ty)),
            "fmt" => textual(&first.ty),
            _ => true,
        };
        if !types_ok {
            self.emit(Code::ExprType, ctx);
            return Checked::invalid();
        }
        if ["sum", "avg", "min", "max"].contains(&name)
            && second.is_none()
            && let Some(Value::List(items)) = &first.value
            && items.iter().any(|v| v.number().is_none())
        {
            self.emit(Code::ExprType, ctx);
            return Checked::invalid();
        }
        if name == "join"
            && let Some(Value::List(items)) = &first.value
            && items.iter().any(|v| v.string().is_none())
        {
            self.emit(Code::ExprType, ctx);
            return Checked::invalid();
        }
        if name == "fmt"
            && let Some(template) = first.value.as_ref().and_then(Value::string)
            && placeholders(template) != positional.len() - 1
        {
            self.emit(Code::FmtArgs, ctx);
        }
        let ty = match name {
            "contains" => T::Bool,
            "len" => T::Int,
            "fmt" | "text" | "upper" | "lower" | "join" | "fixed" => T::Text,
            "map" => T::list(second.map_or(T::Any, |v| v.ty.clone())),
            "filter" | "sort_by" => first.ty.clone(),
            "sum" | "avg" | "min" | "max" | "round" | "floor" | "ceil" | "abs" => T::Number,
            _ => T::Any,
        };
        Checked::dynamic(ty)
    }
    pub(super) fn normalize_action(&mut self, e: &Expr, ctx: &Context) -> (bool, Expr) {
        match e {
            Expr::Name { name } if ctx.locals.contains(name) => (false, e.clone()),
            Expr::Name { name } if self.kinds.get(name) == Some(&Kind::Action) => (
                true,
                self.action_sources
                    .get(name)
                    .cloned()
                    .unwrap_or_else(Expr::null),
            ),
            Expr::List { items } => {
                let mut normalized = Vec::new();
                for e in items.iter().take(self.limits.max_list_len) {
                    let before = self.diagnostics.len();
                    let (valid, source) = self.normalize_action(e, ctx);
                    if valid {
                        normalized.push(source)
                    } else {
                        if self.diagnostics.len() == before {
                            self.emit(Code::ExprType, ctx);
                        }
                        normalized.push(Expr::null());
                    }
                }
                if items.len() > self.limits.max_list_len {
                    self.limit(Code::ListLen, ctx)
                }
                (true, Expr::List { items: normalized })
            }
            Expr::Null { origin } if origin.recovered => (true, Expr::null()),
            Expr::Call { .. } => {
                let before = self.diagnostics.len();
                let valid = self.check_action(e, ctx);
                if valid {
                    (true, e.clone())
                } else if self.diagnostics.len() > before {
                    (true, Expr::null())
                } else {
                    (false, e.clone())
                }
            }
            _ => (false, e.clone()),
        }
    }
    pub(super) fn action_steps(&mut self, e: &Expr) -> Vec<Step> {
        if let Expr::List { items } = e {
            return items.iter().flat_map(|e| self.action_steps(e)).collect();
        }
        let Expr::Call { name, args } = e else {
            return Vec::new();
        };
        let arg = |i: usize| args.get(i).map(|a| &a.value);
        let step = match name.as_str() {
            "set" => {
                if let (Some(Expr::State { name }), Some(value)) = (arg(0), arg(1)) {
                    Some(Step::Set {
                        state: name.clone(),
                        value: self.ir.intern(value),
                    })
                } else {
                    None
                }
            }
            "restore" => Some(Step::Restore {
                states: args
                    .iter()
                    .filter_map(|a| {
                        if let Expr::State { name } = &a.value {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
            }),
            "refresh" => {
                if let Some(Expr::Name { name }) = arg(0) {
                    Some(Step::Refresh {
                        query: name.clone(),
                    })
                } else {
                    None
                }
            }
            "say" => arg(0).map(|text| Step::Say {
                text: self.ir.intern(text),
                ctx: arg(1).map(|e| self.ir.intern(e)),
            }),
            "open" => arg(0).map(|url| Step::Open {
                url: self.ir.intern(url),
            }),
            "emit" => {
                if let Some(Expr::String { value }) = arg(0) {
                    Some(Step::Emit {
                        name: value.clone(),
                        params: args
                            .iter()
                            .skip(1)
                            .map(|a| self.ir.intern(&a.value))
                            .collect(),
                    })
                } else {
                    None
                }
            }
            _ => self.lib.action(name).map(|_| Step::Emit {
                name: name.clone(),
                params: args.iter().map(|a| self.ir.intern(&a.value)).collect(),
            }),
        };
        step.into_iter().collect()
    }
    pub(super) fn check_action(&mut self, e: &Expr, ctx: &Context) -> bool {
        match e {
            Expr::Null { origin } if origin.recovered => false,
            Expr::Name { name } => {
                if self.kinds.get(name) == Some(&Kind::Action) {
                    return self.cache.get(name).is_none_or(|c| c.valid);
                }
                if !self.table.contains_key(name) {
                    self.emit(Code::Unresolved, ctx);
                }
                false
            }
            Expr::List { items } => {
                let mut valid = true;
                for e in items {
                    valid &= self.check_action(e, ctx)
                }
                valid
            }
            Expr::Null { .. } => true,
            Expr::Call { name, args } => {
                if let Some(def) = self.lib.action(name).cloned() {
                    if args.len() != def.params.len() || args.iter().any(|a| a.name.is_some()) {
                        self.emit(Code::ExprType, ctx);
                        return false;
                    }
                    let mut valid = true;
                    for (a, t) in args.iter().zip(&def.params) {
                        let c = self.check_value(&a.value, ctx);
                        valid &= c.valid;
                        if c.valid && c.value.as_ref().is_some_and(|v| !self.lib.accepts(t, v)) {
                            self.emit(Code::ExprType, ctx);
                            valid = false
                        }
                    }
                    return valid;
                }
                if !ACTIONS.contains(&name.as_str()) {
                    if BUILTINS.contains(&name.as_str()) {
                        self.emit(Code::ExprType, ctx);
                        return false;
                    }
                    if self.classify(e) == Kind::Node {
                        self.emit(Code::SlotKind, ctx);
                    } else {
                        self.emit(Code::UnknownFn, ctx);
                    }
                    return false;
                }
                let n = args.len();
                let arity_ok = match name.as_str() {
                    "set" => n == 2,
                    "restore" => n >= 1,
                    "refresh" | "open" => n == 1,
                    "say" => n == 1 || n == 2,
                    "emit" => n == 2,
                    _ => false,
                };
                if !arity_ok || args.iter().any(|a| a.name.is_some()) {
                    self.emit(Code::ExprType, ctx);
                    return false;
                }
                if name == "emit"
                    && !matches!(args.first().map(|a| &a.value), Some(Expr::String { .. }))
                {
                    self.emit(Code::ExprType, ctx);
                    return false;
                }
                let mut valid = true;
                for (i, a) in args.iter().enumerate() {
                    if (name == "set" && i == 0) || name == "restore" {
                        if let Expr::State { name } = &a.value {
                            if !self.ir.states.contains_key(name) {
                                self.emit(Code::Undeclared, ctx);
                                valid = false
                            }
                        } else {
                            self.emit(Code::ExprType, ctx);
                            valid = false
                        }
                        continue;
                    }
                    if name == "refresh" {
                        if !matches!(&a.value,Expr::Name{name}if !ctx.locals.contains(name)&&self.ir.queries.contains_key(name))
                        {
                            self.emit(Code::ExprType, ctx);
                            valid = false
                        }
                        continue;
                    }
                    let c = self.check_value(&a.value, ctx);
                    valid &= c.valid;
                    if !c.valid {
                        continue;
                    }
                    if name == "emit"
                        && i == 1
                        && let Some(Expr::String { value: event }) = args.first().map(|a| &a.value)
                        && let Some(def) = self.lib.action(event)
                        && (def.params.len() != 1
                            || c.value
                                .as_ref()
                                .is_some_and(|v| !self.lib.accepts(&def.params[0], v)))
                    {
                        self.emit(Code::ExprType, ctx);
                        valid = false;
                    }
                    if (name == "open" || (i == 0 && ["say", "emit"].contains(&name.as_str())))
                        && !textual(&c.ty)
                    {
                        self.emit(Code::ExprType, ctx);
                        valid = false
                    }
                    if name == "say" && i == 1 && !unknown(&c.ty) && !matches!(c.ty, T::Map { .. })
                    {
                        self.emit(Code::ExprType, ctx);
                        valid = false
                    }
                    if name == "open" && c.value.as_ref().is_some_and(|v| !self.valid_url(v)) {
                        self.emit(Code::TaintedUrl, ctx);
                        valid = false
                    }
                    if name == "emit"
                        && i == 0
                        && let Some(event) = c.value.as_ref().and_then(Value::string)
                        && self.lib.action(event).is_none()
                    {
                        self.emit(Code::UnknownFn, ctx);
                        valid = false
                    }
                }
                valid
            }
            _ => false,
        }
    }
}
fn placeholders(s: &str) -> usize {
    let mut chars = s.chars().peekable();
    let mut count = 0;
    while let Some(c) = chars.next() {
        if matches!(c, '{' | '}') && chars.peek() == Some(&c) {
            chars.next();
        } else if c == '{' && chars.peek() == Some(&'}') {
            count += 1;
            chars.next();
        }
    }
    count
}
