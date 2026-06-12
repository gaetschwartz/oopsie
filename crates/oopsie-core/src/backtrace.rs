use std::cell::Cell;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, LazyLock};
use std::{env, fmt, path};

use crate::Capturable as _;

/// Whether backtrace capture is enabled, and how verbosely it should render.
///
/// Resolved once from the environment (see [`rust_backtrace`]) following the
/// same convention as `std`: `RUST_LIB_BACKTRACE` is consulted first and, if
/// unset, `RUST_BACKTRACE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RustBacktrace {
    /// Capture disabled — no frames are recorded.
    Disabled = 1,
    /// Capture enabled, rendered as the trimmed "short" view.
    Enabled = 2,
    /// Capture enabled, rendered untrimmed — every frame is shown.
    Full = 3,
}

impl RustBacktrace {
    pub fn detect_opt() -> Option<Self> {
        const NONE: u8 = u8::MAX;
        const NOT_SET: u8 = 0;

        static ENABLED: AtomicU8 = AtomicU8::new(NOT_SET);
        if let Some(cached) = match ENABLED.load(Relaxed) {
            1 => Some(Some(Self::Disabled)),
            2 => Some(Some(Self::Enabled)),
            3 => Some(Some(Self::Full)),
            NONE => Some(None),
            NOT_SET => None,
            _ => unreachable!(),
        } {
            return cached;
        }

        // `RUST_LIB_BACKTRACE` wins when set; otherwise fall back to
        // `RUST_BACKTRACE`. `or_else` only fires on `Err` (var unset or
        // non-unicode), so an explicit `RUST_LIB_BACKTRACE=0` disables even
        // when `RUST_BACKTRACE=1`.
        let raw = env::var("RUST_LIB_BACKTRACE").or_else(|_| env::var("RUST_BACKTRACE"));
        let enabled = match raw.as_deref() {
            Ok("0") => Some(Self::Disabled),
            Ok("full") => Some(Self::Full),
            Ok(_) => Some(Self::Enabled),
            Err(_) => None,
        };
        ENABLED.store(enabled.map_or(NONE, |f| f as u8), Relaxed);
        enabled
    }

    /// Panic-path detection: consults only `RUST_BACKTRACE`, mirroring std's
    /// panic handler. `RUST_LIB_BACKTRACE` deliberately has no effect here —
    /// it exists to control library error capture independently of panics.
    pub fn detect_panic_opt() -> Option<Self> {
        const NONE: u8 = u8::MAX;
        const NOT_SET: u8 = 0;

        static ENABLED: AtomicU8 = AtomicU8::new(NOT_SET);
        if let Some(cached) = match ENABLED.load(Relaxed) {
            1 => Some(Some(Self::Disabled)),
            2 => Some(Some(Self::Enabled)),
            3 => Some(Some(Self::Full)),
            NONE => Some(None),
            NOT_SET => None,
            _ => unreachable!(),
        } {
            return cached;
        }

        let enabled = match env::var("RUST_BACKTRACE").as_deref() {
            Ok("0") => Some(Self::Disabled),
            Ok("full") => Some(Self::Full),
            Ok(_) => Some(Self::Enabled),
            Err(_) => None,
        };
        ENABLED.store(enabled.map_or(NONE, |f| f as u8), Relaxed);
        enabled
    }

    /// Whether frames should be captured at all.
    #[inline]
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Full | Self::Enabled)
    }

    /// Whether rendering should show every frame, skipping the trimming that
    /// hides capture machinery and runtime setup/teardown frames.
    #[inline]
    #[must_use]
    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}

thread_local! {
    /// Per-thread override of the backtrace setting. `None` falls back to the
    /// environment.
    static OVERRIDE: Cell<Option<RustBacktrace>> = const { Cell::new(None) };
}

/// Force [`rust_backtrace`] to return `value` **on the current thread**, taking
/// precedence over the environment until [`clear_rust_backtrace_override`].
///
/// This is the precise, scoped lever — it affects only backtraces captured on
/// this thread, not threads spawned afterwards. For process-wide control use
/// the `RUST_LIB_BACKTRACE` / `RUST_BACKTRACE` environment variables instead.
#[inline]
pub fn set_rust_backtrace_override(value: RustBacktrace) {
    OVERRIDE.with(|o| o.set(Some(value)));
}

