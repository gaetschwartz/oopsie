use std::fmt;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, LazyLock};

use super::current;

// Clones share one `Arc`-held capture, so cloning is a refcount bump and the
// first frame access resolves symbols exactly once for all clones.
#[derive(Clone)]
enum Inner {
    Captured(Arc<Lazy>),
    Disabled(::backtrace::Backtrace), // Always empty, used when capture is disabled to avoid the LazyLock indirection.
}

/// A captured stack backtrace with deferred symbol resolution.
///
/// Captured via [`Capturable`](crate::Capturable). When backtrace capture is
/// disabled (see [`current`](crate::backtrace::current)) the capture is empty and records no
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
        if current().is_enabled() {
            let backtrace = ::backtrace::Backtrace::new_unresolved();
            if backtrace.frames().is_empty() {
                return Self {
                    inner: Inner::Disabled(backtrace),
                    marker: None,
                };
            }
            Self {
                inner: Inner::Captured(Arc::new(Lazy::new(helper::lazy_resolve(Capture {
                    backtrace,
                })))),
                marker: crate::marker::current(),
            }
        } else {
            Self {
                inner: Inner::Disabled(::backtrace::Backtrace::from(vec![])),
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
        fmt::Debug::fmt(raw_backtrace(self), f)
    }
}

/// The raw `backtrace`-crate capture behind `bt`, forcing and caching symbol
/// resolution on first access.
#[must_use]
#[inline]
pub fn raw_backtrace(bt: &Backtrace) -> &::backtrace::Backtrace {
    match &bt.inner {
        Inner::Captured(lazy) => &lazy.force().backtrace,
        Inner::Disabled(raw) => raw,
    }
}

/// The physical frames of `bt`, forcing and caching symbol resolution on
/// first access.
#[must_use]
#[inline]
pub fn backtrace_frames(bt: &Backtrace) -> &[::backtrace::BacktraceFrame] {
    raw_backtrace(bt).frames()
}

impl Backtrace {
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

