# Run cargo check for both stable and nightly toolchains.
# `+stable` overrides the rust-toolchain.toml pin so the no-feature build
# matches what CI's `Test (stable)` job runs; the feature build needs
# nightly so it uses the pinned channel.
check:
    cargo +stable check
    cargo check --features unstable
    cargo +stable check -p oopsie --no-default-features --lib
    cargo +stable check -p oopsie-core --no-default-features --lib

# Run clippy exactly as CI's Lints job does: deny every warning across the
# whole workspace and all targets, for the default-features and
# unstable-features builds. Uses the pinned toolchain, same as CI.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --features unstable -- -D warnings

# Run doctests for all workspace crates.
doctest:
    cargo +stable test --doc --workspace
    cargo test --doc --workspace --features unstable

# Run tests for both stable and nightly toolchains.
nextest *ARGS:
    cargo +stable nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}

test *ARGS: (nextest ARGS) doctest

# Re-run the full test suite, overwriting trybuild stderr (stable-only,
# since nightly diagnostics use wider span underlines that don't match
# stable's renderer) and insta snapshots.
test-bless *ARGS:
    INSTA_UPDATE=always TRYBUILD=overwrite cargo +stable nextest run --no-fail-fast {{ ARGS }} || true
    INSTA_UPDATE=always cargo nextest run --features unstable --no-fail-fast {{ ARGS }} || true
