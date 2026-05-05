# Run cargo check for both stable and nightly toolchains
check:
    cargo check
    cargo check --features unstable

# Run tests for both stable and nightly toolchains
test *ARGS:
    cargo nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}
