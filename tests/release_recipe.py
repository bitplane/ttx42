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
if command.startswith('git rev-parse'):
    sys.exit(1)
if command.startswith('cargo check'):
    lock = pathlib.Path('Cargo.lock')
    lock.write_text(re.sub(r'^version = ".*?"', 'version = "' + version + '"', lock.read_text(), flags=re.M))
"""


class ReleaseRecipe(unittest.TestCase):
    def exercise(self, failure=""):
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
            recipe = textwrap.dedent((ROOT / "justfile").read_text().split("release:\n", 1)[1])
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
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                commands = [command for command, _ in calls]
                validation = next(i for i, command in enumerate(commands) if command.startswith("cargo publish"))
                commit = next(i for i, command in enumerate(commands) if command.startswith("git commit"))
                tag = next(i for i, command in enumerate(commands) if command.startswith("git tag"))
                self.assertLess(validation, commit)
                self.assertLess(commit, tag)
                old_version = re.search(r'^version = "(.*?)"', originals["Cargo.toml"], re.M)[1]
                major, minor, patch = map(int, old_version.split("."))
                new_version = f"{major}.{minor}.{patch + 1}"
                for command, version in calls:
                    if command.startswith(("cargo fmt", "cargo clippy", "cargo test")):
                        self.assertEqual(version, old_version)
                    if command.startswith("cargo publish"):
                        self.assertEqual(version, new_version)

    def test_preflight_failure_leaves_versions_unchanged(self):
        self.exercise("cargo test")

    def test_package_failure_restores_versions(self):
        self.exercise("cargo publish")

    def test_commit_failure_restores_versions(self):
        self.exercise("git commit")

    def test_success_validates_before_committing_and_tagging(self):
        self.exercise()


if __name__ == "__main__":
    unittest.main()
