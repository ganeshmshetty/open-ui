//! Loading a completed program into a retained tree without I/O (SPEC §10.1).
//!
//! The gallery and demo binaries own file reading; this module only turns bytes
//! into a `LoadedProgram`, so it is usable from tests and hosts.

use crate::{
    capabilities::Capabilities,
    components,
    host::{HostData, HostPolicy, SampleHost},
    tree::RetainedTree,
};
use genui_core::{
    Session, SessionOptions,
    diag::{Diagnostic, Severity},
    patch::PatchBatch,
    session::InputMode,
    syntax::streaming::Seal,
    validate::Options,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct LoadedProgram {
    pub tree: RetainedTree,
    /// Every batch the session emitted, including the final snapshot.
    pub batches: Vec<PatchBatch>,
    pub diagnostics: Vec<Diagnostic>,
    pub seals: Vec<Seal>,
    /// Implemented component kinds present in the final tree.
    pub kinds: Vec<String>,
    /// Kinds that still render as placeholders, sorted and unique. Phase 1
    /// implements every catalog kind, so this is normally empty.
    pub deferred: Vec<String>,
    /// Final `$state` values for bound inputs (SPEC §10.4).
    pub state: std::collections::BTreeMap<String, genui_core::runtime::StateValue>,
}

impl LoadedProgram {
    pub fn has_warning_or_worse(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity >= Severity::Warning)
    }
}

/// Builds a session with the renderer's capabilities and the host's options.
pub fn session(source: &[u8], options: &Options) -> Result<LoadedProgram, String> {
    let library = genui_std::std_library().map_err(|errors| format!("std library: {errors:?}"))?;
    let mut options = options.clone();
    options.capabilities = Capabilities::m8().names();
    let policy = HostPolicy::from_options(&options);
    let mut session = Session::new(SessionOptions {
        library: Arc::new(library),
        validation: options,
        input: InputMode::Program,
        prompt_digest: String::new(),
    });
    let data = HostData::demo();
    for reference in &policy.refs {
        if let Some(value) = data.ref_value(reference) {
            session.preload_ref(reference, value);
        }
    }
    let update = session.push(source);
    let mut batches: Vec<PatchBatch> = update.patches.into_iter().collect();
    let finish = session.finish();
    let mut host = SampleHost::new(policy, HostData::demo());
    let delivered = host.settle(&mut session, &finish);
    batches.extend(finish.patches);
    for update in delivered {
        batches.extend(update.patches);
    }
    let snapshot = session.snapshot();
    batches.extend(snapshot.patches);
    let (tree, reports) = RetainedTree::from_batches(&batches);
    for report in &reports {
        if !report.errors.is_empty() && !report.needs_snapshot {
            return Err(format!(
                "renderer rejected the program: {:?}",
                report.errors
            ));
        }
    }
    let mut kinds = Vec::new();
    let mut deferred = Vec::new();
    for entry in tree.nodes.values() {
        if !kinds.contains(&entry.kind) {
            kinds.push(entry.kind.clone());
        }
        if let Some(milestone) = components::deferred_milestone(&entry.kind) {
            let label = format!("{} ({milestone})", entry.kind);
            if !deferred.contains(&label) {
                deferred.push(label);
            }
        }
    }
    kinds.sort();
    deferred.sort();
    Ok(LoadedProgram {
        tree,
        batches,
        diagnostics: session.diagnostics().to_vec(),
        seals: session.seals().to_vec(),
        kinds,
        deferred,
        state: session.state_values(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> Vec<u8> {
        let path = format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).expect("example")
    }

    #[test]
    fn dashboard_example_loads_into_a_retained_tree() {
        let loaded =
            session(&example("01-sales-dashboard.ui"), &Options::default()).expect("session");
        assert!(!loaded.tree.is_empty());
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        assert_eq!(loaded.tree.get("root").unwrap().kind, "Column");
        assert!(loaded.kinds.iter().any(|kind| kind == "Chart"));
        assert!(loaded.kinds.iter().any(|kind| kind == "Stat"));
        assert!(loaded.deferred.is_empty());
        assert!(!loaded.batches.is_empty());
    }

    #[test]
    fn every_example_loads_and_reports_its_deferred_components() {
        let root = format!("{}/../../examples", env!("CARGO_MANIFEST_DIR"));
        let mut paths: Vec<_> = std::fs::read_dir(&root)
            .expect("examples dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "ui"))
            .collect();
        paths.sort();
        assert!(paths.len() >= 24, "all 24 exemplars are present");
        for path in paths {
            let source = std::fs::read(&path).expect("read example");
            let loaded = session(&source, &Options::default())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                !loaded.tree.is_empty(),
                "{} has no concrete root",
                path.display()
            );
        }
    }

    #[test]
    fn host_refs_expand_list_and_table_examples() {
        let options = Options {
            refs: ["t4.orders".into(), "t4.people".into(), "t4.places".into()].into(),
            ..Options::default()
        };
        let loaded = session(&example("06-team-directory.ui"), &options).expect("session");
        assert!(
            loaded.tree.len() > 4,
            "people ref expands into rows: {} nodes",
            loaded.tree.len()
        );
    }

    #[test]
    fn capabilities_come_from_the_renderer() {
        let caps = Capabilities::m6();
        assert!(!caps.text_input);
        let library = genui_std::std_library().expect("std");
        let options = Options {
            capabilities: caps.names(),
            ..Options::default()
        };
        let mut session = Session::new(SessionOptions {
            library: Arc::new(library),
            validation: options,
            input: InputMode::Program,
            prompt_digest: String::new(),
        });
        let update = session.push(b"root=Input(\"Name\")");
        let finish = session.finish();
        assert!(update.patches.is_some() || finish.patches.is_some());
        assert!(
            session
                .diagnostics()
                .iter()
                .any(|diagnostic| format!("{:?}", diagnostic.code).contains("CapFallback")),
            "an unavailable text input falls back rather than claiming support"
        );
    }
}
