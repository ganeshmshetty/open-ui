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
                && !nodes.contains(&dep)
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
    use crate::SharedMap;
    use crate::r#lib::{ComponentDef, GroupDef, Role};

    fn library() -> Library {
        Library::builder("binding", "0.1.0")
            .group(GroupDef::new("G", 0, "Binding fixtures."))
            .component(
                ComponentDef::new("Show", Role::Paragraph)
                    .group("G")
                    .doc("A binding sink.")
                    .req("value", T::binding(T::Text)),
            )
            .component(
                ComponentDef::new("Pair", Role::Paragraph)
                    .group("G")
                    .doc("Two binding sinks.")
                    .req("first", T::binding(T::Text))
                    .req("second", T::binding(T::Int)),
            )
            .component(
                ComponentDef::new("Opt", Role::Paragraph)
                    .group("G")
                    .doc("An optional binding sink.")
                    .opt_none("value", T::binding(T::Text)),
            )
            .component(
                ComponentDef::new("Stack", Role::Group)
                    .group("G")
                    .doc("A stack.")
                    .req("children", T::nodes()),
            )
            .component(
                ComponentDef::new("Card", Role::Group)
                    .group("G")
                    .doc("A titled card.")
                    .req("title", T::text())
                    .req("children", T::nodes()),
            )
            .build()
            .unwrap()
    }
    fn statement(source: &str) -> Statement {
        let mut parsed = parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default());
        parsed.program.statements.pop().unwrap()
    }
    fn value(source: &str) -> Expr {
        statement(source).value
    }
    fn table(entries: &[(&str, &str)]) -> SharedMap<String, Statement> {
        entries
            .iter()
            .map(|(_, source)| {
                let s = statement(source);
                (s.name.clone(), s)
            })
            .collect()
    }
    fn kinds(entries: &[(&str, Kind)]) -> SharedMap<String, Kind> {
        entries
            .iter()
            .map(|(name, kind)| ((*name).to_string(), *kind))
            .collect()
    }
    fn cache(entries: &[(&str, Summary, &[&str])]) -> SharedMap<String, Entry> {
        entries
            .iter()
            .map(|(name, summary, deps)| {
                (
                    (*name).to_string(),
                    Entry {
                        summary: summary.clone(),
                        deps: deps.iter().map(|d| (*d).to_string()).collect(),
                    },
                )
            })
            .collect()
    }
    fn names(values: &[String]) -> Vec<&str> {
        values.iter().map(String::as_str).collect()
    }

    #[test]
    fn referenced_descends_into_every_expression_shape() {
        let mut out = Vec::new();
        referenced(
            &value("root=Leaf({alpha: [x], nested: y.z, slot: x[i]})"),
            &mut out,
        );
        assert_eq!(names(&out), vec!["x", "y", "x", "i"]);

        out.clear();
        referenced(&value("root=Leaf(-x)"), &mut out);
        assert_eq!(names(&out), vec!["x"]);

        out.clear();
        referenced(&value("root=Leaf(x + y)"), &mut out);
        assert_eq!(names(&out), vec!["x", "y"]);

        out.clear();
        referenced(&value("root=each(items, |x| x)"), &mut out);
        assert_eq!(names(&out), vec!["items", "x"]);
    }

    #[test]
    fn positional_bindings_fill_only_unbound_required_arguments() {
        let lib = library();
        let (_, def) = lib.component("Pair").unwrap();
        let mut summary = Summary::new();
        binding(&value("root=Pair($a, $b)"), def, &mut summary);
        assert_eq!(summary, vec![("a".into(), T::Text), ("b".into(), T::Int)]);
    }

    #[test]
    fn duplicate_binding_evidence_keeps_the_first_occurrence() {
        let lib = library();
        let (_, def) = lib.component("Pair").unwrap();
        let mut summary = Summary::new();
        binding(&value("root=Pair($a, $a)"), def, &mut summary);
        assert_eq!(summary, vec![("a".into(), T::Text)]);
    }

    #[test]
    fn positional_arguments_never_fill_optional_arguments() {
        let lib = library();
        let (_, def) = lib.component("Opt").unwrap();
        let mut summary = Summary::new();
        binding(&value("root=Opt($a)"), def, &mut summary);
        assert!(summary.is_empty());
    }

    #[test]
    fn compose_skips_local_names_and_merges_node_summaries() {
        let lib = library();
        let node_cache = cache(&[("child", vec![("s".into(), T::Text)], &[])]);
        let node_kinds = kinds(&[("child", Kind::Node)]);
        let mut out = Summary::new();
        compose(
            &value("root=Stack([child])"),
            &BTreeSet::new(),
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("s".into(), T::Text)]);

        out.clear();
        compose(
            &value("root=Stack([child])"),
            &std::iter::once("child".to_string()).collect(),
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn compose_binds_successive_positionals_to_unbound_required_arguments() {
        let lib = library();
        let node_cache = cache(&[("child", vec![("s".into(), T::Text)], &[])]);
        let node_kinds = kinds(&[("child", Kind::Node)]);
        let mut out = Summary::new();
        compose(
            &value("root=Card(\"title\", child)"),
            &BTreeSet::new(),
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(
            out,
            vec![("s".into(), T::Text)],
            "the second positional must bind the remaining required slot"
        );
    }

    #[test]
    fn compose_follows_each_if_and_coalesce_templates() {
        let lib = library();
        let node_cache = cache(&[
            ("body", vec![("b".into(), T::Text)], &[]),
            ("then", vec![("t".into(), T::Text)], &[]),
            ("otherwise", vec![("o".into(), T::Text)], &[]),
            ("condition", vec![("c".into(), T::Text)], &[]),
            ("left", vec![("l".into(), T::Text)], &[]),
            ("right", vec![("r".into(), T::Text)], &[]),
        ]);
        let node_kinds = kinds(&[
            ("body", Kind::Node),
            ("then", Kind::Node),
            ("otherwise", Kind::Node),
            ("condition", Kind::Node),
            ("left", Kind::Node),
            ("right", Kind::Node),
        ]);
        let locals = BTreeSet::new();
        let mut out = Summary::new();
        compose(
            &value("root=each(items, |x| Stack([body]))"),
            &locals,
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("b".into(), T::Text)]);

        out.clear();
        compose(
            &value("root=if(condition, Stack([then]), Stack([otherwise]))"),
            &locals,
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("t".into(), T::Text), ("o".into(), T::Text)]);

        out.clear();
        compose(
            &value("root=coalesce(Stack([left]), Stack([right]))"),
            &locals,
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("l".into(), T::Text), ("r".into(), T::Text)]);
    }

    #[test]
    fn compose_finds_named_and_positional_component_arguments() {
        let lib = library();
        let node_cache = cache(&[("child", vec![("s".into(), T::Text)], &[])]);
        let node_kinds = kinds(&[("child", Kind::Node)]);
        let mut out = Summary::new();
        compose(
            &value("root=Stack(children: [child])"),
            &BTreeSet::new(),
            &node_cache,
            &node_kinds,
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("s".into(), T::Text)]);

        out.clear();
        compose(
            &value("root=Pair($a, $b)"),
            &BTreeSet::new(),
            &SharedMap::new(),
            &SharedMap::new(),
            &lib,
            &mut out,
        );
        assert_eq!(out, vec![("a".into(), T::Text), ("b".into(), T::Int)]);

        out.clear();
        compose(
            &value("root=Opt($a)"),
            &BTreeSet::new(),
            &SharedMap::new(),
            &SharedMap::new(),
            &lib,
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn refresh_ignores_non_node_and_retained_root_evidence() {
        let lib = library();
        let table = table(&[("root", "root=Stack([child])"), ("s", "s=1")]);
        let node_kinds = kinds(&[("root", Kind::Node), ("s", Kind::Value)]);
        let retained = cache(&[("root", vec![("retained".into(), T::Text)], &["child"])]);
        let (_next, summary, checked) = refresh(
            &retained,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("s".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert_eq!(checked, 0);
        assert_eq!(summary, vec![("retained".into(), T::Text)]);

        let (_next, summary, checked) = refresh(
            &SharedMap::new(),
            &table,
            &node_kinds,
            &lib,
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(checked, 1);
        assert!(summary.is_empty());
    }

    #[test]
    fn refresh_does_not_restore_a_value_root() {
        let lib = library();
        let table = table(&[("root", "root=Leaf(\"x\")")]);
        let node_kinds = kinds(&[("root", Kind::Value)]);
        let (_, summary, checked) = refresh(
            &SharedMap::new(),
            &table,
            &node_kinds,
            &lib,
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(checked, 0);
        assert!(summary.is_empty());
    }

    #[test]
    fn refresh_recomputes_cached_dependencies_before_their_consumers() {
        let lib = library();
        let table = table(&[
            ("a", "a=Stack([b])"),
            ("b", "b=Stack([c])"),
            ("c", "c=Show(value:$s)"),
        ]);
        let node_kinds = kinds(&[("a", Kind::Node), ("b", Kind::Node), ("c", Kind::Node)]);
        let old = cache(&[
            ("a", vec![("old".into(), T::Int)], &["b"]),
            ("b", vec![("old".into(), T::Int)], &["c"]),
        ]);
        let (next, _, _) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert!(next.contains_key("c"));
        assert_eq!(next["a"].summary, vec![("s".into(), T::Text)]);
    }

    #[test]
    fn refresh_recomputes_changed_nodes_from_the_live_table() {
        let lib = library();
        let table = table(&[
            ("root", "root=Stack([a])"),
            ("a", "a=Show(value:$s)"),
            ("b", "b=Show(value:$t)"),
        ]);
        let node_kinds = kinds(&[("root", Kind::Node), ("a", Kind::Node), ("b", Kind::Node)]);
        let old = cache(&[("a", vec![("old".into(), T::Int)], &[])]);
        let (next, _, checked) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &std::iter::once("a".to_string()).collect(),
        );
        assert_eq!(checked, 2);
        assert_eq!(next["a"].summary, vec![("s".into(), T::Text)]);
    }

    #[test]
    fn refresh_pulls_dependencies_from_changed_sources() {
        let lib = library();
        let table = table(&[
            ("root", "root=Stack([a])"),
            ("a", "a=Stack([c])"),
            ("c", "c=Show(value:$s)"),
        ]);
        let node_kinds = kinds(&[("root", Kind::Node), ("a", Kind::Node), ("c", Kind::Node)]);
        let old = cache(&[("a", vec![], &[])]);
        let (next, _, _) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &std::iter::once("a".to_string()).collect(),
        );
        assert!(next.contains_key("a"));
        assert!(next.contains_key("c"));
    }

    #[test]
    fn refresh_reads_changed_dependencies_from_the_live_table() {
        // No root evidence exists, so the changed node's retained (empty) deps
        // must not hide the dependency introduced by its new source text.
        let lib = library();
        let table = table(&[("a", "a=Stack([c])"), ("c", "c=Show(value:$s)")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("c", Kind::Node)]);
        let old = cache(&[("a", vec![], &[])]);
        let (next, _, _) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &std::iter::once("a".to_string()).collect(),
        );
        assert!(
            next.contains_key("a"),
            "{:?}",
            next.keys().collect::<Vec<_>>()
        );
        assert!(
            next.contains_key("c"),
            "{:?}",
            next.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn refresh_ignores_retained_deps_of_a_changed_node() {
        // §3.6: a changed source must be summarised from its live text. Cached
        // deps of the old text may name nodes that are no longer referenced;
        // pulling those in would do (and charge) unrelated work.
        let lib = library();
        let table = table(&[("a", "a=Show(value:1)"), ("b", "b=Show(value:$s)")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("b", Kind::Node)]);
        let old = cache(&[("a", vec![("stale".into(), T::Bool)], &["b"])]);
        let (next, _, checked) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &std::iter::once("a".to_string()).collect(),
        );
        assert_eq!(
            checked,
            1,
            "only the changed node may be recomputed: {:?}",
            next.keys().collect::<Vec<_>>()
        );
        assert!(
            !next.contains_key("b"),
            "an unreferenced retained dependency must not be retained: {:?}",
            next.keys().collect::<Vec<_>>()
        );
        assert!(
            next["a"].deps.is_empty(),
            "the summary must follow the live source"
        );
    }

    #[test]
    fn refresh_orders_dependencies_and_retries_incomplete_entries() {
        let lib = library();
        let table = table(&[("a", "a=Stack([b])"), ("b", "b=Show(value:$s)")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("b", Kind::Node)]);
        let old = cache(&[("a", vec![], &["b"]), ("b", vec![], &[])]);
        let (next, _, checked) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string())
                .chain(std::iter::once("b".to_string()))
                .collect(),
            &std::iter::once("b".to_string()).collect(),
        );
        assert_eq!(checked, 2);
        assert_eq!(next["a"].summary, vec![("s".into(), T::Text)]);
        assert_eq!(next["b"].summary, vec![("s".into(), T::Text)]);
    }

    #[test]
    fn refresh_does_not_schedule_scalar_dependencies() {
        let lib = library();
        let table = table(&[("a", "a=Show(value:s)"), ("s", "s=1")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("s", Kind::Value)]);
        let old = SharedMap::new();
        let (_next, _, checked) = refresh(
            &old,
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert_eq!(checked, 1);
    }

    #[test]
    fn refresh_breaks_self_edges_and_keeps_resolved_forward_edges() {
        let lib = library();
        let self_table = table(&[("a", "a=Stack([a])")]);
        let node_kinds = kinds(&[("a", Kind::Node)]);
        let (next, _, _) = refresh(
            &SharedMap::new(),
            &self_table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert!(!next.contains_key("a"));

        let forward = table(&[("a", "a=Stack([b])"), ("b", "b=Show(value:$s)")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("b", Kind::Node)]);
        let old = cache(&[("b", vec![("s".into(), T::Text)], &[])]);
        let (next, _, _) = refresh(
            &old,
            &forward,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert!(next.contains_key("a"));
    }

    #[test]
    fn refresh_missing_table_nodes_complete_without_evidence() {
        let lib = library();
        let table = table(&[("a", "a=Stack([b])")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("b", Kind::Node)]);
        let (next, _, _) = refresh(
            &SharedMap::new(),
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert!(next.contains_key("a"));
    }

    #[test]
    fn refresh_keeps_entries_whose_scalar_dependencies_lack_evidence() {
        let lib = library();
        let table = table(&[("a", "a=Show(value:s)"), ("s", "s=1")]);
        let node_kinds = kinds(&[("a", Kind::Node), ("s", Kind::Value)]);
        let (next, _, _) = refresh(
            &SharedMap::new(),
            &table,
            &node_kinds,
            &lib,
            &std::iter::once("a".to_string()).collect(),
            &BTreeSet::new(),
        );
        assert!(next.contains_key("a"));
    }

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
