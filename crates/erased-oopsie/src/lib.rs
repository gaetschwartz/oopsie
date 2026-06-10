//! Erased error types for serializable error reporting.
//!
//! This crate provides types that capture error information in a serializable
//! format, preserving the full error context including message, source chain,
//! spantrace, and backtrace.

mod backtrace;
mod spantrace;

pub use backtrace::{ErasedBacktrace, ErasedFrame};
pub use spantrace::{ErasedMetadata, ErasedSpan, ErasedSpanTrace, TracingLevel};

use std::fmt;
use std::io;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use oopsie_core::{ErrorCode, HelpText};

/// Upper bound on real source entries stored by `from_error_ref`; a sentinel
/// entry is appended when the walk would exceed this limit.
const MAX_SOURCE_CHAIN_DEPTH: usize = 128;

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

    /// Diagnostic metadata (code, help).
    #[serde(default)]
    pub diagnostics: Diagnostics,

    /// Serialized span trace.
    pub spantrace: Option<ErasedSpanTrace>,

    /// Serialized backtrace.
    pub backtrace: Option<ErasedBacktrace>,

    /// Snapshot of `source_chain` taken on the first `Error::source()` call;
    /// later mutations of the pub field are not reflected here.
    #[serde(skip)]
    source: OnceLock<Option<Box<ChainNode>>>,
}

impl std::error::Error for ErasedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .get_or_init(|| ChainNode::build(&self.source_chain))
            .as_deref()
            .map(|node| node as &(dyn std::error::Error + 'static))
    }
}

/// One transported cause, re-materialized as a real error value so generic
/// `Error::source()` walkers see the chain instead of bare data.
#[derive(Clone, Debug)]
struct ChainNode {
    message: Box<str>,
    source: Option<Box<Self>>,
}

impl ChainNode {
    fn build(messages: &[Box<str>]) -> Option<Box<Self>> {
        messages.iter().rev().fold(None, |source, message| {
            Some(Box::new(Self {
                message: message.clone(),
                source,
            }))
        })
    }
}

impl fmt::Display for ChainNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChainNode {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|node| node as &(dyn std::error::Error + 'static))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Diagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<HelpText>,
}

impl Diagnostics {
    #[must_use]
    #[inline]
    pub const fn is_none(&self) -> bool {
        self.code.is_none() && self.help.is_none()
    }
    #[must_use]
    #[inline]
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
    #[must_use]
    #[inline]
    pub fn help(&self) -> Option<&str> {
        self.help.as_deref()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ErasedError construction
// ─────────────────────────────────────────────────────────────────────────────

impl ErasedError {
    /// Create an `ErasedError` from any error implementing `Diagnostic`.
    ///
    /// This extracts the message, source chain, backtrace, spantrace,
    /// error code, and help text via the `Diagnostic` trait.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "owned input mirrors the from_error_ref ergonomics"
    )]
    pub fn from_error<E: oopsie_core::Diagnostic>(err: E) -> Self {
        Self::from_error_ref(&err)
    }

    /// Create an `ErasedError` from a reference to any error implementing `Diagnostic`.
    ///
    /// Like `from_error` but takes a reference, useful when ownership cannot be transferred
    /// (e.g., in `Serialize` implementations).
    pub fn from_error_ref<E: oopsie_core::Diagnostic>(err: &E) -> Self {
        let message = err.to_string().into();

        // `Error::source` is user-implemented and may form a cycle (returning
        // `self` or an ancestor); the std contract does not forbid it. Cap the
        // eager walk so a foreign cyclic chain can't hang or OOM this
        // serialization entry point.
        let mut source_chain: Vec<Box<str>> = std::iter::successors(err.source(), |e| e.source())
            .take(MAX_SOURCE_CHAIN_DEPTH + 1)
            .map(ToString::to_string)
            .map(Box::from)
            .collect();
        if source_chain.len() > MAX_SOURCE_CHAIN_DEPTH {
            source_chain.truncate(MAX_SOURCE_CHAIN_DEPTH);
            source_chain.push("\u{2026} source chain truncated".into());
        }

        let diagnostics = Diagnostics {
            code: err.oopsie_error_code(),
            help: err.oopsie_help_text(),
        };
        // Skip empty/unsupported traces: they carry no frames and would render
        // as a lone `SPANTRACE`/`BACKTRACE` header with no body.
        let spantrace = err
            .oopsie_spantrace()
            .filter(|st| st.is_captured())
            .map(ErasedSpanTrace::from);
        let backtrace = err
            .oopsie_backtrace()
            .map(ErasedBacktrace::from_backtrace)
            .filter(|bt| !bt.frames().is_empty());

        Self {
            message,
            source_chain,
            diagnostics,
            spantrace,
            backtrace,
            source: OnceLock::new(),
        }
    }

    pub fn write_json<W: io::Write>(&self, f: &mut W) -> Result<(), serde_json::Error> {
        use io::Write as _;
        let mut buf = io::BufWriter::new(f);
        serde_json::to_writer_pretty(&mut buf, self)?;
        buf.flush().map_err(serde_json::Error::io)?;
        Ok(())
    }

