"""Exercise release ordering and rollback without publishing or changing git."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
MOCK = """#!/usr/bin/env python3
import json, os, pathlib, re, sys
command = pathlib.Path(sys.argv[0]).name + ' ' + ' '.join(sys.argv[1:])
version = re.search(r'^version = "(.*?)"', pathlib.Path('Cargo.toml').read_text(), re.M)[1]
with open('calls.jsonl', 'a') as log:
    log.write(json.dumps([command, version]) + '\\n')
if os.environ.get('FAIL_COMMAND') and command.startswith(os.environ['FAIL_COMMAND']):
    sys.exit(1)
if command.startswith('cargo doc') and os.environ.get('RUSTDOCFLAGS') != '-D warnings':
    sys.exit('documentation warnings must fail the release')
if command.startswith('git rev-parse'):
    sys.exit(1)
if command.startswith('cargo check'):
    lock = pathlib.Path('Cargo.lock')
    lock.write_text(re.sub(r'^version = ".*?"', 'version = "' + version + '"', lock.read_text(), flags=re.M))
"""


def release_body(justfile):
    """Extract only the indented body of the release recipe."""
    lines = justfile.splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.rstrip() == "release:")
    body = []
    for line in lines[start + 1:]:
        if line.strip() and not line.startswith((" ", "\t")):
            break
        body.append(line)
    return textwrap.dedent("".join(body))


class ReleaseRecipe(unittest.TestCase):
    def exercise(self, failure="", suffix=""):
        with tempfile.TemporaryDirectory(prefix="ttx42-release-") as directory:
            root = Path(directory)
            originals = {}
            for name in ["Cargo.toml", "Cargo.lock"]:
                originals[name] = (ROOT / name).read_text()
                (root / name).write_text(originals[name])
            for tool in ["cargo", "git"]:
                executable = root / tool
                executable.write_text(MOCK)
                executable.chmod(0o755)
            recipe = release_body((ROOT / "justfile").read_text() + suffix)
            result = subprocess.run(
                ["bash", "-c", recipe], cwd=root, capture_output=True, text=True,
                env={**os.environ, "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                     "FAIL_COMMAND": failure},
            )
            calls = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
            if failure:
                self.assertNotEqual(result.returncode, 0)
                for name, original in originals.items():
                    self.assertEqual((root / name).read_text(), original)
                self.assertFalse(any(command.startswith(("git tag", "git push")) for command, _ in calls))
                if failure.startswith(("cargo test", "cargo doc")):
                    self.assertFalse(any(command.startswith(("cargo publish", "git commit")) for command, _ in calls))
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                commands = [command for command, _ in calls]
                validation = next(i for i, command in enumerate(commands) if command.startswith("cargo publish"))
                commit = next(i for i, command in enumerate(commands) if command.startswith("git commit"))
                tag = next(i for i, command in enumerate(commands) if command.startswith("git tag"))
                self.assertLess(validation, commit)
                self.assertLess(commit, tag)
                for check in ["cargo test --locked --doc", "cargo doc --locked --no-deps"]:
                    self.assertIn(check, commands)
                    self.assertLess(commands.index(check), validation)
                old_version = re.search(r'^version = "(.*?)"', originals["Cargo.toml"], re.M)[1]
                major, minor, patch = map(int, old_version.split("."))
                new_version = f"{major}.{minor}.{patch + 1}"
                for command, version in calls:
                    if command.startswith(("cargo fmt", "cargo clippy", "cargo test", "cargo doc")):
                        self.assertEqual(version, old_version)
                    if command.startswith("cargo publish"):
                        self.assertEqual(version, new_version)

    def test_preflight_failure_leaves_versions_unchanged(self):
        self.exercise("cargo test")

    def test_doctest_failure_stops_before_version_bump(self):
        self.exercise("cargo test --locked --doc")

    def test_documentation_failure_stops_before_version_bump(self):
        self.exercise("cargo doc")

    def test_package_failure_restores_versions(self):
        self.exercise("cargo publish")

    def test_commit_failure_restores_versions(self):
        self.exercise("git commit")

    def test_success_validates_before_committing_and_tagging(self):
        self.exercise()

    def test_following_recipe_is_not_executed(self):
        self.exercise(suffix="\nother:\n    exit 99\n")


if __name__ == "__main__":
    unittest.main()
