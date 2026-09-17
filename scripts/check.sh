#!/usr/bin/env bash
# Shared by just check, just release and CI. Run from the repository root.
set -euo pipefail

# Explicitly select the repository pin even if the shell overrides rustup.
RUSTUP_TOOLCHAIN=$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')
export RUSTUP_TOOLCHAIN
rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --component clippy,rustfmt --no-self-update
cargo check --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --doc
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps

msrv=$(python3 -c 'import tomllib; v = tomllib.load(open("Cargo.toml", "rb"))["package"]["rust-version"]; print(v + ".0" if v.count(".") == 1 else v)')
rustup toolchain install "$msrv" --profile minimal --no-self-update
cargo +"$msrv" test --locked --all-targets
cargo +"$msrv" test --locked --doc
python3 tests/release_recipe.py
