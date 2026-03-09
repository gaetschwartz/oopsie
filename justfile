# Run cargo check for both stable and nightly toolchains
check:
    cargo check
    cargo check --features unstable

# Run tests for both stable and nightly toolchains
test:
    cargo nextest run
    cargo nextest run --features unstable