    /// Write the error in a text format similar to `Report`.
    pub fn write_text<W: io::Write>(&self, f: &mut W) -> io::Result<()> {
        // Write main error header
        match self.diagnostics.code() {
            Some(code) => writeln!(f, "Error[{code}]:")?,
            None => writeln!(f, "Error:")?,
        }

        writeln!(f, "\n  \u{00d7} {}", self.message)?;

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

        // An empty trace renders as a lone banner with no body; empty traces
        // are reachable directly from the wire format, so gate here too.
        if let Some(spantrace) = &self.spantrace
            && !spantrace.is_empty()
        {
            writeln!(f)?;
            writeln!(f, "{:━^80}", " SPANTRACE ")?;
            writeln!(f, "{spantrace}")?;
        }

        if let Some(backtrace) = &self.backtrace
            && !backtrace.frames().is_empty()
        {
            writeln!(f)?;
            writeln!(f, "{:━^80}", " BACKTRACE ")?;
            write!(f, "{backtrace}")?;
        }

        Ok(())
    }

    /// The full multi-line text report (header, source chain, help, traces)
    /// as a `String`. `Display` intentionally prints only the message so an
    /// `ErasedError` embeds cleanly in another error's source chain.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "writing to a Vec<u8> cannot fail and write_text emits only UTF-8"
    )]
    pub fn to_text(&self) -> String {
        let mut buf = Vec::new();
        self.write_text(&mut buf)
            .expect("Vec<u8> writes are infallible");
        String::from_utf8(buf).expect("write_text emits UTF-8")
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
}

