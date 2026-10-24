//! Recorded core patch streams replay through the renderer tree without panic.

use genui_core::patch::PatchBatch;
use genui_gpui::tree::RetainedTree;
use std::path::PathBuf;

#[test]
fn every_patch_fixture_replays_through_the_retained_tree() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/patches");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("patch fixtures")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    for directory in paths {
        let path = directory.join("want.patches.json");
        let bytes = std::fs::read(&path).expect("want.patches.json");
        let batches: Vec<PatchBatch> = serde_json::from_slice(&bytes).expect("patch batches");
        let (tree, reports) = RetainedTree::from_batches(&batches);
        for report in &reports {
            assert!(
                report.errors.is_empty(),
                "{}: renderer rejected a recorded batch: {:?}",
                path.display(),
                report.errors
            );
        }
        assert!(
            tree.root.is_some(),
            "{}: replay produced no root",
            path.display()
        );
        assert!(
            !tree.is_empty(),
            "{}: replay produced no nodes",
            path.display()
        );
    }
}
