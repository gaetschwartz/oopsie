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
            Ok("0") | Err(_) => RustBacktrace::Disabled,
            Ok("full") => RustBacktrace::Full,
            Ok(_) => RustBacktrace::Enabled,
        }
    }

    /// Whether frames should be captured at all.
    #[inline]
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, RustBacktrace::Full | RustBacktrace::Enabled)
    }

    /// Whether rendering should show every frame, skipping the trimming that
    /// hides capture machinery and runtime setup/teardown frames.
    #[inline]
    #[must_use]
    pub const fn is_full(self) -> bool {
        matches!(self, RustBacktrace::Full)
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
pub struct BackTrace(backtrace::Backtrace);

impl crate::Capturable for BackTrace {
    #[inline]
    fn capture() -> Self {
        if rust_backtrace().is_enabled() {
            BackTrace(backtrace::Backtrace::new_unresolved())
        } else {
            BackTrace(backtrace::Backtrace::from(vec![]))
        }
    }
}

impl crate::CaptureExt for BackTrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        source
            .oopsie_backtrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

impl color_backtrace::Backtrace for BackTrace {
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
/// Covers two classes of noise:
/// - **Top of stack**: the `backtrace` crate's own capture machinery
///   (`backtrace::backtrace::*`, `<backtrace::capture::*>::*`). macOS
///   captures them; Linux inlines them away.
/// - **Bottom of stack**: OS-level thread / libc / pthread entry points
///   (`__pthread_*`, `__libc_start*`, etc.). These appear on some
///   platforms (macOS pthread) and not others (Linux's libc start).
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
        // Bottom-of-stack: OS thread / libc / pthread internals.
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

impl fmt::Debug for BackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Strip backtrace-crate capture frames so the rendered backtrace is
        // identical across platforms (macOS captures them; Linux inlines
        // them away). We rebuild a `backtrace::Backtrace` from the filtered
        // frame vec to reuse the upstream Debug formatter verbatim.
        fn primary_name(frame: &backtrace::BacktraceFrame) -> Option<String> {
            frame
                .symbols()
                .iter()
                .next()
                .and_then(|s| s.name().map(|n| n.to_string()))
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
                !is_internal_frame(name.as_deref(), filename)
            })
            .cloned()
            .collect();
        let bt = backtrace::Backtrace::from(kept);
        fmt::Debug::fmt(&bt, f)
    }
}

impl BackTrace {
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
        backtrace: BackTrace,
    }

    impl fmt::Display for ErrorWithBacktrace {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "error with backtrace")
        }
    }

    impl std::error::Error for ErrorWithBacktrace {}

    impl Diagnostic for ErrorWithBacktrace {
        fn oopsie_backtrace(&self) -> Option<&BackTrace> {
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
        let bt = BackTrace::capture();
        // Backtrace should exist (may be empty on some platforms, but should not panic)
        let _ = bt.as_backtrace().frames();
    }

    #[test]
    fn test_backtrace_capture_or_extract_with_existing_backtrace() {
        let original_bt = BackTrace::capture();
        let error = ErrorWithBacktrace {
            backtrace: original_bt.clone(),
        };

        // Should extract the existing backtrace, not capture a new one
        let extracted = BackTrace::capture_or_extract(&error);
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
        let captured = BackTrace::capture_or_extract(&error);
        // Should produce a valid backtrace (frames list is valid, may be empty)
        let _ = captured.as_backtrace().frames();
    }

    #[test]
    fn test_backtrace_inner_returns_reference() {
        let bt = BackTrace::capture();
        let inner = bt.as_backtrace();
        // Should be able to call methods on the inner backtrace
        let _ = inner.frames();
    }

    const _: () = {
        // Verify that BackTrace implements Capturable
        const fn is_capturable<T: crate::Capturable>() {}
        is_capturable::<BackTrace>();
    };

    const _: () = {
        // Verify that BackTrace implements CaptureExt
        const fn is_capture_ext<T: CaptureExt>() {}
        is_capture_ext::<BackTrace>();
    };
}
