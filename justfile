# Run cargo check for both stable and nightly toolchains.
# `+stable` overrides the rust-toolchain.toml pin so the no-feature build
# matches what CI's `Test (stable)` job runs; the feature build needs
# nightly so it uses the pinned channel.
check:
    cargo +stable check
    cargo check --features unstable

# Run doctests for all workspace crates.
doctest:
    cargo +stable test --doc --workspace
    cargo test --doc --workspace --features unstable

# Run tests for both stable and nightly toolchains.
test *ARGS: doctest
    cargo +stable nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}

# Re-run the full test suite, overwriting trybuild stderr and insta snapshots
test-bless *ARGS:
    TRYBUILD=overwrite INSTA_UPDATE=always just test {{ ARGS }}
