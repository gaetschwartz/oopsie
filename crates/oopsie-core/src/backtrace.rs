use std::fmt;

use crate::Capturable as _;

#[derive(Clone)]
pub struct BackTrace(backtrace::Backtrace);

impl crate::Capturable for BackTrace {
    fn capture() -> Self {
        BackTrace(backtrace::Backtrace::new())
    }
}

impl crate::CaptureExt for BackTrace {
    fn capture_or_extract(source: &dyn crate::ErrorExt) -> Self {
        source
            .oopsie_backtrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

impl color_backtrace::Backtrace for BackTrace {
    fn frames(&self) -> Vec<color_backtrace::Frame> {
        color_backtrace::Backtrace::frames(&self.0)
    }
}

impl fmt::Debug for BackTrace {
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

    pub fn extract_from_error(err: &(impl crate::ErrorExt + ?Sized)) -> Option<&Self> {
        err.oopsie_backtrace()
    }
}