/// Remove any override set by [`set_rust_backtrace_override`] on the current
/// thread, reverting [`rust_backtrace`] to the environment-derived value.
#[inline]
pub fn clear_rust_backtrace_override() {
    OVERRIDE.with(|o| o.set(None));
}

/// The effective backtrace setting for the current thread.
///
/// Returns the thread-local override set via [`set_rust_backtrace_override`] if
/// present; otherwise the value derived from `RUST_LIB_BACKTRACE` then
/// `RUST_BACKTRACE`. Like `std`, the environment is read once and cached — only
/// an override can change the result afterwards.
#[must_use]
#[inline]
pub fn rust_backtrace() -> RustBacktrace {
    if let Some(over) = OVERRIDE.with(Cell::get) {
        return over;
    }
    RustBacktrace::detect_opt().unwrap_or(RustBacktrace::Disabled)
}

/// The effective *panic* backtrace setting for the current thread: the
/// thread-local override if present, else `RUST_BACKTRACE` (only).
#[must_use]
#[inline]
pub fn rust_panic_backtrace() -> RustBacktrace {
    if let Some(over) = OVERRIDE.with(Cell::get) {
        return over;
    }
    RustBacktrace::detect_panic_opt().unwrap_or(RustBacktrace::Disabled)
}

/// Run `f` with the thread's backtrace setting forced to `value`, restoring
/// the previous override (or lack of one) afterwards — including on unwind.
#[inline]
pub fn with_rust_backtrace_override<R>(value: RustBacktrace, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<RustBacktrace>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(OVERRIDE.with(Cell::get));
    OVERRIDE.with(|o| o.set(Some(value)));
    f()
}

// Clones share one `Arc`-held capture, so cloning is a refcount bump and the
// first frame access resolves symbols exactly once for all clones.
#[derive(Clone)]
enum Inner {
    Captured(Arc<LazyLock<Capture, helper::LazyResolve>>),
    Disabled(backtrace::Backtrace), // Always empty, used when capture is disabled to avoid the LazyLock indirection.
}

#[derive(Clone)]
pub struct Backtrace {
    inner: Inner,
    marker: Option<crate::marker::TraceMarker>,
}

impl crate::Capturable for Backtrace {
    #[inline]
    fn capture() -> Self {
        if rust_backtrace().is_enabled() {
            Self {
                inner: Inner::Captured(Arc::new(LazyLock::new(helper::lazy_resolve(Capture {
                    backtrace: backtrace::Backtrace::new_unresolved(),
                })))),
                marker: crate::marker::current(),
            }
        } else {
            Self {
                inner: Inner::Disabled(backtrace::Backtrace::from(vec![])),
                marker: None,
            }
        }
    }
}

impl crate::CaptureExt for Backtrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        match source.oopsie_backtrace() {
            // Keep the source's trace only if capture actually succeeded; an
            // empty trace carries nothing worth preserving over a fresh
            // capture at the wrap site.
            Some(trace) if trace.is_captured() => trace.clone(),
            _ => Self::capture(),
        }
    }
}

/// Returns `true` for frames that are implementation/platform detail and
/// should not appear in user-facing renderings.
#[must_use]
pub fn is_internal_frame(name: Option<&str>, filename: Option<&path::Path>) -> bool {
    let Some(name) = name else {
        // Unresolvable frames are almost certainly internal — e.g. the
        // `__rust_try` shim is always nameless, and the backtrace crate's
        // capture frames often are too. User code is much more likely to have
        // at least some symbol information.
        return true;
    };
    if is_backtrace_capture_code(name, filename) || is_runtime_init_code(name, filename) {
        return true;
    }
    false
}

/// Prefixes for backtrace capture frames that should be skipped.
const BACKTRACE_CAPTURE_PREFIXES: &[&str] = &[
    "std::backtrace_rs::backtrace::",
    "<std::backtrace::Backtrace>::create",
    "<std::backtrace::Backtrace as oopsie_core::Capturable>::",
    "<alloc::boxed::Box<oopsie_core::backtrace::Backtrace> as oopsie_core::Capturable>::",
];

