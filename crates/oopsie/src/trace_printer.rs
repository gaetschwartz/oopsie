//! Unified colored trace rendering for backtraces and span traces.
//!
//! This module provides [`TracePrinter`], which renders backtraces and span
//! traces with configurable colors via [`TraceTheme`]. The rendering follows
//! the style of `color-backtrace` and `color-spantrace` but uses `owo_colors`
//! directly, eliminating the need for those dependencies.

use std::fmt;
use std::ops::Deref;
use std::path;

use owo_colors::{OwoColorize as _, Style};

use crate::Backtrace;

// ─────────────────────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────────────────────

/// A single frame from a backtrace.
pub struct BacktraceFrame {
    /// Instruction pointer of the physical frame this symbol belongs to.
    /// Inline expansion gives several rendered frames the same `ip`.
    pub ip: usize,
    pub name: Option<Box<str>>,
    pub filename: Option<Box<path::Path>>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
}

/// Metadata for a single span in a span trace.
pub struct SpanMetadata<'a> {
    pub name: &'a str,
    pub target: &'a str,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider traits
// ─────────────────────────────────────────────────────────────────────────────

/// Trait for types that can provide backtrace frames.
pub trait BacktraceProvider {
    fn frames(&self) -> Vec<BacktraceFrame>;
}

