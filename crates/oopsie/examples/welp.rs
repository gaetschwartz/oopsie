//! `Welp` — a string-shaped escape hatch when a structured error is overkill.
//!
//! Run with: `cargo run --example welp`

use oopsie::Welp;
use oopsie::prelude::*;

fn read_config(path: &str) -> Result<String, Welp> {
    std::fs::read_to_string(path).welp_context("could not read config")
}

fn parse_port(value: &str) -> Result<u16, Welp> {
    value
        .trim()
        .parse::<u16>()
        .with_welp_context(|err| format!("invalid port {value:?}: {err}"))
}

fn validate(port: u16) -> Result<(), Welp> {
    if port < 1024 {
        return Err(Welp::new(format!("port {port} is privileged")));
    }
    Ok(())
}

fn main() {
    // `welp_context` wraps the underlying error as a source.
    if let Err(err) = read_config("/nonexistent/app.toml") {
        println!("{err}");
        println!("{err:?}");
    }

    if let Err(err) = parse_port("65536") {
        println!("{err}");
    }

    // `Welp::new` builds a fresh message error with no source.
    if let Err(err) = validate(80) {
        println!("{err}");
    }
}