/// Prefixes for the panic *raising* runtime that sits directly above the user's
/// `panic!` site: the `core`/`std` panic machinery and unwind entry points.
///
/// Deliberately excludes `std::panicking::catch_unwind` (and its `try`/`do_call`
/// helpers): those frames sit at the *bottom* of the stack, below `main`, where
/// the runtime catches the unwind. Matching them would let a reverse search for
/// the panic boundary be dragged all the way down, trimming user code.
const POST_PANIC_PREFIXES: &[&str] = &[
    "core::panicking::",
    "std::panicking::panic",
    "std::panicking::begin_panic",
    "std::panicking::rust_panic",
    "std::sys::backtrace::__rust_end_short_backtrace",
    "rust_begin_unwind",
    "__rust_start_panic",
];

/// Crate-less tail forms of [`POST_PANIC_PREFIXES`], matched after a v0-mangled
/// `crate[hash]::` segment (and an inner `sys::backtrace::`) is peeled off.
fn is_post_panic_tail(tail: &str) -> bool {
    tail.starts_with("panicking::panic")
        || tail.starts_with("panicking::begin_panic")
        || tail.starts_with("panicking::rust_panic")
        || tail.starts_with("__rust_end_short_backtrace")
        || tail.starts_with("rust_begin_unwind")
        || tail.starts_with("__rust_start_panic")
}

/// Prefixes for runtime-entry frames below user code, recognized anywhere in
/// a trace (see also [`is_runtime_tail_code`]).
const RUNTIME_INIT_PREFIXES: &[&str] = &[
    "std::sys::backtrace::__rust_begin_short_backtrace",
    "test::__rust_begin_short_backtrace",
    "__rust_begin_short_backtrace",
    "std::rt::lang_start",
    "std::panicking::catch_unwind::",
    "std::panic::catch_unwind::",
    "__rustc",
    "__libc_start",
    "__scrt_common_main",
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

const CRATE_SRC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/");

/// Check if a frame name matches backtrace capture code.
#[inline]
#[must_use]
pub fn is_backtrace_capture_code(name: &str, filename: Option<&path::Path>) -> bool {
    if BACKTRACE_CAPTURE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || filename.is_some_and(|f| f.starts_with(CRATE_SRC_PATH))
    {
        return true;
    }

    false
}

/// Check if a frame name matches panic-runtime code that sits above the user's
/// `panic!` site (`core::panicking`, `std::panicking`, unwind entry points).
#[inline]
#[must_use]
pub fn is_post_panic_code(name: &str, _filename: Option<&path::Path>) -> bool {
    if POST_PANIC_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return true;
    }

    // Newer std renders internal frames as `crate[hash]::path` (v0 mangling)
    // rather than the demangled `crate::path`. The unwind entry in particular
    // shows up as `__rustc[hash]::rust_begin_unwind`. Strip the bracket segment
    // and match the tail against the crate-less prefix forms.
    if let Some(rest) = name
        .strip_prefix("std[")
        .or_else(|| name.strip_prefix("core["))
        .or_else(|| name.strip_prefix("__rustc["))
        && let Some((_, tail)) = rest.split_once("]::")
    {
        let tail = tail.strip_prefix("sys::backtrace::").unwrap_or(tail);
        return is_post_panic_tail(tail);
    }

    false
}

/// Check if a frame name matches runtime initialization code, including the
/// v0-mangled `crate[hash]::path` spelling: an optional leading `<` and the
/// `std[`/`test[` crate designator are peeled, an inner `sys::backtrace::`
/// segment is stripped, and list entries are also tried with their
/// `std::`/`test::` module prefix removed (the peeled tail lacks it).
#[inline]
#[must_use]
pub fn is_runtime_init_code(name: &str, _filename: Option<&path::Path>) -> bool {
    if RUNTIME_INIT_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return true;
    }

    // Allow one leading `<` before the crate designator so angle-bracket
    // wrapped v0 symbols like `<std[hash]::sys::…>::method` are peeled too.
    let bare = name.strip_prefix('<').unwrap_or(name);
    if let Some(rest) = bare
        .strip_prefix("std[")
        .or_else(|| bare.strip_prefix("test["))
        && let Some((_, mut tail)) = rest.split_once("]::")
    {
        tail = tail.strip_prefix("sys::backtrace::").unwrap_or(tail);
        return RUNTIME_INIT_PREFIXES.iter().any(|prefix| {
            tail.starts_with(prefix)
                || prefix
                    .strip_prefix("std::")
                    .is_some_and(|p| tail.starts_with(p))
                || prefix
                    .strip_prefix("test::")
                    .is_some_and(|p| tail.starts_with(p))
        });
    }

    false
}

