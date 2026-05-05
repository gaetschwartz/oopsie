# Run cargo check for both stable and nightly toolchains
check:
    cargo check
    cargo check --features unstable

# Run doctests for all workspace crates
doctest:
    cargo test --doc --workspace
    cargo test --doc --workspace --features unstable

# Run tests for both stable and nightly toolchains
test *ARGS: doctest
    cargo nextest run {{ ARGS }}
    cargo nextest run --features unstable {{ ARGS }}
