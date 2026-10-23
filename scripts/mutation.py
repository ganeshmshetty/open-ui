#!/usr/bin/env python3
"""Run the milestone-only syntax/IR gate without copying research or mutating the checkout."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SHARD_COUNT = 16


def review_outcomes(outcomes, generated, hashes, review):
    errors = []
    baseline = [o for o in outcomes["outcomes"] if o["scenario"] == "Baseline"]
    mutations = [o for o in outcomes["outcomes"] if o["scenario"] != "Baseline"]
    if len(baseline) != 1 or baseline[0]["summary"] != "Success":
        errors.append("unmutated baseline did not pass")
    expected = {m["name"] for m in generated}
    actual = {o["scenario"]["Mutant"]["name"] for o in mutations}
    if (not outcomes.get("end_time") or len(mutations) != len(generated)
            or len(actual) != len(mutations) or expected != actual):
        errors.append("mutation campaign is incomplete")
    for outcome in mutations:
        status = outcome["summary"]
        mutant = outcome["scenario"]["Mutant"]
        if status == "MissedMutant":
            reason = review.get("survivors", {}).get(mutant["name"], "")
            source_hash = hashes.get(mutant["file"])
            matched_source = source_hash is not None and review.get("source_sha256", {}).get(mutant["file"]) == source_hash
            if not reason.strip() or not matched_source:
                errors.append("unreviewed or stale survivor: " + mutant["name"])
        elif status == "Timeout" and (not outcome.get("phase_results")
                or outcome["phase_results"][-1]["phase"] != "Test"):
            errors.append("build timeout is not a killed mutant: " + mutant["name"])
        elif status not in {"CaughtMutant", "Unviable", "Timeout"}:
            errors.append("unexpected mutant outcome: " + status)
    return errors


def inventory_errors(groups, expected):
    names = [m["name"] for group in groups for m in group]
    if len(names) != len(set(names)) or len(expected) != len(set(expected)) or set(names) != set(expected):
        return ["shards overlap or do not cover the complete frozen inventory"]
    return []


def aggregate(directory, review_path, inventory_path=None):
    folders = sorted(p for p in directory.iterdir() if p.is_dir())
    if len(folders) != SHARD_COUNT:
        raise ValueError("expected exactly sixteen mutation shards")
    review = json.loads(review_path.read_text())
    groups, results, manifests, raw_hashes = [], [], [], {}
    errors = []
    for folder in folders:
        hashes = json.loads((folder / "source.json").read_text())
        data_path = folder / "mutants.out/outcomes.json"
        data = json.loads(data_path.read_text())
        generated = json.loads((folder / "mutants.out/mutants.json").read_text())
        errors.extend(review_outcomes(data, generated, hashes, review))
        groups.append(generated)
        results.append(data)
        manifests.append(hashes)
        raw_hashes[folder.name] = hashlib.sha256(data_path.read_bytes()).hexdigest()
    expected = json.loads((inventory_path or ROOT / "conformance/mutations/m3/inventory.json").read_text())
    errors.extend(inventory_errors(groups, expected))
    if any(m != manifests[0] for m in manifests):
        errors.append("shards used different source manifests")
    for name, digest in manifests[0].items():
        if hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != digest:
            errors.append("stale mutation source: " + name)
    if errors:
        raise ValueError("\n".join(errors))
    outcomes = []
    for data in results:
        for outcome in data["outcomes"]:
            if outcome["scenario"] != "Baseline":
                mutant = outcome["scenario"]["Mutant"]
                outcomes.append({"name": mutant["name"], "file": mutant["file"], "summary": outcome["summary"]})
    summary = {
        "acceptance": True,
        "implementation_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "cargo_mutants_version": sorted({r["cargo_mutants_version"] for r in results}),
        "shards": SHARD_COUNT,
        "generated": len(outcomes),
        "counts": dict(sorted(Counter(o["summary"] for o in outcomes).items())),
        "started_utc": min(r["start_time"] for r in results),
        "finished_utc": max(r["end_time"] for r in results),
        "source_sha256": manifests[0],
        "raw_outcomes_sha256": raw_hashes,
        "outcomes": sorted(outcomes, key=lambda o: o["name"]),
    }
    (directory / "acceptance.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: summary[k] for k in ["acceptance", "generated", "counts"]}))


def main():
    parser = argparse.ArgumentParser(__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--output", type=Path)
    mode.add_argument("--aggregate", type=Path)
    parser.add_argument("--shard")
    parser.add_argument("--review", type=Path, default=ROOT / "conformance/mutations/m3/review.json")
    parser.add_argument("--inventory", type=Path, default=ROOT / "conformance/mutations/m3/inventory.json")
    args = parser.parse_args()
    if args.aggregate:
        aggregate(args.aggregate, args.review, args.inventory)
        return 0
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # Keep the full live chunk harness practical under concurrent mutations,
    # without retaining large debug-symbol caches in each isolated build.
    environment = os.environ.copy()
    environment.update({
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "CARGO_PROFILE_TEST_OPT_LEVEL": "1",
    })
    (output / "test-profile.json").write_text(json.dumps({
        key: environment[key] for key in ["CARGO_PROFILE_DEV_DEBUG",
            "CARGO_PROFILE_TEST_DEBUG", "CARGO_PROFILE_TEST_OPT_LEVEL"]
    }, indent=2) + "\n")
    with tempfile.TemporaryDirectory(prefix="genui-mutations-") as directory:
        source = Path(directory)
        paths = subprocess.check_output([
            "git", "ls-files", "-z", "--", "Cargo.toml", "Cargo.lock", "crates",
            "conformance", "examples", "spec", ".cargo",
        ], cwd=ROOT, text=True).split("\0")
        hashes = {}
        for name in sorted(set(paths) - {""}):
            path = ROOT / name
            if not path.is_file():
                continue
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            if name.startswith("crates/genui-core/src/") and name.endswith(".rs"):
                hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        (output / "source.json").write_text(json.dumps(hashes, indent=2) + "\n")
        subprocess.run(["cargo", "test", "--workspace", "--locked"], cwd=source, env=environment, check=True)
        command = ["cargo", "mutants", "--dir", str(source), "-p", "genui-core",
                   "-f", "crates/genui-core/src/syntax.rs", "-f", "crates/genui-core/src/syntax/**",
                   "-f", "crates/genui-core/src/ir.rs",
                   "-f", "crates/genui-core/src/patch.rs", "-f", "crates/genui-core/src/session.rs",
                   "-f", "crates/genui-core/src/canonical.rs",
                   "-f", "crates/genui-core/src/validate/**",
                   "-f", "crates/genui-core/src/diagnostics.rs",
                   "-f", "crates/genui-core/src/arena.rs",
                   "-f", "crates/genui-core/src/persistent.rs",
                   "-f", "crates/genui-core/src/maintenance.rs",
                   "--test-workspace", "true", "--jobs", "2",
                   "--timeout", "60", "--build-timeout", "300", "--output", str(output)]
        if args.shard:
            command.extend(["--shard", args.shard])
        result = subprocess.run(command, cwd=ROOT, env=environment, check=False)
    outcomes_file = output / "mutants.out/outcomes.json"
    mutants_file = output / "mutants.out/mutants.json"
    if not outcomes_file.is_file() or not mutants_file.is_file():
        return result.returncode or 1
    review = json.loads(args.review.read_text()) if args.review.is_file() else {}
    errors = review_outcomes(json.loads(outcomes_file.read_text()),
                             json.loads(mutants_file.read_text()), hashes, review)
    (output / "review-verdict.json").write_text(json.dumps({"errors": errors}, indent=2) + "\n")
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
