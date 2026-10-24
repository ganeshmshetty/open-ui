#!/usr/bin/env python3
"""SPEC §12.8 gates; inspect only project-owned Rust and tracked fixture history.

Golden protection: a modified/deleted want.* file must be paired with both
spec/SPEC.md and spec/CHANGELOG.md somewhere in the pushed range. Newly added
fixtures are tests and need no contract change (fixtures-first workflow).
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True)


def rust_masks(source):
    """Separate code and string literals while honoring nested Rust comments."""
    code, literals = [], []
    token = re.compile(r"""r(?P<hashes>\#{0,255})".*?"(?P=hashes)|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'""", re.S)
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end
            code.append(" ")
        elif source.startswith("/*", index):
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            code.append(" ")
        else:
            match = token.match(source, index)
            if match:
                literals.append(match.group())
                code.append(" ")
                index = match.end()
            else:
                code.append(source[index])
                index += 1
    return "".join(code), literals


def source_errors(path, source):
    errors = []
    code, literals = rust_masks(source)
    if re.search(r"#\s*\[\s*(?:ignore\b|cfg_attr\b[^\]]*\bignore\b)", code):
        errors.append(f"{path}: ignored tests are forbidden")
    if path.startswith("crates/genui-gpui/") and not (
        path == "crates/genui-gpui/src/theme.rs"
        or path.startswith("crates/genui-gpui/src/theme/")
    ):
        if (re.search(r"\b0x(?:[0-9a-fA-F]_*){6,8}\b", code)
                or any(re.search(r"#[0-9a-fA-F]{3,8}\b", literal) for literal in literals)):
            errors.append(f"{path}: color literal outside theme")
        if re.search(r"\b(?:px|Pixels)(?:::(?:new|from))?\s*\(\s*-?\s*\d|\b\d+(?:\.\d+)?\s*(?:_px|px|\.px\s*\()", code):
            errors.append(f"{path}: pixel literal outside theme")
    return errors


def sources(root):
    paths = git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "crates", "fuzz").split("\0")
    errors = []
    for name in sorted(set(paths)):
        path = root / name
        if path.suffix == ".rs" and path.is_file():
            errors.extend(source_errors(name, path.read_text()))
    return errors


def fixture_errors(names, commit):
    if any(Path(name).name.startswith("want.") for name in names):
        missing = {"spec/SPEC.md", "spec/CHANGELOG.md"} - set(names)
        if missing:
            return [f"{commit}: want.* changed without {', '.join(sorted(missing))}"]
    return []


def fixtures(root, base, head):
    # A golden change is accepted when the range also documents the contract in
    # both spec files. Newly added fixtures are tests, not weakened goldens, so
    # they need no spec change (AGENTS.md: fixtures first). Every commit is still
    # inspected; the documentation may land in a sibling commit of the same push.
    revision = head
    if base and set(base) != {"0"}:
        try:
            git(root, "merge-base", "--is-ancestor", base, head)
        except subprocess.CalledProcessError:
            pass
        else:
            revision = f"{base}..{head}"
    commits = git(root, "rev-list", revision).splitlines()
    per_commit = []
    touched = set()
    for commit in commits:
        parents = git(root, "rev-list", "--parents", "-n", "1", commit).split()[1:]
        if parents:
            raw = git(root, "diff", "--name-status", "--no-renames", "-z", parents[0], commit)
        else:
            raw = git(root, "diff-tree", "--root", "--no-commit-id", "--name-status", "-r", "-z", commit)
        parts = [part for part in raw.split("\0") if part]
        entries = list(zip(parts[0::2], parts[1::2]))
        per_commit.append((commit, entries))
        touched.update(name for _, name in entries)
    documented = {"spec/SPEC.md", "spec/CHANGELOG.md"} <= touched
    errors = []
    for commit, entries in per_commit:
        changed = [
            name
            for status, name in entries
            if Path(name).name.startswith("want.") and status != "A"
        ]
        if changed and not documented:
            missing = {"spec/SPEC.md", "spec/CHANGELOG.md"} - touched
            errors.append(f"{commit}: want.* changed without {', '.join(sorted(missing))}")
    return errors


def forbidden(name):
    return (name.startswith(("gpui", "tokio", "async-", "futures", "iced", "egui", "gtk", "tauri"))
            or name in {"smol", "glommio", "monoio", "embassy-executor", "eframe", "winit", "slint", "dioxus", "leptos", "genui-gpui"})


def dependencies(root):
    # cargo-deny scopes this to the core's transitive graph, across target cfgs and
    # all optional features, including core dev-dependencies.
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--format-version", "1", "--all-features", "--locked",
        "--manifest-path", str(root / "crates/genui-core/Cargo.toml"),
    ], text=True))
    names = sorted({p["name"] for p in metadata["packages"] if forbidden(p["name"])})
    # Derive exact deny entries from patterns: cargo-deny does not accept name globs.
    with tempfile.TemporaryDirectory(prefix="genui-deny-") as tmp:
        config = Path(tmp) / "deny.toml"
        config.write_text('[bans]\nmultiple-versions = "allow"\nwildcards = "allow"\ndeny = '
                          + json.dumps(names) + '\n')
        result = subprocess.run([
            "cargo", "deny", "--manifest-path", str(root / "crates/genui-core/Cargo.toml"),
            "--all-features", "--locked", "--config", str(config),
            "check", "bans",
        ], cwd=root, check=False)
    return [] if result.returncode == 0 else ["cargo-deny: core depends on a UI/async crate"]


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("gate", choices=["sources", "fixtures", "dependencies"])
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--base", default="HEAD^")
    parser.add_argument("--head", default="HEAD")
    args = parser.parse_args()
    if args.gate == "sources":
        errors = sources(args.root)
    elif args.gate == "fixtures":
        errors = fixtures(args.root, args.base, args.head)
    else:
        errors = dependencies(args.root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"PASS: quality {args.gate}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
