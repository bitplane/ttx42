# Bump the minor version, verify it, commit, tag, and publish the refs.
release:
    #!/usr/bin/env bash
    set -euo pipefail
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
    IFS=. read -r major minor patch <<< "$current"
    version="$major.$((minor + 1)).0"
    tag="v$version"
    if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
        echo "error: tag $tag already exists" >&2
        exit 1
    fi
    sed -i "0,/^version = \"$current\"$/s//version = \"$version\"/" Cargo.toml
    cargo check
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test --all-targets
    cargo publish --dry-run
    git add Cargo.toml Cargo.lock
    git commit -m "release $version"
    git tag -a "$tag" -m "$tag"
    git push origin HEAD
    git push origin "$tag"
    echo "released $tag"
