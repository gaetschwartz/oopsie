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

use oopsie::backtrace::set_override;
use oopsie::{Report, RustBacktrace, oopsie};

// `traced` captures a backtrace; under the `tracing` feature a span-trace is captured too.
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
    if RustBacktrace::detect_opt().is_none() {
        eprintln!(
            "Warning: RUST_BACKTRACE environment variable not set; force-enabling backtrace capture for this example. Set RUST_BACKTRACE=1 to enable by default."
        );
        set_override(RustBacktrace::Enabled);
    }

    let err = deep_call(3).unwrap_err();
    print!("{}", Report::new(err));
}
