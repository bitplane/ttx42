#!/usr/bin/env bash
set -euo pipefail

level=${1-patch}
status=$(git status --porcelain)
if [[ -n "$status" ]]; then
    echo "error: worktree must be clean before releasing" >&2
    exit 1
fi
current=$(sed -n 's/^version = "\([0-9][0-9]*\)\.\([0-9][0-9]*\)\.\([0-9][0-9]*\)"$/\1.\2.\3/p' Cargo.toml | head -n1)
if [[ -z "$current" ]]; then
    echo "error: could not read the package version from Cargo.toml" >&2
    exit 1
fi
version=$(python3 - "$current" "$level" <<'PY_VERSION'
import re
import sys

current, level = sys.argv[1:]
old = tuple(map(int, current.split('.')))
major, minor, patch = old
if level == 'patch':
    new = (major, minor, patch + 1)
elif level == 'minor':
    new = (major, minor + 1, 0)
elif level == 'major':
    new = (major + 1, 0, 0)
elif re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', level):
    new = tuple(map(int, level.split('.')))
else:
    sys.exit('error: release expects patch, minor, major, or X.Y.Z')
if new <= old:
    sys.exit('error: release version must be greater than ' + current)
print('.'.join(map(str, new)))
PY_VERSION
)
tag="v$version"
if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
fi
source scripts/check.sh
# Keep failed package validation or commits from leaving a version bump.
backup=$(mktemp -d)
cp Cargo.toml Cargo.lock "$backup/"
rollback=true
cleanup() {
    result=$?
    if [[ "$rollback" == true ]]; then
        cp "$backup/Cargo.toml" Cargo.toml
        cp "$backup/Cargo.lock" Cargo.lock
        git reset --quiet -- Cargo.toml Cargo.lock
    fi
    rm -rf "$backup"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
sed -i "0,/^version = \"$current\"$/s//version = \"$version\"/" Cargo.toml
cargo check
cargo publish --dry-run --allow-dirty --locked
git add Cargo.toml Cargo.lock
git commit -m "release $version"
rollback=false
if ! bash scripts/finish-release.sh; then
    echo "error: release commit retained; fix the error and run just release-resume" >&2
    exit 1
fi

