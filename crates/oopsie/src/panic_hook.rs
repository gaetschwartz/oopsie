//! A custom panic hook that renders panics through [`TracePrinter`].
//!
//! It captures a backtrace and span trace at panic time and renders them with
//! the same colored machinery used by [`Report`](crate::Report), so panic
//! output matches the library's error output.

use std::fmt;
use std::panic::PanicHookInfo;
use std::sync::{Mutex, PoisonError};

use owo_colors::Style;

use oopsie_core::Backtrace;
use oopsie_core::Capturable as _;
#[cfg(feature = "tracing")]
use oopsie_core::SpanTrace;

use crate::ColorConfig;
use crate::color::style;
use crate::trace_printer::{TracePrinter, TraceTheme, marker_strip_filter, panic_frame_filter};

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

type PriorHook = Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>;

/// The panic hook is process-global, so overlapping `Report::run` scopes
/// (across threads or nested on one thread) must refcount the swap: only the
/// outermost acquire saves the prior hook and installs ours, and only the
/// matching last release restores it. Unbalanced take/set pairs would either
/// leak our hook or drop the user's.
struct HookGuard {
    depth: usize,
    prior: Option<PriorHook>,
}

static HOOK_GUARD: Mutex<HookGuard> = Mutex::new(HookGuard {
    depth: 0,
    prior: None,
});

pub fn acquire_hook() {
    let mut guard = HOOK_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.depth == 0 {
        guard.prior = Some(std::panic::take_hook());
        install_panic_hook();
    }
    guard.depth += 1;
}

/// Must only be called on a non-panicking thread: `set_hook` aborts during
/// unwind, so callers restore after `catch_unwind`, never from `Drop`.
pub fn release_hook() {
    let mut guard = HOOK_GUARD.lock().unwrap_or_else(PoisonError::into_inner);
    guard.depth -= 1;
    if guard.depth == 0
        && let Some(prior) = guard.prior.take()
    {
        std::panic::set_hook(prior);
    }
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
    #[cfg(feature = "tracing")]
    span_trace: Option<SpanTrace>,
    color_config: ColorConfig,
}

impl<'a> PanicReport<'a> {
    fn new(info: &'a PanicHookInfo<'a>) -> Self {
        let backtrace_setting = oopsie_core::rust_panic_backtrace();
        let backtrace =
            oopsie_core::with_rust_backtrace_override(backtrace_setting, || Backtrace::capture());
        #[cfg(feature = "tracing")]
        let span_trace = {
            let captured = SpanTrace::capture();
            captured.is_captured().then_some(captured)
        };
        Self {
            info,
            backtrace,
            backtrace_setting,
            #[cfg(feature = "tracing")]
            span_trace,
            color_config: ColorConfig::Auto,
        }
    }

    fn write_header(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = self.color_config.should_colorize();
        // `PanicHookInfo::payload_as_str` would be cleaner but postdates the
        // crate's MSRV; downcast manually instead.
        let payload = self.info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s
        } else {
            "<non-string panic payload>"
        };

        write!(
            f,
            "{}\n\
            Message:  {}\n\
            Location: ",
            style!("The application panicked (crashed).", HEADER_STYLE, c),
            style!(message, MESSAGE_STYLE, c)
        )?;

        match self.info.location() {
            Some(loc) => {
                let location = format_args!("{}:{}:{}", loc.file(), loc.line(), loc.column());
                writeln!(f, "{}", style!(location, LOCATION_STYLE, c))?;
            }
            None => writeln!(f, "<unknown>")?,
        }

        Ok(())
    }

    #[cfg(feature = "tracing")]
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
        let c = self.color_config.should_colorize();

        writeln!(f)?;

        // When disabled there are no frames to render, so point the user at
        // the env var instead — mirroring std's default panic message.
        if !self.backtrace_setting.is_enabled() {
            writeln!(
                f,
                "{}",
                style!(
                    "note: run with `RUST_BACKTRACE=1` to display a backtrace",
                    HINT_STYLE,
                    c
                )
            )?;

            return Ok(());
        }

        // `full` means "show everything"; otherwise apply the panic-aware filter
        // that trims the panic plumbing above the call site and the runtime tail
        // below `main` — anchored on the exact marker cut when one was captured.
        let mut printer = if self.backtrace_setting.is_full() {
            TracePrinter::unfiltered()
        } else if let Some(cut) = self.backtrace.marker_hidden_frames() {
            // Marker cut first (exact bottom); panic_frame_filter then trims
            // the panic plumbing on top and the residue left above the cut.
            TracePrinter::with_filter_and_theme(marker_strip_filter(cut), TraceTheme::DEFAULT)
                .add_frame_filter(panic_frame_filter)
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
        #[cfg(feature = "tracing")]
        self.write_span_trace(f)?;
        self.write_backtrace(f)?;
        Ok(())
    }
}
