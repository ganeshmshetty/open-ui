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
