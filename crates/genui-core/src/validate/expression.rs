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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ir::Role,
        r#lib::{ActionDef, FunctionDef},
        syntax::ast::NullOrigin,
        value::Value,
    };

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .group(crate::r#lib::GroupDef::new("G", 0, "`Leaf`."))
            .component(
                crate::r#lib::ComponentDef::new("Leaf", Role::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"ok\")"),
            )
            .component(
                crate::r#lib::ComponentDef::new("Box", Role::Group)
                    .group("G")
                    .doc("A box.")
                    .req("children", T::nodes())
                    .example("Box([])"),
            )
            .component(
                crate::r#lib::ComponentDef::new("Control", Role::Button)
                    .group("G")
                    .doc("A control.")
                    .opt_none("on_click", T::action())
                    .example("Control()"),
            )
            .action(ActionDef {
                name: "notify".into(),
                params: vec![T::text()],
                doc: "Notify.".into(),
            })
            .function(FunctionDef {
                name: "pick_two".into(),
                params: vec![T::int(), T::text()],
                result: T::text(),
                fuel: 8,
                implementation: None,
            })
            .build()
            .unwrap()
    }

    fn options() -> Options {
        Options::default()
    }

    fn validator<'a>(lib: &'a Library, opt: &'a Options) -> Validator<'a> {
        Validator::new(&Program::default(), Vec::new(), lib, opt)
    }

    fn state(v: &mut Validator<'_>, name: &str, ty: T, default: Value) {
        v.ir.states.insert(name.into(), StateDef { default, ty });
    }

    fn codes(v: &Validator<'_>) -> Vec<Code> {
        v.diagnostics.iter().map(|d| d.code).collect()
    }

    fn expr(text: &str) -> Expr {
        let source = format!("x={text}");
        parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default())
            .program
            .statements
            .into_iter()
            .next()
            .unwrap()
            .value
    }

    #[test]
    fn unary_index_and_lambda_value_forms_are_checked() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "n", T::int(), Value::from(1));
        state(&mut v, "b", T::bool(), Value::from(true));
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(std::sync::Arc::new(vec![Value::from(1)])),
        );
        state(
            &mut v,
            "m",
            T::map(T::Any),
            Value::Map(std::sync::Arc::new(vec![])),
        );

        let neg = v.check_value(&expr("-$n"), &Context::at("x", None));
        assert!(neg.valid);
        assert_eq!(neg.ty, T::Int);
        let not = v.check_value(&expr("!$b"), &Context::at("x", None));
        assert!(not.valid);
        assert_eq!(not.ty, T::Bool);
        let bad = v.check_value(&expr("!$n"), &Context::at("x", None));
        assert!(!bad.valid);

        let indexed = v.check_value(&expr("$l[0]"), &Context::at("x", None));
        assert!(indexed.valid);
        let keyed = v.check_value(&expr("$m[\"a\"]"), &Context::at("x", None));
        assert!(keyed.valid);
        let wrong = v.check_value(&expr("$l[\"a\"]"), &Context::at("x", None));
        assert!(!wrong.valid);

        let lambda = v.check_value(&expr("map($l, |x| x + 1)"), &Context::at("x", None));
        assert!(lambda.valid, "map lambda is legal");
        let misuse = v.check_value(
            &Expr::Lambda {
                params: vec!["x".into()],
                body: Box::new(expr("x + 1")),
            },
            &Context::at("x", None),
        );
        assert!(!misuse.valid);
        assert!(codes(&v).contains(&Code::LambdaMisuse));
    }

    #[test]
    fn list_limits_and_element_validity() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        v.limits.max_list_len = 2;
        state(&mut v, "n", T::int(), Value::from(1));

        let at_limit = v.check_value(&expr("[$n,1]"), &Context::at("x", None));
        assert!(at_limit.valid);
        assert!(!codes(&v).contains(&Code::ListLen));
        let over = v.check_value(&expr("[$n,1,2]"), &Context::at("x", None));
        assert!(over.valid);
        assert_eq!(codes(&v).iter().filter(|c| **c == Code::ListLen).count(), 1);

        let mut v = validator(&lib, &opt);
        state(&mut v, "n", T::int(), Value::from(1));
        let invalid = v.check_value(&expr("[$n,missing]"), &Context::at("x", None));
        assert!(!invalid.valid);
        let map = v.check_value(&expr("{a: 1, b: missing}"), &Context::at("x", None));
        assert!(!map.valid);
        let good = v.check_value(&expr("{a: $n}"), &Context::at("x", None));
        assert!(good.valid);
    }

    #[test]
    fn binary_operator_type_rules_are_exact() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "t", T::text(), Value::from("a"));
        state(&mut v, "u", T::text(), Value::from("b"));
        state(&mut v, "n", T::int(), Value::from(1));
        state(&mut v, "flag", T::bool(), Value::from(true));

        let text_add = v.check_value(&expr("$t + $u"), &Context::at("x", None));
        assert!(text_add.valid);
        assert_eq!(text_add.ty, T::Text);
        let number_add = v.check_value(&expr("$n + $n"), &Context::at("x", None));
        assert!(number_add.valid);
        assert_eq!(number_add.ty, T::Number);
        let mixed = v.check_value(&expr("$t + $n"), &Context::at("x", None));
        assert!(!mixed.valid, "mixed addition is an expr.type");
        let text_mul = v.check_value(&expr("$t * $u"), &Context::at("x", None));
        assert!(!text_mul.valid, "multiplication is numeric only");
        let eq = v.check_value(&expr("$t == $n"), &Context::at("x", None));
        assert!(eq.valid);
        assert_eq!(eq.ty, T::Bool);
        let cmp = v.check_value(&expr("$n < 2"), &Context::at("x", None));
        assert!(cmp.valid);
        assert_eq!(cmp.ty, T::Bool);
        let logic = v.check_value(&expr("$flag && $n"), &Context::at("x", None));
        assert!(!logic.valid, "logical operands are booleans");

        let invalid_left = v.check_value(&expr("missing + 1"), &Context::at("x", None));
        assert!(!invalid_left.valid);
        let mut v = validator(&lib, &opt);
        state(&mut v, "n", T::int(), Value::from(1));
        let invalid_right = v.check_value(&expr("$n + missing"), &Context::at("x", None));
        assert!(!invalid_right.valid);
    }

    #[test]
    fn member_and_index_follow_container_types() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(
            &mut v,
            "m",
            T::map(T::Any),
            Value::Map(std::sync::Arc::new(vec![])),
        );
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(std::sync::Arc::new(vec![])),
        );
        state(&mut v, "s", T::text(), Value::from("x"));
        state(&mut v, "k", T::int(), Value::from(0));

        assert!(v.check_value(&expr("$m.a"), &Context::at("x", None)).valid);
        assert!(!v.check_value(&expr("$s.a"), &Context::at("x", None)).valid);

        assert!(
            v.check_value(&expr("$l[$k]"), &Context::at("x", None))
                .valid
        );
        let mut unknown_index = validator(&lib, &opt);
        state(
            &mut unknown_index,
            "l",
            T::list(T::int()),
            Value::List(std::sync::Arc::new(vec![])),
        );
        state(
            &mut unknown_index,
            "k",
            T::Any,
            Value::Map(std::sync::Arc::new(vec![])),
        );
        assert!(
            unknown_index
                .check_value(&expr("$l[$k]"), &Context::at("x", None))
                .valid
        );
        let invalid_index = v.check_value(&expr("$l[missing]"), &Context::at("x", None));
        assert!(!invalid_index.valid);
        let invalid_base = v.check_value(&expr("missing[$k]"), &Context::at("x", None));
        assert!(!invalid_base.valid);
        let wrong_base = v.check_value(&expr("$s[$k]"), &Context::at("x", None));
        assert!(!wrong_base.valid);
        let mut map_key = validator(&lib, &opt);
        state(
            &mut map_key,
            "m",
            T::map(T::Any),
            Value::Map(std::sync::Arc::new(vec![])),
        );
        state(&mut map_key, "s", T::text(), Value::from("x"));
        assert!(
            map_key
                .check_value(&expr("$m[$s]"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("$s[$s]"), &Context::at("x", None))
                .valid
        );
    }

    #[test]
    fn recovered_null_is_invalid_as_a_value() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        let recovered = Expr::Null {
            origin: NullOrigin { recovered: true },
        };
        let checked = v.check_value(&recovered, &Context::at("x", None));
        assert!(!checked.valid);
        assert!(v.diagnostics.is_empty());
    }

    #[test]
    fn cap_value_truncates_nested_map_lists() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        v.limits.max_list_len = 2;
        let mut value = Value::Map(std::sync::Arc::new(vec![(
            "a".into(),
            Value::List(std::sync::Arc::new(vec![
                Value::from(1),
                Value::from(2),
                Value::from(3),
            ])),
        )]));
        v.cap_value(&mut value, &Context::at("x", None));
        assert_eq!(
            v.diagnostics
                .iter()
                .filter(|d| d.code == Code::ListLen)
                .count(),
            1
        );
        let Value::Map(entries) = &value else {
            panic!("map");
        };
        let Value::List(items) = &entries[0].1 else {
            panic!("list");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn normalize_action_keeps_good_steps_and_marks_invalid_ones() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "s", T::int(), Value::from(1));
        v.kinds.insert("a".into(), Kind::Action);
        v.cache.insert("a".into(), Checked::dynamic(T::Action));
        v.action_sources.insert("a".into(), expr("say(\"hi\")"));

        let (valid, source) = v.normalize_action(&expr("a"), &Context::at("x", None));
        assert!(valid);
        assert_eq!(source, expr("say(\"hi\")"));
        let (valid, source) = v.normalize_action(&expr("s"), &Context::at("x", None));
        assert!(!valid);
        assert_eq!(source, expr("s"));

        let recovered = Expr::Null {
            origin: NullOrigin { recovered: true },
        };
        assert_eq!(
            v.normalize_action(&recovered, &Context::at("x", None)),
            (true, Expr::null())
        );

        let before = v.diagnostics.len();
        let (valid, source) =
            v.normalize_action(&expr("[say(1),say(\"ok\")]"), &Context::at("x", None));
        assert!(valid);
        assert_eq!(v.diagnostics.len() - before, 1);
        assert!(
            matches!(source, Expr::List { items } if items.len() == 2 && items[1] == expr("say(\"ok\")"))
        );

        let (valid, source) = v.normalize_action(&expr("say(1)"), &Context::at("x", None));
        assert!(valid);
        assert_eq!(source, Expr::null());

        let mut v = validator(&lib, &opt);
        v.limits.max_list_len = 1;
        let before = v.diagnostics.len();
        v.normalize_action(&expr("[say(\"a\"),say(\"b\")]"), &Context::at("x", None));
        assert_eq!(
            v.diagnostics[before..]
                .iter()
                .filter(|d| d.code == Code::ListLen)
                .count(),
            1
        );
        let mut v = validator(&lib, &opt);
        v.limits.max_list_len = 1;
        v.normalize_action(&expr("[say(\"a\")]"), &Context::at("x", None));
        assert!(!v.diagnostics.iter().any(|d| d.code == Code::ListLen));
    }

    #[test]
    fn action_steps_cover_every_builtin_action() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "s", T::int(), Value::from(1));

        assert!(matches!(
            v.action_steps(&expr("set($s,1)")).as_slice(),
            [Step::Set { state, .. }] if state == "s"
        ));
        assert!(matches!(
            v.action_steps(&expr("restore($s)")).as_slice(),
            [Step::Restore { states }] if states == &vec!["s".to_string()]
        ));
        assert!(matches!(
            v.action_steps(&expr("refresh(q)")).as_slice(),
            [Step::Refresh { query }] if query == "q"
        ));
        assert!(matches!(
            v.action_steps(&expr("say(\"hi\")")).as_slice(),
            [Step::Say { .. }]
        ));
        assert!(matches!(
            v.action_steps(&expr("open(\"https://example.com\")"))
                .as_slice(),
            [Step::Open { .. }]
        ));
        assert!(matches!(
            v.action_steps(&expr("emit(\"notify\",\"x\")")).as_slice(),
            [Step::Emit { name, params }] if name == "notify" && params.len() == 1
        ));
        assert!(matches!(
            v.action_steps(&expr("notify(\"x\")")).as_slice(),
            [Step::Emit { name, .. }] if name == "notify"
        ));
    }

    #[test]
    fn check_action_validates_names_lists_and_custom_arity() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "s", T::int(), Value::from(1));
        v.kinds.insert("a".into(), Kind::Action);
        v.cache.insert("a".into(), Checked::dynamic(T::Action));
        v.table.insert(
            "known".into(),
            crate::syntax::ast::Statement {
                kind: crate::syntax::ast::StatementKind::Bind,
                name: "known".into(),
                value: expr("1"),
            },
        );
        v.kinds.insert("known".into(), Kind::Value);

        assert!(v.check_action(&expr("a"), &Context::at("x", None)));
        let before = v.diagnostics.len();
        assert!(!v.check_action(&expr("known"), &Context::at("x", None)));
        assert_eq!(
            v.diagnostics.len(),
            before,
            "known non-action name is resolved"
        );
        assert!(!v.check_action(&expr("missing"), &Context::at("x", None)));
        assert_eq!(v.diagnostics.last().map(|d| d.code), Some(Code::Unresolved));

        assert!(!v.check_action(&expr("[say(\"ok\"),say(1)]"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("notify(msg: \"x\")"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("notify(missing)"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("missing"), &Context::at("x", None)));
        let recovered = Expr::Null {
            origin: NullOrigin { recovered: true },
        };
        assert!(!v.check_action(&recovered, &Context::at("x", None)));
    }

    #[test]
    fn check_action_enforces_arity_and_emit_rules() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(&mut v, "s", T::int(), Value::from(1));

        assert!(v.check_action(&expr("restore($s)"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("restore()"), &Context::at("x", None)));
        assert!(v.check_action(&expr("emit(\"notify\",\"x\")"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("emit(\"notify\")"), &Context::at("x", None)));
        assert!(!v.check_action(&expr("emit(1,2)"), &Context::at("x", None)));

        let before = v.diagnostics.len();
        assert!(!v.check_action(&expr("emit(\"nope\",1)"), &Context::at("x", None)));
        let unknown = v.diagnostics[before..]
            .iter()
            .filter(|d| d.code == Code::UnknownFn)
            .count();
        assert_eq!(unknown, 1);
    }

    #[test]
    fn calls_that_name_slots_or_actions_are_not_values() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(Default::default()),
        );

        for source in [
            "Leaf(\"x\")",
            "Widget()",
            "each($l)",
            "say(\"hi\")",
            "notify(\"x\")",
        ] {
            let before = v.diagnostics.len();
            let checked = v.check_value(&expr(source), &Context::at("x", None));
            assert!(!checked.valid, "{source} is a slot, not a value");
            assert_eq!(
                codes(&v)[before..],
                [Code::SlotKind],
                "{source} reports exactly one slot-kind diagnostic"
            );
        }
    }

    #[test]
    fn library_functions_check_arity_names_and_argument_types() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);

        assert!(
            v.check_value(&expr("pick_two(1,\"x\")"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("pick_two(1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("pick_two(1,\"x\",2)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("pick_two(key: 1,\"x\")"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("pick_two(missing,\"x\")"), &Context::at("x", None))
                .valid,
            "an unresolved argument makes the call invalid"
        );
        assert!(
            !v.check_value(&expr("pick_two(1,2)"), &Context::at("x", None))
                .valid,
            "literal argument type mismatches are rejected"
        );
        assert!(
            !v.check_value(&expr("pick_two(1,missing)"), &Context::at("x", None))
                .valid
        );
    }

    #[test]
    fn builtin_arity_and_sort_named_arguments_are_exact() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(Default::default()),
        );
        state(&mut v, "flag", T::bool(), Value::from(true));

        assert!(
            v.check_value(&expr("if($flag,1,2)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("if($flag,1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("if(1,2,3)"), &Context::at("x", None))
                .valid,
            "if requires a boolean condition"
        );
        assert!(
            !v.check_value(&expr("len(1)"), &Context::at("x", None))
                .valid,
            "len requires a list, text or map"
        );
        assert!(
            v.check_value(&expr("coalesce(1,2)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("coalesce(1)"), &Context::at("x", None))
                .valid
        );

        assert!(
            v.check_value(&expr("sort_by($l,|x| x)"), &Context::at("x", None))
                .valid
        );
        assert!(
            v.check_value(
                &expr("sort_by($l,|x| x,desc: true)"),
                &Context::at("x", None)
            )
            .valid
        );
        assert!(
            !v.check_value(
                &expr("sort_by($l,|x| x,key: true)"),
                &Context::at("x", None)
            )
            .valid,
            "sort_by only accepts the desc named argument"
        );
        assert!(
            !v.check_value(&expr("sort_by($l,|x| x,desc: 1)"), &Context::at("x", None))
                .valid,
            "named arguments must be boolean"
        );
        assert!(
            !v.check_value(&expr("len($l,desc: true)"), &Context::at("x", None))
                .valid,
            "only sort_by accepts named arguments"
        );
        assert!(
            !v.check_value(
                &expr("sort_by($l,|x| x,desc: missing)"),
                &Context::at("x", None)
            )
            .valid
        );
    }

    #[test]
    fn builtin_argument_types_and_lambda_placement_are_checked() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(Default::default()),
        );
        state(&mut v, "t", T::text(), Value::from("a"));
        state(&mut v, "u", T::url(), Value::from("https://example.com"));
        state(&mut v, "n", T::int(), Value::from(1));

        let url_sum = v.check_value(&expr("$t + $u"), &Context::at("x", None));
        assert!(url_sum.valid);
        assert_eq!(url_sum.ty, T::Number, "only text plus text stays text");

        assert!(
            !v.check_value(&expr("first(1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("last(1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("min(1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("round(\"x\")"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("fixed(\"x\",1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("contains(1,\"x\")"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("join(1,\",\")"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("fmt(1)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("sum($l,1)"), &Context::at("x", None))
                .valid,
            "a numeric second sum argument must be textual"
        );
        assert!(
            !v.check_value(&expr("join([1],\",\")"), &Context::at("x", None))
                .valid,
            "join rejects non-string literal elements"
        );

        assert!(
            !v.check_value(&expr("filter($l,|x| x + 1)"), &Context::at("x", None))
                .valid,
            "filter needs a boolean predicate"
        );
        assert!(
            v.check_value(&expr("filter($l,|x| true)"), &Context::at("x", None))
                .valid
        );
        assert!(
            !v.check_value(&expr("map($l,|x,y| x)"), &Context::at("x", None))
                .valid,
            "lambdas take exactly one parameter"
        );
        assert!(
            !v.check_value(&expr("map(|x| x,$l)"), &Context::at("x", None))
                .valid,
            "the lambda must be the second positional argument"
        );
        assert!(
            !v.check_value(&expr("map($l,|x| missing)"), &Context::at("x", None))
                .valid,
            "an invalid lambda body invalidates the call"
        );
    }

    #[test]
    fn builtin_results_have_exact_types() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);
        state(
            &mut v,
            "l",
            T::list(T::int()),
            Value::List(Default::default()),
        );

        let cases = [
            ("contains(\"a\",\"b\")", T::Bool),
            ("len($l)", T::Int),
            ("len(\"abc\")", T::Int),
            ("fmt(\"{}\",1)", T::Text),
            ("text(1)", T::Text),
            ("upper(\"a\")", T::Text),
            ("lower(\"A\")", T::Text),
            ("join($l,\",\")", T::Text),
            ("fixed(1,2)", T::Text),
            ("map($l,|x| x + 1)", T::list(T::Number)),
            ("filter($l,|x| true)", T::list(T::int())),
            ("sort_by($l,|x| x)", T::list(T::int())),
            ("sum($l)", T::Number),
            ("round(1)", T::Number),
        ];
        for (source, expected) in cases {
            let checked = v.check_value(&expr(source), &Context::at("x", None));
            assert!(checked.valid, "{source} is valid");
            assert_eq!(checked.ty, expected, "{source} returns {expected:?}");
        }
    }

    #[test]
    fn action_lists_and_unknown_component_names_are_slot_kinds() {
        let lib = library();
        let opt = options();
        let mut v = validator(&lib, &opt);

        assert!(v.check_action(&expr("[say(\"ok\")]"), &Context::at("x", None)));
        let before = v.diagnostics.len();
        assert!(!v.check_action(&expr("Widget()"), &Context::at("x", None)));
        assert_eq!(codes(&v)[before..], [Code::SlotKind]);

        let before = v.diagnostics.len();
        assert!(!v.check_action(&expr("restore(1)"), &Context::at("x", None)));
        assert_eq!(codes(&v)[before..], [Code::ExprType]);
    }

    #[test]
    fn fmt_placeholder_counting_handles_braces() {
        let lib = library();
        let opt = options();
        let r = validate(b"root=Leaf(fmt(\"{}\",1))", &lib, &opt);
        assert!(!r.diagnostics.iter().any(|d| d.code == Code::FmtArgs));
        let r = validate(b"root=Leaf(fmt(\"a}\",1))", &lib, &opt);
        assert!(r.diagnostics.iter().any(|d| d.code == Code::FmtArgs));
        let r = validate(b"root=Leaf(fmt(\"{{}}\",1))", &lib, &opt);
        assert!(r.diagnostics.iter().any(|d| d.code == Code::FmtArgs));
    }
}
