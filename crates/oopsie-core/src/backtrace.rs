//! Thread-local control over backtrace capture.
//!
//! [`current`] reports the effective [`RustBacktrace`] setting for the calling
//! thread, derived from the environment unless overridden. [`set_override`],
//! [`clear_override`], and [`with_override`] force that setting on the current
//! thread, taking precedence over the environment. [`current_panic`] reports
//! the setting that applies to panic backtraces, which honor `RUST_BACKTRACE`
//! only.

use std::cell::Cell;

use crate::RustBacktrace;

pub(crate) mod capture;
pub(crate) mod setting;

thread_local! {
    /// Per-thread override of the backtrace setting. `None` falls back to the
    /// environment.
    static OVERRIDE: Cell<Option<RustBacktrace>> = const { Cell::new(None) };
}

/// Force [`current`] to return `value` **on the current thread**, taking
/// precedence over the environment until [`clear_override`].
///
/// This is the precise, scoped lever — it affects only backtraces captured on
/// this thread, not threads spawned afterwards. For process-wide control use
/// the `RUST_LIB_BACKTRACE` / `RUST_BACKTRACE` environment variables instead.
#[inline]
pub fn set_override(value: RustBacktrace) {
    OVERRIDE.with(|o| o.set(Some(value)));
}

/// Remove any override set by [`set_override`] on the current thread,
/// reverting [`current`] to the environment-derived value.
#[inline]
pub fn clear_override() {
    OVERRIDE.with(|o| o.set(None));
}

/// The effective backtrace setting for the current thread.
///
/// Returns the thread-local override set via [`set_override`] if present;
/// otherwise the value derived from `RUST_LIB_BACKTRACE` then
/// `RUST_BACKTRACE`. Like `std`, the environment is read once and cached — only
/// an override can change the result afterwards.
#[must_use]
#[inline]
pub fn current() -> RustBacktrace {
    if let Some(over) = OVERRIDE.with(Cell::get) {
        return over;
    }
    RustBacktrace::detect_opt().unwrap_or(RustBacktrace::Disabled)
}

/// The effective *panic* backtrace setting for the current thread: the
/// thread-local override if present, else `RUST_BACKTRACE` (only).
#[must_use]
#[inline]
pub fn current_panic() -> RustBacktrace {
    if let Some(over) = OVERRIDE.with(Cell::get) {
        return over;
    }
    RustBacktrace::detect_panic_opt().unwrap_or(RustBacktrace::Disabled)
}

/// Run `f` with the thread's backtrace setting forced to `value`, restoring
/// the previous override (or lack of one) afterwards — including on unwind.
#[inline]
pub fn with_override<R>(value: RustBacktrace, f: impl FnOnce() -> R) -> R {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_override_takes_effect() {
        set_override(RustBacktrace::Enabled);
        assert_eq!(current(), RustBacktrace::Enabled);
    }

    #[test]
    fn with_override_scopes_and_restores() {
        let baseline = current();
        let panic_baseline = current_panic();
        let inside = with_override(RustBacktrace::Full, current);
        assert_eq!(inside, RustBacktrace::Full);
        assert_eq!(current(), baseline);
        let inside = with_override(RustBacktrace::Disabled, current_panic);
        assert_eq!(inside, RustBacktrace::Disabled);
        assert_eq!(current_panic(), panic_baseline);
    }

    #[test]
    fn with_override_restores_on_unwind() {
        let baseline = current();
        let _ = std::panic::catch_unwind(|| with_override(RustBacktrace::Full, || panic!("boom")));
        assert_eq!(current(), baseline);
    }
}
