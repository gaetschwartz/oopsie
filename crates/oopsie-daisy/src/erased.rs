//! Erased error types for serializable error reporting.
//!
//! This module provides types that capture error information in a serializable
//! format, preserving the full error context including message, source chain,
//! spantrace, and backtrace.

use std::borrow::Cow;
use std::fmt;
use std::io;
use std::path::PathBuf;

use color_backtrace::termcolor;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorSpec, NoColor};

use crate::extract_value_from_error;
use crate::fancy_report::error_backtrace_frame_filter;
use oopsie_core::SpanTrace;
use oopsie_core::spantrace::SpanTraceInner;
use oopsie_core::{ErrorCode, HelpText};

/// A serializable, cloneable error representation that preserves the full
/// error context including backtrace, spantrace, and source chain.
///
/// This type is designed for API error responses where the original error
/// cannot be directly serialized.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ErasedError {
    /// Primary error message (Display representation).
    pub message: Box<str>,

    /// Error source chain (Display representation of each cause).
    #[serde(default)]
    pub source_chain: Vec<Box<str>>,

    /// Diagnostic metadata (code, severity, help, url, labels).
    pub diagnostics: Diagnostics,

    /// Serialized span trace.
    pub spantrace: Option<SpanTrace>,

    /// Serialized backtrace.
    pub backtrace: Option<oopsie_core::BackTrace>,
}

impl std::error::Error for ErasedError {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<HelpText>,
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
// ErasedError construction
// ─────────────────────────────────────────────────────────────────────────────

impl ErasedError {
    /// Create an `ErasedError` from any error implementing `std::error::Error`.
    ///
    /// This extracts the message, source chain, and (on unstable) backtrace,
    /// spantrace, and ErrorCode via the Provider API.
    pub fn from_error<E: std::error::Error>(err: E) -> Self {
        Self::from_error_ref(&err)
    }

    /// Create an `ErasedError` from a reference to any error implementing `std::error::Error`.
    ///
    /// Like `from_error` but takes a reference, useful when ownership cannot be transferred
    /// (e.g., in `Serialize` implementations).
    pub fn from_error_ref<E: std::error::Error>(err: &E) -> Self {
        let message = err.to_string().into();

        let source_chain = std::iter::successors(err.source(), |e| e.source())
            .map(ToString::to_string)
            .map(Box::from)
            .collect();

        let diagnostics = Diagnostics {
            code: extract_value_from_error::<oopsie_core::ErrorCode>(err),
            help: extract_value_from_error::<oopsie_core::HelpText>(err),
        };
        let spantrace = oopsie_core::SpanTrace::extract_from_error(err).map(Cow::into_owned);
        let backtrace = oopsie_core::BackTrace::extract_from_error(err).map(Cow::into_owned);

        Self {
            message,
            source_chain,
            diagnostics,
            spantrace,
            backtrace,
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
            spantrace: &'a Option<SpanTrace>,
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
                SpanTraceInner::Tracing(spantrace) => {
                    if f.supports_color() {
                        write!(f, "{}", color_spantrace::colorize(spantrace))?;
                    } else {
                        writeln!(f, "{:━^80}", " SPANTRACE ")?;
                        writeln!(f, "{spantrace}")?;
                    }
                }
                SpanTraceInner::Fallback(fallback_spantrace) => {
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
        #[oopsie(display("Inner error happened"), transparent)]
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
    #[oopsie(help = "Try restarting the service")]
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

    // ─────────────────────────────────────────────────────────────────────
    // Tests for from_error / from_error_ref (kills Default::default mutants)
    // ─────────────────────────────────────────────────────────────────────

    /// A simple chained error type for testing from_error / from_error_ref.
    #[derive(Debug)]
    struct ChainedError {
        msg: &'static str,
        source: Option<Box<ChainedError>>,
    }

    impl fmt::Display for ChainedError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.msg)
        }
    }

