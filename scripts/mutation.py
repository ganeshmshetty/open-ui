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


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--output", type=Path, required=True)
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
        command = ["cargo", "mutants", "--dir", str(source), "-p", "genui-core",
                   "-f", "crates/genui-core/src/syntax.rs", "-f", "crates/genui-core/src/syntax/**",
                   "-f", "crates/genui-core/src/ir.rs", "--test-workspace", "true", "--jobs", "2",
                   "--timeout", "60", "--build-timeout", "300", "--output", str(output)]
        result = subprocess.run(command, cwd=ROOT, check=False)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
