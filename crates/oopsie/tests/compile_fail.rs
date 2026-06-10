//! Trybuild compile-fail snapshot suite.
//!
//! Most cases have toolchain-agnostic `.stderr` files and live at the top
//! level of `tests/compile-fail/`. A handful of cases produce a different
//! diagnostic span shape between stable and nightly rustc — those have their
//! own copies in `tests/compile-fail/{stable,nightly}/` with their respective
//! expected output. The `#[rustversion::*]` helpers pick the right subset
//! at compile time, so each channel's CI job runs exactly its variants.
//!
//! Cases whose expected diagnostic only fires with `tracing` enabled (anything
//! that reaches a `spantrace`-dependent error rather than the earlier
//! "requires the `tracing` feature" gate) live under a `tracing/` subdirectory
//! and are globbed only when that feature is on.

#[rustversion::stable]
fn run_channel_specific(t: &trybuild::TestCases) {
    t.compile_fail("tests/compile-fail/stable/*.rs");
    #[cfg(feature = "tracing")]
    t.compile_fail("tests/compile-fail/stable/tracing/*.rs");
}

#[rustversion::nightly]
fn run_channel_specific(t: &trybuild::TestCases) {
    t.compile_fail("tests/compile-fail/nightly/*.rs");
    #[cfg(feature = "tracing")]
    t.compile_fail("tests/compile-fail/nightly/tracing/*.rs");
}

#[rustversion::not(any(stable, nightly))]
fn run_channel_specific(_t: &trybuild::TestCases) {
    // Beta / dev / custom channels: skip; their diagnostics may match
    // neither stored form.
}

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
    run_channel_specific(&t);
}
