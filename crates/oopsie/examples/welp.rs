//! `Welp` — a string-shaped escape hatch when a structured error is overkill,
//! and `Report<Welp>` as the catch-all `main` story.
//!
//! Run with: `cargo run --example welp` (exits non-zero on error).

use oopsie::prelude::*;

fn read_config(path: &str) -> Result<String, Welp> {
    // `.welp()?` converts the foreign `io::Error` with no invented message —
    // the kind of `?`-style conversion anyhow/eyre give you for free.
    let raw = std::fs::read_to_string(path).welp()?;
    Ok(raw)
}

fn parse_port(value: &str) -> Result<u16, Welp> {
    // `.welp_context(...)` instead, when a message adds information the source
    // doesn't already carry.
    value
        .trim()
        .parse::<u16>()
        .with_welp_context(|err| format!("invalid port {value:?}: {err}"))
}

fn validate(port: u16) -> Result<(), Welp> {
    if port < 1024 {
        // `Welp::new` builds a fresh message error with no source.
        return Err(Welp::new(format!("port {port} is privileged")));
    }
    Ok(())
}

fn run() -> Result<(), Welp> {
    let raw = read_config("/nonexistent/app.toml")?;
    let port = parse_port(&raw)?;
    validate(port)?;
    Ok(())
}

// `Report<Welp>` is the blessed catch-all: any function in the program can
// return `Result<_, Welp>` and bubble foreign errors up with `.welp()?`.
fn main() -> Report<Welp> {
    Report::run(run)
}
