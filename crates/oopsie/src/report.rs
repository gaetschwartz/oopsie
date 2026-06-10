//! Report for rich, colorized error output.
//!
//! This module provides [`Report`], a wrapper that formats errors with
//! colorized output including the error chain, span traces, and backtraces.

use std::fmt;
use std::process::{ExitCode, Termination};

use owo_colors::OwoColorize as _;

use crate::ColorConfig;

use crate::Diagnostic;

use crate::trace_printer::TracePrinter;

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
pub struct Report<E> {
    res: Result<(), E>,
    color_config: ColorConfig,
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
    pub fn from_std(error: E) -> Self {
        let res = Err(error);
        Self {
            backtrace: Self::resolve_backtrace(&res),
            res,
            color_config: ColorConfig::auto(),
        }
    }

    /// Create a new `Report` with a successful result.
    #[must_use]
    #[inline]
    pub const fn ok() -> Self {
        Self {
            res: Ok(()),
            color_config: ColorConfig::auto(),
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(func));
        crate::panic_hook::release_hook();
        let result = match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        };
        Self {
            backtrace: Self::resolve_backtrace(&result),
            res: result,
            color_config: ColorConfig::auto(),
        }
    }

    /// Create with explicit color configuration.
    #[must_use]
    #[inline]
    pub fn with_colors(error: E, color_config: ColorConfig) -> Self {
        let res = Err(error);
        Self {
            backtrace: Self::resolve_backtrace(&res),
            res,
            color_config,
        }
    }

    /// Disable colors in output.
    #[must_use]
    #[inline]
    pub const fn no_colors(mut self) -> Self {
        self.color_config = ColorConfig::Never;
        self
    }

    /// Force colors in output.
    #[must_use]
    #[inline]
    pub const fn force_colors(mut self) -> Self {
        self.color_config = ColorConfig::Always;
        self
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
        /// Matches erased-oopsie's MAX_SOURCE_CHAIN_DEPTH: Error::source is
        /// user-implemented and the std contract does not forbid cycles.
        const MAX_SOURCE_CHAIN_DEPTH: usize = 128;

        let colors_enabled = self.color_config.should_colorize();
        let Err(err) = &self.res else { return Ok(()) };

        let error_code = err.oopsie_error_code();
        let help_text = err.oopsie_help_text();

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
        let mut depth = 0_usize;
        while let Some(err) = source {
            if depth == MAX_SOURCE_CHAIN_DEPTH {
                if colors_enabled {
                    writeln!(f, "  {} (source chain truncated)", "╰─▶".yellow())?;
                } else {
                    writeln!(f, "  ╰─▶ (source chain truncated)")?;
                }
                break;
            }
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
            depth += 1;
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
        let Some(span_trace) = self.error().and_then(|e| e.oopsie_spantrace()) else {
            return Ok(());
        };
        // An empty/unsupported trace yields zero frames; emitting the header
        // would leave a lone `SPANTRACE` banner with no body. Mirror the
        // backtrace path, which suppresses empty traces.
        if !span_trace.is_captured() {
            return Ok(());
        }

        writeln!(f)?;
        let mut printer = TracePrinter::new();
        if !self.color_config.should_colorize() {
            printer = printer.plain();
        }
        printer.write_spantrace(f, span_trace)?;

        Ok(())
    }

    /// Format the backtrace if available and captured.
    ///
    /// Both the colored and plain paths go through [`TracePrinter`]; the plain
    /// path just swaps in the empty theme. This avoids the upstream `backtrace`
    /// Debug formatter, which calls `std::env::current_dir()` (a filesystem
    /// syscall) on every render.
    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(backtrace) = &self.backtrace else {
            return Ok(());
        };

        writeln!(f)?;
        let mut printer = if oopsie_core::rust_backtrace().is_full() {
            TracePrinter::unfiltered()
        } else {
            TracePrinter::new()
        };
        if !self.color_config.should_colorize() {
            printer = printer.plain();
        }
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

                #[cfg(feature = "unstable-error-generic-member-access")]
                {
                    core::error::request_ref::<ExitCode>(e)
                        .copied()
                        .unwrap_or(ExitCode::FAILURE)
                }
                #[cfg(not(feature = "unstable-error-generic-member-access"))]
                {
                    let _ = e;
                    ExitCode::FAILURE
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
            color_config: ColorConfig::default(),
        }
    }
}

impl<E: Diagnostic> fmt::Display for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_error_chain(f)?;
        self.write_span_trace(f)?;
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
        Self::from_std(error)
    }
}
