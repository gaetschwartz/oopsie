use std::fmt;

use crate::Capturable as _;

#[derive(Clone)]
pub struct BackTrace(backtrace::Backtrace);

impl crate::Capturable for BackTrace {
    #[inline]
    fn capture() -> Self {
        BackTrace(backtrace::Backtrace::new())
    }
}

impl crate::CaptureExt for BackTrace {
    #[inline]
    fn capture_or_extract(source: &dyn crate::ErrorExt) -> Self {
        source
            .oopsie_backtrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

impl color_backtrace::Backtrace for BackTrace {
    #[inline]
    fn frames(&self) -> Vec<color_backtrace::Frame> {
        color_backtrace::Backtrace::frames(&self.0)
    }
}

impl fmt::Debug for BackTrace {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl BackTrace {
    /// Returns a reference to the inner [`backtrace::Backtrace`].
    #[must_use]
    pub const fn inner(&self) -> &backtrace::Backtrace {
        &self.0
    }

    #[must_use]
    #[inline]
    pub fn extract_from_error(err: &(impl crate::ErrorExt + ?Sized)) -> Option<&Self> {
        err.oopsie_backtrace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureExt, ErrorExt};

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

    impl ErrorExt for ErrorWithBacktrace {
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

    impl ErrorExt for ErrorWithoutBacktrace {}

    #[test]
    fn test_backtrace_capture_produces_backtrace() {
        let bt = BackTrace::capture();
        // Backtrace should exist (may be empty on some platforms, but should not panic)
        let _ = bt.inner().frames();
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
            extracted.inner().frames().len(),
            original_bt.inner().frames().len()
        );
    }

    #[test]
    fn test_backtrace_capture_or_extract_without_existing_backtrace() {
        let error = ErrorWithoutBacktrace;

        // Should capture a fresh backtrace
        let captured = BackTrace::capture_or_extract(&error);
        // Should produce a valid backtrace (frames list is valid, may be empty)
        let _ = captured.inner().frames();
    }

    #[test]
    fn test_backtrace_extract_from_error_with_backtrace() {
        let bt = BackTrace::capture();
        let error = ErrorWithBacktrace {
            backtrace: bt.clone(),
        };

        let extracted = BackTrace::extract_from_error(&error);
        assert!(extracted.is_some());
        assert_eq!(
            extracted.unwrap().inner().frames().len(),
            bt.inner().frames().len()
        );
    }

    #[test]
    fn test_backtrace_extract_from_error_without_backtrace() {
        let error = ErrorWithoutBacktrace;
        let extracted = BackTrace::extract_from_error(&error);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_backtrace_inner_returns_reference() {
        let bt = BackTrace::capture();
        let inner = bt.inner();
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