    impl std::error::Error for ChainedError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|s| s.as_ref() as &dyn std::error::Error)
        }
    }

    #[test]
    fn test_from_error_preserves_message_and_chain() {
        let error = ChainedError {
            msg: "outer error",
            source: Some(Box::new(ChainedError {
                msg: "inner cause",
                source: None,
            })),
        };

        let erased = ErasedError::from_error(error);
        assert_eq!(&*erased.message, "outer error");
        assert_eq!(erased.source_chain.len(), 1);
        assert_eq!(&*erased.source_chain[0], "inner cause");
    }

    #[test]
    fn test_from_error_ref_preserves_message_and_chain() {
        let error = ChainedError {
            msg: "outer error",
            source: Some(Box::new(ChainedError {
                msg: "inner cause",
                source: None,
            })),
        };

        let erased = ErasedError::from_error_ref(&error);
        assert_eq!(&*erased.message, "outer error");
        assert_eq!(erased.source_chain.len(), 1);
        assert_eq!(&*erased.source_chain[0], "inner cause");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for write_json (kills Ok(()) mutant)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_json_produces_valid_json() {
        let erased = ErasedError {
            message: "something broke".into(),
            source_chain: vec!["inner cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let mut buf = Vec::new();
        erased.write_json(&mut buf).unwrap();
        assert!(!buf.is_empty(), "write_json must produce output");

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["message"], "something broke");
        assert_eq!(json["source_chain"][0], "inner cause");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for write_text arrow logic (kills <, -, == mutants)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_text_single_cause_uses_last_arrow() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["only cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let mut buf = Vec::new();
        erased.write_text(&mut NoColor::new(&mut buf)).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("╰─▶"),
            "single cause should use last-arrow ╰─▶"
        );
        assert!(
            !output.contains("├─▶"),
            "single cause should NOT use middle-arrow ├─▶"
        );
    }

    #[test]
    fn test_write_text_two_causes_arrows() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["middle".into(), "root".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let mut buf = Vec::new();
        erased.write_text(&mut NoColor::new(&mut buf)).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("├─▶"),
            "first of two causes should use middle-arrow"
        );
        assert!(output.contains("╰─▶"), "last cause should use last-arrow");
    }

    #[test]
    fn test_write_text_three_causes_arrows() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["first".into(), "second".into(), "third".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let mut buf = Vec::new();
        erased.write_text(&mut NoColor::new(&mut buf)).unwrap();
        let output = String::from_utf8(buf).unwrap();

        // Count occurrences of each arrow
        let middle_count = output.matches("├─▶").count();
        let last_count = output.matches("╰─▶").count();
        assert_eq!(
            middle_count, 2,
            "first two causes should use middle-arrow ├─▶"
        );
        assert_eq!(last_count, 1, "only last cause should use last-arrow ╰─▶");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for format_short arrow logic (kills <, -, == mutants)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_short_single_cause_uses_last_arrow() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["only cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let short = erased.format_short();
        assert!(
            short.contains("╰─▶"),
            "single cause should use last-arrow ╰─▶"
        );
        assert!(
            !short.contains("├─▶"),
            "single cause should NOT use middle-arrow ├─▶"
        );
    }

    #[test]
    fn test_format_short_two_causes_arrows() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["middle".into(), "root".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let short = erased.format_short();
        assert!(
            short.contains("├─▶"),
            "first of two causes should use middle-arrow"
        );
        assert!(short.contains("╰─▶"), "last cause should use last-arrow");
    }

    #[test]
    fn test_format_short_three_causes_arrows() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["first".into(), "second".into(), "third".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let short = erased.format_short();
        let middle_count = short.matches("├─▶").count();
        let last_count = short.matches("╰─▶").count();
        assert_eq!(middle_count, 2, "first two causes should use middle-arrow");
        assert_eq!(last_count, 1, "only last cause should use last-arrow");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Test for write_html (kills Ok(()) mutant)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_html_produces_output() {
        let erased = ErasedError {
            message: "html error".into(),
            source_chain: vec![],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let mut buf = Vec::new();
        erased.write_html(&mut NoColor::new(&mut buf)).unwrap();
        assert!(!buf.is_empty(), "write_html must produce output");
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("html error"),
            "write_html output should contain the error message"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Test for IoToFmt::write (kills Ok(0) and Ok(1) mutants)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_io_to_fmt_write_returns_correct_byte_count() {
        use std::io::Write;

        let mut s = String::new();
        {
            let mut adapter = IoToFmt(&mut s);
            let n = adapter.write(b"hello").unwrap();
            assert_eq!(n, 5, "IoToFmt::write must return the input byte count");
        }
        assert_eq!(s, "hello");

        {
            let mut adapter = IoToFmt(&mut s);
            // Test with empty input
            let n = adapter.write(b"").unwrap();
            assert_eq!(n, 0, "IoToFmt::write on empty input must return 0");

            // Test with longer input
            let n = adapter.write(b"world!").unwrap();
            assert_eq!(n, 6);
        }
        assert_eq!(s, "helloworld!");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Test for Display impl (kills Ok(Default::default()) mutant)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_display_for_erased_error_is_non_empty() {
        let erased = ErasedError {
            message: "display test".into(),
            source_chain: vec!["cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };

        let displayed = erased.to_string();
        assert!(
            !displayed.is_empty(),
            "Display must produce non-empty output"
        );
        assert!(
            displayed.contains("display test"),
            "Display output should contain the error message"
        );
        assert!(
            displayed.contains("cause"),
            "Display output should contain source chain entries"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for BackTrace/ErrorCode extraction (feature = "unstable")
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
    fn test_extract_backtrace_returns_some_when_provided() {
        let error = make_error();
        let bt = oopsie_core::BackTrace::extract_from_error(&error);
        assert!(
            bt.is_some(),
            "extract_backtrace should return Some for oopsie errors"
        );
    }

    #[test]
    #[cfg_attr(not(feature = "unstable"), ignore = "requires unstable provider API")]
    fn test_extract_error_code_returns_some_for_oopsie_errors() {
        let error = ErrorWithHelpOopsie { message: "test" }.build();
        let code = crate::extract_value_from_error::<oopsie_core::ErrorCode>(&error);
        assert!(
            code.is_some(),
            "extract_error_code should return Some for oopsie errors with code"
        );
    }

    #[test]
    fn test_extract_backtrace_returns_none_for_plain_errors() {
        let error = std::io::Error::new(std::io::ErrorKind::Other, "plain error");
        let bt = oopsie_core::BackTrace::extract_from_error(&error);
        assert!(
            bt.is_none(),
            "extract_backtrace should return None for plain errors"
        );
    }

    #[test]
    fn test_extract_error_code_returns_none_for_plain_errors() {
        let error = std::io::Error::new(std::io::ErrorKind::Other, "plain error");
        let code = crate::extract_value_from_error::<oopsie_core::ErrorCode>(&error);
        assert!(
            code.is_none(),
            "extract_error_code should return None for plain errors"
        );
    }
}
