use std::cell::Cell;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, LazyLock};
use std::{env, fmt};

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
    /// The environment-derived setting, or `None` when neither
    /// `RUST_LIB_BACKTRACE` nor `RUST_BACKTRACE` is set. The environment is
    /// read once and cached for the life of the process.
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
    Captured(Arc<Lazy>),
    Disabled(backtrace::Backtrace), // Always empty, used when capture is disabled to avoid the LazyLock indirection.
}

/// A captured stack backtrace with deferred symbol resolution.
///
/// Captured via [`Capturable`](crate::Capturable). When backtrace capture is
/// disabled (see [`rust_backtrace`]) the capture is empty and records no
/// frames. Symbols are resolved lazily on first frame access and cached;
/// clones share one capture, so cloning is a refcount bump and resolution
/// happens once for all clones.
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
                inner: Inner::Captured(Arc::new(Lazy::new(helper::lazy_resolve(Capture {
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

/// Source directory of this crate; frames whose filename lives here are the
/// capture machinery itself.
pub const CORE_SRC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/");

impl fmt::Debug for Backtrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_backtrace(), f)
    }
}

impl Backtrace {
    /// Returns a reference to the inner [`backtrace::Backtrace`].
    ///
    /// Forces and caches full symbol resolution on first access.
    #[must_use]
    #[inline]
    pub fn as_backtrace(&self) -> &backtrace::Backtrace {
        match &self.inner {
            Inner::Captured(bt) => &bt.force().backtrace,
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
    ///
    /// Forces and caches full symbol resolution on first access.
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
        // cut into that currency — and re-apply the never-hide-everything
        // guard in it, since the kept physical frames may carry no symbols.
        let rendered = |frames: &[backtrace::BacktraceFrame]| {
            frames
                .iter()
                .map(|frame| frame.symbols().len())
                .sum::<usize>()
        };
        let hidden = rendered(&frames[frames.len() - cut..]);
        (hidden > 0 && hidden < rendered(frames)).then_some(hidden)
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
            Inner::Captured(bt) => bt.is_resolved(),
            Inner::Disabled(_) => true,
        }
    }
}

struct Capture {
    backtrace: backtrace::Backtrace,
}

/// A lazily symbol-resolved [`Capture`], shared across clones via the enclosing
/// `Arc` so resolution happens once for the whole family.
struct Lazy {
    cell: LazyLock<Capture, helper::LazyResolve>,
    // Test-only mirror of `cell`'s forced state: the direct `LazyLock::get` probe
    // isn't stable on this crate's MSRV, and a test's needs must not raise the
    // library's floor. `force` is the only path that resolves `cell`, so the two
    // stay in step.
    #[cfg(test)]
    forced: AtomicBool,
}

impl Lazy {
    fn new(resolve: helper::LazyResolve) -> Self {
        Self {
            cell: LazyLock::new(resolve),
            #[cfg(test)]
            forced: AtomicBool::new(false),
        }
    }

    /// Force symbol resolution (idempotent) and return the cached capture.
    #[inline]
    fn force(&self) -> &Capture {
        let capture = &*self.cell;
        #[cfg(test)]
        self.forced.store(true, Relaxed);
        capture
    }

    #[cfg(test)]
    fn is_resolved(&self) -> bool {
        self.forced.load(Relaxed)
    }
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
    use crate::{Capturable, Diagnostic};

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
            <Backtrace as Capturable>::capture_or_extract(&src)
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
