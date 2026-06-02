use std::cell::Cell;
use std::sync::LazyLock;
use std::{fmt, path};

use crate::Capturable as _;

/// Whether backtrace capture is enabled, and how verbosely it should render.
///
/// Resolved once from the environment (see [`rust_backtrace`]) following the
/// same convention as `std`: `RUST_LIB_BACKTRACE` is consulted first and, if
/// unset, `RUST_BACKTRACE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustBacktrace {
    /// Capture disabled — no frames are recorded.
    Disabled,
    /// Capture enabled, rendered as the trimmed "short" view.
    Enabled,
    /// Capture enabled, rendered untrimmed — every frame is shown.
    Full,
}

impl RustBacktrace {
    fn detect() -> Self {
        // `RUST_LIB_BACKTRACE` wins when set; otherwise fall back to
        // `RUST_BACKTRACE`. `or_else` only fires on `Err` (var unset or
        // non-unicode), so an explicit `RUST_LIB_BACKTRACE=0` disables even
        // when `RUST_BACKTRACE=1`.
        let raw = std::env::var("RUST_LIB_BACKTRACE").or_else(|_| std::env::var("RUST_BACKTRACE"));
        match raw.as_deref() {
            Ok("0") | Err(_) => Self::Disabled,
            Ok("full") => Self::Full,
            Ok(_) => Self::Enabled,
        }
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
    static CACHED: LazyLock<RustBacktrace> = LazyLock::new(RustBacktrace::detect);
    if let Some(over) = OVERRIDE.with(Cell::get) {
        return over;
    }
    *CACHED
}

#[derive(Clone)]
pub struct Backtrace(backtrace::Backtrace);

impl crate::Capturable for Backtrace {
    #[inline]
    fn capture() -> Self {
        if rust_backtrace().is_enabled() {
            Self(backtrace::Backtrace::new_unresolved())
        } else {
            Self(backtrace::Backtrace::from(vec![]))
        }
    }
}

impl crate::CaptureExt for Backtrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        source
            .oopsie_backtrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

impl color_backtrace::Backtrace for Backtrace {
    #[inline]
    fn frames(&self) -> Vec<color_backtrace::Frame> {
        let mut frames = color_backtrace::Backtrace::frames(&self.0);
        if !rust_backtrace().is_full() {
            frames.retain(|f| !is_internal_frame(f.name.as_deref(), f.filename.as_deref()));
        }
        frames
    }
}

/// Returns `true` for frames that are implementation/platform detail and
/// should not appear in user-facing renderings.
///
/// Covers three classes of noise:
/// - **Top of stack**: the `backtrace` crate's own capture machinery
///   (`backtrace::backtrace::*`, `<backtrace::capture::*>::*`). macOS
///   captures them; Linux inlines them away.
/// - **Bottom of stack**: OS-level thread / libc / pthread entry points
///   (`__pthread_*`, `__libc_start*`, etc.). These appear on some
///   platforms (macOS pthread) and not others (Linux's libc start).
/// - **Panic/unwind plumbing**: the `__rust_try` shim emitted around
///   `catch_unwind` boundaries — runtime plumbing, not user code.
///
/// Filtering at the source means `Report`, `ErasedError`, and any future
/// consumer all see a stable backtrace shape across macOS and Linux.
#[must_use]
pub fn is_internal_frame(name: Option<&str>, filename: Option<&path::Path>) -> bool {
    // Unresolvable frame (no symbol name, no filename). On Linux these
    // typically sit at the bottom of stack where the dynamic linker can't
    // resolve into a Rust/libc function — they're never user-actionable and
    // their presence varies by platform/build. Drop them.
    if name.is_none() && filename.is_none() {
        return true;
    }
    if let Some(n) = name {
        // Top-of-stack: `backtrace` crate capture machinery.
        if n.starts_with("backtrace::") || n.starts_with("<backtrace::") {
            return true;
        }
        // Bottom-of-stack OS thread / libc / pthread internals, plus the
        // `__rust_try` panic/unwind shim.
        if n.starts_with("__pthread_")
            || n.starts_with("_pthread_")
            || n.starts_with("__libc_start")
            || n.starts_with("__GI___")
            || n.starts_with("__rust_try")
        {
            return true;
        }
    }
    if let Some(p) = filename {
        // Path components are the cleanest match: avoids false positives on
        // e.g. `/home/.../my-backtrace-experiments/...`.
        for component in p.components() {
            if let path::Component::Normal(s) = component
                && let Some(s) = s.to_str()
                && s.starts_with("backtrace-")
            {
                return true;
            }
        }
    }
    false
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
            return fmt::Debug::fmt(&self.0, f);
        }
        let kept: Vec<backtrace::BacktraceFrame> = self
            .frames()
            .iter()
            .filter(|frame| {
                let name = primary_name(frame);
                let filename = primary_filename(frame);
                !is_internal_frame(name, filename)
            })
            .cloned()
            .collect();
        let bt = backtrace::Backtrace::from(kept);
        fmt::Debug::fmt(&bt, f)
    }
}

