#![cfg_attr(
    feature = "unstable",
    feature(error_generic_member_access, try_trait_v2)
)]

mod erased;
mod fancy_report;
pub mod trace_printer;

pub use erased::{Diagnostics, ErasedError};
pub use fancy_report::FancyReport;
pub use trace_printer::{
    BacktraceProvider, FrameFilter, SpanTraceProvider, TracePrinter, TraceTheme,
};
