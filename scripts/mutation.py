#!/usr/bin/env python3
"""Run the milestone-only syntax/IR gate without copying research or mutating the checkout."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


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
        elif status not in {"CaughtMutant", "Unviable", "Timeout"}:
            errors.append("unexpected mutant outcome: " + status)
    return errors


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--review", type=Path, default=ROOT / "conformance/mutations/review.json")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
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
            if name.startswith("crates/genui-core/src/syntax") or name == "crates/genui-core/src/ir.rs":
                hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        (output / "source.json").write_text(json.dumps(hashes, indent=2) + "\n")
        subprocess.run(["cargo", "test", "--workspace", "--locked"], cwd=source, check=True)
        command = ["cargo", "mutants", "--dir", str(source), "-p", "genui-core",
                   "-f", "crates/genui-core/src/syntax.rs", "-f", "crates/genui-core/src/syntax/**",
                   "-f", "crates/genui-core/src/ir.rs", "--test-workspace", "true", "--jobs", "2",
                   "--timeout", "60", "--build-timeout", "300", "--output", str(output)]
        result = subprocess.run(command, cwd=ROOT, check=False)
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
