#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples print to demonstrate library output"
)]
//! Define a structured error enum and attach context as errors propagate.
//!
//! Run with: `cargo run --example quickstart`

use oopsie::oopsie;
use oopsie::prelude::*;

#[oopsie]
pub enum AppError {
    #[oopsie("connection to {host} failed")]
    Connect {
        host: String,
        source: std::io::Error,
    },

    #[oopsie("key not found: {key}")]
    MissingKey { key: String },
}

// `#[oopsie]` generates the `app_oopsies` module with one context selector per
// variant. `.context(..)` turns the underlying error into the chosen variant,
// filling the non-source fields from the selector.
fn connect(host: &str) -> Result<std::net::TcpStream, AppError> {
    std::net::TcpStream::connect(host).context(app_oopsies::Connect { host })
}

fn lookup(key: &str) -> Result<String, AppError> {
    None.context(app_oopsies::MissingKey { key })
}

fn main() {
    // Port 1 has nothing listening, so the io::Error becomes `Connect`.
    if let Err(err) = connect("127.0.0.1:1") {
        // `.chain()` yields the error first, then each `source()` cause.
        println!("{err}");
        for cause in err.chain().skip(1) {
            println!("  caused by: {cause}");
        }
        println!("root cause: {}", err.root_cause());
    }

    if let Err(err) = lookup("api_token") {
        println!("{err}");
    }
}
