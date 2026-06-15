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
#[non_exhaustive]
pub struct BacktraceFrame {
    /// Instruction pointer of the physical frame this symbol belongs to.
    /// Inline expansion gives several rendered frames the same `ip`.
    pub ip: usize,
    /// Resolved symbol name, demangled when available.
    pub name: Option<Box<str>>,
    /// Source file the symbol was compiled from.
    pub filename: Option<Box<path::Path>>,
    /// Line number within [`filename`](Self::filename).
    pub lineno: Option<u32>,
    /// Column number within [`filename`](Self::filename).
    pub colno: Option<u32>,
}

impl BacktraceFrame {
    /// Construct a frame from its resolved symbol parts.
    #[must_use]
    #[inline]
    pub const fn new(
        ip: usize,
        name: Option<Box<str>>,
        filename: Option<Box<path::Path>>,
        lineno: Option<u32>,
        colno: Option<u32>,
    ) -> Self {
        Self {
            ip,
            name,
            filename,
            lineno,
            colno,
        }
    }
}

/// Metadata for a single span in a span trace.
#[non_exhaustive]
pub struct SpanMetadata<'a> {
    /// The span's name.
    pub name: &'a str,
    /// The span's target (typically the module path it was created in).
    pub target: &'a str,
    /// Source file where the span was created.
    pub file: Option<&'a str>,
    /// Line number within [`file`](Self::file).
    pub line: Option<u32>,
}

