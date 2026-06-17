#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "examples print to demonstrate library output"
)]
#![allow(
    clippy::result_large_err,
    reason = "example error types nest the full chain inline for illustration"
)]

//! Complete example showing `oopsie`'s full feature set in action: context selectors,
//! backtraces, span traces, and caller locations.
//!
//! Run with: `RUST_BACKTRACE=1 cargo run --example complete -F fancy,tracing`

use oopsie::{Report, ResultExt as _, RustBacktrace, Theme, oopsie};

#[oopsie(traced)]
#[oopsie("network error: {kind} while sending {request:?}")]
pub struct NetworkError {
    kind: &'static str,
    request: String,
}

#[oopsie(traced)]
pub enum QueryError {
    #[oopsie("syntax error in query: {query}")]
    Syntax { query: String },

    #[oopsie("network error while executing query: {query}")]
    ConnectionLost { query: String, source: NetworkError },
}

#[oopsie(traced)]
pub enum HandlerError {
    #[oopsie("failed to handle request for user {user}")]
    Request { user: String, source: QueryError },
}

#[cfg_attr(feature = "tracing", tracing::instrument)]
fn send_network_request(request: &str) -> Result<(), NetworkError> {
    network_oopsies::Network {
        kind: "connection reset",
        request,
    }
    .fail()
}
#[cfg_attr(feature = "tracing", tracing::instrument)]
fn run_query(query: &str) -> Result<(), QueryError> {
    let () = send_network_request(query).context(query_oopsies::ConnectionLost { query })?;
    Ok(())
}

#[cfg_attr(feature = "tracing", tracing::instrument)]
fn handle_request(user: &str) -> Result<(), HandlerError> {
    let () = run_query(&format!("SELECT * FROM sessions WHERE user = '{user}'"))
        .context(handler_oopsies::Request { user })?;
    Ok(())
}

fn main() -> Report<HandlerError> {
    setup_example();

    init_tracing();

    Report::run(|| handle_request("alice"))
}

fn init_tracing() {
    #[cfg(feature = "tracing")]
    {
        use tracing_subscriber::prelude::*;

        tracing_subscriber::registry()
            .with({
                #[cfg(feature = "serde")]
                {
                    oopsie::tracing::json_error_layer()
                }
                #[cfg(not(feature = "serde"))]
                {
                    oopsie::tracing::default_error_layer()
                }
            })
            .init();
    }
}

/// Sets up the example theme and prints a warning if backtraces are not enabled.
/// You don't need to call this in your own code.
fn setup_example() {
    if RustBacktrace::detect_opt().is_none() {
        eprintln!(
            "Info: RUST_BACKTRACE environment variable not set. \
            Set RUST_BACKTRACE=1 to enable backtraces."
        );
    }

    let theme_env = std::env::var("OOPSIE_THEME");
    let theme = match theme_env.as_deref() {
        Ok(s) => match s {
            "latte" => Some(Theme::CATPPUCCIN_LATTE),
            "frappe" => Some(Theme::CATPPUCCIN_FRAPPE),
            "macchiato" => Some(Theme::CATPPUCCIN_MACCHIATO),
            "mocha" => Some(Theme::CATPPUCCIN_MOCHA),
            "dracula" => Some(Theme::DRACULA),
            "nord" => Some(Theme::NORD),
            _ => {
                eprintln!(
                    "Warning: unrecognized OOPSIE_THEME value {s:?}; \
                        using default theme. \
                        Valid values: latte, frappe, macchiato, mocha, dracula, nord."
                );
                None
            }
        }
        .map(|t| (t, s)),
        _ => None,
    };

    if let Some((theme, name)) = theme {
        eprintln!("Using theme: {name}");
        oopsie::set_theme(theme);
    }
}
