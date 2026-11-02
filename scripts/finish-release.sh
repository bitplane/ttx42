#!/usr/bin/env bash
# Finish an already validated release commit. Run from the repository root.
set -euo pipefail

if [[ -n $(git status --porcelain) ]]; then
    echo "error: worktree must be clean before publishing release refs" >&2
    exit 1
fi
branch=$(git symbolic-ref --quiet --short HEAD) || {
    echo "error: release refs must be pushed from a branch" >&2
    exit 1
}
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
if [[ $(git log -1 --format=%s) != "release $version" ]]; then
    echo "error: HEAD must be the release $version commit; use just release for a new release" >&2
    exit 1
fi
tag="v$version"
if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
    if [[ $(git rev-parse "refs/tags/$tag^{commit}") != $(git rev-parse HEAD) ]]; then
        echo "error: tag $tag does not point to HEAD" >&2
        echo "after a rebase, rerun just check and cargo publish --dry-run --locked" >&2
        echo "check the remote tag: git ls-remote --tags origin refs/tags/$tag" >&2
        echo "if absent, run git tag -d $tag, then just release-resume" >&2
        echo "if already on origin, keep the tag and prepare a new release version" >&2
        exit 1
    fi
else
    git tag -a "$tag" -m "$tag"
fi
# Either both refs reach origin, or neither does. Do not fall back to separate
# pushes if the remote lacks atomic-push support.
git push --atomic origin "HEAD:refs/heads/$branch" "refs/tags/$tag:refs/tags/$tag"
echo "pushed release refs for $tag"
