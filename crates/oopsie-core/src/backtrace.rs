use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Backtrace(backtrace::Backtrace);

impl crate::GenerateImplicitData for Backtrace {
    fn generate() -> Self {
        Backtrace(backtrace::Backtrace::new())
    }
}

impl color_backtrace::Backtrace for Backtrace {
    fn frames(&self) -> Vec<color_backtrace::Frame> {
        color_backtrace::Backtrace::frames(&self.0)
    }
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