/// Like [`is_runtime_init_code`], plus matching that is only safe when
/// anchored at the bottom of the stack: frames owned by the standard-library
/// crates, core-trait impl shims, the C `main` shim, and OS entry symbols. A
/// mid-stack frame must never be classified by these rules — the bottom peel
/// stops at the first miss, which is what bounds them.
#[inline]
#[must_use]
pub fn is_runtime_tail_code(name: &str, filename: Option<&path::Path>) -> bool {
    if is_runtime_init_code(name, filename) {
        return true;
    }
    // The C entry shim; the user's own Rust `main` demangles crate-qualified.
    if name == "main" {
        return true;
    }
    // Frames owned by the standard-library crates are never user code; at
    // the bottom-contiguous tail they are all plumbing.
    let bare = name.strip_prefix('<').unwrap_or(name);
    if ["std", "core", "alloc", "test"].iter().any(|krate| {
        bare.strip_prefix(krate)
            .is_some_and(|rest| rest.starts_with("::") || rest.starts_with('['))
    }) {
        return true;
    }
    // `<… as core…>::…` impl shims (fn-pointer and boxed FnOnce dispatch).
    if name.starts_with('<') && name.contains(" as core") {
        return true;
    }
    OS_ENTRY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Strip backtrace-crate capture frames so the rendered backtrace is
        // identical across platforms (macOS captures them; Linux inlines
        // them away). We rebuild a `backtrace::Backtrace` from the filtered
        // frame vec to reuse the upstream Debug formatter verbatim.
        fn primary_name(frame: &backtrace::BacktraceFrame) -> Option<&str> {
            let symbol = frame.symbols().first()?;
            symbol.name()?.as_str()
        }
        fn primary_filename(frame: &backtrace::BacktraceFrame) -> Option<&path::Path> {
            frame
                .symbols()
                .iter()
                .next()
                .and_then(|s| s.filename().map(AsRef::as_ref))
        }
        // `full` means "show everything captured" — defer to the upstream
        // formatter without any trimming.
        if rust_backtrace().is_full() {
            return fmt::Debug::fmt(&self.as_backtrace(), f);
        }
        let frames = self.frames();
        let kept: Vec<backtrace::BacktraceFrame> = frames
            .iter()
            .filter(|frame| {
                let name = primary_name(frame);
                let filename = primary_filename(frame);
                !is_internal_frame(name, filename)
            })
            .cloned()
            .collect();
        // Trimming classifies nameless frames as internal; when no symbols
        // resolved at all that would erase the whole capture. A raw rendering
        // beats a silently empty one.
        let bt = if kept.is_empty() && !frames.is_empty() {
            backtrace::Backtrace::from(frames.to_vec())
        } else {
            backtrace::Backtrace::from(kept)
        };
        fmt::Debug::fmt(&bt, f)
    }
}

impl Backtrace {
    /// Returns a reference to the inner [`backtrace::Backtrace`].
    #[must_use]
    #[inline]
    pub fn as_backtrace(&self) -> &backtrace::Backtrace {
        match &self.inner {
            Inner::Captured(bt) => &bt.backtrace,
            Inner::Disabled(bt) => bt,
        }
    }

    /// `true` if frames were recorded at construction (capture was enabled and
    /// the platform produced a stack). An empty backtrace renders nothing and
    /// is not worth propagating over a fresh capture.
    ///
    /// Never forces symbol resolution.
    #[must_use]
    #[inline]
    pub const fn is_captured(&self) -> bool {
        match &self.inner {
            Inner::Captured(_) => true,
            Inner::Disabled(_) => false,
        }
    }

    /// Returns the frames of the backtrace.
    #[must_use]
    #[inline]
    pub fn frames(&self) -> &[backtrace::BacktraceFrame] {
        self.as_backtrace().frames()
    }

    /// Number of trailing frames hidden by this trace's marker: the suffix
    /// shared with the marker stack, counted in rendered (symbol-level)
    /// frames. `None` when no marker was set on the capturing thread, the
    /// stacks share no frames (e.g. the marker came from another thread), the
    /// cut would hide every frame, or none of the hidden frames carry symbols.
    ///
    /// Forces symbol resolution, like [`frames`](Self::frames).
    #[must_use]
    pub fn marker_hidden_frames(&self) -> Option<usize> {
        let marker = self.marker.as_ref()?;
        let frames = self.frames();
        let cut = marker.cut_len(frames);
        if cut == 0 || cut >= frames.len() {
            return None;
        }
        // The render view is symbol-level: inlined frames expand to several
        // rendered frames, unresolvable ones to none. Convert the physical
        // cut into that currency.
        let hidden = frames[frames.len() - cut..]
            .iter()
            .map(|frame| frame.symbols().len())
            .sum::<usize>();
        (hidden > 0).then_some(hidden)
    }

