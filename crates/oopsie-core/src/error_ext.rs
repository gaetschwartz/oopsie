//! Simple trait for errors that expose diagnostic data.

use crate::{BackTrace, ErrorCode, HelpText, SpanTrace};

/// Trait for errors that expose diagnostic data.
///
/// Always implemented by `#[derive(Oopsie)]`. Provides a stable mechanism
/// for extracting backtraces, span traces, error codes, and help text
/// without relying on the unstable `Provide`/`Request` API.
pub trait ErrorExt: std::error::Error {
    /// Returns the backtrace captured when this error was created.
    fn oopsie_backtrace(&self) -> Option<&BackTrace> {
        None
    }

    /// Returns the span trace captured when this error was created.
    fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
        None
    }

    /// Returns the error code associated with this error.
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        None
    }

    /// Returns the help text associated with this error.
    fn oopsie_help_text(&self) -> Option<HelpText> {
        None
    }
}
