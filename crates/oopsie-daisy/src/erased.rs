//! Erased error types for serializable error reporting.
//!
//! This module provides types that capture error information in a serializable
//! format, preserving the full error context including message, source chain,
//! spantrace, and backtrace.

use std::fmt::{self};
use std::io;
use std::path::PathBuf;

use color_backtrace::termcolor;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorSpec, NoColor};

use crate::fancy_report::error_backtrace_frame_filter;
use oopsie_core::ErrorCode;
use oopsie_core::Spantrace;
use oopsie_core::spantrace::SpantraceInner;

/// A serializable, cloneable error representation that preserves the full
/// error context including backtrace, spantrace, and source chain.
///
/// This type is designed for API error responses where the original error
/// cannot be directly serialized.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErasedError {
    /// Primary error message (Display representation).
    pub message: Box<str>,

    /// Error source chain (Display representation of each cause).
    #[serde(default)]
    pub source_chain: Vec<Box<str>>,

    /// Diagnostic metadata (code, severity, help, url, labels).
    pub diagnostics: Diagnostics,

    /// Serialized span trace.
    pub spantrace: Option<Spantrace>,

    /// Serialized backtrace.
    pub backtrace: Option<oopsie_core::Backtrace>,
}

impl std::error::Error for ErasedError {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<Box<str>>,
}

impl Diagnostics {
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.code.is_none() && self.help.is_none()
    }
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
    #[must_use]
    pub fn help(&self) -> Option<&str> {
        self.help.as_deref()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Extraction helpers (FancyReport-style, using Provider API)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "unstable")]
fn extract_backtrace(err: &dyn std::error::Error) -> Option<&oopsie_core::Backtrace> {
    std::error::request_ref::<oopsie_core::Backtrace>(err)
}

#[cfg(not(feature = "unstable"))]
#[allow(dead_code)]
fn extract_backtrace(_err: &dyn std::error::Error) -> Option<&oopsie_core::Backtrace> {
    None
}

#[cfg(feature = "unstable")]
fn extract_error_code(err: &dyn std::error::Error) -> Option<ErrorCode> {
    std::error::request_value::<oopsie_core::ErrorCode>(err)
        .or_else(|| std::error::request_ref::<oopsie_core::ErrorCode>(err).cloned())
}

