//! Serializable, type-erased error representations (the `serde` feature).
mod backtrace;
pub use backtrace::{ErasedBacktrace, ErasedFrame};

mod spantrace;
pub use spantrace::{ErasedMetadata, ErasedSpan, ErasedSpanTrace, TracingLevel};
