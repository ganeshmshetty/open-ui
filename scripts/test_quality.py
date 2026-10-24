import unittest
from quality import fixture_errors, forbidden, source_errors


class QualityTests(unittest.TestCase):
    def test_ignore_and_cfg_attr_are_rejected(self):
        for source in ['#[ignore]', '#[ignore = "slow"]', '#[cfg_attr(feature="x", ignore)]']:
            self.assertEqual(len(source_errors("crates/x/tests.rs", source)), 1)
        self.assertEqual(source_errors("crates/x/tests.rs", '// #[ignore]\n#[test]'), [])
        self.assertEqual(source_errors("crates/x/tests.rs", 'r###"#[ignore] /*"###'), [])
        self.assertEqual(source_errors("crates/x/tests.rs", '/* outer /* inner */ #[ignore] */'), [])
        self.assertEqual(len(source_errors("crates/x/tests.rs", 'let url = "http://x"; #[ignore]')), 1)

    def test_styles_are_confined_to_theme(self):
        for source in ['rgb(0xffaacc)', 'let c = "#aabbcc";', 'px(12.)', 'Pixels(-8.0)']:
            self.assertEqual(len(source_errors("crates/genui-gpui/src/view.rs", source)), 1)
            self.assertEqual(source_errors("crates/genui-gpui/src/theme.rs", source), [])
            self.assertEqual(source_errors("crates/genui-gpui/src/theme/dark.rs", source), [])
        self.assertEqual(source_errors("crates/genui-gpui/src/view.rs", 'px(theme.spacing) // px(8)'), [])
        self.assertEqual(source_errors("crates/genui-gpui/src/not_theme.rs", 'px(8)'),
                         ['crates/genui-gpui/src/not_theme.rs: pixel literal outside theme'])

    def test_fixture_changes_require_both_contract_files(self):
        for name in ["conformance/x/want.ir.json", "crates/genui-cli/tests/want.txt"]:
            self.assertTrue(fixture_errors([name], "abc"))
            self.assertTrue(fixture_errors([name, "spec/SPEC.md"], "abc"))
            self.assertEqual(fixture_errors([name, "spec/SPEC.md", "spec/CHANGELOG.md"], "abc"), [])
        self.assertEqual(fixture_errors(["crates/x/src/lib.rs"], "abc"), [])

    def test_core_dependency_prefixes_and_renamed_packages(self):
        for name in ["gpui", "gpui-pre", "gpui-new", "tokio", "tokio-util", "futures-core", "async-std", "egui", "smol", "genui-gpui"]:
            self.assertTrue(forbidden(name))
        for name in ["genui-core", "serde", "url", "xxhash-rust"]:
            self.assertFalse(forbidden(name))


