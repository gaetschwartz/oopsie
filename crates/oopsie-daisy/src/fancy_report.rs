//! FancyReport for rich, colorized error output.
//!
//! This module provides [`FancyReport`], a wrapper that formats errors with
//! colorized output including the error chain, span traces, and backtraces.

use std::fmt;
use std::process::{ExitCode, Termination};

use color_backtrace::Frame;
use owo_colors::OwoColorize as _;

use oopsie_core::ColorConfig;
use oopsie_core::spantrace::SpanTraceInner;

use crate::extract_value_from_error;

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

    /// Format the error chain.
    fn write_error_chain(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let colors_enabled = self.color_config.should_colorize();
        let Err(err) = &self.res else { return Ok(()) };

        let error_code = extract_value_from_error::<oopsie_core::ErrorCode>(err);
        let help_text = extract_value_from_error::<oopsie_core::HelpText>(err);

        // Write main error
        if colors_enabled {
            write!(f, "{}", "Error".red().bold())?;
        } else {
            write!(f, "Error")?;
        }

        if let Some(code) = error_code {
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
        if let Some(help) = help_text {
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
        let Some(span_trace) = self
            .error()
            .and_then(|e| oopsie_core::SpanTrace::extract_from_error(e))
        else {
            return Ok(());
        };

        writeln!(f)?;
        match &span_trace.inner {
            SpanTraceInner::Tracing(span_trace) => {
                if self.color_config.should_colorize() {
                    write!(f, "{}", color_spantrace::colorize(span_trace))?;
                } else {
                    writeln!(f, "{:━^80}", " SPANTRACE ")?;
                    write!(f, "{span_trace}")?;
                }
            }
            SpanTraceInner::Fallback(fallback_spantrace) => {
                writeln!(f, "{:━^80}", " SPANTRACE ")?;
                write!(f, "{fallback_spantrace}")?;
            }
        }

        Ok(())
    }

    /// Format the backtrace if available and captured.
    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(backtrace) = self
            .error()
            .and_then(|e| oopsie_core::BackTrace::extract_from_error(e))
        else {
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
            if let Ok(formatted) = printer.format_trace_to_string(&*backtrace) {
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

                #[cfg(feature = "unstable")]
                {
                    core::error::request_ref::<ExitCode>(e)
                        .copied()
                        .unwrap_or(ExitCode::FAILURE)
                }
                #[cfg(not(feature = "unstable"))]
                {
                    let _ = e;
                    ExitCode::FAILURE
                }
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
    "<std::backtrace::Backtrace as oopsie_core::Capturable>::",
    "<alloc::boxed::Box<oopsie_core::backtrace::Backtrace> as oopsie_core::Capturable>::",
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
    use super::*;

    /// Construct a `Frame` for testing. `Frame` is `#[non_exhaustive]` so
    /// struct literal syntax is unavailable from outside the crate.
    fn make_frame(n: usize, name: Option<String>, lineno: Option<u32>) -> Frame {
        use std::mem::MaybeUninit;

        let mut frame = MaybeUninit::<Frame>::zeroed();
        let ptr = frame.as_mut_ptr();

        // SAFETY: Frame has only public fields (n, name, lineno, filename, ip).
        // We write every field, and zeroed memory is valid for Option<_> (= None).
        unsafe {
            std::ptr::addr_of_mut!((*ptr).n).write(n);
            std::ptr::addr_of_mut!((*ptr).name).write(name);
            std::ptr::addr_of_mut!((*ptr).lineno).write(lineno);
            std::ptr::addr_of_mut!((*ptr).filename).write(None);
            std::ptr::addr_of_mut!((*ptr).ip).write(None);
            frame.assume_init()
        }
    }

    #[test]
    fn test_is_backtrace_capture_code() {
        let matching = make_frame(
            0,
            Some("std::backtrace_rs::backtrace::libunwind::trace".into()),
            None,
        );
        let not_matching = make_frame(0, Some("my_crate::do_stuff".into()), None);
        let no_name = make_frame(0, None, None);
        assert!(is_backtrace_capture_code(&matching));
        assert!(!is_backtrace_capture_code(&not_matching));
        assert!(!is_backtrace_capture_code(&no_name));
    }

    #[test]
    fn test_is_runtime_init_code() {
        let matching = make_frame(
            0,
            Some("std::rt::lang_start_internal::something".into()),
            None,
        );
        let not_matching = make_frame(0, Some("my_crate::main_logic".into()), None);
        let no_name = make_frame(0, None, None);
        assert!(is_runtime_init_code(&matching));
        assert!(!is_runtime_init_code(&not_matching));
        assert!(!is_runtime_init_code(&no_name));
    }

    #[test]
    fn test_error_backtrace_frame_filter() {
        let capture = make_frame(
            0,
            Some("std::backtrace_rs::backtrace::libunwind::trace".into()),
            None,
        );
        let app1 = make_frame(1, Some("my_crate::function_a".into()), None);
        let app2 = make_frame(2, Some("my_crate::function_b".into()), None);
        let runtime = make_frame(3, Some("std::rt::lang_start_internal::invoke".into()), None);

        let mut frames: Vec<&Frame> = vec![&capture, &app1, &app2, &runtime];
        error_backtrace_frame_filter(&mut frames);

        // Should keep only app frames, stripping capture from top and runtime from bottom
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].n, 1);
        assert_eq!(frames[1].n, 2);
    }

    #[test]
    fn test_error_backtrace_frame_filter_no_capture_no_runtime() {
        let app1 = make_frame(0, Some("my_crate::function_a".into()), None);
        let app2 = make_frame(1, Some("my_crate::function_b".into()), None);

        let mut frames: Vec<&Frame> = vec![&app1, &app2];
        error_backtrace_frame_filter(&mut frames);

        // All frames should be kept
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].n, 0);
        assert_eq!(frames[1].n, 1);
    }

    #[test]
    fn test_error_backtrace_frame_filter_multiple_capture_frames() {
        // Two capture frames at the top — idx+1 arithmetic matters here
        let capture1 = make_frame(
            0,
            Some("<std::backtrace::Backtrace>::create::something".into()),
            None,
        );
        let capture2 = make_frame(
            1,
            Some("std::backtrace_rs::backtrace::libunwind::trace".into()),
            None,
        );
        let app = make_frame(2, Some("my_crate::function_a".into()), None);

        let mut frames: Vec<&Frame> = vec![&capture1, &capture2, &app];
        error_backtrace_frame_filter(&mut frames);

        // Should keep only the app frame (after last capture frame at idx 1, so idx+1=2)
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].n, 2);
    }
}
