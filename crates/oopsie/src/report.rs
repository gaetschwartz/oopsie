//! Report for rich, colorized error output.
//!
//! This module provides [`Report`], a wrapper that formats errors with
//! colorized output including the error chain, span traces, and backtraces.

use std::borrow::Cow;
use std::fmt;
use std::process::{ExitCode, Termination};

use crate::ColorMode;

use crate::Diagnostic;

use crate::color::style;
use crate::theme::{Theme, get_theme};
use crate::trace_printer::{TracePrinter, error_backtrace_frame_filter, marker_strip_filter};

/// A wrapper around an error that provides rich, colorized output.
///
/// The error-chain section renders the message of every `source()` in the
/// chain. The backtrace and span trace are read from the top-level error's
/// [`Diagnostic`] accessors only — `Report` does not walk the chain looking for
/// traces. `#[oopsie]`-generated and transparent errors still surface the
/// origin-most trace at the top level: their accessors recursively forward to
/// the source's own accessors, so the deepest captured trace bubbles up to the
/// top error without `Report` reaching for it. A hand-written top-level error
/// that does not forward [`Diagnostic`] renders without a deep trace, even if a
/// source deeper in its chain carries one.
///
/// # Exit code
///
/// Returning a `Report` from `main` exits with a non-`SUCCESS` code on error.
/// The code is resolved in this order: a runtime [`ExitCode`] requested through
/// the nightly provider API (`unstable-error-generic-member-access`) wins when
/// present, as the more specific runtime signal; otherwise the declarative
/// [`Diagnostic::oopsie_exit_code`] (from `#[oopsie(exit_code = N)]`) is used;
/// failing both, the code is [`ExitCode::FAILURE`].
pub struct Report<E> {
    res: Result<(), E>,
    color_config: ColorMode,
    /// Per-report theme; `None` falls back to the process-global [`get_theme`]
    /// at render time.
    theme_override: Option<Theme>,
    /// Resolved once at construction so repeated rendering never re-symbolicates.
    /// `None` when there is no error or the captured backtrace is empty.
    backtrace: Option<oopsie_core::Backtrace>,
}

impl<E: Diagnostic> Report<E> {
    /// Resolve the error's backtrace once. Symbol resolution is the expensive
    /// part of backtrace rendering, so we pay it here rather than on every
    /// `Display`.
    fn resolve_backtrace(res: &Result<(), E>) -> Option<oopsie_core::Backtrace> {
        let backtrace = res.as_ref().err()?.oopsie_backtrace()?.clone();
        backtrace.resolve();
        (!backtrace.frames().is_empty()).then_some(backtrace)
    }

    /// Create a new `Report` wrapping the given error.
    ///
    /// Uses automatic color detection based on environment variables and
    /// terminal detection.
    #[must_use]
    #[inline]
    pub fn new(error: E) -> Self {
        let res = Err(error);
        Self {
            backtrace: Self::resolve_backtrace(&res),
            res,
            color_config: ColorMode::Auto,
            theme_override: None,
        }
    }

    /// Create a new `Report` with a successful result.
    #[must_use]
    #[inline]
    pub const fn ok() -> Self {
        Self {
            res: Ok(()),
            color_config: ColorMode::Auto,
            theme_override: None,
            backtrace: None,
        }
    }

