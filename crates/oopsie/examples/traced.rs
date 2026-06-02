#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples print to demonstrate library output"
)]
//! Capture a backtrace automatically with `traced` and render it via `Report`.
//!
//! Run with: `cargo run --example traced`

use oopsie::{Report, RustBacktrace, oopsie, set_rust_backtrace_override};

// With `#[oopsie(traced)]` the backtrace and spantrace are stored together in
// a single `Box<(Backtrace, SpanTrace)>` field (the default `packed` layout).
#[oopsie(traced)]
#[oopsie("failed to load layer {index}")]
pub struct LoadError {
    index: u32,
}

fn load(index: u32) -> Result<(), LoadError> {
    LoadOopsie { index }.fail()
}

fn deep_call(index: u32) -> Result<(), LoadError> {
    load(index)
}

fn main() {
    // Capture is gated on RUST_BACKTRACE; force it on so the example always
    // shows frames regardless of the ambient environment.
    set_rust_backtrace_override(RustBacktrace::Enabled);

    let err = deep_call(3).unwrap_err();
    print!("{}", Report::from_std(err));
}