impl<'a> SpanMetadata<'a> {
    /// Construct span metadata from its borrowed parts.
    #[must_use]
    #[inline]
    pub const fn new(
        name: &'a str,
        target: &'a str,
        file: Option<&'a str>,
        line: Option<u32>,
    ) -> Self {
        Self {
            name,
            target,
            file,
            line,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Provider traits
// ─────────────────────────────────────────────────────────────────────────────

/// Trait for types that can provide backtrace frames.
pub trait BacktraceProvider {
    /// Resolve the backtrace into rendered frames, top of stack first.
    fn frames(&self) -> Vec<BacktraceFrame>;
}

/// Trait for types that can provide span trace information.
pub trait SpanTraceProvider {
    /// Invoke `f` once per span, from innermost to outermost, passing the
    /// span's metadata and its formatted fields. Iteration stops early when
    /// `f` returns `false`.
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
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TraceTheme {
    /// Style for the per-frame index.
    pub frame_number: Style,
    /// Style for a frame's function (or `target::name`) symbol.
    pub function_name: Style,
    /// Style for the `::h…` hash suffix appended to a mangled symbol.
    pub function_hash: Style,
    /// Style for a frame's source file path.
    pub file_path: Style,
    /// Style for the `:line[:col]` suffix after a file path.
    pub line_number: Style,
    /// Style for fixed separators (`: `, `at `, `with `).
    pub separator: Style,
    /// Style for a span frame's formatted fields.
    pub fields: Style,
    /// Style for the ` BACKTRACE ` / ` SPANTRACE ` section banner.
    pub header: Style,
    /// Style for the "… N frames hidden …" notice.
    pub frames_hidden: Style,
}

impl TraceTheme {
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

// ─────────────────────────────────────────────────────────────────────────────
// Frame filtering
// ─────────────────────────────────────────────────────────────────────────────

/// Prefixes for backtrace capture frames that should be skipped.
const BACKTRACE_CAPTURE_PREFIXES: &[&str] = &[
    "std::backtrace_rs::backtrace::",
    "<std::backtrace::Backtrace>::create",
    "<std::backtrace::Backtrace as oopsie_core::Capturable>::",
    "<alloc::boxed::Box<oopsie_core::backtrace::Backtrace> as oopsie_core::Capturable>::",
];

/// A symbol split into its owning crate and path, across spellings:
/// `std::rt::lang_start`, `std[hash]::rt::lang_start`, and
/// `<std[hash]::rt::X>::method` all yield crate `"std"`. When the self type
/// of an `<X as Trait>::method` impl shim carries no crate of its own (fn
/// pointers, closures), the trait side takes over as the owner.
struct ParsedSymbol<'a> {
    name: &'a str,
    krate: &'a str,
    path: &'a str,
}

fn scan_crate(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('<').unwrap_or(s);
    let ident_len = s
        .bytes()
        .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_')
        .count();
    if ident_len == 0 {
        return None;
    }
    let (krate, rest) = s.split_at(ident_len);
    if let Some(designator) = rest.strip_prefix('[') {
        let (_, path) = designator.split_once("]::")?;
        return Some((krate, path));
    }
    rest.strip_prefix("::").map(|path| (krate, path))
}

fn impl_trait_side(name: &str) -> Option<(&str, &str)> {
    if !name.starts_with('<') {
        return None;
    }
    let (_, trait_side) = name.rsplit_once(" as ")?;
    scan_crate(trait_side)
}

fn parse_symbol(name: &str) -> Option<ParsedSymbol<'_>> {
    let (krate, path) = scan_crate(name).or_else(|| impl_trait_side(name))?;
    Some(ParsedSymbol { name, krate, path })
}

impl<'a> ParsedSymbol<'a> {
    /// The trait side of an `<X as Trait>::method` impl shim, parsed on
    /// demand — most callers never ask.
    fn trait_crate(&self) -> Option<&'a str> {
        impl_trait_side(self.name).map(|(krate, _)| krate)
    }
}

/// A frame-name pattern set: bare symbols matched as raw prefixes, plus
/// crate-scoped path prefixes matched after [`parse_symbol`] normalization.
/// A normalized path is also tried against the bare set — the same symbol
/// may appear under a crate designator.
fn matches_symbol(name: &str, bare: &[&str], scoped: &[(&str, &str)]) -> bool {
    if bare.iter().any(|prefix| name.starts_with(prefix)) {
        return true;
    }
    let Some(symbol) = parse_symbol(name) else {
        return false;
    };
    scoped
        .iter()
        .any(|&(k, p)| symbol.krate == k && symbol.path.starts_with(p))
        // A reserved bare symbol under a crate designator must end at a
        // segment boundary — a user symbol may merely share the prefix.
        || bare.iter().any(|prefix| {
            symbol.path.strip_prefix(prefix).is_some_and(|rest| {
                !rest
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        })
}

/// The panic *raising* runtime that sits directly above the user's `panic!`
/// site: the `core`/`std` panic machinery and unwind entry points.
const POST_PANIC_BARE: &[&str] = &[
    "rust_begin_unwind",
    "__rust_start_panic",
    "__rust_end_short_backtrace",
];

/// Deliberately excludes `std::panicking::catch_unwind` (and its `try`/`do_call`
/// helpers): those frames sit at the *bottom* of the stack, below `main`, where
/// the runtime catches the unwind. Matching them would let a reverse search for
/// the panic boundary be dragged all the way down, trimming user code.
const POST_PANIC_SCOPED: &[(&str, &str)] = &[
    ("core", "panicking::"),
    ("std", "panicking::panic"),
    ("std", "panicking::begin_panic"),
    ("std", "panicking::rust_panic"),
    ("std", "sys::backtrace::__rust_end_short_backtrace"),
];

/// Runtime-entry frames below user code, recognized anywhere in a trace
/// (see also [`is_runtime_tail_code`]).
const RUNTIME_INIT_BARE: &[&str] = &[
    "__rust_begin_short_backtrace",
    "__rustc",
    "__libc_start",
    "__scrt_common_main",
];

const RUNTIME_INIT_SCOPED: &[(&str, &str)] = &[
    ("std", "sys::backtrace::__rust_begin_short_backtrace"),
    ("std", "rt::lang_start"),
    ("test", "__rust_begin_short_backtrace"),
];

/// OS / C-runtime entry symbols at the very bottom of a stack, recognized
/// only by the bottom-anchored tail peel.
const OS_ENTRY_PREFIXES: &[&str] = &[
    "_main",
    "___rust_try",
    "__rust_try",
    "_start",
    "start_thread",
    "__clone",
    "clone3",
    "__pthread",
    "RtlUserThreadStart",
    "BaseThreadInitThunk",
    "invoke_main",
    "mainCRTStartup",
];

/// Check if a frame name matches backtrace capture code.
fn is_backtrace_capture_code(name: &str, filename: Option<&path::Path>) -> bool {
    BACKTRACE_CAPTURE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || filename.is_some_and(|f| f.starts_with(oopsie_core::__private::CORE_SRC_PATH))
}

/// Frames emitted by `#[oopsie]` expansions resolve to the macro invocation
/// site; the registry of those sites classifies them like capture machinery.
fn is_generated_site_frame(frame: &BacktraceFrame) -> bool {
    let Some(name) = frame.name.as_deref() else {
        return false;
    };
    let Some(symbol) = parse_symbol(name) else {
        return false;
    };
    let Some(line) = frame.lineno else {
        return false;
    };
    let Some(filename) = frame.filename.as_deref() else {
        return false;
    };
    crate::generated::GENERATED_SITES.iter().any(|site| {
        matches_site(
            symbol.krate,
            line,
            filename,
            site.krate,
            site.file,
            site.line,
        )
    })
}

fn matches_site(
    frame_krate: &str,
    frame_line: u32,
    frame_file: &path::Path,
    site_krate: &str,
    site_file: &str,
    site_line: u32,
) -> bool {
    if frame_krate != site_krate || frame_line != site_line {
        return false;
    }
    let frame_file = frame_file.to_string_lossy();
    frame_file.as_ref() == site_file
        || (frame_file.ends_with(site_file)
            && frame_file
                .as_bytes()
                .get(frame_file.len() - site_file.len() - 1)
                .is_some_and(|&b| b == b'/' || b == b'\\'))
}

/// Check if a frame name matches panic-runtime code that sits above the user's
/// `panic!` site (`core::panicking`, `std::panicking`, unwind entry points),
/// in both demangled and v0-mangled spellings.
fn is_post_panic_code(name: &str, _filename: Option<&path::Path>) -> bool {
    matches_symbol(name, POST_PANIC_BARE, POST_PANIC_SCOPED)
}

/// Check if a frame name matches runtime-entry code below user code
/// (`lang_start`, the short-backtrace markers), in both demangled and
/// v0-mangled spellings. `catch_unwind` plumbing is deliberately absent:
/// per-frame consumers would hide a user's own mid-stack cluster; the
/// bottom-anchored tail peel covers it via std ownership instead.
fn is_runtime_init_code(name: &str, _filename: Option<&path::Path>) -> bool {
    matches_symbol(name, RUNTIME_INIT_BARE, RUNTIME_INIT_SCOPED)
}

/// Like [`is_runtime_init_code`], plus matching that is only safe when
/// anchored at the bottom of the stack: frames owned by the standard-library
/// crates, the C `main` shim, and OS entry symbols. A mid-stack frame must
/// never be classified by these rules — the bottom peel stops at the first
/// miss, which is what bounds them.
fn is_runtime_tail_code(name: &str, filename: Option<&path::Path>) -> bool {
    // Frames owned by the standard-library crates are never user code; at
    // the bottom-contiguous tail they are all plumbing, and they are the
    // peel's common case — checked first. The trait side of an impl shim
    // counts too: the dispatch shim for a user-crate closure is core's
    // `FnOnce::call_once` even though the self type carries the user's
    // crate.
    let std_owned = |krate: &str| matches!(krate, "std" | "core" | "alloc" | "test");
    if let Some(symbol) = parse_symbol(name)
        && (std_owned(symbol.krate) || symbol.trait_crate().is_some_and(std_owned))
    {
        return true;
    }
    if is_runtime_init_code(name, filename) {
        return true;
    }
    // The C entry shim; the user's own Rust `main` demangles crate-qualified.
    if name == "main" {
        return true;
    }
    OS_ENTRY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Default frame filter for error backtraces.
///
/// This filter:
/// 1. Removes the runtime tail from the bottom: a contiguous run of
///    runtime-init/nameless frames. Stopping at the first real user frame
///    keeps frames below a user's own mid-stack `catch_unwind` intact —
///    the cost is that an unrecognized tail spelling leaks frames instead
///    of hiding user code.
/// 2. Skips frames from the top that are backtrace capture machinery.
pub fn error_backtrace_frame_filter(frames: &mut [Option<&BacktraceFrame>]) {
    let internal = |frame: &BacktraceFrame| match frame.name.as_ref() {
        Some(name) => is_runtime_tail_code(name, frame.filename.as_deref()),
        // Unresolvable frames are runtime/shim detail (`__rust_try` etc.).
        None => true,
    };
    // Bottom peel; a slot already hidden by an earlier filter keeps the
    // contiguous run going.
    let mut keep = frames.len();
    while keep > 0 {
        match frames[keep - 1] {
            Some(frame) if !internal(frame) => break,
            _ => keep -= 1,
        }
    }
    // A fully symbol-stripped trace would peel to nothing; show it instead.
    if keep > 0 {
        for slot in &mut frames[keep..] {
            *slot = None;
        }
    }

    let top_cutoff_idx = frames.iter().rposition(|slot| {
        slot.is_some_and(|frame| {
            frame
                .name
                .as_ref()
                .is_some_and(|name| is_backtrace_capture_code(name, frame.filename.as_deref()))
                || is_generated_site_frame(frame)
        })
    });
    if let Some(top) = top_cutoff_idx {
        for slot in &mut frames[..=top] {
            *slot = None;
        }
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
pub fn post_panic_frame_filter(frames: &mut [Option<&BacktraceFrame>]) {
    let top_cutoff_idx = frames.iter().rposition(|slot| {
        slot.is_some_and(|frame| {
            frame
                .name
                .as_ref()
                .is_some_and(|name| is_post_panic_code(name, frame.filename.as_deref()))
        })
    });
    if let Some(top) = top_cutoff_idx {
        for slot in &mut frames[..=top] {
            *slot = None;
        }
    }
}

/// Frame filter for panic backtraces.
///
/// Trims the panic-raising plumbing above the user's `panic!` site
/// ([`post_panic_frame_filter`]) and then the runtime-init tail below `main`
/// ([`error_backtrace_frame_filter`]). The top trim runs first so the unwind
/// entry frame never reaches the bottom peel.
pub fn panic_frame_filter(frames: &mut [Option<&BacktraceFrame>]) {
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
type FrameFilterBox = BoxOrBorrow<'static, dyn Fn(&mut [Option<&BacktraceFrame>])>;

/// Renders backtraces and span traces with colors.
pub struct TracePrinter {
    /// An explicit theme, or `None` to follow the current global theme
    /// ([`get_theme`](crate::get_theme)) resolved at render time.
    theme: Option<TraceTheme>,
    frame_filter: FrameFilterBox,
    strip_cwd: bool,
}

impl TracePrinter {
    /// Default `TracePrinter` with the standard frame filter, following the
    /// current global theme.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self::with_const_filter(&error_backtrace_frame_filter)
    }

    /// Create a new `TracePrinter` with no frame filtering, following the
    /// current global theme.
    ///
    /// Every captured frame is rendered. Used to honor `RUST_BACKTRACE=full`.
    #[must_use]
    #[inline]
    pub const fn unfiltered() -> Self {
        Self::with_const_filter(&noop_frame_filter).absolute_paths()
    }

    /// Create a new `TracePrinter` with a custom frame filter, following the
    /// current global theme.
    #[must_use]
    #[inline]
    pub fn with_filter(filter: impl Fn(&mut [Option<&BacktraceFrame>]) + 'static) -> Self {
        Self {
            frame_filter: BoxOrBorrow::Box(Box::new(filter)),
            theme: None,
            strip_cwd: true,
        }
    }

    /// Create a new `TracePrinter` with a `const` frame filter.
    #[must_use]
    #[inline]
    pub const fn with_const_filter(
        filter: &'static (dyn Fn(&mut [Option<&BacktraceFrame>]) + 'static),
    ) -> Self {
        Self {
            frame_filter: BoxOrBorrow::Borrow(filter),
            theme: None,
            strip_cwd: true,
        }
    }

    /// Add a frame filter for backtrace rendering.
    #[must_use]
    pub fn add_frame_filter(
        mut self,
        filter: impl Fn(&mut [Option<&BacktraceFrame>]) + 'static,
    ) -> Self {
        self.frame_filter =
            overlay_frame_filters(self.frame_filter, BoxOrBorrow::Box(Box::new(filter)));
        self
    }

    /// Render file paths exactly as captured, without stripping the current
    /// working directory. [`unfiltered`](Self::unfiltered) implies this —
    /// `RUST_BACKTRACE=full` means "show everything".
    #[must_use]
    #[inline]
    pub const fn absolute_paths(mut self) -> Self {
        self.strip_cwd = false;
        self
    }

    /// Pin an explicit color theme, overriding the global default.
    #[must_use]
    #[inline]
    pub const fn with_theme(mut self, theme: TraceTheme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Switch to the plain (uncolored) theme, keeping the frame filter.
    #[must_use]
    #[inline]
    pub const fn plain(mut self) -> Self {
        self.theme = Some(TraceTheme::PLAIN);
        self
    }

    /// The theme to render with: the pinned one, else the current global
    /// theme's trace styles.
    fn resolved_theme(&self) -> TraceTheme {
        self.theme
            .unwrap_or_else(|| crate::theme::get_theme().trace())
    }

    /// Render a colored backtrace, including the ` BACKTRACE ` header.
    ///
    /// Writes nothing at all when the filter masks every frame, so a degenerate
    /// trace never leaves a lone banner behind.
    pub fn write_backtrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        bt: &impl BacktraceProvider,
    ) -> fmt::Result {
        let all_frames = bt.frames();
        let theme = self.resolved_theme();

        let mut filtered: Vec<_> = all_frames.iter().map(Some).collect();
        (self.frame_filter)(&mut filtered);

        // An all-masked trace would render a lone banner above a hidden-frames
        // notice and no frames; suppress the whole section instead.
        if !filtered.iter().any(Option::is_some) {
            return Ok(());
        }

        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " BACKTRACE ").style(theme.header)
        )?;

        // Read per render: the working directory is mutable process state,
        // and a failed read (deleted cwd, sandbox) just disables shortening.
        let cwd = if self.strip_cwd {
            std::env::current_dir().ok()
        } else {
            None
        };

        // Masked slots render as a count exactly where the gap sits — top,
        // middle, or bottom runs alike.
        let mut hidden_run = 0_usize;
        let mut number = 0_usize;
        for slot in &filtered {
            match slot {
                Some(frame) => {
                    if hidden_run > 0 {
                        Self::write_hidden_notice(f, hidden_run, &theme)?;
                        hidden_run = 0;
                    }
                    number += 1;
                    Self::write_backtrace_frame(f, number, frame, cwd.as_deref(), &theme)?;
                }
                None => hidden_run += 1,
            }
        }
        if hidden_run > 0 {
            Self::write_hidden_notice(f, hidden_run, &theme)?;
        }

        Ok(())
    }

    fn write_hidden_notice(
        f: &mut fmt::Formatter<'_>,
        count: usize,
        theme: &TraceTheme,
    ) -> fmt::Result {
        writeln!(
            f,
            "{}",
            format_args!("   ... {count} frames hidden ...").style(theme.frames_hidden)
        )
    }

    /// Render a single backtrace frame.
    fn write_backtrace_frame(
        f: &mut fmt::Formatter<'_>,
        number: usize,
        frame: &BacktraceFrame,
        cwd: Option<&path::Path>,
        theme: &TraceTheme,
    ) -> fmt::Result {
        // Frame number: right-aligned in 3 chars
        write!(
            f,
            "{}{}",
            format_args!("{number:>3}").style(theme.frame_number),
            ": ".style(theme.separator)
        )?;

        // Function name
        if let Some(name) = &frame.name {
            let (base, hash) = split_function_hash(name);
            write!(f, "{}", base.style(theme.function_name))?;
            if let Some(h) = hash {
                write!(f, "{}", h.style(theme.function_hash))?;
            }
        } else {
            write!(f, "{}", "<unknown>".style(theme.function_name))?;
        }
        writeln!(f)?;

        // File location
        if let Some(filename) = &frame.filename {
            let display_path = cwd
                .and_then(|cwd| filename.strip_prefix(cwd).ok())
                .unwrap_or(filename);
            write!(
                f,
                "           {}{}",
                "at ".style(theme.separator),
                display_path.display().style(theme.file_path)
            )?;
            if let Some(lineno) = frame.lineno {
                write!(f, "{}", format_args!(":{lineno}").style(theme.line_number))?;
                if let Some(colno) = frame.colno {
                    write!(f, "{}", format_args!(":{colno}").style(theme.line_number))?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }

    /// Render a colored span trace, including the ` SPANTRACE ` header.
    ///
    /// Always writes the header, so callers must pre-check that the provider
    /// yields at least one span — an empty provider leaves a lone banner.
    pub fn write_spantrace(
        &self,
        f: &mut fmt::Formatter<'_>,
        st: &impl SpanTraceProvider,
    ) -> fmt::Result {
        let theme = self.resolved_theme();
        // Header
        writeln!(
            f,
            "{}",
            format_args!("{:━^80}", " SPANTRACE ").style(theme.header)
        )?;

        let mut index = 1usize;
        let mut err = Ok(());

        st.with_spans(&mut |meta, fields| {
            if let Err(e) = Self::write_span_frame(f, index, meta, fields, &theme) {
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
        f: &mut fmt::Formatter<'_>,
        index: usize,
        meta: &SpanMetadata,
        fields: &str,
        theme: &TraceTheme,
    ) -> fmt::Result {
        // Frame number
        write!(
            f,
            "{}",
            format_args!("{index:>3}").style(theme.frame_number)
        )?;
        write!(f, "{}", ": ".style(theme.separator))?;

        // target::name
        write!(
            f,
            "{}",
            format_args!("{}::{}", meta.target, meta.name).style(theme.function_name)
        )?;
        writeln!(f)?;

        // Fields line (only if non-empty)
        if !fields.is_empty() {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "with ".style(theme.separator))?;
            writeln!(f, "{}", fields.style(theme.fields))?;
        }

        // File location
        if let Some(file) = &meta.file {
            write!(f, "           ")?; // 11 spaces
            write!(f, "{}", "at ".style(theme.separator))?;
            write!(f, "{}", file.style(theme.file_path))?;
            if let Some(line) = meta.line {
                write!(f, "{}", format_args!(":{line}").style(theme.line_number))?;
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
const fn noop_frame_filter(_frames: &mut [Option<&BacktraceFrame>]) {}

/// Frame filter dropping the trailing `cut` rendered frames — the tail
/// [`Backtrace::marker_hidden_frames`] attributes to the marker. Never
/// empties the list.
///
/// [`Backtrace::marker_hidden_frames`]: oopsie_core::Backtrace::marker_hidden_frames
pub fn marker_strip_filter(cut: usize) -> impl Fn(&mut [Option<&BacktraceFrame>]) {
    move |frames: &mut [Option<&BacktraceFrame>]| {
        let keep = frames.len().saturating_sub(cut);
        if keep > 0 {
            for slot in &mut frames[keep..] {
                *slot = None;
            }
        }
    }
}

fn overlay_frame_filters(under: FrameFilterBox, above: FrameFilterBox) -> FrameFilterBox {
    BoxOrBorrow::Box(Box::new(move |frames: &mut [Option<&BacktraceFrame>]| {
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

    fn kept_names<'a>(frames: &[Option<&'a BacktraceFrame>]) -> Vec<&'a str> {
        frames
            .iter()
            .flatten()
            .map(|frame| frame.name.as_deref().unwrap_or("<unknown>"))
            .collect()
    }

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

        let mut frames: Vec<Option<&BacktraceFrame>> =
            vec![Some(&capture), Some(&app1), Some(&app2), Some(&runtime)];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 2);
        assert_eq!(kept_names(&frames)[0], "my_crate::function_a");
        assert_eq!(kept_names(&frames)[1], "my_crate::function_b");
    }

    #[test]
    fn test_error_backtrace_frame_filter_no_capture_no_runtime() {
        let app1 = make_frame(Some("my_crate::function_a"), None);
        let app2 = make_frame(Some("my_crate::function_b"), None);

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![Some(&app1), Some(&app2)];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 2);
        assert_eq!(kept_names(&frames)[0], "my_crate::function_a");
        assert_eq!(kept_names(&frames)[1], "my_crate::function_b");
    }

    #[test]
    fn test_error_backtrace_frame_filter_multiple_capture_frames() {
        let capture1 = make_frame(Some("<std::backtrace::Backtrace>::create::something"), None);
        let capture2 = make_frame(Some("std::backtrace_rs::backtrace::libunwind::trace"), None);
        let app = make_frame(Some("my_crate::function_a"), None);

        let mut frames: Vec<Option<&BacktraceFrame>> =
            vec![Some(&capture1), Some(&capture2), Some(&app)];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 1);
        assert_eq!(kept_names(&frames)[0], "my_crate::function_a");
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

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![
            Some(&hook),
            Some(&rust_panic),
            Some(&begin),
            Some(&panic_fmt),
            Some(&user_site),
            Some(&user_main),
        ];
        post_panic_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 2);
        assert_eq!(kept_names(&frames)[0], "my_crate::do_work");
        assert_eq!(kept_names(&frames)[1], "my_crate::main");
    }

    #[test]
    fn test_post_panic_frame_filter_no_panic_frames() {
        // With no panic-runtime frames present the filter is a no-op.
        let app1 = make_frame(Some("my_crate::function_a"), None);
        let app2 = make_frame(Some("my_crate::function_b"), None);

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![Some(&app1), Some(&app2)];
        post_panic_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 2);
        assert_eq!(kept_names(&frames)[0], "my_crate::function_a");
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

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![
            Some(&capture),
            Some(&unwind),
            Some(&panic_fmt),
            Some(&user_site),
            Some(&user_main),
            Some(&begin),
            Some(&catch),
            Some(&main_c),
        ];
        panic_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 2);
        assert_eq!(kept_names(&frames)[0], "my_crate::parse_header");
        assert_eq!(kept_names(&frames)[1], "my_crate::main");
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

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![
            Some(&app_top),
            Some(&catch),
            Some(&supervisor),
            Some(&runtime),
        ];
        error_backtrace_frame_filter(&mut frames);

        // The contiguous peel stops at `supervisor`; only the true tail goes.
        assert_eq!(
            kept_names(&frames),
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

        let mut frames: Vec<Option<&BacktraceFrame>> =
            vec![Some(&app), Some(&runtime), Some(&nameless)];
        error_backtrace_frame_filter(&mut frames);

        assert_eq!(kept_names(&frames).len(), 1);
        assert_eq!(kept_names(&frames)[0], "my_crate::function_a");
    }

    #[test]
    fn bottom_trim_keeps_everything_when_all_frames_are_nameless() {
        let a = make_frame(None::<String>, None);
        let b = make_frame(None::<String>, None);

        let mut frames: Vec<Option<&BacktraceFrame>> = vec![Some(&a), Some(&b)];
        error_backtrace_frame_filter(&mut frames);

        // A fully symbol-stripped trace must not be trimmed to nothing.
        assert_eq!(kept_names(&frames).len(), 2);
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
    fn marker_strip_drops_exactly_the_trailing_cut() {
        let a = make_frame_at(1, "my_crate::a");
        let b = make_frame_at(7, "my_crate::b");
        let c = make_frame_at(2, "my_crate::c");
        let tail1 = make_frame_at(7, "std::rt::whatever");
        let tail2 = make_frame_at(8, "std::rt::deeper");

        let filter = marker_strip_filter(2);
        let mut frames: Vec<Option<&BacktraceFrame>> =
            vec![Some(&a), Some(&b), Some(&c), Some(&tail1), Some(&tail2)];
        filter(&mut frames);

        assert_eq!(
            frames.iter().flatten().map(|f| f.ip).collect::<Vec<_>>(),
            [1, 7, 2],
            "the cut is positional; frames above it survive regardless of ip"
        );
    }

    #[test]
    fn rendered_paths_are_stripped_of_the_current_dir() {
        struct OneFrame(std::path::PathBuf);
        impl BacktraceProvider for OneFrame {
            fn frames(&self) -> Vec<BacktraceFrame> {
                vec![BacktraceFrame {
                    ip: 1,
                    name: Some("my_crate::work".into()),
                    filename: Some(self.0.clone().into_boxed_path()),
                    lineno: Some(7),
                    colno: None,
                }]
            }
        }
        fn render(printer: &TracePrinter, bt: &impl BacktraceProvider) -> String {
            struct Render<'a, P>(&'a TracePrinter, &'a P);
            impl<P: BacktraceProvider> fmt::Display for Render<'_, P> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.write_backtrace(f, self.1)
                }
            }
            Render(printer, bt).to_string()
        }

        let cwd = std::env::current_dir().unwrap();
        let provider = OneFrame(cwd.join("src/work.rs"));

        let stripped = render(&TracePrinter::new().plain(), &provider);
        assert!(stripped.contains("at src/work.rs:7"), "{stripped}");

        // Full mode keeps absolute paths.
        let absolute = render(&TracePrinter::unfiltered().plain(), &provider);
        assert!(
            absolute.contains(&format!("at {}:7", cwd.join("src/work.rs").display())),
            "{absolute}"
        );
    }

    #[test]
    fn capture_code_matches_core_src_filenames() {
        let core_file = std::path::PathBuf::from(format!(
            "{}backtrace.rs",
            oopsie_core::__private::CORE_SRC_PATH
        ));
        assert!(is_backtrace_capture_code("any::name", Some(&core_file)));
        // A user path merely containing "backtrace" must not match.
        let user_file = std::path::Path::new("/home/u/my_backtrace_tool/src/lib.rs");
        assert!(!is_backtrace_capture_code("any::name", Some(user_file)));
    }

    #[test]
    fn matches_site_path_forms() {
        // Absolute DWARF path vs the compiler-relative `file!()`.
        assert!(matches_site(
            "myapp",
            17,
            std::path::Path::new("/home/u/project/src/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
        // Full equality (trim-paths builds emit the relative path verbatim).
        assert!(matches_site(
            "myapp",
            17,
            std::path::Path::new("src/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
        // Suffix match must land on a separator boundary.
        assert!(!matches_site(
            "myapp",
            17,
            std::path::Path::new("/home/u/notsrc/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
        // Windows separator before the suffix.
        assert!(matches_site(
            "myapp",
            17,
            std::path::Path::new(r"C:\project\src/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
    }

    #[test]
    fn matches_site_rejects_crate_and_line_mismatch() {
        // Same short suffix in a different crate must not collide.
        assert!(!matches_site(
            "other_crate",
            17,
            std::path::Path::new("/home/u/other/src/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
        assert!(!matches_site(
            "myapp",
            18,
            std::path::Path::new("/home/u/project/src/lib.rs"),
            "myapp",
            "src/lib.rs",
            17
        ));
    }

    #[test]
    fn marker_strip_never_empties_the_frame_list() {
        let only = make_frame_at(3, "my_crate::a");

        // A cut at or beyond the rendered length must leave the list intact.
        for cut in [1, 2, usize::MAX] {
            let filter = marker_strip_filter(cut);
            let mut frames: Vec<Option<&BacktraceFrame>> = vec![Some(&only)];
            filter(&mut frames);
            assert_eq!(kept_names(&frames).len(), 1);
        }
    }

    #[test]
    fn test_is_backtrace_capture_code() {
        assert!(is_backtrace_capture_code(
            "std::backtrace_rs::backtrace::libunwind::trace",
            None
        ));
        assert!(!is_backtrace_capture_code("my_crate::do_stuff", None));
    }

    #[test]
    fn test_is_runtime_init_code() {
        assert!(is_runtime_init_code(
            "std::rt::lang_start_internal::something",
            None
        ));
        assert!(is_runtime_init_code(
            "__rust_begin_short_backtrace<fn(), ()>",
            None
        ));
        assert!(!is_runtime_init_code("my_crate::main_logic", None));
        // A bare `main` prefix would match (and hide) the user's own entry point.
        assert!(!is_runtime_init_code("main", None));
        assert!(!is_runtime_init_code("my_app::main", None));
    }

    #[test]
    fn test_is_runtime_init_code_bracket_form() {
        // v0-mangled `crate[hash]::path` spellings.
        assert!(is_runtime_init_code(
            "test[a1b2c3d4]::__rust_begin_short_backtrace",
            None
        ));
        assert!(is_runtime_init_code(
            "std[a1b2c3d4]::sys::backtrace::__rust_begin_short_backtrace",
            None
        ));
        // A user symbol inside the bracket form is kept.
        assert!(!is_runtime_init_code(
            "std[a1b2c3d4]::collections::HashMap::insert",
            None
        ));
        // Scoped paths are crate-owned and never match under a foreign crate;
        // bare reserved symbols match under any crate designator.
        assert!(!is_runtime_init_code(
            "mycrate[a1b2c3d4]::rt::lang_start",
            None
        ));
        assert!(is_runtime_init_code(
            "mycrate[a1b2c3d4]::__rust_begin_short_backtrace",
            None
        ));
    }

    #[test]
    fn test_is_post_panic_code() {
        assert!(is_post_panic_code("core::panicking::panic_fmt", None));
        assert!(is_post_panic_code(
            "std::panicking::begin_panic_handler::{{closure}}",
            None
        ));
        assert!(is_post_panic_code("rust_begin_unwind", None));
        assert!(is_post_panic_code(
            "std::sys::backtrace::__rust_end_short_backtrace::<…>",
            None
        ));
        // `catch_unwind` sits below `main`, not above the panic site, and must
        // NOT be treated as panic-raising plumbing.
        assert!(!is_post_panic_code(
            "std::panicking::catch_unwind::do_call",
            None
        ));
        // User code and the user's own panic call site are kept.
        assert!(!is_post_panic_code("my_app::do_work", None));
        assert!(!is_post_panic_code("my_app::main", None));
    }

    #[test]
    fn test_is_post_panic_code_bracket_form() {
        // v0-mangled `crate[hash]::path` form for the panic runtime.
        assert!(is_post_panic_code(
            "std[a1b2c3d4]::panicking::begin_panic_handler",
            None
        ));
        assert!(is_post_panic_code(
            "core[a1b2c3d4]::panicking::panic_fmt",
            None
        ));
        assert!(is_post_panic_code(
            "std[a1b2c3d4]::sys::backtrace::__rust_end_short_backtrace",
            None
        ));
        // The unwind entry is emitted under the `__rustc` pseudo-crate.
        assert!(is_post_panic_code(
            "__rustc[a1b2c3d4]::rust_begin_unwind",
            None
        ));
        // `catch_unwind` in bracket form is still excluded.
        assert!(!is_post_panic_code(
            "std[a1b2c3d4]::panicking::catch_unwind::do_call",
            None
        ));
        // A user symbol inside the bracket form is kept.
        assert!(!is_post_panic_code(
            "std[a1b2c3d4]::collections::HashMap::insert",
            None
        ));
    }

    #[test]
    fn pin_post_panic_spellings() {
        for name in [
            "__rustc[1a2b]::rust_begin_unwind",
            "std[1a2b]::panicking::begin_panic_handler",
            "core[9f]::panicking::panic_fmt",
            "std[1a2b]::sys::backtrace::__rust_end_short_backtrace",
            "core::panicking::panic_fmt",
            "std::panicking::rust_panic_with_hook",
        ] {
            assert!(is_post_panic_code(name, None), "should match: {name}");
        }
        for name in [
            // The deliberate exclusion: catch_unwind sits below `main`.
            "std::panicking::catch_unwind::do_call",
            "my_crate::panicking::panic_like",
            // Bare reserved symbols must end at a segment boundary.
            "mycrate::rust_begin_unwinding",
            "app::__rust_start_panicky",
        ] {
            assert!(!is_post_panic_code(name, None), "must not match: {name}");
        }
    }

    #[test]
    fn pin_runtime_init_spellings() {
        for name in [
            "std[1a2b]::rt::lang_start_internal",
            "test[3c]::__rust_begin_short_backtrace",
        ] {
            assert!(is_runtime_init_code(name, None), "should match: {name}");
        }
        // `catch_unwind` plumbing is std-owned tail material; per-frame
        // classification must not hide a user-initiated mid-stack cluster.
        assert!(!is_runtime_init_code(
            "std::panic::catch_unwind::{{closure}}",
            None
        ));
        assert!(is_runtime_tail_code(
            "std::panic::catch_unwind::{{closure}}",
            None
        ));
        for name in [
            // Scoped paths are crate-owned: a foreign crate's `rt::lang_start`
            // is user code, and the crate ident must compare exactly.
            "my_crate::rt::lang_start",
            "std_extras::rt::lang_start",
            // Bare reserved symbols must end at a segment boundary.
            "user::__rustc_internal_thing",
            "app::__libc_startup_helper",
        ] {
            assert!(!is_runtime_init_code(name, None), "must not match: {name}");
        }
    }

    #[test]
    fn parse_symbol_across_spellings() {
        fn krate_and_path(name: &str) -> Option<(&str, &str)> {
            parse_symbol(name).map(|s| (s.krate, s.path))
        }
        fn owner(name: &str) -> Option<&str> {
            parse_symbol(name).map(|s| s.krate)
        }
        fn trait_crate(name: &str) -> Option<&str> {
            parse_symbol(name)?.trait_crate()
        }

        assert_eq!(
            krate_and_path("std::rt::lang_start"),
            Some(("std", "rt::lang_start"))
        );
        assert_eq!(
            krate_and_path("std[1a2b]::rt::lang_start"),
            Some(("std", "rt::lang_start"))
        );
        // The path is the raw remainder — prefix matching tolerates the
        // trailing `>::method` of angle-bracket forms.
        assert_eq!(
            krate_and_path("<std[1a2b]::sys::thread::Thread>::new"),
            Some(("std", "sys::thread::Thread>::new"))
        );
        // A crate-carrying self type owns the symbol; the trait side is
        // reported alongside it.
        let assert_unwind_shim = "<core[9f]::panic::unwind_safe::AssertUnwindSafe<f> as core[9f]::ops::function::FnOnce<()>>::call_once";
        assert_eq!(owner(assert_unwind_shim), Some("core"));
        assert_eq!(trait_crate(assert_unwind_shim), Some("core"));
        // A user-crate self type keeps ownership, but the std trait side
        // stays visible for the tail classifier.
        let user_closure_shim = "<my::Foo as core[9f]::ops::function::FnOnce<()>>::call_once";
        assert_eq!(owner(user_closure_shim), Some("my"));
        assert_eq!(trait_crate(user_closure_shim), Some("core"));
        // A crate-less self type (fn pointer) defers to the trait side.
        assert_eq!(
            owner("<fn() -> i32 as core[9f]::ops::function::FnOnce<()>>::call_once"),
            Some("core")
        );
        // Nested impls: the outermost trait (last ` as `) owns the symbol.
        assert_eq!(owner("<<a::A as b::B>::C as d::D>::m"), Some("d"));
        // Non-shim symbols report no trait side.
        assert_eq!(trait_crate("std::rt::lang_start"), None);
        assert!(parse_symbol("main_loop").is_none());
        assert!(parse_symbol("rust_begin_unwind").is_none());
        assert!(parse_symbol("std").is_none());
        assert_eq!(krate_and_path("corey::parse"), Some(("corey", "parse")));
    }

    #[test]
    fn scoped_tables_match_all_spellings() {
        type Classifier = fn(&str, Option<&std::path::Path>) -> bool;
        for (table, classify) in [
            (RUNTIME_INIT_SCOPED, is_runtime_init_code as Classifier),
            (POST_PANIC_SCOPED, is_post_panic_code as Classifier),
        ] {
            for &(krate, path) in table {
                for name in [
                    format!("{krate}::{path}x"),
                    format!("{krate}[abc123]::{path}x"),
                    format!("<{krate}[abc123]::{path}x>::m"),
                ] {
                    assert!(classify(&name, None), "should match: {name}");
                }
            }
        }
    }

    #[test]
    fn runtime_tail_recognizes_test_thread_tail_spellings() {
        for name in [
            "__pthread_cond_wait",
            "<std[1a2b]::sys::thread::unix::Thread>::new::thread_start",
            "<alloc[9f]::boxed::Box<dyn core[9f]::ops::function::FnOnce<(), Output = ()> + core[9f]::marker::Send> as core[9f]::ops::function::FnOnce<()>>::call_once",
            "<std[1a2b]::thread::lifecycle::spawn_unchecked<f, ()>::{closure#1} as core[9f]::ops::function::FnOnce<()>>::call_once::{shim:vtable#0}",
            "std[1a2b]::thread::lifecycle::spawn_unchecked::<f, ()>::{closure#1}",
            "<core[9f]::panic::unwind_safe::AssertUnwindSafe<f> as core[9f]::ops::function::FnOnce<()>>::call_once",
            "test[3c]::run_test_in_process",
            "test[3c]::run_test::{closure#0}",
            "std::thread::lifecycle::spawn_unchecked",
            "std::sys::pal::unix::thread::Thread::new::thread_start",
            "_start",
            "start_thread",
            "__clone",
            "clone3",
            "RtlUserThreadStart",
            "BaseThreadInitThunk",
            "invoke_main",
            "mainCRTStartup",
            "__rust_try",
            "main",
        ] {
            assert!(is_runtime_tail_code(name, None), "should match: {name}");
        }
    }

    #[test]
    fn runtime_tail_hides_user_closure_dispatch_shims() {
        // The self type carries the user's crate, but the dispatched method
        // is core's `FnOnce::call_once` — still tail plumbing.
        assert!(is_runtime_tail_code(
            "<my_app[1a2b]::main::{closure#0} as core[9f]::ops::function::FnOnce<()>>::call_once",
            None
        ));
    }

    #[test]
    fn runtime_tail_spares_user_spellings() {
        for name in [
            "my_crate::run_tests",
            "my_crate::test::run_testish",
            "<my_crate::Foo as my_crate::Bar>::call_me",
            "<my_crate::Foo as my_crate::Bar>::call_once",
            "testing::utils::run",
            "corey::parse",
            "my_crate::sys::thread_pool::spawn",
            "my_crate::thread::worker",
            "main_loop",
            "mainframe::connect",
        ] {
            assert!(!is_runtime_tail_code(name, None), "must not match: {name}");
        }
    }

    #[test]
    fn tail_only_rules_do_not_classify_per_frame_internal() {
        for name in ["std::thread::sleep", "std::sys::pal::unix::futex", "main"] {
            assert!(
                !is_runtime_init_code(name, None),
                "leaked into per-frame: {name}"
            );
            assert!(
                is_runtime_tail_code(name, None),
                "missing from tail: {name}"
            );
        }
    }
}
