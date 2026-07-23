//! Install oopsie's custom panic hook and trigger a panic.
//!
//! The hook renders the panic message, a span trace (when a `tracing`
//! subscriber with an `ErrorLayer` is installed), and a backtrace — all through
//! the same colored machinery as [`oopsie::Report`], with no `color_eyre`
//! dependency.
//!
//! Run with: `cargo run --example panic_hook -F fancy,tracing,serde`
//! With a backtrace: `RUST_BACKTRACE=1 cargo run --example panic_hook -F fancy,tracing,serde`

use oopsie::install_panic_hook;
use tracing::instrument;
use tracing_subscriber::prelude::*;

#[instrument]
fn compute_checksum(path: &str) -> u32 {
    parse_header(path)
}

#[instrument]
fn parse_header(path: &str) -> u32 {
    panic!("unexpected end of file while reading {path}");
}

fn main() {
    // The span trace is collected by the `ErrorLayer` installed here; without
    // it the hook simply omits the SPANTRACE section.
    tracing_subscriber::registry()
        .with(oopsie::tracing::json_error_layer())
        .init();

    // Replace the default panic hook with oopsie's colored one.
    install_panic_hook();

    compute_checksum("/tmp/archive.bin");
}
