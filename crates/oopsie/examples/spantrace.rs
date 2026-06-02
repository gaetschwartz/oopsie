#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples print to demonstrate library output"
)]
//! Capture a `tracing` span trace alongside a `traced` error.
//!
//! Run with: `cargo run --example spantrace`

use oopsie::{Report, RustBacktrace, oopsie, set_rust_backtrace_override};
use tracing::instrument;
use tracing_subscriber::prelude::*;

#[oopsie(traced)]
#[oopsie("database query failed: {query}")]
pub struct QueryError {
    query: String,
}

#[instrument]
fn run_query(query: &str) -> Result<(), QueryError> {
    QueryOopsie { query }.fail()
}

#[instrument]
fn handle_request(user: &str) -> Result<(), QueryError> {
    run_query(&format!("SELECT * FROM sessions WHERE user = '{user}'"))
}

fn main() {
    // The span trace is collected by the `ErrorLayer` installed here.
    tracing_subscriber::registry()
        .with(oopsie::tracing::json_error_layer())
        .init();

    // Disable the backtrace so the rendered output is just the span trace.
    set_rust_backtrace_override(RustBacktrace::Disabled);

    let err = handle_request("alice").unwrap_err();
    print!("{}", Report::from_std(err));
}
