//! Canonical syntactic binding evidence, independent of node allocation.
//!
//! A state with an ambiguous default (`null` or `[]`) takes the target type of the
//! first binding occurrence in canonical traversal order. The evidence is a pure
//! function of the reachable statement table, so it is retained per statement and
//! recomposed only through the dirty reverse-dependency closure. Unchanged
//! templates contribute their cached summary in O(1).

use super::*;

/// First binding occurrence per state: `(state name, binding target type)`.
pub(super) type Summary = Vec<(String, T)>;

#[derive(Clone, Default)]
pub(super) struct Entry {
    pub(super) summary: Summary,
    /// Every name referenced by the statement, in syntax order. Kept unfiltered so
    /// a later value→node kind change invalidates consumers without re-reading text.
    pub(super) deps: Vec<String>,
}

fn merge(into: &mut Summary, from: &[(String, T)]) {
    for (name, ty) in from {
        if !into.iter().any(|(known, _)| known == name) {
            into.push((name.clone(), ty.clone()));
        }
    }
}

fn referenced(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Name { name } => out.push(name.clone()),
        Expr::List { items } => {
            for e in items {
                referenced(e, out);
            }
        }
        Expr::Map { entries } => {
            for e in entries {
                referenced(&e.value, out);
            }
        }
        Expr::Call { args, .. } => {
            for a in args {
                referenced(&a.value, out);
            }
        }
        Expr::Unary { expr, .. } | Expr::Member { base: expr, .. } => referenced(expr, out),
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => {
            referenced(left, out);
            referenced(right, out);
        }
        Expr::Lambda { body, .. } => referenced(body, out),
        _ => {}
    }
}

fn binding(name: &Expr, def: &crate::r#lib::ComponentDef, out: &mut Summary) {
    let Expr::Call { args, .. } = name else {
        return;
    };
    let mut bound = BTreeMap::new();
    for a in args.iter().filter(|a| a.name.is_some()) {
        if let Some(i) = def
            .args
            .iter()
            .position(|p| Some(p.name.as_str()) == a.name.as_deref())
        {
            bound.entry(i).or_insert_with(|| a.value.clone());
        }
    }
    for a in args.iter().filter(|a| a.name.is_none()) {
        if let Some(i) = def
            .args
            .iter()
            .enumerate()
            .find(|(i, p)| p.required && !bound.contains_key(i))
            .map(|(i, _)| i)
        {
            bound.insert(i, a.value.clone());
        }
    }
    for (i, arg) in def.args.iter().enumerate() {
        if let T::Binding { of } = arg.ty.non_null()
            && let Some(Expr::State { name }) = bound.get(&i)
            && !out.iter().any(|(known, _)| known == name)
        {
            out.push((name.clone(), of.as_ref().clone()));
        }
    }
}

/// Collect binding evidence for every syntactic path in canonical order. Node
/// references are replaced by their retained summaries; cycle edges contribute
/// nothing because static cycles are already invalid.
fn compose(
    e: &Expr,
    locals: &BTreeSet<String>,
    cache: &crate::SharedMap<String, Entry>,
    kinds: &crate::SharedMap<String, Kind>,
    lib: &Library,
    out: &mut Summary,
) {
    match e {
        Expr::Name { name } if !locals.contains(name) => {
            if kinds.get(name) == Some(&Kind::Node)
                && let Some(entry) = cache.get(name)
            {
                merge(out, &entry.summary);
            }
        }
        Expr::List { items } => {
            for e in items {
                compose(e, locals, cache, kinds, lib, out);
            }
        }
        Expr::Call { name, args } if name == "each" => {
            if let Some(crate::syntax::ast::Argument {
                value: Expr::Lambda { params, body },
                ..
            }) = args.get(1)
            {
                let mut local = locals.clone();
                local.extend(params.iter().cloned());
                compose(body, &local, cache, kinds, lib, out);
            }
        }
        Expr::Call { name, args } if name == "if" || name == "coalesce" => {
            for a in args.iter().skip(usize::from(name == "if")) {
                compose(&a.value, locals, cache, kinds, lib, out);
            }
        }
        Expr::Call { name, args } => {
            let Some((_, def)) = lib.component(name) else {
                return;
            };
            binding(e, def, out);
            let mut bound = BTreeMap::new();
            for a in args.iter().filter(|a| a.name.is_some()) {
                if let Some(i) = def
                    .args
                    .iter()
                    .position(|p| Some(p.name.as_str()) == a.name.as_deref())
                {
                    bound.entry(i).or_insert_with(|| a.value.clone());
                }
            }
            for a in args.iter().filter(|a| a.name.is_none()) {
                if let Some(i) = def
                    .args
                    .iter()
                    .enumerate()
                    .find(|(i, p)| p.required && !bound.contains_key(i))
                    .map(|(i, _)| i)
                {
                    bound.insert(i, a.value.clone());
                }
            }
            for (i, arg) in def.args.iter().enumerate() {
                if arg.ty.slot().is_some()
                    && let Some(e) = bound.get(&i)
                {
                    compose(e, locals, cache, kinds, lib, out);
                }
            }
        }
        _ => {}
    }
}

