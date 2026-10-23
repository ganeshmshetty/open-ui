//! Static query checks, shared by batch and retained maintenance. No requests.
use super::*;
impl Validator<'_> {
    pub(super) fn query_header(&mut self, source: &Statement) {
        let ctx = Context::at(&source.name, None);
        let Expr::Call { name: call, args } = &source.value else {
            self.emit(Code::ExprType, &ctx);
            return;
        };
        let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
        let default = args.iter().find(|a| a.name.as_deref() == Some("default"));
        let id = positional.first().and_then(|a| {
            if let Expr::String { value } = &a.value {
                Some(value.clone())
            } else {
                None
            }
        });
        if call != "tool"
            || positional.len() != 2
            || default.is_none()
            || args.len() != 3
            || id.is_none()
        {
            self.emit(Code::ExprType, &ctx);
            return;
        }
        if let (Some(tool), Some(default), Some(arg)) = (id, default, positional.get(1)) {
            let allowed = self.options.tools.contains(&tool);
            if !allowed {
                self.emit(Code::UnknownTool, &ctx);
            }
            self.ir.queries.insert(
                source.name.clone(),
                QueryDef {
                    tool,
                    args: arg.value.clone(),
                    default: default.value.clone(),
                    allowed,
                },
            );
        }
    }
    pub(super) fn query_checked(&mut self, name: &str, ctx: &Context) -> Checked {
        if let Some(query) = self.ir.queries.get(&name.to_owned()).cloned() {
            self.check_value(&query.args, ctx);
            let default = self.check_value(&query.default, ctx);
            Checked::dynamic(default.ty)
        } else {
            Checked::invalid()
        }
    }
    pub(super) fn extend_reachable(
        &mut self,
        sources: &[Statement],
        diagnostics: &mut crate::Diagnostics,
    ) {
        let mut queue = Vec::new();
        for source in sources {
            if self.reachable.contains(&source.name) {
                let mut dependencies = BTreeSet::new();
                names(&source.value, &mut dependencies);
                queue.extend(dependencies);
            }
        }
        while let Some(name) = queue.pop() {
            self.work.statements_checked += 1;
            if self.reachable.contains(&name) {
                continue;
            }
            self.reachable.insert(name.clone());
            if let Some(old) = self.orphan_diagnostics.remove(&name) {
                for diagnostic in old {
                    diagnostics.remove_raw(&diagnostic);
                    self.work.diagnostics_examined += 1;
                }
            }
            if let Some(source) = self
                .table
                .get(&name)
                .or_else(|| self.query_sources.get(&name))
            {
                let mut dependencies = BTreeSet::new();
                names(&source.value, &mut dependencies);
                queue.extend(dependencies);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Role;
    use crate::r#lib::{ComponentDef, GroupDef};
    use crate::syntax::ast::{Argument, StatementKind};

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .group(GroupDef::new("G", 0, "`Leaf`."))
            .component(
                ComponentDef::new("Leaf", Role::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"ok\")"),
            )
            .build()
            .unwrap()
    }

    fn arg(name: Option<&str>, value: Expr) -> Argument {
        Argument {
            name: name.map(str::to_string),
            value,
        }
    }

    fn string(s: &str) -> Expr {
        Expr::String { value: s.into() }
    }

    fn query(call: &str, args: Vec<Argument>) -> Statement {
        Statement {
            kind: StatementKind::Query,
            name: "q".into(),
            value: Expr::Call {
                name: call.into(),
                args,
            },
        }
    }

    fn valid_args() -> Vec<Argument> {
        vec![
            arg(None, string("tickets.list")),
            arg(None, expr("{}")),
            arg(Some("default"), expr("[]")),
        ]
    }

    fn expr(text: &str) -> Expr {
        let source = format!("x={text}");
        crate::syntax::parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default())
            .program
            .statements
            .into_iter()
            .next()
            .unwrap()
            .value
    }

    #[test]
    fn query_header_rejects_each_malformed_shape_independently() {
        let lib = library();
        let opt = Options::default();
        let mut cases = Vec::new();
        cases.push(query("other", valid_args()));
        cases.push(query(
            "tool",
            vec![
                arg(None, string("tickets.list")),
                arg(Some("default"), expr("[]")),
                arg(Some("extra"), expr("1")),
            ],
        ));
        cases.push(query(
            "tool",
            vec![
                arg(None, string("tickets.list")),
                arg(None, expr("{}")),
                arg(None, expr("[]")),
            ],
        ));
        cases.push(query(
            "tool",
            vec![
                arg(None, string("tickets.list")),
                arg(None, expr("{}")),
                arg(Some("default"), expr("[]")),
                arg(Some("extra"), expr("1")),
            ],
        ));
        cases.push(query(
            "tool",
            vec![
                arg(None, expr("1")),
                arg(None, expr("{}")),
                arg(Some("default"), expr("[]")),
            ],
        ));
        for case in cases {
            let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
            v.query_header(&case);
            assert_eq!(
                v.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
                vec![Code::ExprType],
                "{case:?}"
            );
            assert!(v.ir.queries.is_empty());
        }
    }

    #[test]
    fn valid_and_unknown_query_headers_are_retained() {
        let lib = library();
        let mut opt = Options::default();
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        v.query_header(&query("tool", valid_args()));
        assert_eq!(
            v.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            vec![Code::UnknownTool]
        );
        assert_eq!(
            v.ir.queries.get("q").map(|q| (q.tool.as_str(), q.allowed)),
            Some(("tickets.list", false))
        );

        opt.tools.insert("tickets.list".into());
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        v.query_header(&query("tool", valid_args()));
        assert!(v.diagnostics.is_empty());
        assert_eq!(v.ir.queries.get("q").map(|q| q.allowed), Some(true));
    }

    #[test]
    fn query_checked_uses_header_types_and_reports_bad_args() {
        let lib = library();
        let opt = Options::default();
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        v.query_header(&query("tool", valid_args()));
        let checked = v.query_checked("q", &Context::at("q", None));
        assert!(checked.valid);
        assert_eq!(checked.ty, T::list(T::Any));
        let missing = v.query_checked("missing", &Context::at("q", None));
        assert!(!missing.valid);
    }

    #[test]
    fn extend_reachable_walks_new_dependencies_and_withdraws_orphans() {
        let lib = library();
        let opt = Options::default();
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        v.table.insert(
            "a".into(),
            Statement {
                kind: StatementKind::Bind,
                name: "a".into(),
                value: Expr::Name { name: "b".into() },
            },
        );
        v.table.insert(
            "b".into(),
            Statement {
                kind: StatementKind::Bind,
                name: "b".into(),
                value: Expr::Int { value: 1 },
            },
        );
        v.reachable.insert("a".into());
        let orphan = Diagnostic::new(Code::Orphan, Some("b".into()), None);
        let mut diagnostics = crate::Diagnostics::default();
        diagnostics.extend_raw([orphan.clone()]);
        v.orphan_diagnostics
            .insert("b".into(), vec![orphan.clone()]);
        let source = v.table.get("a").cloned().unwrap();
        v.extend_reachable(&[source], &mut diagnostics);
        assert!(v.reachable.contains("b"));
        assert!(!v.orphan_diagnostics.contains_key("b"));
        assert_eq!(diagnostics.len(), 0);
        assert!(v.work.statements_checked > 0);
        assert!(v.work.diagnostics_examined > 0);
    }
}
