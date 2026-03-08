//! FancyReport for rich, colorized error output.
//!
//! This module provides [`FancyReport`], a wrapper that formats errors with
//! colorized output including the error chain, span traces, and backtraces.

use core::error;
use std::borrow::Cow;
use std::fmt;
use std::process::{ExitCode, Termination};

use color_backtrace::Frame;
use owo_colors::OwoColorize as _;

#[cfg(feature = "unstable")]
use crate::erased::Diagnostics;
use oopsie_core::ColorConfig;
use oopsie_core::Spantrace;
use oopsie_core::spantrace::SpantraceInner;
#[cfg(feature = "unstable")]
use oopsie_core::{Backtrace, ErrorCode, HelpText};

/// A wrapper around an error that provides rich, colorized output.
///
/// `FancyReport` extracts backtraces and span traces from the error chain
/// (when available via the Provider API) and formats them with colors.
pub struct FancyReport<E> {
    res: Result<(), E>,
    color_config: ColorConfig,
}

impl<E: std::error::Error> FancyReport<E> {
    /// Create a new `FancyReport` wrapping the given error.
    ///
    /// Uses automatic color detection based on environment variables and
    /// terminal detection.
    #[must_use]
    pub const fn from_std(error: E) -> Self {
        Self {
            res: Err(error),
            color_config: ColorConfig::auto(),
        }
    }

    /// Create a new `FancyReport` with a successful result.
    #[must_use]
    pub const fn ok() -> Self {
        Self {
            res: Ok(()),
            color_config: ColorConfig::auto(),
        }
    }

    /// Runs the given function and returns a `FancyReport` with the result.
    #[must_use]
    pub fn run<F>(func: F) -> Self
    where
        F: FnOnce() -> Result<(), E>,
    {
        let hook = std::panic::take_hook();
        oopsie_core::install();
        let result = func();
        std::panic::set_hook(hook);
        Self {
            res: result,
            color_config: ColorConfig::auto(),
        }
    }

    /// Create with explicit color configuration.
    #[must_use]
    pub const fn with_colors(error: E, color_config: ColorConfig) -> Self {
        Self {
            res: Err(error),
            color_config,
        }
    }

    /// Disable colors in output.
    #[must_use]
    pub const fn no_colors(mut self) -> Self {
        self.color_config = ColorConfig::Never;
        self
    }

    /// Force colors in output.
    #[must_use]
    pub const fn force_colors(mut self) -> Self {
        self.color_config = ColorConfig::Always;
        self
    }

    /// Get a reference to the wrapped error.
    #[must_use]
    pub const fn error(&self) -> Option<&E> {
        match &self.res {
            Err(err) => Some(err),
            Ok(()) => None,
        }
    }

    /// Consume self and return the wrapped error.
    #[must_use]
    pub fn into_error(self) -> Option<E> {
        self.res.err()
    }

    /// Extract backtrace from error chain using Provider API.
    #[cfg(feature = "unstable")]
    fn extract_backtrace(&self) -> Option<&Backtrace> {
        std::error::request_ref::<Backtrace>(self.error()?)
    }

    /// Extract backtrace - not available on stable.
    #[cfg(not(feature = "unstable"))]
    fn extract_backtrace(&self) -> Option<&Backtrace> {
        None
    }