class IntegrationTests(unittest.TestCase):
    def _repo(self):
        from pathlib import Path
        import subprocess
        import tempfile
        directory = tempfile.TemporaryDirectory()
        root = Path(directory.name)
        def git(*args):
            return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()
        git("init", "-q")
        git("config", "user.name", "Gate Test")
        git("config", "user.email", "gate@example.invalid")
        return directory, root, git

    def test_spec_change_may_land_later_in_the_same_push(self):
        from quality import fixtures
        directory, root, git = self._repo()
        with directory:
            (root / "want.ir.json").write_text("{}")
            git("add", ".")
            git("commit", "-qm", "initial golden")
            base = git("rev-parse", "HEAD")
            (root / "want.ir.json").write_text('{"changed":true}')
            git("add", ".")
            git("commit", "-qm", "golden change")
            (root / "spec").mkdir()
            (root / "spec/SPEC.md").write_text("contract")
            (root / "spec/CHANGELOG.md").write_text("later")
            git("add", ".")
            git("commit", "-qm", "documented")
            self.assertEqual(fixtures(root, base, "HEAD"), [])

    def test_modified_golden_without_any_spec_change_fails(self):
        from quality import fixtures
        directory, root, git = self._repo()
        with directory:
            (root / "want.ir.json").write_text("{}")
            git("add", ".")
            git("commit", "-qm", "initial golden")
            base = git("rev-parse", "HEAD")
            (root / "want.ir.json").write_text('{"changed":true}')
            git("add", ".")
            git("commit", "-qm", "unprotected change")
            bad = git("rev-parse", "HEAD")
            errors = fixtures(root, base, "HEAD")
            self.assertEqual(len(errors), 1)
            self.assertIn(bad, errors[0])

    def test_new_fixture_needs_no_spec_change(self):
        from quality import fixtures
        directory, root, git = self._repo()
        with directory:
            (root / "initial").write_text("base")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (root / "want.ir.json").write_text("{}")
            git("add", ".")
            git("commit", "-qm", "new fixture")
            self.assertEqual(fixtures(root, base, "HEAD"), [])

    def test_force_pushed_base_checks_the_new_head(self):
        from quality import fixtures
        directory, root, git = self._repo()
        with directory:
            (root / "want.ir.json").write_text("{}")
            git("add", ".")
            git("commit", "-qm", "base golden")
            abandoned = git("rev-parse", "HEAD")
            git("checkout", "-q", "--orphan", "rewritten")
            (root / "want.ir.json").write_text("{}")
            git("add", ".")
            git("commit", "-qm", "rewritten base golden")
            (root / "want.ir.json").write_text('{"changed":true}')
            git("add", ".")
            git("commit", "-qm", "unprotected change after force push")
            head = git("rev-parse", "HEAD")
            errors = fixtures(root, abandoned, "HEAD")
            self.assertEqual(len(errors), 1)
            self.assertIn(head, errors[0])

    def test_cargo_deny_rejects_a_renamed_gpui_dependency(self):
        from pathlib import Path
        import subprocess
        import tempfile
        from quality import ROOT
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[workspace]\nmembers=["crates/genui-core", "crates/banned"]\nresolver="3"\n')
            for folder, name in [("genui-core", "genui-core"), ("banned", "gpui-future")]:
                crate = root / "crates" / folder
                (crate / "src").mkdir(parents=True)
                (crate / "src/lib.rs").write_text("")
                (crate / "Cargo.toml").write_text(f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2024"\n')
            with (root / "crates/genui-core/Cargo.toml").open("a") as file:
                file.write('[dependencies]\ninnocent = {package="gpui-future",path="../banned"}\n')
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root, check=True, capture_output=True)
            result = subprocess.run(["python3", str(ROOT / "scripts/quality.py"), "dependencies", "--root", str(root)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("gpui-future", result.stderr)
            self.assertIn("banned", result.stderr)

    def test_core_deny_lints_are_effective(self):
        from pathlib import Path
        import subprocess
        import tempfile
        from quality import ROOT
        # Copy only the crate-level attributes, then prove every denied lint fires.
        header = (ROOT / "crates/genui-core/src/lib.rs").read_text().split("pub mod ")[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text('[package]\nname="lint-probe"\nversion="0.1.0"\nedition="2024"\n')
            (root / "src/lib.rs").write_text(header + '''
pub fn a(x: Option<u8>) -> u8 { x.unwrap() }
pub fn b(x: Option<u8>) -> u8 { x.expect("x") }
pub fn c() { panic!("x") }
pub fn d() { todo!() }
pub fn e() { unimplemented!() }
''')
            result = subprocess.run(["cargo", "clippy", "--offline"], cwd=root, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            for lint in ["unwrap_used", "expect_used", "panic", "todo", "unimplemented"]:
                self.assertIn(lint, result.stderr)


class MutationReviewTests(unittest.TestCase):
    def test_only_complete_baseline_passed_reviewed_campaigns_are_accepted(self):
        from mutation import review_outcomes
        data = {"end_time": "finished", "outcomes": [
            {"scenario": "Baseline", "summary": "Success"},
            {"scenario": {"Mutant": {"name": "m", "file": "file.rs"}}, "summary": "MissedMutant"},
        ]}
        hashes = {"file.rs": "sha"}
        self.assertTrue(review_outcomes(data, [{"name": "m"}], hashes, {}))
        review = {"source_sha256": hashes, "survivors": {"m": "Equivalent under the documented contract."}}
        self.assertEqual(review_outcomes(data, [{"name": "m"}], hashes, review), [])
        self.assertTrue(review_outcomes(data, [{"name": "m"}, {"name": "other"}], hashes, review))
        self.assertTrue(review_outcomes(data, [{"name": "m"}], {"file.rs": "changed"}, review))
        data["outcomes"][0]["summary"] = "Failure"
        self.assertTrue(review_outcomes(data, [{"name": "m"}], hashes, review))


class MutationInventoryTests(unittest.TestCase):
    def test_shards_are_disjoint_and_cover_exact_inventory(self):
        from mutation import inventory_errors
        groups = [[{"name": "a"}], [{"name": "b"}]]
        self.assertEqual(inventory_errors(groups, ["a", "b"]), [])
        self.assertTrue(inventory_errors(groups, ["a", "b", "c"]))
        self.assertTrue(inventory_errors(groups + [[{"name": "a"}]], ["a", "b"]))


class MutationAggregationTests(unittest.TestCase):
    def test_aggregation_checks_manifests_and_writes_exact_counts(self):
        import contextlib
        import hashlib
        import io
        import json
        from pathlib import Path
        import tempfile
        from unittest.mock import patch
        import mutation
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source.rs").write_text("original")
            hashes = {"source.rs": hashlib.sha256(b"original").hexdigest()}
            inventory = [f"m{i}" for i in range(mutation.SHARD_COUNT)]
            (root / "conformance/mutations/m3").mkdir(parents=True)
            (root / "conformance/mutations/m3/inventory.json").write_text(json.dumps(inventory))
            review = root / "review.json"
            review.write_text('{}')
            artifacts = root / "artifacts"
            for i, name in enumerate(inventory):
                shard = artifacts / f"shard{i}"
                (shard / "mutants.out").mkdir(parents=True)
                (shard / "source.json").write_text(json.dumps(hashes))
                (shard / "mutants.out/mutants.json").write_text(json.dumps([{"name": name}]))
                data = {"start_time": "start", "end_time": "end", "cargo_mutants_version": "test", "outcomes": [
                    {"scenario": "Baseline", "summary": "Success"},
                    {"scenario": {"Mutant": {"name": name, "file": "source.rs"}}, "summary": "CaughtMutant"},
                ]}
                (shard / "mutants.out/outcomes.json").write_text(json.dumps(data))
            with patch.object(mutation, "ROOT", root), patch.object(mutation.subprocess, "check_output", return_value="test-head\n"), contextlib.redirect_stdout(io.StringIO()):
                mutation.aggregate(artifacts, review)
                summary = json.loads((artifacts / "acceptance.json").read_text())
                self.assertTrue(summary["acceptance"])
                self.assertEqual(summary["generated"], mutation.SHARD_COUNT)
                self.assertEqual(summary["counts"], {"CaughtMutant": mutation.SHARD_COUNT})
                (root / "source.rs").write_text("changed")
                with self.assertRaisesRegex(ValueError, "stale mutation source"):
                    mutation.aggregate(artifacts, review)


if __name__ == "__main__":
    unittest.main()