#[cfg(not(feature = "unstable"))]
#[allow(dead_code)]
fn extract_error_code(_err: &dyn std::error::Error) -> Option<ErrorCode> {
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// ErasedError construction
// ─────────────────────────────────────────────────────────────────────────────

impl ErasedError {
    /// Create an `ErasedError` from any error implementing `std::error::Error`.
    ///
    /// This extracts the message, source chain, and (on unstable) backtrace,
    /// spantrace, and ErrorCode via the Provider API.
    #[cfg(feature = "unstable")]
    pub fn from_error<E: std::error::Error>(err: E) -> Self {
        use std::borrow::Cow;

        use oopsie_core::HelpText;

        let message = err.to_string().into();

        // Build source chain
        let source_chain = std::iter::successors(err.source(), |e| e.source())
            .map(ToString::to_string)
            .map(Box::from)
            .collect();

        // Extract from Provider API
        let code = extract_error_code(&err);
        let help = std::error::request_value::<HelpText>(&err)
            .or_else(|| std::error::request_ref::<HelpText>(&err).cloned())
            .map(|h| h.0.into());
        let diagnostics = Diagnostics { code, help };
        let spantrace = Spantrace::extract(&err).map(Cow::into_owned);
        let backtrace = extract_backtrace(&err).cloned();

        Self {
            message,
            source_chain,
            diagnostics,
            spantrace,
            backtrace,
        }
    }

    /// Create an `ErasedError` from a reference to any error implementing `std::error::Error`.
    ///
    /// Like `from_error` but takes a reference, useful when ownership cannot be transferred
    /// (e.g., in `Serialize` implementations).
    #[cfg(feature = "unstable")]
    pub fn from_error_ref<E: std::error::Error>(err: &E) -> Self {
        use std::borrow::Cow;

        use oopsie_core::HelpText;

        let message = err.to_string().into();

        // Build source chain
        let source_chain = std::iter::successors(err.source(), |e| e.source())
            .map(ToString::to_string)
            .map(Box::from)
            .collect();

        // Extract from Provider API
        let code = extract_error_code(err);
        let help = std::error::request_value::<HelpText>(err)
            .or_else(|| std::error::request_ref::<HelpText>(err).cloned())
            .map(|h| h.0.into());
        let diagnostics = Diagnostics { code, help };
        let spantrace = Spantrace::extract(err).map(Cow::into_owned);
        let backtrace = extract_backtrace(err).cloned();

        Self {
            message,
            source_chain,
            diagnostics,
            spantrace,
            backtrace,
        }
    }

    /// Create an `ErasedError` from any error implementing `std::error::Error`.
    ///
    /// On stable Rust, backtrace, spantrace, and ErrorCode extraction via
    /// Provider API is not available.
    #[cfg(not(feature = "unstable"))]
    pub fn from_error<E: std::error::Error>(err: E) -> Self {
        Self::from_error_ref(&err)
    }

    /// Create an `ErasedError` from a reference to any error.
    #[cfg(not(feature = "unstable"))]
    pub fn from_error_ref<E: std::error::Error>(err: &E) -> Self {
        let message = err.to_string().into();

        // Build source chain
        let source_chain = std::iter::successors(err.source(), |e| e.source())
            .map(|e| Box::from(e.to_string()))
            .collect();

        Self {
            message,
            source_chain,
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        }
    }

    pub fn write_json<W: io::Write>(&self, f: &mut W) -> Result<(), serde_json::Error> {
        #[derive(serde::Serialize)]
        struct PrettyFrame {
            name: Option<String>,
            filename: Option<PathBuf>,
            line: Option<u32>,
        }

        #[derive(serde::Serialize)]
        struct PrettyBacktrace {
            frames: Vec<PrettyFrame>,
        }

        #[derive(serde::Serialize)]
        struct PrettyError<'a> {
            message: &'a str,
            source_chain: &'a [Box<str>],
            diagnostics: &'a Diagnostics,
            spantrace: &'a Option<Spantrace>,
            backtrace: PrettyBacktrace,
        }

        serde_json::to_writer_pretty(
            io::BufWriter::new(f),
            &PrettyError {
                message: &self.message,
                source_chain: &self.source_chain,
                diagnostics: &self.diagnostics,
                spantrace: &self.spantrace,
                backtrace: PrettyBacktrace {
                    frames: self.backtrace.as_ref().map_or(vec![], |bt| {
                        color_backtrace::Backtrace::frames(bt)
                            .into_iter()
                            .map(|frame| PrettyFrame {
                                name: frame.name,
                                filename: frame.filename,
                                line: frame.lineno,
                            })
                            .collect()
                    }),
                },
            },
        )?;

        Ok(())
    }

    /// Write the error in a text format similar to `FancyReport`.
    pub fn write_text<W: color_backtrace::termcolor::WriteColor>(
        &self,
        f: &mut W,
    ) -> io::Result<()> {
        // Write main error header
        write!(f, "Error: ")?;
        if let Some(code) = self.diagnostics.code() {
            f.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
            write!(f, "{code}")?;
            f.reset()?;
        }
        f.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
        write!(f, "\n\n  ✕ ")?;
        f.reset()?;
        writeln!(f, "{}", self.message)?;

        // Write source chain with box-drawing characters
        let chain_len = self.source_chain.len();
        for (i, cause) in self.source_chain.iter().enumerate() {
            f.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
            let arrow = if i < chain_len - 1 {
                "├─▶"
            } else {
                "╰─▶"
            };
            write!(f, "  {arrow} ")?;
            f.reset()?;
            writeln!(f, "{cause}")?;
        }

        // Write help text if present
        if let Some(help) = self.diagnostics.help() {
            f.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(f, "\n  help: ")?;
            f.reset()?;
            writeln!(f, "{help}")?;
        }

        // Write spantrace if present
        if let Some(spantrace) = &self.spantrace {
            writeln!(f)?;
            match &spantrace.inner {
                SpantraceInner::Tracing(spantrace) => {
                    if f.supports_color() {
                        write!(f, "{}", color_spantrace::colorize(spantrace))?;
                    } else {
                        writeln!(f, "{:━^80}", " SPANTRACE ")?;
                        writeln!(f, "{spantrace}")?;
                    }
                }
                SpantraceInner::Fallback(fallback_spantrace) => {
                    writeln!(f, "{:━^80}", " SPANTRACE ")?;
                    writeln!(f, "{fallback_spantrace}")?;
                }
            }
        }

        // Write backtrace if present
        if let Some(backtrace) = &self.backtrace {
            if f.supports_color() {
                writeln!(f)?;
                let printer = color_backtrace::BacktracePrinter::new()
                    .add_frame_filter(Box::new(error_backtrace_frame_filter));
                printer.print_trace(backtrace, f)?;
            } else {
                writeln!(f, "{:━^80}", " BACKTRACE ")?;
                write!(f, "{backtrace:?}")?;
            }
        }

        Ok(())
    }

    /// Format the error as a short string without backtrace or spantrace.
    #[must_use]
    pub fn format_short(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();

        // Code
        if let Some(code) = self.diagnostics.code() {
            let _ = writeln!(out, "{code}");
            let _ = writeln!(out);
        }

        // Message
        let _ = write!(out, "  \u{00d7} {}", self.message);

        // Source chain
        let chain_len = self.source_chain.len();
        for (i, cause) in self.source_chain.iter().enumerate() {
            let arrow = if i < chain_len - 1 {
                "\u{251c}\u{2500}\u{25b6}"
            } else {
                "\u{2570}\u{2500}\u{25b6}"
            };
            let _ = write!(out, "\n  {arrow} {cause}");
        }

        // Help
        if let Some(help) = self.diagnostics.help() {
            let _ = write!(out, "\n  help: {help}");
        }

        let _ = writeln!(out);
        out
    }

    /// Write the error in HTML format.
    pub fn write_html<W: color_backtrace::termcolor::WriteColor>(
        &self,
        f: &mut W,
    ) -> io::Result<()> {
        self.write_text(f)
    }
}