    /// Extract SpanTrace from error chain using Provider API.
    #[cfg(feature = "unstable")]
    fn extract_span_trace(&self) -> Option<Cow<'_, Spantrace>> {
        let err = self.error()?;
        Spantrace::extract(err)
    }

    /// Extract SpanTrace - not available on stable.
    #[cfg(not(feature = "unstable"))]
    fn extract_span_trace(&self) -> Option<Cow<'_, Spantrace>> {
        None
    }

    /// Diagnostic information from error chain using Provider API.
    #[cfg(feature = "unstable")]
    fn extract_diagnostic(&self) -> Option<Diagnostics> {
        let err = self.error()?;
        let code = error::request_value::<ErrorCode>(err)
            .or_else(|| error::request_ref::<ErrorCode>(err).cloned());
        let help = error::request_value::<HelpText>(err)
            .or_else(|| error::request_ref::<HelpText>(err).cloned());
        (code.is_some() || help.is_some()).then(|| Diagnostics {
            code,
            help: help.map(|h| Box::from(h.0)),
        })
    }

    /// Diagnostic information - not available on stable.
    #[cfg(not(feature = "unstable"))]
    fn extract_diagnostic(&self) -> Option<Diagnostics> {
        None
    }

    /// Format the error chain.
    fn write_error_chain(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let colors_enabled = self.color_config.should_colorize();
        let Err(err) = &self.res else { return Ok(()) };

        let diag = self.extract_diagnostic();

        // Write main error
        if colors_enabled {
            write!(f, "{}", "Error".red().bold())?;
        } else {
            write!(f, "Error")?;
        }

        if let Some(ref diag) = diag
            && let Some(code) = diag.code()
        {
            if colors_enabled {
                write!(
                    f,
                    "{}{}{}",
                    "[".dimmed(),
                    code.dimmed().blue(),
                    "]".dimmed()
                )?;
            } else {
                write!(f, "[{code}]")?;
            }
        }
        writeln!(f, ": {err}")?;

        // Write error chain
        let mut source = err.source();
        while let Some(err) = source {
            let next_source = err.source();
            let arrow = if next_source.is_some() {
                "├─▶"
            } else {
                "╰─▶"
            };
            if colors_enabled {
                writeln!(f, "  {} {err}", arrow.yellow())?;
            } else {
                writeln!(f, "  {arrow} {err}")?;
            }
            source = next_source;
        }

        // Write help text if present
        if let Some(ref diag) = diag
            && let Some(help) = diag.help()
        {
            if colors_enabled {
                write!(f, "\n  {}: {help}", "help".cyan())?;
            } else {
                write!(f, "\n  help: {help}")?;
            }
            writeln!(f)?;
        }

        Ok(())
    }

    /// Format the span trace if available.
    fn write_span_trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Try our SpanTrace wrapper first
        let Some(span_trace) = self.extract_span_trace() else {
            return Ok(());
        };

        writeln!(f)?;
        match &span_trace.inner {
            SpantraceInner::Tracing(span_trace) => {
                if self.color_config.should_colorize() {
                    write!(f, "{}", color_spantrace::colorize(span_trace))?;
                } else {
                    writeln!(f, "{:━^80}", " SPANTRACE ")?;
                    write!(f, "{span_trace}")?;
                }
            }
            SpantraceInner::Fallback(fallback_spantrace) => {
                writeln!(f, "{:━^80}", " SPANTRACE ")?;
                write!(f, "{fallback_spantrace}")?;
            }
        }

        Ok(())
    }

    /// Format the backtrace if available and captured.
    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(backtrace) = self.extract_backtrace() else {
            return Ok(());
        };
        writeln!(f)?;
        // Only display if backtrace was actually captured
        if self.color_config.should_colorize() {
            writeln!(f)?;
            // Use color-backtrace for colorized output
            // Note: format_trace_to_string already includes the "━━━ BACKTRACE ━━━" header
            let printer = color_backtrace::BacktracePrinter::default()
                .clear_frame_filters()
                .add_frame_filter(Box::new(error_backtrace_frame_filter));
            if let Ok(formatted) = printer.format_trace_to_string(backtrace) {
                write!(f, "{formatted}")?;
            } else {
                writeln!(f, "{:━^80}", " BACKTRACE ")?;
                write!(f, "{backtrace:?}")?;
            }
        } else {
            writeln!(f, "{:━^80}", " BACKTRACE ")?;
            write!(f, "{backtrace:?}")?;
        }
        Ok(())
    }
}

impl<E> Termination for FancyReport<E>
where
    E: std::error::Error,
{
    fn report(self) -> ExitCode {
        match self.res {
            Ok(()) => ExitCode::SUCCESS,
            Err(ref e) => {
                eprintln!("{self}");

                error::request_ref::<ExitCode>(&e)
                    .copied()
                    .unwrap_or(ExitCode::FAILURE)
            }
        }
    }
}

#[cfg(feature = "unstable")]
impl<T, E> core::ops::FromResidual<Result<T, E>> for FancyReport<E> {
    fn from_residual(residual: Result<T, E>) -> Self {
        Self {
            res: residual.map(drop),
            color_config: ColorConfig::default(),
        }
    }
}

impl<E: std::error::Error> fmt::Display for FancyReport<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_error_chain(f)?;
        self.write_span_trace(f)?;
        self.write_backtrace(f)?;
        Ok(())
    }
}

impl<E: std::error::Error> fmt::Debug for FancyReport<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<E: std::error::Error> From<E> for FancyReport<E> {
    fn from(error: E) -> Self {
        Self::from_std(error)
    }
}

/// Prefixes for backtrace capture frames that should be skipped.
const BACKTRACE_CAPTURE_PREFIXES: &[&str] = &[
    "std::backtrace_rs::backtrace::",
    "<std::backtrace::Backtrace>::create",
    "<std::backtrace::Backtrace as snafu::GenerateImplicitData>::",
];

/// Prefixes for runtime initialization frames that should be skipped.
const RUNTIME_INIT_PREFIXES: &[&str] = &[
    "std::rt::lang_start::",
    "std::rt::lang_start_internal::",
    "std::panicking::catch_unwind::",
    "std::panic::catch_unwind::",
    "__rustc",
    "_main",
    "main",
    "__libc_start",
    "__scrt_common_main",
];