    /// Force symbol resolution now, caching the result in place.
    ///
    /// Resolution is otherwise deferred until the first frame access
    /// ([`as_backtrace`](Self::as_backtrace) / [`frames`](Self::frames)). Call
    /// this to pay that cost at a controlled point — e.g. once up front rather
    /// than during rendering.
    #[inline]
    pub fn resolve(&self) {
        let _ = self.as_backtrace();
    }

    #[cfg(test)]
    fn is_resolved(&self) -> bool {
        match &self.inner {
            Inner::Captured(bt) => LazyLock::get(&**bt).is_some(),
            Inner::Disabled(_) => true,
        }
    }
}

struct Capture {
    backtrace: backtrace::Backtrace,
}

mod helper {
    use std::panic::UnwindSafe;

    use super::*;

    // `LazyLock<T, F>`'s second parameter must be a *named* type, but
    // `lazy_resolve` returns a state-capturing closure — and stable Rust cannot
    // name an unboxed closure type. Boxing erases it behind `dyn FnOnce`, which
    // itself satisfies `F`, at the cost of one allocation per captured trace.
    // Once `type_alias_impl_trait` stabilizes, replace this alias with
    // `impl FnOnce() -> Capture + Send + Sync + UnwindSafe` (+ `#[define_opaque]`
    // on `lazy_resolve`) to store the closure inline and drop the allocation —
    // exactly what `std`'s own `Backtrace` does.
    pub(super) type LazyResolve = Box<dyn FnOnce() -> Capture + Send + Sync + UnwindSafe>;

    pub(super) fn lazy_resolve(mut capture: Capture) -> LazyResolve {
        Box::new(move || {
            capture.backtrace.resolve();
            capture
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureExt, Diagnostic};

    #[derive(Debug)]
    struct ErrorWithBacktrace {
        backtrace: Backtrace,
    }

    impl fmt::Display for ErrorWithBacktrace {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error with backtrace")
        }
    }

    impl std::error::Error for ErrorWithBacktrace {}

    impl Diagnostic for ErrorWithBacktrace {
        fn oopsie_backtrace(&self) -> Option<&Backtrace> {
            Some(&self.backtrace)
        }
    }

    #[derive(Debug)]
    struct ErrorWithoutBacktrace;

    impl fmt::Display for ErrorWithoutBacktrace {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error without backtrace")
        }
    }

    impl std::error::Error for ErrorWithoutBacktrace {}

    impl Diagnostic for ErrorWithoutBacktrace {}

    #[test]
    fn test_rust_backtrace_override() {
        set_rust_backtrace_override(RustBacktrace::Enabled);
        assert_eq!(rust_backtrace(), RustBacktrace::Enabled);
    }

    #[test]
    fn test_backtrace_capture_produces_backtrace() {
        let bt = Backtrace::capture();
        // Backtrace should exist (may be empty on some platforms, but should not panic)
        let _ = bt.as_backtrace().frames();
    }

    #[test]
    fn test_backtrace_capture_or_extract_with_existing_backtrace() {
        let original_bt = Backtrace::capture();
        let error = ErrorWithBacktrace {
            backtrace: original_bt.clone(),
        };

        // Should extract the existing backtrace, not capture a new one
        let extracted = Backtrace::capture_or_extract(&error);
        // Both should have the same frames count
        assert_eq!(
            extracted.as_backtrace().frames().len(),
            original_bt.as_backtrace().frames().len()
        );
    }

    #[test]
    fn test_backtrace_capture_or_extract_without_existing_backtrace() {
        let error = ErrorWithoutBacktrace;

        // Should capture a fresh backtrace
        let captured = Backtrace::capture_or_extract(&error);
        // Should produce a valid backtrace (frames list is valid, may be empty)
        let _ = captured.as_backtrace().frames();
    }

    #[test]
    fn test_backtrace_inner_returns_reference() {
        let bt = Backtrace::capture();
        let inner = bt.as_backtrace();
        // Should be able to call methods on the inner backtrace
        let _ = inner.frames();
    }

