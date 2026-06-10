# Run cargo check for both stable and nightly toolchains.
# `+stable` overrides the rust-toolchain.toml pin so the no-feature build
# matches what CI's `Test (stable)` job runs; the feature build needs
# nightly so it uses the pinned channel.
check:
    cargo +stable check --workspace --no-default-features
    cargo +stable check --workspace --no-default-features --features serde
    cargo +stable check --workspace --no-default-features --features tracing
    cargo +stable check --workspace --no-default-features --features serde,tracing
    cargo +stable check --workspace --no-default-features --features chrono
    cargo +stable check --workspace --no-default-features --features fancy,serde,tracing,chrono
    cargo check --workspace --features unstable,fancy,serde,tracing,chrono
    cargo +stable check -p oopsie --no-default-features --lib
    cargo +stable check -p oopsie-core --no-default-features --lib

# Run clippy across the same feature combos as CI's Lints job: deny every
# warning across the whole workspace and all targets. Uses the pinned toolchain.
clippy:
    cargo +stable clippy --workspace --all-targets --no-default-features -- -D warnings
    cargo +stable clippy --workspace --all-targets --no-default-features --features serde -- -D warnings
    cargo +stable clippy --workspace --all-targets --no-default-features --features tracing -- -D warnings
    cargo +stable clippy --workspace --all-targets --no-default-features --features serde,tracing -- -D warnings
    cargo +stable clippy --workspace --all-targets --no-default-features --features chrono -- -D warnings
    cargo +stable clippy --workspace --all-targets --no-default-features --features fancy,serde,tracing,chrono -- -D warnings
    cargo clippy --workspace --all-targets --features unstable,fancy,serde,tracing,chrono -- -D warnings

# Run doctests for all workspace crates across the same feature combos used by
# the other recipes.
doctest:
    cargo +stable test --doc --workspace --no-default-features
    cargo +stable test --doc --workspace --no-default-features --features serde
    cargo +stable test --doc --workspace --no-default-features --features tracing
    cargo +stable test --doc --workspace --no-default-features --features serde,tracing
    cargo +stable test --doc --workspace --no-default-features --features chrono
    cargo +stable test --doc --workspace --no-default-features --features fancy,serde,tracing,chrono
    cargo test --doc --workspace --features unstable,fancy,serde,tracing,chrono

# Run tests for both stable and nightly toolchains.
nextest *ARGS:
    cargo +stable nextest run --workspace --no-default-features {{ ARGS }}
    cargo +stable nextest run --workspace --no-default-features --features serde {{ ARGS }}
    cargo +stable nextest run --workspace --no-default-features --features tracing {{ ARGS }}
    cargo +stable nextest run --workspace --no-default-features --features serde,tracing {{ ARGS }}
    cargo +stable nextest run --workspace --no-default-features --features chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo +stable nextest run -p oopsie -F chrono {{ ARGS }}

test *ARGS: (nextest ARGS) doctest

# Re-run the full test suite, overwriting trybuild stderr (stable-only,
# since nightly diagnostics use wider span underlines that don't match
# stable's renderer) and insta snapshots.
test-bless *ARGS:
    cargo +stable nextest run --workspace --no-default-features --no-fail-fast {{ ARGS }} || true
    cargo +stable nextest run --workspace --no-default-features --features serde --no-fail-fast {{ ARGS }} || true
    cargo +stable nextest run --workspace --no-default-features --features tracing --no-fail-fast {{ ARGS }} || true
    cargo +stable nextest run --workspace --no-default-features --features serde,tracing --no-fail-fast {{ ARGS }} || true
    cargo +stable nextest run --workspace --no-default-features --features chrono --no-fail-fast {{ ARGS }} || true
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 INSTA_UPDATE=always TRYBUILD=overwrite cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 INSTA_UPDATE=always cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
