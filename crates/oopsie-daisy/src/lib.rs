#![cfg_attr(
    feature = "unstable",
    feature(error_generic_member_access, try_trait_v2)
)]

mod erased;
mod fancy_report;

pub use erased::{Diagnostics, ErasedError};
pub use fancy_report::{FancyReport, error_backtrace_frame_filter};