/// Recompute binding summaries inside the dirty closure and return the root
/// summary for ambiguous-state inference.
pub(super) fn refresh(
    cached: &crate::SharedMap<String, Entry>,
    table: &crate::SharedMap<String, Statement>,
    kinds: &crate::SharedMap<String, Kind>,
    lib: &Library,
    dirty: &BTreeSet<String>,
    changed: &BTreeSet<String>,
) -> (crate::SharedMap<String, Entry>, Summary, u64) {
    let mut next = cached.clone();
    let old = cached.clone();
    let mut nodes: BTreeSet<String> = dirty
        .iter()
        .chain(changed.iter())
        .filter(|name| table.contains_key(*name) && kinds.get(*name) == Some(&Kind::Node))
        .cloned()
        .collect();
    // A cache restored from a different snapshot can lack root evidence even
    // when no statement is dirty; recompute it from retained child summaries.
    if table.get("root").is_some()
        && kinds.get("root") == Some(&Kind::Node)
        && !next.contains_key("root")
    {
        nodes.insert("root".into());
    }
    // Pull in referenced node statements that have no retained evidence yet.
    let mut pending: Vec<String> = nodes.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let entry = old
            .get(&name)
            .filter(|_| !changed.contains(&name))
            .or_else(|| next.get(&name).filter(|_| !changed.contains(&name)));
        let deps = if let Some(entry) = entry {
            entry.deps.clone()
        } else if let Some(source) = table.get(&name) {
            let mut deps = Vec::new();
            referenced(&source.value, &mut deps);
            deps
        } else {
            continue;
        };
        for dep in deps {
            if kinds.get(&dep) == Some(&Kind::Node)
                && table.contains_key(&dep)
                && !next.contains_key(&dep)
                && !changed.contains(&dep)
            {
                nodes.insert(dep.clone());
                pending.push(dep);
            }
        }
    }
    // Dependency-first order. Cycles are already rejected by static checking;
    // breaking the back-edge here cannot hide a valid binding.
    let mut order = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    let mut cyclic = BTreeSet::new();
    for start in nodes.iter() {
        if done.contains(start) {
            continue;
        }
        let mut stack = vec![(start.clone(), false)];
        while let Some((name, finish)) = stack.pop() {
            if finish {
                visiting.remove(&name);
                done.insert(name.clone());
                order.push(name);
                continue;
            }
            if done.contains(&name) {
                continue;
            }
            if !visiting.insert(name.clone()) {
                cyclic.insert(name);
                continue;
            }
            stack.push((name.clone(), true));
            let deps = old
                .get(&name)
                .filter(|_| !changed.contains(&name))
                .map(|entry| entry.deps.clone())
                .or_else(|| {
                    table.get(&name).map(|source| {
                        let mut deps = Vec::new();
                        referenced(&source.value, &mut deps);
                        deps
                    })
                })
                .unwrap_or_default();
            for dep in deps.into_iter().rev() {
                if dep != name
                    && kinds.get(&dep) == Some(&Kind::Node)
                    && table.contains_key(&dep)
                    && !done.contains(&dep)
                {
                    stack.push((dep, false));
                }
            }
        }
    }
    let mut updated = BTreeSet::new();
    let mut checked = 0;
    for name in order {
        let Some(source) = table.get(&name) else {
            continue;
        };
        let deps: Vec<String> = {
            let mut deps = Vec::new();
            referenced(&source.value, &mut deps);
            deps
        };
        if next.contains_key(&name)
            && !changed.contains(&name)
            && deps.iter().all(|dep| !updated.contains(dep))
        {
            continue;
        }
        checked += 1;
        let mut summary = Summary::new();
        compose(
            &source.value,
            &BTreeSet::new(),
            &next,
            kinds,
            lib,
            &mut summary,
        );
        // Evidence is only final once every referenced node template has a
        // retained summary. Forward references seal later; leaving the entry
        // absent makes the next update retry instead of caching an empty result.
        let complete = deps.iter().all(|dep| {
            kinds.get(dep) != Some(&Kind::Node)
                || !table.contains_key(dep)
                || next.contains_key(dep)
                || cyclic.contains(dep)
        });
        if !complete {
            next.remove(&name);
            continue;
        }
        let previous = old.get(&name).map(|entry| entry.summary.clone());
        if previous.as_ref() != Some(&summary) {
            updated.insert(name.clone());
        }
        next.insert(name, Entry { summary, deps });
    }
    let summary = next
        .get("root")
        .map(|entry| entry.summary.clone())
        .unwrap_or_default();
    (next, summary, checked)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_binding_wins_in_canonical_order() {
        let mut a = Summary::new();
        a.push(("v".into(), T::Text));
        merge(&mut a, &[("v".into(), T::Bool)]);
        assert_eq!(a, vec![("v".into(), T::Text)]);
    }
    #[test]
    fn composite_summaries_remain_ordered_and_shared() {
        let mut summary = Summary::new();
        merge(&mut summary, &[("a".into(), T::Text)]);
        merge(&mut summary, &[("b".into(), T::Int), ("a".into(), T::Bool)]);
        assert_eq!(summary, vec![("a".into(), T::Text), ("b".into(), T::Int)]);
    }
}