impl fmt::Display for ErasedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl oopsie_core::Diagnostic for ErasedError {
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        self.diagnostics.code.clone()
    }

    fn oopsie_help_text(&self) -> Option<HelpText> {
        self.diagnostics.help.clone()
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
    // Tests for from_error / from_error_ref
    // ─────────────────────────────────────────────────────────────────────

    /// A simple chained error type for testing from_error / from_error_ref.
    #[derive(Debug)]
    struct ChainedError {
        msg: &'static str,
        source: Option<Box<Self>>,
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

    impl oopsie_core::Diagnostic for ChainedError {}

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
    // Tests for write_json
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_json_produces_valid_json() {
        let erased = ErasedError {
            message: "something broke".into(),
            source_chain: vec!["inner cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };

        let mut buf = Vec::new();
        erased.write_json(&mut buf).unwrap();
        assert!(!buf.is_empty(), "write_json must produce output");

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(json["message"], "something broke");
        assert_eq!(json["source_chain"][0], "inner cause");
    }

    struct FailingWriter;
    impl io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "peer gone"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "peer gone"))
        }
    }

    #[test]
    fn write_json_surfaces_io_errors() {
        let erased = ErasedError {
            message: "x".into(),
            source_chain: vec![],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };
        let err = erased.write_json(&mut FailingWriter).unwrap_err();
        assert!(err.is_io(), "{err}");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for write_text arrow logic
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_text_single_cause_uses_last_arrow() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["only cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
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
            source: OnceLock::new(),
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
            source: OnceLock::new(),
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
    // Tests for format_short arrow logic
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_format_short_single_cause_uses_last_arrow() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["only cause".into()],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
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
            source: OnceLock::new(),
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
            source: OnceLock::new(),
        };

        let short = erased.format_short();
        let middle_count = short.matches("\u{251c}\u{2500}\u{25b6}").count();
        let last_count = short.matches("\u{2570}\u{2500}\u{25b6}").count();
        assert_eq!(middle_count, 2, "first two causes should use middle-arrow");
        assert_eq!(last_count, 1, "only last cause should use last-arrow");
    }

    // ─────────────────────────────────────────────────────────────────────
    // Tests for Display impl and to_text
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_display_is_exactly_the_message() {
        let erased = ErasedError {
            message: "display test".into(),
            source_chain: vec!["cause".into()],
            diagnostics: Diagnostics {
                code: Some("app::code".into()),
                help: Some("try again".into()),
            },
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };

        let displayed = erased.to_string();
        assert_eq!(
            displayed, "display test",
            "Display must be only the message so an ErasedError embeds \
             cleanly in another error's source chain"
        );
        assert!(
            !displayed.contains('\n'),
            "Display must be single-line, got {displayed:?}"
        );
    }

    #[test]
    fn test_to_text_contains_full_report() {
        let erased = ErasedError {
            message: "top".into(),
            source_chain: vec!["middle".into(), "root".into()],
            diagnostics: Diagnostics {
                code: Some("app::db::timeout".into()),
                help: Some("retry later".into()),
            },
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };

        let text = erased.to_text();
        assert!(text.contains("top"), "to_text must contain the message");
        assert!(
            text.contains("\u{251c}\u{2500}\u{25b6} middle"),
            "to_text must render the source chain"
        );
        assert!(
            text.contains("\u{2570}\u{2500}\u{25b6} root"),
            "to_text must render the last cause"
        );
        assert!(
            text.contains("help: retry later"),
            "to_text must render the help text"
        );
    }

    #[test]
    fn test_write_text_renders_code_in_header() {
        let erased = ErasedError {
            message: "query timed out".into(),
            source_chain: vec![],
            diagnostics: Diagnostics {
                code: Some("app::db::timeout".into()),
                help: None,
            },
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };

        let text = erased.to_text();
        assert_eq!(
            text.lines().next(),
            Some("Error[app::db::timeout]:"),
            "header must carry the error code"
        );
    }

    #[test]
    fn test_write_text_header_without_code() {
        let erased = ErasedError {
            message: "plain".into(),
            source_chain: vec![],
            diagnostics: Diagnostics::default(),
            spantrace: None,
            backtrace: None,
            source: OnceLock::new(),
        };

        assert_eq!(erased.to_text().lines().next(), Some("Error:"));
    }

    #[test]
    fn source_walk_yields_transported_chain_in_order() {
        let erased: ErasedError =
            serde_json::from_str(r#"{"message":"outer","source_chain":["middle","root"]}"#)
                .unwrap();

        let mut walked = Vec::new();
        let mut src = std::error::Error::source(&erased);
        while let Some(e) = src {
            walked.push(e.to_string());
            src = e.source();
        }
        assert_eq!(walked, ["middle", "root"]);

        let empty: ErasedError = serde_json::from_str(r#"{"message":"x"}"#).unwrap();
        assert!(std::error::Error::source(&empty).is_none());
    }

    #[test]
    fn write_text_suppresses_banners_for_empty_traces() {
        let erased: ErasedError = serde_json::from_str(
            r#"{"message":"transported","spantrace":{"spans":[]},"backtrace":{"frames":[]}}"#,
        )
        .unwrap();

        let text = erased.to_text();
        assert!(
            !text.contains("SPANTRACE"),
            "empty spantrace must not banner:\n{text}"
        );
        assert!(
            !text.contains("BACKTRACE"),
            "empty backtrace must not banner:\n{text}"
        );
    }

    #[test]
    fn write_text_backtrace_only_gets_blank_line_before_banner() {
        let erased: ErasedError = serde_json::from_str(
            r#"{"message":"m","source_chain":["c"],"spantrace":null,
                "backtrace":{"frames":[{"name":"f","filename":null,"line":null,"column":null}]}}"#,
        )
        .unwrap();
        let text = erased.to_text();
        assert!(
            text.contains("c\n\n━"),
            "blank line must separate the chain from the BACKTRACE banner:\n{text}"
        );
    }

    #[test]
    fn test_extract_backtrace_returns_none_for_plain_errors() {
        let error = ChainedError {
            msg: "plain error",
            source: None,
        };
        let bt = oopsie_core::Diagnostic::oopsie_backtrace(&error);
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
            source: OnceLock::new(),
        };
        assert!(
            erased.diagnostics.code().is_none(),
            "plain error should have no error code"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Serde tolerance: missing optional fields deserialize without error.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn serde_message_only_payload_deserializes() {
        let erased: ErasedError = serde_json::from_str(r#"{"message":"x"}"#)
            .expect("missing optional fields must not fail");
        assert_eq!(&*erased.message, "x");
        assert!(erased.source_chain.is_empty());
        assert!(erased.diagnostics.is_none());
        assert!(erased.spantrace.is_none());
        assert!(erased.backtrace.is_none());
    }

    // ─────────────────────────────────────────────────────────────────────
    // Truncation marker: cyclic source yields MAX+1 entries, last is the
    // sentinel.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn from_error_ref_appends_truncation_sentinel_on_cyclic_source() {
        #[derive(Debug)]
        struct Cyclic;
        impl fmt::Display for Cyclic {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("cyclic")
            }
        }
        impl std::error::Error for Cyclic {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(self)
            }
        }
        impl oopsie_core::Diagnostic for Cyclic {}

        let erased = ErasedError::from_error_ref(&Cyclic);
        assert_eq!(
            erased.source_chain.len(),
            MAX_SOURCE_CHAIN_DEPTH + 1,
            "cyclic chain must produce exactly MAX+1 entries (MAX real + sentinel)"
        );
        assert_eq!(
            &*erased.source_chain[MAX_SOURCE_CHAIN_DEPTH], "\u{2026} source chain truncated",
            "last entry must be the truncation sentinel"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Lossy filename: ErasedFrame.filename is now Box<str>, not Box<Path>.
    // Smoke-test that a real backtrace produces str filenames.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn erased_frame_filename_is_str() {
        oopsie_core::set_rust_backtrace_override(oopsie_core::RustBacktrace::Enabled);
        let bt = <oopsie_core::Backtrace as oopsie_core::Capturable>::capture();
        oopsie_core::clear_rust_backtrace_override();

        let erased = crate::backtrace::ErasedBacktrace::from_backtrace(&bt);
        // filename is already Box<str> — this compiles only if the type is correct.
        // Verify at least one frame has a non-empty filename string.
        let has_filename = erased
            .frames()
            .iter()
            .any(|fr| fr.filename.as_deref().is_some_and(|s| !s.is_empty()));
        assert!(has_filename, "at least one frame should have a filename");
    }
}
