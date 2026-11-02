# Check, bump (patch/minor/major or X.Y.Z), commit, tag and push.
[positional-arguments]
release level="patch":
    bash scripts/release.sh "$1"

# Retry tagging/pushing the current release commit without another version bump.
release-resume:
    bash scripts/finish-release.sh

# Run the same pinned-toolchain and MSRV checks as CI.
check:
    bash scripts/check.sh
