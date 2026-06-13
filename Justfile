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

# The two snapshot-bearing combos: stable + nightly, full feature set, with the
# env-gated backtrace snapshot tests switched on. Every other combo leaves them
# skipped, so these are the only place snapshots actually run.
_nextest-snapshots *ARGS:
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono {{ ARGS }}
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono {{ ARGS }}

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

# Trybuild stderr is overwritten on stable only — nightly's wider diagnostic
# span underlines don't match the stored stable form.
# Re-bless insta snapshots + trybuild stderr for the two blessed combos.
test-bless *ARGS:
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 INSTA_UPDATE=always TRYBUILD=overwrite cargo +stable nextest run --workspace --no-default-features --features fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
    OOPSIE_BACKTRACE_SNAPSHOT_TESTS=1 INSTA_UPDATE=always cargo nextest run --workspace --features unstable,fancy,serde,tracing,chrono --no-fail-fast {{ ARGS }} || true
