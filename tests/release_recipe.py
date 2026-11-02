"""Exercise release ordering and rollback without publishing or changing git."""

import json
import os
from pathlib import Path
import re
import shutil
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
if command.startswith('cargo '):
    expected = re.search(r'^channel = "(.*?)"', pathlib.Path('rust-toolchain.toml').read_text(), re.M)[1]
    if os.environ.get('RUSTUP_TOOLCHAIN') != expected:
        sys.exit('release must use the repository toolchain pin')
if command.startswith('cargo doc') and os.environ.get('RUSTDOCFLAGS') != '-D warnings':
    sys.exit('documentation warnings must fail the release')
if command.startswith('git rev-parse'):
    sys.exit(1)
if command.startswith('git symbolic-ref'):
    print('main')
if command.startswith('git log'):
    print('release ' + version)
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
            for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]:
                originals[name] = (ROOT / name).read_text()
                (root / name).write_text(originals[name])
            (root / "scripts").mkdir()
            shutil.copyfile(ROOT / "scripts/check.sh", root / "scripts/check.sh")
            shutil.copyfile(ROOT / "scripts/finish-release.sh", root / "scripts/finish-release.sh")
            (root / "tests").mkdir()
            # The nested release-test invocation is a subprocess boundary,
            # not another execution of this test suite inside its own mocks.
            (root / "tests/release_recipe.py").write_text(
                "import os, sys\nsys.exit(os.environ.get('FAIL_COMMAND') == 'release tests')\n"
            )
            for tool in ["cargo", "git", "rustup"]:
                executable = root / tool
                executable.write_text(MOCK)
                executable.chmod(0o755)
            recipe = release_body((ROOT / "justfile").read_text() + suffix)
            result = subprocess.run(
                ["bash", "-c", recipe], cwd=root, capture_output=True, text=True,
                env={**os.environ, "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                     "FAIL_COMMAND": failure, "RUSTUP_TOOLCHAIN": "wrong-toolchain"},
            )
            calls = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
            if failure:
                self.assertNotEqual(result.returncode, 0)
                if failure in ("git tag", "git push"):
                    self.assertIn("just release-resume", result.stderr)
                    self.assertNotEqual((root / "Cargo.toml").read_text(), originals["Cargo.toml"])
                    self.assertFalse(any(command.startswith("git reset") for command, _ in calls))
                    return
                for name, original in originals.items():
                    self.assertEqual((root / name).read_text(), original)
                self.assertFalse(any(command.startswith(("git tag", "git push")) for command, _ in calls))
                if failure not in ("cargo publish", "git commit"):
                    self.assertFalse(any(command.startswith(("cargo publish", "git commit")) for command, _ in calls))
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                commands = [command for command, _ in calls]
                validation = next(i for i, command in enumerate(commands) if command.startswith("cargo publish"))
                commit = next(i for i, command in enumerate(commands) if command.startswith("git commit"))
                tag = next(i for i, command in enumerate(commands) if command.startswith("git tag"))
                self.assertLess(validation, commit)
                self.assertLess(commit, tag)
                for check in ["cargo clippy --locked --all-targets -- -D warnings",
                              "cargo test --locked --doc", "cargo doc --locked --no-deps",
                              "cargo +1.88.0 test --locked --all-targets",
                              "cargo +1.88.0 test --locked --doc"]:
                    self.assertIn(check, commands)
                    self.assertLess(commands.index(check), validation)
                old_version = re.search(r'^version = "(.*?)"', originals["Cargo.toml"], re.M)[1]
                major, minor, patch = map(int, old_version.split("."))
                new_version = f"{major}.{minor}.{patch + 1}"
                tag_name = f"v{new_version}"
                self.assertEqual(
                    [command for command in commands if command.startswith("git push")],
                    [f"git push --atomic origin HEAD:refs/heads/main refs/tags/{tag_name}:refs/tags/{tag_name}"],
                )
                for command, version in calls:
                    if command.startswith(("cargo fmt", "cargo clippy", "cargo test", "cargo doc", "cargo +")):
                        self.assertEqual(version, old_version)
                    if command.startswith("cargo publish"):
                        self.assertEqual(version, new_version)

    def test_preflight_failure_leaves_versions_unchanged(self):
        self.exercise("cargo test")

    def test_doctest_failure_stops_before_version_bump(self):
        self.exercise("cargo test --locked --doc")

    def test_documentation_failure_stops_before_version_bump(self):
        self.exercise("cargo doc")

    def test_clippy_failure_stops_before_version_bump(self):
        self.exercise("cargo clippy")

    def test_msrv_failure_stops_before_version_bump(self):
        self.exercise("cargo +1.88.0 test")

    def test_release_tests_failure_stops_before_version_bump(self):
        self.exercise("release tests")

    def test_package_failure_restores_versions(self):
        self.exercise("cargo publish")

    def test_commit_failure_restores_versions(self):
        self.exercise("git commit")

    def test_tag_failure_keeps_release_commit_for_retry(self):
        self.exercise("git tag")

    def test_push_failure_keeps_release_commit_for_retry(self):
        self.exercise("git push")

    def test_success_validates_before_committing_and_tagging(self):
        self.exercise()

    def test_following_recipe_is_not_executed(self):
        self.exercise(suffix="\nother:\n    exit 99\n")


class ReleasePush(unittest.TestCase):
    """Use small local repositories to verify actual Git transaction behavior."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="ttx42-release-push-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "work"
        self.root.mkdir()
        self.remote = Path(self.directory.name) / "origin.git"
        # Git hooks and callers can export paths to a real repository/index.
        # Keep all Git operations confined to these disposable repositories.
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
        self.git("init", "--initial-branch=main")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release@example.invalid")
        self.git("init", "--bare", str(self.remote))
        self.git("remote", "add", "origin", str(self.remote))
        (self.root / "Cargo.toml").write_text('[package]\nversion = "1.0.0"\n')
        self.git("add", "Cargo.toml")
        self.git("commit", "-m", "initial")
        self.initial = self.git("rev-parse", "HEAD").stdout.strip()
        self.git("push", "origin", "main")
        (self.root / "Cargo.toml").write_text('[package]\nversion = "1.0.1"\n')
        self.git("commit", "-am", "release 1.0.1")
        self.release = self.git("rev-parse", "HEAD").stdout.strip()

    def git(self, *args, check=True):
        return subprocess.run(["git", *args], cwd=self.root, env=self.env,
                              text=True, capture_output=True, check=check)

    def finish(self):
        return subprocess.run(["bash", str(ROOT / "scripts/finish-release.sh")],
                              cwd=self.root, env=self.env, text=True, capture_output=True)

    def remote_ref(self, ref):
        result = self.git("--git-dir", str(self.remote), "rev-parse", "--verify", ref, check=False)
        return result.stdout.strip() if result.returncode == 0 else None

    def test_rejected_ref_changes_neither_remote_ref_and_retry_reuses_commit(self):
        for rejected in ["refs/tags/v1.0.1", "refs/heads/main"]:
            with self.subTest(rejected=rejected):
                hook = self.remote / "hooks/update"
                hook.write_text(f'#!/bin/sh\n[ "$1" != "{rejected}" ]\n')
                hook.chmod(0o755)
                result = self.finish()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.remote_ref("refs/heads/main"), self.initial)
                self.assertIsNone(self.remote_ref("refs/tags/v1.0.1"))
                self.assertEqual(self.git("rev-parse", "HEAD").stdout.strip(), self.release)
                self.assertEqual(self.git("rev-parse", "v1.0.1^{commit}").stdout.strip(), self.release)
        hook.unlink()
        result = self.finish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.remote_ref("refs/heads/main"), self.release)
        self.assertEqual(self.remote_ref("refs/tags/v1.0.1^{commit}"), self.release)
        self.assertEqual(self.finish().returncode, 0)  # Retrying after success is harmless.

    def test_retry_creates_tag_if_tagging_previously_failed(self):
        self.assertNotEqual(self.git("show-ref", "--verify", "refs/tags/v1.0.1", check=False).returncode, 0)
        result = self.finish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.remote_ref("refs/tags/v1.0.1^{commit}"), self.release)

    def test_wrong_tag_target_is_rejected(self):
        self.git("tag", "v1.0.1", self.initial)
        result = self.finish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not point to HEAD", result.stderr)
        self.assertEqual(self.remote_ref("refs/heads/main"), self.initial)
        self.assertIsNone(self.remote_ref("refs/tags/v1.0.1"))

    def test_dirty_tree_detached_head_and_non_release_commit_are_rejected(self):
        (self.root / "untracked").write_text("dirty")
        self.assertIn("worktree must be clean", self.finish().stderr)
        (self.root / "untracked").unlink()
        self.git("checkout", "--detach")
        self.assertIn("must be pushed from a branch", self.finish().stderr)
        self.git("checkout", "main")
        self.git("commit", "--allow-empty", "-m", "not a release")
        self.assertIn("HEAD must be the release", self.finish().stderr)
        self.assertEqual(self.remote_ref("refs/heads/main"), self.initial)


if __name__ == "__main__":
    unittest.main()