    #[test]
    fn test_capture_is_empty_when_disabled() {
        set_rust_backtrace_override(RustBacktrace::Disabled);
        let bt = Backtrace::capture();
        clear_rust_backtrace_override();
        assert!(
            bt.frames().is_empty(),
            "disabled capture must record no frames, got {}",
            bt.frames().len()
        );
    }

    #[test]
    fn test_capture_or_extract_is_empty_when_disabled() {
        set_rust_backtrace_override(RustBacktrace::Disabled);
        // No existing backtrace on the source, so this exercises the fresh
        // capture path the derive macro uses.
        let bt = Backtrace::capture_or_extract(&ErrorWithoutBacktrace);
        clear_rust_backtrace_override();
        assert!(bt.frames().is_empty());
    }

    #[test]
    fn capture_or_extract_recaptures_over_empty_source_backtrace() {
        let src = with_rust_backtrace_override(RustBacktrace::Disabled, || ErrorWithBacktrace {
            backtrace: Backtrace::capture(),
        });
        assert!(
            src.backtrace.frames().is_empty(),
            "precondition: source bt empty"
        );
        let extracted = with_rust_backtrace_override(RustBacktrace::Enabled, || {
            <Backtrace as CaptureExt>::capture_or_extract(&src)
        });
        assert!(!extracted.frames().is_empty());
    }

    #[test]
    fn clone_does_not_force_resolution() {
        let bt = with_rust_backtrace_override(RustBacktrace::Enabled, Backtrace::capture);
        let clone = bt.clone();
        assert!(!bt.is_resolved(), "clone must not symbolicate");
        assert!(!clone.is_resolved());
        let _ = clone.frames();
        assert!(bt.is_resolved(), "clones share a single resolution");
    }

    #[test]
    fn debug_never_renders_empty_for_nonempty_capture() {
        let bt = with_rust_backtrace_override(RustBacktrace::Enabled, Backtrace::capture);
        assert!(!bt.frames().is_empty());
        let rendered = with_rust_backtrace_override(RustBacktrace::Enabled, || format!("{bt:?}"));
        // Whatever the platform's symbol situation, a non-empty capture must
        // render at least one frame entry.
        assert!(rendered.contains("0:"), "rendered: {rendered}");
    }

    #[test]
    fn test_capture_records_frames_when_enabled() {
        set_rust_backtrace_override(RustBacktrace::Enabled);
        let bt = Backtrace::capture();
        clear_rust_backtrace_override();
        // Contrast with the disabled case: capture genuinely records frames
        // when enabled, so the empty-when-disabled assertions aren't vacuous.
        assert!(!bt.frames().is_empty());
    }

    #[test]
    fn is_internal_frame_drops_nameless_frames() {
        // Any frame without a symbol name is treated as internal, whether or
        // not a filename resolved.
        assert!(is_internal_frame(None, None));
        assert!(is_internal_frame(
            None,
            Some(std::path::Path::new("/home/u/src/main.rs"))
        ));
    }

    #[test]
    fn is_internal_frame_drops_backtrace_capture_machinery() {
        assert!(is_internal_frame(
            Some("std::backtrace_rs::backtrace::libunwind::trace"),
            None
        ));
        assert!(is_internal_frame(
            Some("<std::backtrace::Backtrace>::create"),
            None
        ));
    }