impl Backtrace {
    /// Returns a reference to the inner [`backtrace::Backtrace`].
    #[must_use]
    #[inline]
    pub const fn as_backtrace(&self) -> &backtrace::Backtrace {
        &self.0
    }

    /// Returns a mutable reference to the inner [`backtrace::Backtrace`].
    #[must_use]
    #[inline]
    pub const fn as_backtrace_mut(&mut self) -> &mut backtrace::Backtrace {
        &mut self.0
    }

    /// Returns the frames of the backtrace.
    #[must_use]
    #[inline]
    pub fn frames(&self) -> &[backtrace::BacktraceFrame] {
        self.as_backtrace().frames()
    }

    /// Resolves the backtrace's symbols.
    #[inline]
    pub fn resolve(&mut self) {
        self.0.resolve();
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
    fn test_capture_records_frames_when_enabled() {
        set_rust_backtrace_override(RustBacktrace::Enabled);
        let bt = Backtrace::capture();
        clear_rust_backtrace_override();
        // Contrast with the disabled case: capture genuinely records frames
        // when enabled, so the empty-when-disabled assertions aren't vacuous.
        assert!(!bt.frames().is_empty());
    }

    #[test]
    fn is_internal_frame_drops_unresolvable_frames() {
        // Neither symbol name nor filename — only the no/no arm returns true.
        assert!(is_internal_frame(None, None));
        assert!(!is_internal_frame(
            None,
            Some(std::path::Path::new("/home/u/src/main.rs"))
        ));
    }

    #[test]
    fn is_internal_frame_drops_backtrace_capture_machinery() {
        assert!(is_internal_frame(Some("backtrace::backtrace::trace"), None));
        assert!(is_internal_frame(
            Some("<backtrace::capture::Backtrace>::new"),
            None
        ));
    }

    #[test]
    fn is_internal_frame_drops_os_and_unwind_internals() {
        for name in [
            "__pthread_cond_wait",
            "_pthread_start",
            "__libc_start_main",
            "__GI___clone",
            "__rust_try",
        ] {
            assert!(
                is_internal_frame(Some(name), None),
                "expected `{name}` to be filtered"
            );
        }
    }

    #[test]
    fn is_internal_frame_drops_by_backtrace_crate_path_component() {
        let p =
            std::path::Path::new("/home/u/.cargo/registry/src/index/backtrace-0.3.71/src/lib.rs");
        assert!(is_internal_frame(Some("backtrace_rs::foo"), Some(p)));
        // The name alone does NOT match (`backtrace_rs::` is not `backtrace::`),
        // so the path component `backtrace-0.3.71` is what triggers the filter.
        assert!(!is_internal_frame(Some("backtrace_rs::foo"), None));
    }

    #[test]
    fn is_internal_frame_keeps_user_code() {
        // Benign user frame.
        assert!(!is_internal_frame(
            Some("my_crate::do_work"),
            Some(std::path::Path::new("/home/u/proj/src/main.rs"))
        ));
        // False-positive guard: a path containing `my-backtrace-experiments`
        // must NOT match — the check is on whole components prefixed
        // `backtrace-`, not substrings.
        assert!(!is_internal_frame(
            Some("my_crate::do_work"),
            Some(std::path::Path::new(
                "/home/u/my-backtrace-experiments/src/lib.rs"
            ))
        ));
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
}
