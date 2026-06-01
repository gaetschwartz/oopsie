//! Return `Report` from `main` for colored, top-level error output.
//!
//! Run with: `cargo run --example report_main` (exits non-zero on error).

use oopsie::oopsie;
use oopsie::prelude::*;
use oopsie::{Report, RustBacktrace, set_rust_backtrace_override};

#[oopsie(traced)]
pub enum CliError {
    #[oopsie("missing argument: {name}")]
    MissingArg { name: String },

    #[oopsie("could not read {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
}

fn run() -> Result<(), CliError> {
    let path = "/nonexistent/config.toml";
    std::fs::read_to_string(path).context(cli_oopsies::Read { path })?;
    Ok(())
}

fn main() -> Report<CliError> {
    set_rust_backtrace_override(RustBacktrace::Enabled);
    // `Report::run` runs the closure and captures the error; returning it from
    // `main` renders the diagnostic and sets a failing exit code via `Termination`.
    Report::run(run)
}