struct IoToFmt<W>(W);

impl<W: fmt::Write> io::Write for IoToFmt<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        self.0.write_str(&s).map_err(io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Display for ErasedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_text(&mut NoColor::new(IoToFmt(f)))
            .map_err(|_err| fmt::Error)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use oopsie_macros::{Oopsie, oopsie};
    use tracing::instrument;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;

    use super::*;
    use oopsie_core::NewJsonErrorLayer as _;

    /// Initialize a test subscriber with ErrorLayer for spantrace support.
    /// Returns a guard that must be held for the duration of the test.
    fn init_test_subscriber() -> tracing::subscriber::DefaultGuard {
        let subscriber = tracing_subscriber::registry().with(ErrorLayer::json());
        tracing::subscriber::set_default(subscriber)
    }

    #[oopsie(path = "crate")]
    #[derive(Debug)]
    pub enum ErrorWithSpanTrace {
        #[oopsie("Inner error happened", transparent)]
        Inner { source: ErrorWithSpanTraceInner },
    }

    #[oopsie(path = "crate", display = "Error: {message}")]
    #[derive(Debug)]
    pub struct ErrorWithSpanTraceInner {
        message: String,
    }

    pub fn make_error() -> ErrorWithSpanTrace {
        let _guard = init_test_subscriber();

        // Create error within instrumented functions to capture spantrace
        #[instrument(target = "sys", fields(id = 42))]
        fn inner_function(foo: bool, name: &str) -> Result<(), ErrorWithSpanTraceInner> {
            ErrorWithSpanTraceInnerOopsie {
                message: format!("Inner function failed (foo={foo}, name={name})"),
            }
            .fail()
        }

        #[instrument(target = "controller")]
        fn outer_function(foo: bool, name: &str) -> Result<(), ErrorWithSpanTrace> {
            Ok(inner_function(foo, name)?)
        }

        outer_function(true, "Alice").expect_err("Should produce an error")
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "snapshot depends on provider API")]
    fn test_erased_error_display() {
        let error = ErasedError::from_error(make_error());
        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!(error);
        });
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "snapshot depends on provider API")]
    fn test_erased_error_json() {
        let error = ErasedError::from_error(make_error());
        insta::assert_json_snapshot!(error, {
            ".backtrace.frames" => "[..]"
        });
    }

    #[oopsie(path = "crate", display = "Something went wrong: {message}")]
    #[derive(Debug)]
    #[help("Try restarting the service")]
    pub struct ErrorWithHelp {
        message: String,
    }

    #[oopsie(path = "crate", display = "Code-only error: {message}")]
    #[derive(Debug)]
    pub struct ErrorWithCodeOnly {
        message: String,
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "snapshot depends on provider API")]
    fn test_help_extraction() {
        let error = ErrorWithHelpOopsie {
            message: "connection refused",
        }
        .build();
        let erased = ErasedError::from_error(error);

        assert_eq!(
            erased.diagnostics.help(),
            Some("Try restarting the service")
        );
        assert!(erased.diagnostics.code().is_some());
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "snapshot depends on provider API")]
    fn test_code_only_extraction() {
        let error = ErrorWithCodeOnlyOopsie { message: "timeout" }.build();
        let erased = ErasedError::from_error(error);

        assert!(erased.diagnostics.code().is_some());
        assert_eq!(erased.diagnostics.help(), None);
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "snapshot depends on provider API")]
    fn test_format_short_includes_help() {
        let error = ErrorWithHelpOopsie {
            message: "connection refused",
        }
        .build();
        let erased = ErasedError::from_error(error);
        let short = erased.format_short();

        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!("format_short_with_help", short);
        });
    }

    #[test]
    fn test_diagnostics_struct() {
        let empty = Diagnostics::default();
        assert!(empty.is_none());
        assert_eq!(empty.code(), None);
        assert_eq!(empty.help(), None);

        let with_both = Diagnostics {
            code: Some("test::code".into()),
            help: Some("try this".into()),
        };
        assert!(!with_both.is_none());
        assert_eq!(with_both.code(), Some("test::code"));
        assert_eq!(with_both.help(), Some("try this"));

        let code_only = Diagnostics {
            code: Some("test::code".into()),
            help: None,
        };
        assert!(!code_only.is_none());
        assert_eq!(code_only.help(), None);
    }
}