/// Trait for types that can provide span trace information.
pub trait SpanTraceProvider {
    fn with_spans(&self, f: &mut dyn FnMut(&SpanMetadata<'_>, &str) -> bool);
}

// ─────────────────────────────────────────────────────────────────────────────
// Implementations for core types
// ─────────────────────────────────────────────────────────────────────────────

impl BacktraceProvider for Backtrace {
    fn frames(&self) -> Vec<BacktraceFrame> {
        Self::frames(self)
            .iter()
            .flat_map(|frame| {
                frame.symbols().iter().map(|sym| BacktraceFrame {
                    ip: frame.ip() as usize,
                    name: sym.name().map(|s| s.to_string().into_boxed_str()),
                    filename: sym.filename().map(Box::from),
                    lineno: sym.lineno(),
                    colno: sym.colno(),
                })
            })
            .collect()
    }
}

#[cfg(feature = "tracing")]
impl SpanTraceProvider for crate::SpanTrace {
    fn with_spans(&self, f: &mut dyn FnMut(&SpanMetadata<'_>, &str) -> bool) {
        self.as_span_trace().with_spans(|md, fields| {
            let meta = SpanMetadata {
                name: md.name(),
                target: md.target(),
                file: md.file(),
                line: md.line(),
            };
            f(&meta, fields)
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Theme
// ─────────────────────────────────────────────────────────────────────────────

/// Color theme for trace rendering.
pub struct TraceTheme {
    pub frame_number: Style,
    pub function_name: Style,
    pub function_hash: Style,
    pub file_path: Style,
    pub line_number: Style,
    pub separator: Style,
    pub fields: Style,
    pub header: Style,
    pub frames_hidden: Style,
}

impl TraceTheme {
    /// The default color theme used for trace rendering.
    pub const DEFAULT: Self = Self {
        frame_number: Style::new().dimmed(),
        function_name: Style::new().bright_red(),
        function_hash: Style::new().bright_black(),
        file_path: Style::new().purple(),
        line_number: Style::new().purple(),
        separator: Style::new().dimmed(),
        fields: Style::new().bright_cyan(),
        header: Style::new().red(),
        frames_hidden: Style::new().cyan(),
    };

    /// A theme that applies no styling — an empty `Style` emits no ANSI codes,
    /// so this renders identically to the colored path minus the colors.
    pub const PLAIN: Self = Self {
        frame_number: Style::new(),
        function_name: Style::new(),
        function_hash: Style::new(),
        file_path: Style::new(),
        line_number: Style::new(),
        separator: Style::new(),
        fields: Style::new(),
        header: Style::new(),
        frames_hidden: Style::new(),
    };
}

impl Default for TraceTheme {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame filtering
// ─────────────────────────────────────────────────────────────────────────────

/// Default frame filter for error backtraces.
///
/// This filter:
/// 1. Removes the runtime tail from the bottom: a contiguous run of
///    runtime-init/nameless frames. Stopping at the first real user frame
///    keeps frames below a user's own mid-stack `catch_unwind` intact —
///    the cost is that an unrecognized tail spelling leaks frames instead
///    of hiding user code.
/// 2. Skips frames from the top that are backtrace capture machinery.
pub fn error_backtrace_frame_filter(frames: &mut Vec<&BacktraceFrame>) {
    let mut keep = frames.len();
    while keep > 0 {
        let frame = frames[keep - 1];
        let internal = match frame.name.as_ref() {
            Some(name) => {
                oopsie_core::__private::is_runtime_tail_code(name, frame.filename.as_deref())
            }
            // Unresolvable frames are runtime/shim detail (`__rust_try` etc.).
            None => true,
        };
        if !internal {
            break;
        }
        keep -= 1;
    }
    // A fully symbol-stripped trace would peel to nothing; show it instead.
    if keep > 0 {
        frames.truncate(keep);
    }

    let top_cutoff_idx = frames
        .iter()
        .rposition(|frame| {
            frame.name.as_ref().is_some_and(|name| {
                oopsie_core::__private::is_backtrace_capture_code(name, frame.filename.as_deref())
            })
        })
        .map(|idx| idx + 1);
    if let Some(top) = top_cutoff_idx {
        frames.drain(..top);
    }
}

/// Trims the panic runtime that sits *above* the user's `panic!` call site.
///
/// A panicking stack carries the panic hook's own frames plus the `core`/`std`
/// panic-raising plumbing above the user code that triggered the panic. This
/// drains everything down to (and including) the last such frame so the user's
/// call site becomes the first frame. It does not touch the bottom of the
/// stack — pair it with [`error_backtrace_frame_filter`] (see
/// [`panic_frame_filter`]) to also trim the runtime-init tail.
pub fn post_panic_frame_filter(frames: &mut Vec<&BacktraceFrame>) {
    let top_cutoff_idx = frames
        .iter()
        .rposition(|frame| {
            frame.name.as_ref().is_some_and(|name| {
                oopsie_core::__private::is_post_panic_code(name, frame.filename.as_deref())
            })
        })
        .map(|idx| idx + 1);
    if let Some(top) = top_cutoff_idx {
        frames.drain(..top);
    }
}

/// Frame filter for panic backtraces.
///
/// Trims the panic-raising plumbing above the user's `panic!` site
/// ([`post_panic_frame_filter`]) and then the runtime-init tail below `main`
/// ([`error_backtrace_frame_filter`]). The top trim runs first so the unwind
/// entry frame never reaches the bottom peel.
pub fn panic_frame_filter(frames: &mut Vec<&BacktraceFrame>) {
    post_panic_frame_filter(frames);
    error_backtrace_frame_filter(frames);
}

/// Split a function name into (base, hash_suffix).
/// The hash suffix is `::h` followed by exactly 16 hex characters at the end.
fn split_function_hash(name: &str) -> (&str, Option<&str>) {
    // Look for ::h followed by exactly 16 hex chars at end.
    if name.len() >= 20 {
        let suffix_start = name.len() - 19; // "::h" (3) + 16 hex chars
        // A valid hash suffix is all ASCII, so a `suffix_start` that lands mid
        // UTF-8 char (demangled symbols can be non-ASCII) can never match —
        // bail out rather than panic on the slice.
        if name.is_char_boundary(suffix_start) {
            let candidate = &name[suffix_start..];
            if candidate.starts_with("::h")
                && candidate[3..].len() == 16
                && candidate[3..].chars().all(|c| c.is_ascii_hexdigit())
            {
                return (&name[..suffix_start], Some(candidate));
            }
        }
    }
    (name, None)
}

// ─────────────────────────────────────────────────────────────────────────────
// TracePrinter
// ─────────────────────────────────────────────────────────────────────────────

/// A closure that filters backtrace frames in-place.
type FrameFilterBox = BoxOrBorrow<'static, dyn Fn(&mut Vec<&BacktraceFrame>)>;

/// Renders backtraces and span traces with colors.
pub struct TracePrinter {
    theme: TraceTheme,
    frame_filter: FrameFilterBox,
}

impl TracePrinter {
    /// Default `TracePrinter` with the default theme and frame filter.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self::with_filter_and_theme_const(&error_backtrace_frame_filter, TraceTheme::DEFAULT)
    }

    /// Create a new `TracePrinter` with the default theme and no frame filtering.
    ///
    /// Every captured frame is rendered. Used to honor `RUST_BACKTRACE=full`.
    #[must_use]
    #[inline]
    pub const fn unfiltered() -> Self {
        Self::with_filter_and_theme_const(&noop_frame_filter, TraceTheme::DEFAULT)
    }

    /// Create a new `TracePrinter` with a custom frame filter and theme.
    #[must_use]
    #[inline]
    pub fn with_filter_and_theme(
        filter: impl Fn(&mut Vec<&BacktraceFrame>) + 'static,
        theme: TraceTheme,
    ) -> Self {
        Self {
            frame_filter: BoxOrBorrow::Box(Box::new(filter)),
            theme,
        }
    }

    /// `const` version of `[with_filter_and_theme]`.
    #[must_use]
    #[inline]
    pub const fn with_filter_and_theme_const(
        filter: &'static (dyn Fn(&mut Vec<&BacktraceFrame>) + 'static),
        theme: TraceTheme,
    ) -> Self {
        Self {
            frame_filter: BoxOrBorrow::Borrow(filter),
            theme,
        }
    }

    /// Add a frame filter for backtrace rendering.
    #[must_use]
    pub fn add_frame_filter(
        mut self,
        filter: impl Fn(&mut Vec<&BacktraceFrame>) + 'static,
    ) -> Self {
        self.frame_filter =
            overlay_frame_filters(self.frame_filter, BoxOrBorrow::Box(Box::new(filter)));
        self
    }

    /// Switch to the plain (uncolored) theme, keeping the frame filter.
    #[must_use]
    #[inline]
    pub const fn plain(mut self) -> Self {
        self.theme = TraceTheme::PLAIN;
        self
    }

    /// Render a colored backtrace.
    pub fn write_backtrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        bt: &impl BacktraceProvider,
    ) -> fmt::Result {
        let all_frames = bt.frames();
        let total_count = all_frames.len();

        let mut filtered: Vec<&BacktraceFrame> = all_frames.iter().collect();
        (self.frame_filter)(&mut filtered);

        // Split the hidden count by which end frames were trimmed from, so the
        // notice renders where the gap actually is. Kept frames still point into
        // `all_frames`, so the last one's original index gives the bottom count;
        // any remainder (including holes a custom filter punches) goes on top.
        let total_hidden = total_count - filtered.len();
        let bottom_hidden = match filtered.last() {
            Some(last) => {
                let last_idx = all_frames
                    .iter()
                    .rposition(|frame| std::ptr::eq(frame, *last))
                    .unwrap_or(total_count - 1);
                total_count - 1 - last_idx
            }
            None => 0,
        };
        let top_hidden = total_hidden - bottom_hidden;

        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " BACKTRACE ").style(self.theme.header)
        )?;

        if top_hidden > 0 {
            writeln!(
                f,
                "{}",
                format_args!("   ... {top_hidden} frames hidden ...")
                    .style(self.theme.frames_hidden)
            )?;
        }

        for (i, frame) in filtered.iter().enumerate() {
            self.write_backtrace_frame(f, i + 1, frame)?;
        }

        if bottom_hidden > 0 {
            writeln!(
                f,
                "{}",
                format_args!("   ... {bottom_hidden} frames hidden ...")
                    .style(self.theme.frames_hidden)
            )?;
        }

        Ok(())
    }

    /// Render a single backtrace frame.
    fn write_backtrace_frame(
        &self,
        f: &mut fmt::Formatter<'_>,
        number: usize,
        frame: &BacktraceFrame,
    ) -> fmt::Result {
        // Frame number: right-aligned in 3 chars
        write!(
            f,
            "{}{}",
            format_args!("{number:>3}").style(self.theme.frame_number),
            ": ".style(self.theme.separator)
        )?;

        // Function name
        if let Some(name) = &frame.name {
            let (base, hash) = split_function_hash(name);
            write!(f, "{}", base.style(self.theme.function_name))?;
            if let Some(h) = hash {
                write!(f, "{}", h.style(self.theme.function_hash))?;
            }
        } else {
            write!(f, "{}", "<unknown>".style(self.theme.function_name))?;
        }
        writeln!(f)?;

        // File location
        if let Some(filename) = &frame.filename {
            write!(
                f,
                "           {}{}",
                "at ".style(self.theme.separator),
                filename.display().style(self.theme.file_path)
            )?;
            if let Some(lineno) = frame.lineno {
                write!(
                    f,
                    "{}",
                    format_args!(":{lineno}").style(self.theme.line_number)
                )?;
                if let Some(colno) = frame.colno {
                    write!(
                        f,
                        "{}",
                        format_args!(":{colno}").style(self.theme.line_number)
                    )?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }

    /// Render a colored span trace.
    pub fn write_spantrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        st: &impl SpanTraceProvider,
    ) -> fmt::Result {
        // Header
        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " SPANTRACE ").style(self.theme.header)
        )?;

        let mut index = 1usize;
        let mut err = Ok(());

        st.with_spans(&mut |meta, fields| {
            if let Err(e) = self.write_span_frame(f, index, meta, fields) {
                err = Err(e);
                return false;
            }
            index += 1;
            true
        });

        err
    }

    /// Render a single span trace frame.
    fn write_span_frame(
        &self,
        f: &mut fmt::Formatter<'_>,
        index: usize,
        meta: &SpanMetadata,
        fields: &str,
    ) -> fmt::Result {
        // Frame number
        write!(
            f,
            "{}",
            format_args!("{index:>3}").style(self.theme.frame_number)
        )?;
        write!(f, "{}", ": ".style(self.theme.separator))?;

        // target::name
        write!(
            f,
            "{}",
            format_args!("{}::{}", meta.target, meta.name).style(self.theme.function_name)
        )?;
        writeln!(f)?;

        // Fields line (only if non-empty)
        if !fields.is_empty() {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "with ".style(self.theme.separator))?;
            writeln!(f, "{}", fields.style(self.theme.fields))?;
        }

        // File location
        if let Some(file) = &meta.file {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "at ".style(self.theme.separator))?;
            write!(f, "{}", file.style(self.theme.file_path))?;
            if let Some(line) = meta.line {
                write!(
                    f,
                    "{}",
                    format_args!(":{line}").style(self.theme.line_number)
                )?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl Default for TracePrinter {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

enum BoxOrBorrow<'a, T: ?Sized> {
    Borrow(&'a T),
    Box(Box<T>),
}
impl<T: ?Sized> BoxOrBorrow<'_, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        match self {
            BoxOrBorrow::Borrow(b) => b,
            BoxOrBorrow::Box(b) => b,
        }
    }
}
impl<T: ?Sized> Deref for BoxOrBorrow<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

/// A frame filter that keeps every frame.
const fn noop_frame_filter(_frames: &mut Vec<&BacktraceFrame>) {}

/// Frame filter hiding a marker-stripped tail: trailing frames whose physical
/// `ip` is in `hidden` (see [`Backtrace::marker_hidden_ips`]). Inline runs
/// sharing one `ip` are consumed together. Never empties the list.
///
/// [`Backtrace::marker_hidden_ips`]: oopsie_core::Backtrace::marker_hidden_ips
pub fn marker_strip_filter(hidden: Vec<usize>) -> impl Fn(&mut Vec<&BacktraceFrame>) {
    move |frames: &mut Vec<&BacktraceFrame>| {
        let mut keep = frames.len();
        for &ip in hidden.iter().rev() {
            let run_start = keep;
            while keep > 0 && frames[keep - 1].ip == ip {
                keep -= 1;
            }
            if keep == run_start {
                break;
            }
        }
        if keep > 0 {
            frames.truncate(keep);
        }
    }
}

fn overlay_frame_filters(under: FrameFilterBox, above: FrameFilterBox) -> FrameFilterBox {
    BoxOrBorrow::Box(Box::new(move |frames: &mut Vec<&BacktraceFrame>| {
        under.as_ref()(frames);
        above.as_ref()(frames);
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(name: Option<impl Into<String>>, lineno: Option<u32>) -> BacktraceFrame {
        BacktraceFrame {
            ip: 0,
            name: name.map(|s| s.into().into_boxed_str()),
            filename: None,
            lineno,
            colno: None,
        }
    }

    #[test]
    fn test_error_backtrace_frame_filter() {
        let capture = make_frame(Some("std::backtrace_rs::backtrace::libunwind::trace"), None);
        let app1 = make_frame(Some("my_crate::function_a"), None);
        let app2 = make_frame(Some("my_crate::function_b"), None);
        let runtime = make_frame(Some("std::rt::lang_start_internal::invoke"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&capture, &app1, &app2, &runtime];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::function_a"));
        assert_eq!(frames[1].name.as_deref(), Some("my_crate::function_b"));
    }

    #[test]
    fn test_error_backtrace_frame_filter_no_capture_no_runtime() {
        let app1 = make_frame(Some("my_crate::function_a"), None);
        let app2 = make_frame(Some("my_crate::function_b"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&app1, &app2];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::function_a"));
        assert_eq!(frames[1].name.as_deref(), Some("my_crate::function_b"));
    }

    #[test]
    fn test_error_backtrace_frame_filter_multiple_capture_frames() {
        let capture1 = make_frame(Some("<std::backtrace::Backtrace>::create::something"), None);
        let capture2 = make_frame(Some("std::backtrace_rs::backtrace::libunwind::trace"), None);
        let app = make_frame(Some("my_crate::function_a"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&capture1, &capture2, &app];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::function_a"));
    }

    #[test]
    fn test_post_panic_frame_filter() {
        // Top-of-stack order on a panic: the hook closure, std/core panic
        // plumbing, then the user's `panic!` site, then user code.
        let hook = make_frame(
            Some("oopsie::panic_hook::install_panic_hook::{{closure}}"),
            None,
        );
        let rust_panic = make_frame(Some("std::panicking::rust_panic_with_hook"), None);
        let begin = make_frame(
            Some("std::panicking::begin_panic_handler::{{closure}}"),
            None,
        );
        let panic_fmt = make_frame(Some("core::panicking::panic_fmt"), None);
        let user_site = make_frame(Some("my_crate::do_work"), None);
        let user_main = make_frame(Some("my_crate::main"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![
            &hook,
            &rust_panic,
            &begin,
            &panic_fmt,
            &user_site,
            &user_main,
        ];
        post_panic_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::do_work"));
        assert_eq!(frames[1].name.as_deref(), Some("my_crate::main"));
    }

    #[test]
    fn test_post_panic_frame_filter_no_panic_frames() {
        // With no panic-runtime frames present the filter is a no-op.
        let app1 = make_frame(Some("my_crate::function_a"), None);
        let app2 = make_frame(Some("my_crate::function_b"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&app1, &app2];
        post_panic_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::function_a"));
    }

    #[test]
    fn test_panic_frame_filter_trims_both_ends() {
        // A realistic panic stack: capture machinery + the unwind entry above
        // user code, and the runtime-init tail (with `catch_unwind` frames that
        // must NOT pull the top cut down) below `main`.
        let capture = make_frame(Some("std::backtrace_rs::backtrace::libunwind::trace"), None);
        let unwind = make_frame(Some("__rustc[ab12cd34]::rust_begin_unwind"), None);
        let panic_fmt = make_frame(Some("core[ab12cd34]::panicking::panic_fmt"), None);
        let user_site = make_frame(Some("my_crate::parse_header"), None);
        let user_main = make_frame(Some("my_crate::main"), None);
        let begin = make_frame(Some("__rust_begin_short_backtrace<fn(), ()>"), None);
        let catch = make_frame(
            Some("std[ab12cd34]::panicking::catch_unwind::do_call"),
            None,
        );
        let main_c = make_frame(Some("_main"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![
            &capture, &unwind, &panic_fmt, &user_site, &user_main, &begin, &catch, &main_c,
        ];
        panic_frame_filter(&mut frames);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::parse_header"));
        assert_eq!(frames[1].name.as_deref(), Some("my_crate::main"));
    }

    #[test]
    fn test_split_function_hash() {
        let (base, hash) = split_function_hash("my_crate::foo::hab1234567890abcd");
        assert_eq!(base, "my_crate::foo");
        assert_eq!(hash, Some("::hab1234567890abcd"));

        let (base, hash) = split_function_hash("my_crate::foo");
        assert_eq!(base, "my_crate::foo");
        assert_eq!(hash, None);

        // Not exactly 16 hex chars
        let (base, hash) = split_function_hash("my_crate::foo::habcd");
        assert_eq!(base, "my_crate::foo::habcd");
        assert_eq!(hash, None);
    }

    #[test]
    fn bottom_trim_spares_user_frames_below_mid_stack_catch_unwind() {
        let app_top = make_frame(Some("my_crate::inner_work"), None);
        let catch = make_frame(Some("std::panic::catch_unwind::do_call"), None);
        let supervisor = make_frame(Some("my_crate::supervisor"), None);
        let runtime = make_frame(Some("std::rt::lang_start_internal"), None);

        let mut frames: Vec<&BacktraceFrame> = vec![&app_top, &catch, &supervisor, &runtime];
        error_backtrace_frame_filter(&mut frames);

        // The contiguous peel stops at `supervisor`; only the true tail goes.
        assert_eq!(
            frames
                .iter()
                .map(|f| f.name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            [
                "my_crate::inner_work",
                "std::panic::catch_unwind::do_call",
                "my_crate::supervisor"
            ],
        );
    }

    #[test]
    fn bottom_trim_peels_nameless_frames_in_the_tail() {
        let app = make_frame(Some("my_crate::function_a"), None);
        let runtime = make_frame(Some("std::rt::lang_start"), None);
        let nameless = make_frame(None::<String>, None);

        let mut frames: Vec<&BacktraceFrame> = vec![&app, &runtime, &nameless];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].name.as_deref(), Some("my_crate::function_a"));
    }

    #[test]
    fn bottom_trim_keeps_everything_when_all_frames_are_nameless() {
        let a = make_frame(None::<String>, None);
        let b = make_frame(None::<String>, None);

        let mut frames: Vec<&BacktraceFrame> = vec![&a, &b];
        error_backtrace_frame_filter(&mut frames);

        // A fully symbol-stripped trace must not be trimmed to nothing.
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn split_function_hash_non_ascii_does_not_panic() {
        // 2-byte `é` at bytes 0..2 then 18 ASCII bytes: len is 20 and the
        // `len - 19 == 1` slice offset lands mid-char. Must not panic.
        let name = format!("é{}", "a".repeat(18));
        assert_eq!(name.len(), 20);
        assert!(!name.is_char_boundary(1));
        let (base, hash) = split_function_hash(&name);
        assert_eq!(base, name);
        assert_eq!(hash, None);
    }

    fn make_frame_at(ip: usize, name: &str) -> BacktraceFrame {
        BacktraceFrame {
            ip,
            name: Some(name.to_owned().into_boxed_str()),
            filename: None,
            lineno: None,
            colno: None,
        }
    }

    #[test]
    fn marker_strip_hides_only_the_trailing_hidden_run() {
        let a = make_frame_at(1, "my_crate::a");
        let mid = make_frame_at(7, "my_crate::mid"); // ip 7 hidden, but not trailing
        let b = make_frame_at(2, "my_crate::b");
        let tail1 = make_frame_at(7, "std::rt::whatever");
        let tail2 = make_frame_at(8, "std::rt::deeper");

        let filter = marker_strip_filter(vec![7, 8]);
        let mut frames: Vec<&BacktraceFrame> = vec![&a, &mid, &b, &tail1, &tail2];
        filter(&mut frames);

        assert_eq!(
            frames.iter().map(|f| f.ip).collect::<Vec<_>>(),
            [1, 7, 2],
            "mid-stack frame with a hidden ip must survive; only the tail goes"
        );
    }

    #[test]
    fn marker_strip_consumes_inline_runs_sharing_one_ip() {
        let user = make_frame_at(1, "my_crate::a");
        // One physical frame expanded into two rendered frames by inlining.
        let inline_a = make_frame_at(9, "std::rt::outer_inlined");
        let inline_b = make_frame_at(9, "std::rt::inner_inlined");

        let filter = marker_strip_filter(vec![9]);
        let mut frames: Vec<&BacktraceFrame> = vec![&user, &inline_a, &inline_b];
        filter(&mut frames);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].ip, 1);
    }

    #[test]
    fn marker_strip_is_ordered_not_membership() {
        // Visible recursion frames (ips 7,8) above the hidden tail (7,8): ordered
        // consumption removes exactly the trailing suffix and keeps the visible pair.
        let user = make_frame_at(5, "my_crate::a");
        let rec_a = make_frame_at(7, "my_crate::recurse");
        let rec_b = make_frame_at(8, "my_crate::helper");
        let tail_a = make_frame_at(7, "std::rt::x");
        let tail_b = make_frame_at(8, "std::rt::y");

        let filter = marker_strip_filter(vec![7, 8]);
        let mut frames: Vec<&BacktraceFrame> = vec![&user, &rec_a, &rec_b, &tail_a, &tail_b];
        filter(&mut frames);

        assert_eq!(frames.iter().map(|f| f.ip).collect::<Vec<_>>(), [5, 7, 8]);
    }

    #[test]
    fn marker_strip_never_empties_the_frame_list() {
        let only = make_frame_at(3, "my_crate::a");

        let filter = marker_strip_filter(vec![3]);
        let mut frames: Vec<&BacktraceFrame> = vec![&only];
        filter(&mut frames);

        assert_eq!(frames.len(), 1);
    }
}
