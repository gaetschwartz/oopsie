//! Simple trait for errors that expose diagnostic data.

use crate::{Backtrace, ErrorCode, HelpText, SpanTrace};

/// Trait for errors that expose diagnostic data.
///
/// Always implemented by `#[derive(Oopsie)]`. Provides a stable mechanism
/// for extracting backtraces, span traces, error codes, and help text
/// without relying on the unstable `Provide`/`Request` API.
pub trait Diagnostic: std::error::Error {
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
    fn oopsie_error_code(&self) -> Option<ErrorCode> {
        (**self).oopsie_error_code()
    }

    #[inline]
    fn oopsie_help_text(&self) -> Option<HelpText> {
        (**self).oopsie_help_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Capturable as _;
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
        use crate::{CaptureExt, RustBacktrace, with_rust_backtrace_override};

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
            let extracted = <(Backtrace, SpanTrace) as CaptureExt>::capture_or_extract(&boxed);
            assert_eq!(extracted.0.frames().len(), expected_frames);
        });
    }
}
