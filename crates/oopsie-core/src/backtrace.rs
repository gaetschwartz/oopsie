use std::fmt;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct BackTrace(backtrace::Backtrace);

impl crate::Capturable for BackTrace {
    fn capture() -> Self {
        BackTrace(backtrace::Backtrace::new())
    }

    fn capture_from(source: &dyn std::error::Error) -> Self {
        #[cfg(feature = "unstable")]
        {
            if let Some(bt) = core::error::request_ref::<Self>(source) {
                return bt.clone();
            }
        }
        #[cfg(not(feature = "unstable"))]
        {
            _ = source;
        }
        Self::capture()
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
    pub fn extract_from_error(err: &(impl crate::ErrorExt + ?Sized)) -> Option<&Self> {
        err.oopsie_backtrace()
    }
}