    #[test]
    fn is_internal_frame_drops_runtime_entry_points() {
        // Frames at the bottom-of-stack runtime boundary. Lower-level OS/libc
        // frames below these (pthread, `__GI___*`) are not matched per-frame —
        // the bottom peel ([`is_runtime_tail_code`]) recognizes them instead.
        for name in [
            "__libc_start_main",
            "__scrt_common_main_seh",
            "std::rt::lang_start_internal",
            "__rust_begin_short_backtrace<fn(), ()>",
        ] {
            assert!(
                is_internal_frame(Some(name), None),
                "expected `{name}` to be filtered"
            );
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
        // v0-mangled `crate[hash]::path` form: the `crate[hash]` segment is
        // stripped and an inner `sys::backtrace::` prefix is peeled before
        // matching against the runtime-init prefixes.
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
        // Only `std[`/`test[` get the bracket treatment.
        assert!(!is_runtime_init_code(
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
    fn is_internal_frame_drops_oopsie_core_src_path() {
        // Capture frames live in oopsie-core's own `src/`; matching them by
        // path lets the renderer sweep them even when their symbol names
        // (`capture`, backtrace-crate internals) match no known prefix.
        let p = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/backtrace.rs"));
        assert!(is_internal_frame(
            Some("oopsie_core::backtrace::Backtrace::capture"),
            Some(p)
        ));
        // The same name outside oopsie-core/src is kept.
        assert!(!is_internal_frame(
            Some("oopsie_core::backtrace::Backtrace::capture"),
            None
        ));
    }

    #[test]
    fn is_internal_frame_keeps_user_code() {
        // Benign user frame.
        assert!(!is_internal_frame(
            Some("my_crate::do_work"),
            Some(std::path::Path::new("/home/u/proj/src/main.rs"))
        ));
        // A user path that merely contains `backtrace` is not capture code:
        // only oopsie-core's own `src/` matches by path.
        assert!(!is_internal_frame(
            Some("my_crate::do_work"),
            Some(std::path::Path::new(
                "/home/u/my-backtrace-experiments/src/lib.rs"
            ))
        ));
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

    #[test]
    fn with_override_scopes_and_restores() {
        let baseline = rust_backtrace();
        let panic_baseline = rust_panic_backtrace();
        let inside = with_rust_backtrace_override(RustBacktrace::Full, rust_backtrace);
        assert_eq!(inside, RustBacktrace::Full);
        assert_eq!(rust_backtrace(), baseline);
        let inside = with_rust_backtrace_override(RustBacktrace::Disabled, rust_panic_backtrace);
        assert_eq!(inside, RustBacktrace::Disabled);
        assert_eq!(rust_panic_backtrace(), panic_baseline);
    }

    #[test]
    fn with_override_restores_on_unwind() {
        let baseline = rust_backtrace();
        let _ = std::panic::catch_unwind(|| {
            with_rust_backtrace_override(RustBacktrace::Full, || panic!("boom"))
        });
        assert_eq!(rust_backtrace(), baseline);
    }

    const _: () = {
        // Verify that Backtrace implements Capturable
        const fn is_capturable<T: crate::Capturable>() {}
        is_capturable::<Backtrace>();
    };

    const _: () = {
        // Verify that Backtrace implements CaptureExt
        const fn is_capture_ext<T: CaptureExt>() {}
        is_capture_ext::<Backtrace>();
    };

    #[test]
    fn capture_embeds_marker_and_computes_hidden_frames() {
        crate::with_rust_backtrace_override(RustBacktrace::Enabled, || {
            let _restore_guard = {
                // Isolate the TLS slot from other tests on this thread.
                let prev = crate::marker::current();
                scopeguard(prev)
            };
            let _ = crate::__private::set_marker();
            let bt = <Backtrace as crate::Capturable>::capture();
            let hidden = bt
                .marker_hidden_frames()
                .expect("same-thread marker must produce a cut");
            assert!(hidden > 0);
            // The count is in rendered (symbol-level) currency and must not
            // swallow the whole trace.
            let total_rendered: usize = bt.frames().iter().map(|f| f.symbols().len()).sum();
            assert!(hidden < total_rendered);
        });
    }

    #[test]
    fn capture_without_marker_has_no_hidden_frames() {
        crate::with_rust_backtrace_override(RustBacktrace::Enabled, || {
            let _restore_guard = scopeguard(crate::marker::current());
            crate::__private::restore_marker(None);
            let bt = <Backtrace as crate::Capturable>::capture();
            assert!(bt.marker_hidden_frames().is_none());
        });
    }

    #[test]
    fn marker_does_not_cross_threads() {
        crate::with_rust_backtrace_override(RustBacktrace::Enabled, || {
            let _ = crate::__private::set_marker();
            let bt = std::thread::spawn(|| {
                crate::with_rust_backtrace_override(RustBacktrace::Enabled, || {
                    <Backtrace as crate::Capturable>::capture()
                })
            })
            .join()
            .unwrap();
            // The spawned thread has no marker of its own.
            assert!(bt.marker_hidden_frames().is_none());
        });
    }

    /// Restore the previous marker when the test scope ends.
    fn scopeguard(prev: Option<crate::marker::TraceMarker>) -> impl Drop {
        struct Restore(Option<crate::marker::TraceMarker>);
        impl Drop for Restore {
            fn drop(&mut self) {
                crate::__private::restore_marker(self.0.take());
            }
        }
        Restore(prev)
    }
}
