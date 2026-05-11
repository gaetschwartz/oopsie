//! Erased error types for serializable error reporting.
//!
//! This crate provides types that capture error information in a serializable
//! format, preserving the full error context including message, source chain,
//! spantrace, and backtrace.

mod backtrace;
mod spantrace;

pub use backtrace::{ErasedBackTrace, ErasedFrame};
pub use spantrace::{ErasedMetadata, ErasedSpan, ErasedSpanTrace, TracingLevel};

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

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
    pub spantrace: Option<ErasedSpanTrace>,

    /// Serialized backtrace.
    pub backtrace: Option<ErasedBackTrace>,
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
    /// Create an `ErasedError` from any error implementing `ErrorExt`.
    ///
    /// This extracts the message, source chain, backtrace, spantrace,
    /// error code, and help text via the `ErrorExt` trait.
    #[expect(clippy::needless_pass_by_value)]
    pub fn from_error<E: oopsie_core::ErrorExt>(err: E) -> Self {
        Self::from_error_ref(&err)
    }

    /// Create an `ErasedError` from a reference to any error implementing `ErrorExt`.
    ///
    /// Like `from_error` but takes a reference, useful when ownership cannot be transferred
    /// (e.g., in `Serialize` implementations).
    pub fn from_error_ref<E: oopsie_core::ErrorExt>(err: &E) -> Self {
        let message = err.to_string().into();

        let source_chain = std::iter::successors(err.source(), |e| e.source())
            .map(ToString::to_string)
            .map(Box::from)
            .collect();

        let diagnostics = Diagnostics {
            code: err.oopsie_error_code(),
            help: err.oopsie_help_text(),
        };
        let spantrace = err.oopsie_spantrace().map(ErasedSpanTrace::from);
        let backtrace = err.oopsie_backtrace().map(ErasedBackTrace::from);

        Self {
            message,
            source_chain,
            diagnostics,
            spantrace,
            backtrace,
        }
    }

    pub fn write_json<W: io::Write>(&self, f: &mut W) -> Result<(), serde_json::Error> {
        serde_json::to_writer_pretty(io::BufWriter::new(f), self)?;
        Ok(())
    }

    /// Write the error in a text format similar to `Report`.
    pub fn write_text<W: io::Write>(&self, f: &mut W) -> io::Result<()> {
        // Write main error header
        write!(f, "Error: ")?;
        writeln!(f)?;

        writeln!(f, "\n  \u{2715} {}", self.message)?;

        // Write source chain with box-drawing characters
        let chain_len = self.source_chain.len();
        for (i, cause) in self.source_chain.iter().enumerate() {
            let arrow = if i < chain_len - 1 {
                "\u{251c}\u{2500}\u{25b6}"
            } else {
                "\u{2570}\u{2500}\u{25b6}"
            };
            write!(f, "  {arrow} ")?;
            writeln!(f, "{cause}")?;
        }

        // Write help text if present
        if let Some(help) = self.diagnostics.help() {
            write!(f, "\n  help: ")?;
            writeln!(f, "{help}")?;
        }

        // Write spantrace if present
        if let Some(spantrace) = &self.spantrace {
            writeln!(f)?;
            writeln!(f, "{:━^80}", " SPANTRACE ")?;
            writeln!(f, "{spantrace}")?;
        }

        // Write backtrace if present
        if let Some(backtrace) = &self.backtrace {
            writeln!(f, "{:━^80}", " BACKTRACE ")?;
            write!(f, "{backtrace}")?;
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
    pub fn write_html<W: io::Write>(&self, f: &mut W) -> io::Result<()> {
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
        self.write_text(&mut IoToFmt(f)).map_err(|_err| fmt::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    impl oopsie_core::ErrorExt for ChainedError {}

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
        erased.write_text(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("\u{2570}\u{2500}\u{25b6}"),
            "single cause should use last-arrow"
        );
        assert!(
            !output.contains("\u{251c}\u{2500}\u{25b6}"),
            "single cause should NOT use middle-arrow"
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
        erased.write_text(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("\u{251c}\u{2500}\u{25b6}"),
            "first of two causes should use middle-arrow"
        );
        assert!(
            output.contains("\u{2570}\u{2500}\u{25b6}"),
            "last cause should use last-arrow"
        );
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
        erased.write_text(&mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        // Count occurrences of each arrow
        let middle_count = output.matches("\u{251c}\u{2500}\u{25b6}").count();
        let last_count = output.matches("\u{2570}\u{2500}\u{25b6}").count();
        assert_eq!(middle_count, 2, "first two causes should use middle-arrow");
        assert_eq!(last_count, 1, "only last cause should use last-arrow");
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
            short.contains("\u{2570}\u{2500}\u{25b6}"),
            "single cause should use last-arrow"
        );
        assert!(
            !short.contains("\u{251c}\u{2500}\u{25b6}"),
            "single cause should NOT use middle-arrow"
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
            short.contains("\u{251c}\u{2500}\u{25b6}"),
            "first of two causes should use middle-arrow"
        );
        assert!(
            short.contains("\u{2570}\u{2500}\u{25b6}"),
            "last cause should use last-arrow"
        );
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
        let middle_count = short.matches("\u{251c}\u{2500}\u{25b6}").count();
        let last_count = short.matches("\u{2570}\u{2500}\u{25b6}").count();
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
        erased.write_html(&mut buf).unwrap();
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
        use std::io::Write as _;

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

    #[test]
    fn test_extract_backtrace_returns_none_for_plain_errors() {
        let error = ChainedError {
            msg: "plain error",
            source: None,
        };
        let bt = oopsie_core::ErrorExt::oopsie_backtrace(&error);
        assert!(
            bt.is_none(),
            "extract_backtrace should return None for plain errors"
        );
    }

    #[test]
    fn test_extract_error_code_returns_none_for_plain_errors() {
        let erased = ErasedError {
            message: "plain error".into(),
            source_chain: vec![],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
        };
        assert!(
            erased.diagnostics.code().is_none(),
            "plain error should have no error code"
        );
    }
}
