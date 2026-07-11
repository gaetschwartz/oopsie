//! no_std replacements for the std / `backtrace`-crate-backed types.
//!
//! Under `not(feature = "std")` there is no unwinder, so `Backtrace` is an
//! always-empty stub and trace markers are no-ops. They exist only to keep the
//! public API shape identical to the std build.

use core::fmt;

/// An always-empty backtrace: the no_std build has no stack unwinder.
#[derive(Clone, Default)]
pub struct Backtrace {
    _priv: (),
}

impl Backtrace {
    /// Always `false` under no_std — no frames are ever recorded.
    #[must_use]
    #[inline]
    pub const fn is_captured(&self) -> bool {
        false
    }
}

impl crate::Capturable for Backtrace {
    #[inline]
    fn capture() -> Self {
        Self { _priv: () }
    }

    #[inline]
    fn capture_or_extract(source: &dyn crate::Diagnostic) -> Self {
        match source.oopsie_backtrace() {
            Some(trace) if trace.is_captured() => trace.clone(),
            _ => Self::capture(),
        }
    }
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<backtrace unavailable (no_std)>")
    }
}

impl fmt::Display for Backtrace {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

/// A recorded stack marker. Under no_std no stack is walked; this is a
/// zero-sized placeholder kept so `start_marker!` resolves.
#[doc(hidden)]
#[derive(Debug, Clone, Default)]
pub struct TraceMarker;

/// No-op under no_std: there is no thread-local marker stack to push to.
#[doc(hidden)]
#[must_use]
#[inline]
pub const fn set_marker() -> Option<TraceMarker> {
    None
}

/// No-op under no_std: there is no thread-local marker stack to restore.
#[doc(hidden)]
#[inline]
pub const fn restore_marker(_prev: Option<TraceMarker>) {}