    /// Runs the given function and returns a `Report` with the result.
    ///
    /// The library's [`install_panic_hook`](crate::install_panic_hook) hook is
    /// installed only for the duration of `func` and the previously installed
    /// hook is restored afterwards — including when `func` panics and the panic
    /// is caught by a caller further up. Overlapping calls (concurrent threads
    /// or nested on one thread) share a single installation: the hook present
    /// before the first call is restored when the last one finishes. A hook
    /// installed by other means *while* a `run` is in flight is overwritten by
    /// that restore.
    ///
    /// For the duration of `func` the thread's trace marker is set inside
    /// this call, so rendered traces end at the `run` closure boundary; the
    /// previous marker is restored afterwards. A `start_marker!` set inside
    /// `func` does not outlive it.
    #[must_use]
    pub fn run<F>(func: F) -> Self
    where
        F: FnOnce() -> Result<(), E>,
    {
        crate::panic_hook::acquire_hook();
        // `set_hook` panics on a panicking thread, so restoring from a `Drop`
        // guard would abort during unwind. Catch the unwind instead: the hook
        // has already rendered the panic by then, and `resume_unwind` doesn't
        // re-run it.
        let mut prev_marker = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Set inside the unwind boundary so its plumbing sits below the
            // marker and lands in the frozen suffix.
            prev_marker = oopsie_core::__private::set_marker();
            func()
        }));
        oopsie_core::__private::restore_marker(prev_marker);
        crate::panic_hook::release_hook();
        let result = match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        Self {
            backtrace: Self::resolve_backtrace(&result),
            res: result,
            color_config: ColorMode::Auto,
            theme_override: None,
        }
    }

    /// Disable colors in output.
    #[must_use]
    #[inline]
    pub const fn no_colors(mut self) -> Self {
        self.color_config = ColorMode::Never;
        self
    }

    /// Force colors in output.
    #[must_use]
    #[inline]
    pub const fn force_colors(mut self) -> Self {
        self.color_config = ColorMode::Always;
        self
    }

    /// Render this report with an explicit [`Theme`], overriding the
    /// process-global default set by [`set_theme`](crate::set_theme).
    #[must_use]
    #[inline]
    pub const fn with_theme(mut self, theme: Theme) -> Self {
        self.theme_override = Some(theme);
        self
    }

    /// The theme to render with: the per-report override, else the global.
    #[inline]
    fn resolved_theme(&self) -> Cow<'_, Theme> {
        match self.theme_override.as_ref() {
            Some(theme) => Cow::Borrowed(theme),
            None => Cow::Owned(get_theme()),
        }
    }

    /// Get a reference to the wrapped error.
    #[must_use]
    #[inline]
    pub const fn error(&self) -> Option<&E> {
        match &self.res {
            Err(err) => Some(err),
            Ok(()) => None,
        }
    }

    /// Consume self and return the wrapped error.
    #[must_use]
    #[inline]
    pub fn into_error(self) -> Option<E> {
        self.res.err()
    }

    /// Format the error chain.
    fn write_error_chain(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Error::source is user-implemented and the std contract does not forbid cycles.
        const MAX_SOURCE_CHAIN_DEPTH: usize = 128;

        let c = self.color_config.should_colorize();
        let theme = self.resolved_theme();

        let Err(err) = &self.res else { return Ok(()) };

        let error_code = err.oopsie_error_code();
        let help_text = err.oopsie_help_text();

        // Write main error
        write!(f, "{}", style!("Error", theme.error_title(), c))?;

        if let Some(code) = error_code {
            write!(
                f,
                "{}{}{}",
                style!("[", theme.delimiter(), c),
                style!(code, theme.error_code(), c),
                style!("]", theme.delimiter(), c)
            )?;
        }
        writeln!(f, ": {err}")?;

        // Caller location, directly under the header so it reads even with
        // backtraces disabled.
        if let Some(location) = err.oopsie_location() {
            let at = format_args!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
            writeln!(
                f,
                "  {} {}",
                style!("at", theme.hint(), c),
                style!(at, theme.hint(), c)
            )?;
        }

        // Write error chain
        let mut source = err.source();
        let mut depth = 0_usize;
        while let Some(err) = source {
            if depth == MAX_SOURCE_CHAIN_DEPTH {
                writeln!(
                    f,
                    "  {} (source chain truncated)",
                    style!("╰─▶", theme.cause(), c)
                )?;

                break;
            }
            let next_source = err.source();
            let arrow = if next_source.is_some() {
                "├─▶"
            } else {
                "╰─▶"
            };
            writeln!(f, "  {} {err}", style!(arrow, theme.cause(), c))?;

            source = next_source;
            depth += 1;
        }

        // Write help text if present
        if let Some(help) = help_text {
            writeln!(f, "\n  {}: {help}", style!("help", theme.help(), c))?;
        }

        Ok(())
    }

    /// Format the span trace if available.
    #[cfg(feature = "tracing")]
    fn write_spantrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(span_trace) = self.error().and_then(|e| e.oopsie_spantrace()) else {
            return Ok(());
        };

        writeln!(f)?;
        let printer = TracePrinter::new();
        let printer = if self.color_config.should_colorize() {
            printer.with_theme(self.resolved_theme().trace())
        } else {
            printer.plain()
        };
        printer.write_spantrace(f, span_trace)?;

        Ok(())
    }

    /// Format the backtrace if available and captured.
    ///
    /// Both the colored and plain paths go through [`TracePrinter`]; the plain
    /// path just swaps in the empty theme.
    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(backtrace) = &self.backtrace else {
            return Ok(());
        };

        writeln!(f)?;
        let mut printer = if oopsie_core::rust_backtrace().is_full() {
            TracePrinter::unfiltered()
        } else if let Some(cut) = backtrace.marker_hidden_frames() {
            // Marker cut first (exact bottom); the name filter then trims the
            // runtime frames left above the cut.
            TracePrinter::with_filter(marker_strip_filter(cut))
                .add_frame_filter(error_backtrace_frame_filter)
        } else {
            TracePrinter::new()
        };
        printer = if self.color_config.should_colorize() {
            printer.with_theme(self.resolved_theme().trace())
        } else {
            printer.plain()
        };
        printer.write_backtrace(f, backtrace)?;
        Ok(())
    }
}

impl<E> Termination for Report<E>
where
    E: Diagnostic,
{
    #[expect(
        clippy::print_stderr,
        reason = "Termination renders the error report to stderr at process exit"
    )]
    fn report(self) -> ExitCode {
        match &self.res {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{self}");

                let declared = e
                    .oopsie_exit_code()
                    .map_or(ExitCode::FAILURE, |n| ExitCode::from(n.get()));

                #[cfg(feature = "unstable-error-generic-member-access")]
                {
                    core::error::request_ref::<ExitCode>(e)
                        .copied()
                        .unwrap_or(declared)
                }
                #[cfg(not(feature = "unstable-error-generic-member-access"))]
                {
                    declared
                }
            }
        }
    }
}

#[cfg(feature = "unstable-try-trait-v2")]
impl<T, E: Diagnostic> core::ops::FromResidual<Result<T, E>> for Report<E> {
    fn from_residual(residual: Result<T, E>) -> Self {
        let res = residual.map(drop);
        Self {
            backtrace: Self::resolve_backtrace(&res),
            res,
            color_config: ColorMode::default(),
            theme_override: None,
        }
    }
}

impl<E: Diagnostic> fmt::Display for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_error_chain(f)?;
        #[cfg(feature = "tracing")]
        self.write_spantrace(f)?;
        self.write_backtrace(f)?;
        Ok(())
    }
}

impl<E: Diagnostic> fmt::Debug for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<E: Diagnostic> From<E> for Report<E> {
    #[inline]
    fn from(error: E) -> Self {
        Self::new(error)
    }
}
