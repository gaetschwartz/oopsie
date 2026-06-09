//! A custom panic hook that renders panics through [`TracePrinter`].
//!
//! It captures a backtrace and span trace at panic time and renders them with
//! the same colored machinery used by [`Report`](crate::Report), so panic
//! output matches the library's error output.

use std::fmt;
use std::panic::PanicHookInfo;

use owo_colors::{OwoColorize as _, Style};

use oopsie_core::Capturable as _;
use oopsie_core::{Backtrace, SpanTrace};

use crate::ColorConfig;
use crate::trace_printer::{TracePrinter, TraceTheme, panic_frame_filter};

const HEADER_STYLE: Style = Style::new().red().bold();
const MESSAGE_STYLE: Style = Style::new().bright_cyan();
const LOCATION_STYLE: Style = Style::new().purple();
const HINT_STYLE: Style = Style::new().dimmed();

/// Install a process-global panic hook that renders panics with a colored
/// message, span trace, and backtrace via [`TracePrinter`].
///
/// Replaces any previously installed hook through `std::panic::set_hook`, so
/// call it once early in `main`, before spawning threads. Backtrace capture
/// follows std's panic semantics: `RUST_BACKTRACE` only. `RUST_LIB_BACKTRACE`
/// intentionally has no effect on panic output. With capture disabled, only the
/// message and location are shown plus a hint to enable it.
#[expect(
    clippy::print_stderr,
    reason = "a panic hook renders the crash report to stderr, like std's default hook"
)]
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprint!("{}", PanicReport::new(info));
    }));
}

/// A renderable view over a single panic. Captures the backtrace and span trace
/// eagerly at construction — i.e. inside the panic hook, while the panicking
/// stack is still live — and defers only formatting to [`fmt::Display`].
struct PanicReport<'a> {
    info: &'a PanicHookInfo<'a>,
    backtrace: Backtrace,
    /// Resolved once from the panic-path env semantics (`RUST_BACKTRACE`
    /// only) and forced as the capture setting, so library-level backtrace
    /// settings never affect panic capture.
    backtrace_setting: oopsie_core::RustBacktrace,
    span_trace: Option<SpanTrace>,
    color_config: ColorConfig,
}

impl<'a> PanicReport<'a> {
    fn new(info: &'a PanicHookInfo<'a>) -> Self {
        let backtrace_setting = oopsie_core::rust_panic_backtrace();
        let backtrace =
            oopsie_core::with_rust_backtrace_override(backtrace_setting, || Backtrace::capture());
        let span_trace = {
            let captured = SpanTrace::capture();
            captured.is_captured().then_some(captured)
        };
        Self {
            info,
            backtrace,
            backtrace_setting,
            span_trace,
            color_config: ColorConfig::auto(),
        }
    }

    fn write_header(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let colorize = self.color_config.should_colorize();
        // `PanicHookInfo::payload_as_str` would be cleaner but postdates the
        // crate's MSRV; downcast manually instead.
        let payload = self.info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");

        let header = "The application panicked (crashed).";
        if colorize {
            writeln!(f, "{}", header.style(HEADER_STYLE))?;
            writeln!(f, "Message:  {}", message.style(MESSAGE_STYLE))?;
        } else {
            writeln!(f, "{header}")?;
            writeln!(f, "Message:  {message}")?;
        }

        write!(f, "Location: ")?;
        match self.info.location() {
            Some(loc) => {
                let location = format_args!("{}:{}:{}", loc.file(), loc.line(), loc.column());
                if colorize {
                    writeln!(f, "{}", location.style(LOCATION_STYLE))?;
                } else {
                    writeln!(f, "{location}")?;
                }
            }
            None => writeln!(f, "<unknown>")?,
        }

        Ok(())
    }

    fn write_span_trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(span_trace) = &self.span_trace else {
            return Ok(());
        };

        writeln!(f)?;
        let mut printer = TracePrinter::new();
        if !self.color_config.should_colorize() {
            printer = printer.plain();
        }
        printer.write_spantrace(f, span_trace)?;
        Ok(())
    }

    fn write_backtrace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;

        // When disabled there are no frames to render, so point the user at
        // the env var instead — mirroring std's default panic message.
        if !self.backtrace_setting.is_enabled() {
            let hint = "note: run with `RUST_BACKTRACE=1` to display a backtrace";
            if self.color_config.should_colorize() {
                writeln!(f, "{}", hint.style(HINT_STYLE))?;
            } else {
                writeln!(f, "{hint}")?;
            }
            return Ok(());
        }

        // `full` means "show everything"; otherwise apply the panic-aware filter
        // that trims the panic plumbing above the call site and the runtime tail
        // below `main`.
        let mut printer = if self.backtrace_setting.is_full() {
            TracePrinter::unfiltered()
        } else {
            TracePrinter::with_filter_and_theme(panic_frame_filter, TraceTheme::DEFAULT)
        };
        if !self.color_config.should_colorize() {
            printer = printer.plain();
        }
        printer.write_backtrace(f, &self.backtrace)
    }
}

impl fmt::Display for PanicReport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_header(f)?;
        self.write_span_trace(f)?;
        self.write_backtrace(f)?;
        Ok(())
    }
}
