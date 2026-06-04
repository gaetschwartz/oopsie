# Run cargo check for both stable and nightly toolchains.
# `+stable` overrides the rust-toolchain.toml pin so the no-feature build
# matches what CI's `Test (stable)` job runs; the feature build needs
# nightly so it uses the pinned channel.
check:
    cargo +stable check
    cargo check --features unstable
    cargo +stable check -p oopsie --no-default-features --lib
    cargo +stable check -p oopsie-core --no-default-features --lib

# Run doctests for all workspace crates.
doctest:
    cargo +stable test --doc --workspace
    cargo test --doc --workspace --features unstable

# Run tests for both stable and nightly toolchains.
test *ARGS: doctest
    cargo +stable nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}

# Re-run the full test suite, overwriting trybuild stderr (stable-only,
# since nightly diagnostics use wider span underlines that don't match
# stable's renderer) and insta snapshots.
[env("INSTA_UPDATE", "always")]
test-bless *ARGS:
    TRYBUILD=overwrite cargo +stable nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}
