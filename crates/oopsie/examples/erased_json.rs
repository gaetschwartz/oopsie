#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples print to demonstrate library output"
)]
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

//! Convert a live error into a serializable `ErasedError` and emit it as JSON.
//!
//! Run with: `cargo run --example erased_json -p oopsie --features serde`

use oopsie::erased::ErasedError;
use oopsie::{Contextual as _, RustBacktrace, oopsie, set_rust_backtrace_override};

#[oopsie(traced)]
#[oopsie("upstream service {service} failed with status {status}")]
#[oopsie(
    code = "service::upstream",
    help = "retry with backoff or check the upstream health endpoint"
)]
pub struct UpstreamError {
    service: String,
    status: u16,
    source: std::io::Error,
}

fn call_upstream() -> Result<(), UpstreamError> {
    let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
    Err(UpstreamOopsie {
        service: "billing",
        status: 503u16,
    }
    .build_error(io))
}

fn main() {
    set_rust_backtrace_override(RustBacktrace::Enabled);

    let err = call_upstream().unwrap_err();

    // `ErasedError` flattens message, source chain, code/help, span trace, and
    // backtrace into a `Serialize` value usable in API responses or logs.
    let erased = ErasedError::from_error(err);
    let json = serde_json::to_string_pretty(&erased).expect("serialize ErasedError");
    println!("{json}");
}
