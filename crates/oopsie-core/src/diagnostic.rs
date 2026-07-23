//! Simple trait for errors that expose diagnostic data.

use alloc::boxed::Box;
use alloc::sync::Arc;

use crate::{Backtrace, ErrorCode, HelpText, SpanTrace};

/// Trait for errors that expose diagnostic data.
///
/// Always implemented by `#[derive(Oopsie)]`. Provides a stable mechanism
/// for extracting backtraces, span traces, error codes, and help text
/// without relying on the unstable `Provide`/`Request` API.
pub trait Diagnostic: core::error::Error {
    /// Returns the backtrace captured when this error was created.
    #[inline]
    fn oopsie_backtrace(&self) -> Option<&Backtrace> {
        None
    }

    /// Returns the span trace captured when this error was created.
    #[inline]
    fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
        None
    }

    /// Returns the caller location captured when this error was created.
    #[inline]
    fn oopsie_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    /// Returns the error code associated with this error.
    #[inline]
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        None
    }

    /// Returns the help text associated with this error.
    #[inline]
    fn oopsie_help_text(&self) -> Option<HelpText> {
        None
    }

    /// Returns the process exit code this error should terminate with.
    ///
    /// Honored by `Report`'s `Termination` impl on stable. `NonZeroU8` is what
    /// `ExitCode::from` accepts portably, and zero would denote success on a
    /// path that is, by construction, a failure.
    #[inline]
    fn oopsie_exit_code(&self) -> Option<core::num::NonZeroU8> {
        None
    }
}

impl<T: Diagnostic> Diagnostic for Box<T> {
    #[inline]
    fn oopsie_backtrace(&self) -> Option<&Backtrace> {
        (**self).oopsie_backtrace()
    }

    #[inline]
    fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
        (**self).oopsie_spantrace()
    }

    #[inline]
    fn oopsie_location(&self) -> Option<&'static core::panic::Location<'static>> {
        (**self).oopsie_location()
    }

    #[inline]
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        (**self).oopsie_error_code()
    }

    #[inline]
    fn oopsie_help_text(&self) -> Option<HelpText> {
        (**self).oopsie_help_text()
    }

    #[inline]
    fn oopsie_exit_code(&self) -> Option<core::num::NonZeroU8> {
        (**self).oopsie_exit_code()
    }
}

// `Rc<T>` is intentionally absent: `std` provides `Error` for `Box`/`Arc` but
// not `Rc`, so `Rc<T>` cannot satisfy this trait's `Error` supertrait — and an
// `Rc` source can't satisfy `Error::source`'s return type either.
impl<T: Diagnostic> Diagnostic for Arc<T> {
    #[inline]
    fn oopsie_backtrace(&self) -> Option<&Backtrace> {
        (**self).oopsie_backtrace()
    }

    #[inline]
    fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
        (**self).oopsie_spantrace()
    }

    #[inline]
    fn oopsie_location(&self) -> Option<&'static core::panic::Location<'static>> {
        (**self).oopsie_location()
    }

    #[inline]
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        (**self).oopsie_error_code()
    }

    #[inline]
    fn oopsie_help_text(&self) -> Option<HelpText> {
        (**self).oopsie_help_text()
    }

    #[inline]
    fn oopsie_exit_code(&self) -> Option<core::num::NonZeroU8> {
        (**self).oopsie_exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Capturable;
    use std::fmt;

    #[derive(Debug)]
    struct Src {
        backtrace: Backtrace,
        spantrace: SpanTrace,
    }

    impl fmt::Display for Src {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("src")
        }
    }

    impl std::error::Error for Src {}

    impl Diagnostic for Src {
        fn oopsie_backtrace(&self) -> Option<&Backtrace> {
            Some(&self.backtrace)
        }

        fn oopsie_spantrace(&self) -> Option<&SpanTrace> {
            Some(&self.spantrace)
        }
    }

    #[test]
    fn box_delegates_diagnostic_accessors() {
        let src = Src {
            backtrace: Backtrace::capture(),
            spantrace: SpanTrace::capture(),
        };
        let boxed = Box::new(src);
        assert!(boxed.oopsie_backtrace().is_some());
        assert!(boxed.oopsie_spantrace().is_some());
    }

    #[test]
    fn box_diagnostic_extraction_reuses_source_frames() {
        use crate::{RustBacktrace, with_rust_backtrace_override};

        with_rust_backtrace_override(RustBacktrace::Enabled, || {
            let src = Src {
                backtrace: Backtrace::capture(),
                spantrace: SpanTrace::capture(),
            };
            let expected_frames = src.backtrace.frames().len();
            assert!(
                expected_frames > 0,
                "backtrace must be enabled for this test to be probative"
            );
            let boxed = Box::new(src);
            let extracted = <(Backtrace, SpanTrace) as Capturable>::capture_or_extract(&boxed);
            assert_eq!(extracted.0.frames().len(), expected_frames);
        });
    }

    #[test]
    fn arc_delegates_diagnostic_accessors() {
        let src = Src {
            backtrace: Backtrace::capture(),
            spantrace: SpanTrace::capture(),
        };
        let arced = Arc::new(src);
        assert!(arced.oopsie_backtrace().is_some());
        assert!(arced.oopsie_spantrace().is_some());
    }

    #[test]
    fn arc_diagnostic_extraction_reuses_source_frames() {
        use crate::{RustBacktrace, with_rust_backtrace_override};

        with_rust_backtrace_override(RustBacktrace::Enabled, || {
            let src = Src {
                backtrace: Backtrace::capture(),
                spantrace: SpanTrace::capture(),
            };
            let expected_frames = src.backtrace.frames().len();
            assert!(
                expected_frames > 0,
                "backtrace must be enabled for this test to be probative"
            );
            let arced = Arc::new(src);
            let extracted = <(Backtrace, SpanTrace) as Capturable>::capture_or_extract(&arced);
            assert_eq!(extracted.0.frames().len(), expected_frames);
        });
    }
}
