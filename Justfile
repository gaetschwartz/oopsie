# Powerset sweeps for the *-full recipes. Stable excludes every nightly-only
# feature (can't compile them) plus the empty `default`; nightly instead pins
# `unstable` on for all combos, dropping the sub-features it already implies.
stable_powerset := "--feature-powerset --exclude-features default,unstable,unstable-error-generic-member-access,unstable-try-trait-v2"
nightly_powerset := "--feature-powerset --exclude-features default,unstable-error-generic-member-access,unstable-try-trait-v2 -F unstable"

# Run `cargo <cmd>` on both stable and nightly. First arg "1" sweeps the feature
# powerset of every workspace package; "0" runs default features only.
[positional-arguments]
_cargo full *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    shift
    if [ "{{ full }}" = "1" ]; then
        cargo +stable hack {{ stable_powerset }} "$@"
        cargo hack {{ nightly_powerset }} "$@"
    else
        cargo +stable "$@"
        cargo "$@"
    fi

# Type-check the whole feature powerset on stable + nightly.
check-full: (_cargo "1" "check" "--workspace")
# Type-check default features on stable + nightly.
check: (_cargo "0" "check" "--workspace")

# Lint the whole feature powerset on stable + nightly, denying warnings.
clippy-full: (_cargo "1" "clippy" "--workspace" "--all-targets" "--" "-D" "warnings")
# Lint default features on stable + nightly, denying warnings.
clippy: (_cargo "0" "clippy" "--workspace" "--all-targets" "--" "-D" "warnings")

# Run doctests across the whole feature powerset on stable + nightly.
doctest-full: (_cargo "1" "test" "--doc" "--workspace")
# Run doctests for default features on stable + nightly.
doctest: (_cargo "0" "test" "--doc" "--workspace")

# The snapshot-bearing combos: stable + nightly channels, crossed with serde
# on/off because serde swaps the spantrace field formatter and so the rendered
# snapshots. The env-gated backtrace snapshot tests run here and nowhere else.
_nextest-snapshots *ARGS:
    OOPSIE_SETTINGS_E2E=1 OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo +stable nextest run --workspace --no-default-features --features fancy,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo nextest run --workspace --no-default-features --features unstable,fancy,tracing,chrono {{ ARGS }}

# Run the snapshot-bearing combos — the meaningful everyday test.
nextest *ARGS: (_nextest-snapshots ARGS)

# Run the feature powerset (snapshots skip), then the snapshot-bearing combos.
nextest-full *ARGS: (_nextest-snapshots ARGS)
    cargo +stable hack {{ stable_powerset }} nextest run --workspace {{ ARGS }}
    cargo hack {{ nightly_powerset }} nextest run --workspace {{ ARGS }}

# Run the snapshot-bearing test combos plus doctests.
test *ARGS: (nextest ARGS) doctest

# Run the whole feature powerset, then doctests for every combo.
test-full *ARGS: (nextest-full ARGS) doctest-full

# Re-bless insta snapshots + trybuild stderr on both blessed combos.
[env("OOPSIE_BACKTRACE_SNAPSHOT_TESTS", "1")]
[env("INSTA_UPDATE", "always")]
[env("TRYBUILD", "overwrite")]
test-bless *ARGS:
    OOPSIE_SETTINGS_E2E=1 OOPSIE_SETTINGS_BLESS=1 cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
    cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
    cargo +stable nextest run --workspace --no-default-features --features fancy,tracing,chrono --no-fail-fast {{ ARGS }} || true
    cargo nextest run --workspace --no-default-features --features unstable,fancy,tracing,chrono --no-fail-fast {{ ARGS }} || true

# Build the runtime crates for no_std (host + bare-metal) and assert fancy+no_std is rejected.
nostd:
    cargo build -p oopsie-core --no-default-features
    cargo build -p oopsie-core --no-default-features --features serde
    cargo build -p oopsie --no-default-features
    cargo build --manifest-path crates/nostd-smoke/Cargo.toml --target thumbv7em-none-eabihf
    ! cargo build -p oopsie --no-default-features --features fancy 2>/dev/null
