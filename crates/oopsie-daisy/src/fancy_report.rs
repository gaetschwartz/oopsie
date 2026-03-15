//! FancyReport for rich, colorized error output.
//!
//! This module provides [`FancyReport`], a wrapper that formats errors with
//! colorized output including the error chain, span traces, and backtraces.

use std::fmt;
use std::process::{ExitCode, Termination};

use owo_colors::OwoColorize as _;

use oopsie_core::ColorConfig;

use oopsie_core::ErrorExt;

use crate::trace_printer::TracePrinter;

/// A wrapper around an error that provides rich, colorized output.
///
/// `FancyReport` extracts backtraces and span traces from the error chain
/// (when available via the Provider API) and formats them with colors.
pub struct FancyReport<E> {
    res: Result<(), E>,
    color_config: ColorConfig,
}

impl<E: ErrorExt> FancyReport<E> {
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
        let Some(span_trace) = self.error().and_then(|e| e.oopsie_spantrace()) else {
            return Ok(());
        };

        writeln!(f)?;
        if self.color_config.should_colorize() {
            TracePrinter::new().write_spantrace(f, span_trace)?;
        } else {
            writeln!(f, "{:━^80}", " SPANTRACE ")?;
            write!(f, "{span_trace}")?;
        }

        Ok(())
    }

    /// Format the backtrace if available and captured.
    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(backtrace) = self.error().and_then(|e| e.oopsie_backtrace()) else {
            return Ok(());
        };

        writeln!(f)?;
        if self.color_config.should_colorize() {
            writeln!(f)?;
            TracePrinter::new().write_backtrace(f, backtrace)?;
        } else {
            writeln!(f, "{:━^80}", " BACKTRACE ")?;
            write!(f, "{backtrace:?}")?;
        }
        Ok(())
    }
}

impl<E> Termination for FancyReport<E>
where
    E: ErrorExt,
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

impl<E: ErrorExt> fmt::Display for FancyReport<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_error_chain(f)?;
        self.write_span_trace(f)?;
        self.write_backtrace(f)?;
        Ok(())
    }
}

impl<E: ErrorExt> fmt::Debug for FancyReport<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<E: ErrorExt> From<E> for FancyReport<E> {
    fn from(error: E) -> Self {
        Self::from_std(error)
    }
}