/// Frame filter tailored for error backtraces (not panics).
///
/// This filter:
/// 1. Skips frames from the top that are backtrace capture machinery
/// 2. Removes runtime initialization frames from the bottom
pub fn error_backtrace_frame_filter(frames: &mut Vec<&Frame>) {
    // Find the index of the last backtrace capture frame
    // We want to skip everything up to and including this frame
    let top_cutoff_idx = frames
        .iter()
        .rposition(|frame| is_backtrace_capture_code(frame))
        .map_or(0, |idx| idx + 1);

    // Find the index of runtime init code at the bottom
    let bottom_cutoff_idx = frames
        .iter()
        .position(|frame| is_runtime_init_code(frame))
        .unwrap_or(frames.len());

    // Keep only frames within the valid range
    let frames_to_keep: Vec<usize> = frames[top_cutoff_idx..bottom_cutoff_idx]
        .iter()
        .map(|f| f.n)
        .collect();

    frames.retain(|frame| frames_to_keep.contains(&frame.n));
}

/// Check if a frame is backtrace capture code that should be skipped.
fn is_backtrace_capture_code(frame: &Frame) -> bool {
    frame.name.as_ref().is_some_and(|name| {
        BACKTRACE_CAPTURE_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    })
}

/// Check if a frame is runtime initialization code.
fn is_runtime_init_code(frame: &Frame) -> bool {
    frame.name.as_ref().is_some_and(|name| {
        RUNTIME_INIT_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    })
}

#[cfg(test)]
mod tests {
    use oopsie_macros::oopsie;
    use snafu::{IntoError as _, Snafu};

    use super::*;
    use crate::erased::tests::make_error;

    /// Strip ANSI escape codes for consistent snapshot testing.
    fn strip_ansi(s: &str) -> String {
        String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
    }

    #[oopsie(path = "crate")]
    #[derive(Debug, Snafu)]
    #[snafu(display("Test error: {message}"), visibility(pub))]
    pub struct TestError {
        message: String,
    }

    #[oopsie(path = "crate")]
    #[derive(Debug, Snafu)]
    #[snafu(display("Outer error"), visibility(pub))]
    pub struct OuterError {
        source: TestError,
    }

    #[test]
    fn test_fancy_report_basic() {
        let error = TestSnafu {
            message: "something failed",
        }
        .build();
        let report = FancyReport::from_std(error).no_colors();

        insta::assert_snapshot!("fancy_report_basic", report.to_string());
    }

    #[test]
    fn test_fancy_report_chain() {
        let inner = TestSnafu {
            message: "root cause",
        }
        .build();
        let outer: OuterError = OuterSnafu.into_error(inner);
        let report = FancyReport::from_std(outer).no_colors();

        insta::assert_snapshot!("fancy_report_chain", report.to_string());
    }

    #[test]
    fn test_fancy_report_colored() {
        let error = TestSnafu {
            message: "colored test",
        }
        .build();
        let report = FancyReport::from_std(error).force_colors();

        // Strip ANSI for snapshot consistency
        let output = strip_ansi(&report.to_string());
        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!("fancy_report_colored_stripped", output);
        });
    }

    #[test]
    fn test_fancy_report_from() {
        let error = TestSnafu {
            message: "from test",
        }
        .build();
        let report: FancyReport<_> = error.into();
        assert!(report.to_string().contains("from test"));
    }

    #[oopsie(path = "crate")]
    #[derive(Debug, Snafu)]
    #[snafu(display("Something went wrong: {message}"), visibility(pub))]
    #[help("Try restarting the service")]
    pub struct ErrorWithHelp {
        message: String,
    }

    #[test]
    fn test_fancy_report_with_help() {
        let error = ErrorWithHelpSnafu {
            message: "connection refused",
        }
        .build();
        let report = FancyReport::from_std(error).no_colors();

        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!("fancy_report_with_help", report.to_string());
        });
    }

    #[test]
    fn test_fancy_report_with_spantrace() {
        let error = make_error();
        let report = FancyReport::from_std(error).no_colors();

        // Redact file paths and line numbers for stable snapshots
        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!("fancy_report_with_spantrace", report);
        });
    }

    #[test]
    fn test_fancy_report_with_spantrace_debug() {
        let error = make_error();

        // Redact file paths and line numbers for stable snapshots
        insta::with_settings!({
            filters => [
                (r"\[[0-9a-f]{16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ]
        }, {
            insta::assert_snapshot!("fancy_report_with_spantrace_debug", format!("{error:#}"));
        });
    }
}
