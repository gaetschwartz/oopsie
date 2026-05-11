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
        frames.retain(|f| !is_internal_capture_frame(f.name.as_deref(), f.filename.as_deref()));
        frames
    }
}

/// Returns `true` for frames belonging to the `backtrace` crate's own
/// capture machinery (`backtrace::backtrace::*`, `<backtrace::capture::*>::*`).
///
/// These frames are platform/toolchain dependent: macOS captures them,
/// Linux inlines them away. They're never user-relevant — the user wants
/// the stack from *their* code, not the implementation of the capture
/// mechanism. Filtering them at the source means `Report`, `ErasedError`,
/// and any future consumer all see a stable backtrace shape.
pub fn is_internal_capture_frame(name: Option<&str>, filename: Option<&std::path::Path>) -> bool {
    if let Some(n) = name
        && (n.starts_with("backtrace::") || n.starts_with("<backtrace::"))
    {
        return true;
    }
    if let Some(p) = filename {
        // Path components are the cleanest match: avoids false positives on
        // e.g. `/home/.../my-backtrace-experiments/...`.
        for component in p.components() {
            if let std::path::Component::Normal(s) = component
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
        let primary_name = |frame: &backtrace::BacktraceFrame| -> Option<String> {
            frame
                .symbols()
                .iter()
                .next()
                .and_then(|s| s.name().map(|n| n.to_string()))
        };
        let primary_filename = |frame: &backtrace::BacktraceFrame| {
            frame
                .symbols()
                .iter()
                .next()
                .and_then(|s| s.filename().map(std::borrow::ToOwned::to_owned))
        };
        let kept: Vec<backtrace::BacktraceFrame> = self
            .0
            .frames()
            .iter()
            .filter(|frame| {
                let name = primary_name(frame);
                let filename = primary_filename(frame);
                !is_internal_capture_frame(name.as_deref(), filename.as_deref())
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
    pub const fn inner(&self) -> &backtrace::Backtrace {
        &self.0
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
