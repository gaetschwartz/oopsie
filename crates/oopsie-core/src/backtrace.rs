use core::error;
use std::{borrow::Cow, fmt};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct BackTrace(backtrace::Backtrace);

impl crate::Capturable for BackTrace {
    fn capture() -> Self {
        BackTrace(backtrace::Backtrace::new())
    }

    fn capture_from(source: &dyn error::Error) -> Self {
        if let Some(source_bt) = Self::extract_from_error(source) {
            return source_bt.into_owned();
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
    pub fn extract_from_error(err: &dyn error::Error) -> Option<Cow<'_, Self>> {
        #[cfg(feature = "unstable")]
        {
            if let Some(bt_ref) = error::request_ref::<BackTrace>(err) {
                return Some(Cow::Borrowed(bt_ref));
            }
            if let Some(bt_val) = error::request_ref::<backtrace::Backtrace>(err) {
                return Some(Cow::Owned(BackTrace(bt_val.clone())));
            }
        }

        #[cfg(not(feature = "unstable"))]
        {
            _ = err;
        }

        None
    }
}