    /// Number of trailing frames hidden by this trace's marker: the suffix
    /// shared with the marker stack, counted in rendered (symbol-level)
    /// frames. `None` when no marker was set on the capturing thread, the
    /// stacks share no frames (e.g. the marker came from another thread), the
    /// cut would hide every frame, or none of the hidden frames carry symbols.
    ///
    /// Forces symbol resolution.
    #[must_use]
    pub fn marker_hidden_frames(&self) -> Option<usize> {
        let marker = self.marker.as_ref()?;
        let frames = backtrace_frames(self);
        let cut = marker.cut_len(frames);
        if cut == 0 || cut >= frames.len() {
            return None;
        }
        // The render view is symbol-level: inlined frames expand to several
        // rendered frames, unresolvable ones to none. Convert the physical
        // cut into that currency — and re-apply the never-hide-everything
        // guard in it, since the kept physical frames may carry no symbols.
        let rendered = |frames: &[::backtrace::BacktraceFrame]| {
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
    /// Resolution is otherwise deferred until the frames are first read. Call
    /// this to pay that cost at a controlled point — e.g. once up front rather
    /// than during rendering.
    #[inline]
    pub fn resolve(&self) {
        let _ = raw_backtrace(self);
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
    backtrace: ::backtrace::Backtrace,
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
    use crate::backtrace::{clear_override, set_override, with_override};
    use crate::{Capturable, Diagnostic, RustBacktrace};

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
    fn test_backtrace_capture_produces_backtrace() {
        let bt = Backtrace::capture();
        // Backtrace should exist (may be empty on some platforms, but should not panic)
        let _ = raw_backtrace(&bt).frames();
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
            raw_backtrace(&extracted).frames().len(),
            raw_backtrace(&original_bt).frames().len()
        );
    }

    #[test]
    fn test_backtrace_capture_or_extract_without_existing_backtrace() {
        let error = ErrorWithoutBacktrace;

        // Should capture a fresh backtrace
        let captured = Backtrace::capture_or_extract(&error);
        // Should produce a valid backtrace (frames list is valid, may be empty)
        let _ = raw_backtrace(&captured).frames();
    }

    #[test]
    fn test_backtrace_inner_returns_reference() {
        let bt = Backtrace::capture();
        let inner = raw_backtrace(&bt);
        // Should be able to call methods on the inner backtrace
        let _ = inner.frames();
    }

    #[test]
    fn test_capture_is_empty_when_disabled() {
        set_override(RustBacktrace::Disabled);
        let bt = Backtrace::capture();
        clear_override();
        assert!(
            backtrace_frames(&bt).is_empty(),
            "disabled capture must record no frames, got {}",
            backtrace_frames(&bt).len()
        );
    }

    #[test]
    fn test_capture_or_extract_is_empty_when_disabled() {
        set_override(RustBacktrace::Disabled);
        // No existing backtrace on the source, so this exercises the fresh
        // capture path the derive macro uses.
        let bt = Backtrace::capture_or_extract(&ErrorWithoutBacktrace);
        clear_override();
        assert!(backtrace_frames(&bt).is_empty());
    }

    #[test]
    fn is_captured_matches_frame_emptiness() {
        let enabled = with_override(RustBacktrace::Enabled, Backtrace::capture);
        assert_eq!(
            enabled.is_captured(),
            !backtrace_frames(&enabled).is_empty()
        );

        let disabled = with_override(RustBacktrace::Disabled, Backtrace::capture);
        assert_eq!(
            disabled.is_captured(),
            !backtrace_frames(&disabled).is_empty()
        );
        assert!(!disabled.is_captured());
    }

    #[test]
    fn capture_or_extract_recaptures_over_empty_source_backtrace() {
        let src = with_override(RustBacktrace::Disabled, || ErrorWithBacktrace {
            backtrace: Backtrace::capture(),
        });
        assert!(
            backtrace_frames(&src.backtrace).is_empty(),
            "precondition: source bt empty"
        );
        let extracted = with_override(RustBacktrace::Enabled, || {
            <Backtrace as Capturable>::capture_or_extract(&src)
        });
        assert!(!backtrace_frames(&extracted).is_empty());
    }

    #[test]
    fn clone_does_not_force_resolution() {
        let bt = with_override(RustBacktrace::Enabled, Backtrace::capture);
        let clone = bt.clone();
        assert!(!bt.is_resolved(), "clone must not symbolicate");
        assert!(!clone.is_resolved());
        let _ = backtrace_frames(&clone);
        assert!(bt.is_resolved(), "clones share a single resolution");
    }

    #[test]
    fn debug_never_renders_empty_for_nonempty_capture() {
        let bt = with_override(RustBacktrace::Enabled, Backtrace::capture);
        assert!(!backtrace_frames(&bt).is_empty());
        let rendered = with_override(RustBacktrace::Enabled, || format!("{bt:?}"));
        // Whatever the platform's symbol situation, a non-empty capture must
        // render at least one frame entry.
        assert!(rendered.contains("0:"), "rendered: {rendered}");
    }

    #[test]
    fn test_capture_records_frames_when_enabled() {
        set_override(RustBacktrace::Enabled);
        let bt = Backtrace::capture();
        clear_override();
        // Contrast with the disabled case: capture genuinely records frames
        // when enabled, so the empty-when-disabled assertions aren't vacuous.
        assert!(!backtrace_frames(&bt).is_empty());
    }

    const _: () = {
        // Verify that Backtrace implements Capturable
        const fn is_capturable<T: crate::Capturable>() {}
        is_capturable::<Backtrace>();
    };

    #[test]
    fn capture_embeds_marker_and_computes_hidden_frames() {
        with_override(RustBacktrace::Enabled, || {
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
            let total_rendered: usize = backtrace_frames(&bt)
                .iter()
                .map(|f| f.symbols().len())
                .sum();
            assert!(hidden < total_rendered);
        });
    }

    #[test]
    fn capture_without_marker_has_no_hidden_frames() {
        with_override(RustBacktrace::Enabled, || {
            let _restore_guard = scopeguard(crate::marker::current());
            crate::__private::restore_marker(None);
            let bt = <Backtrace as crate::Capturable>::capture();
            assert!(bt.marker_hidden_frames().is_none());
        });
    }

    #[test]
    fn marker_does_not_cross_threads() {
        with_override(RustBacktrace::Enabled, || {
            let _ = crate::__private::set_marker();
            let bt = std::thread::spawn(|| {
                with_override(RustBacktrace::Enabled, || {
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
