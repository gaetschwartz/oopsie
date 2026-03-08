#![cfg_attr(
    feature = "unstable",
    feature(error_generic_member_access, try_trait_v2)
)]

mod erased;
mod fancy_report;

pub use erased::{Diagnostics, ErasedError};
pub use fancy_report::{FancyReport, error_backtrace_frame_filter};

// Re-exports needed by the `#[oopsie(path = "crate")]` macro when used within this crate's tests.
#[doc(hidden)]
pub use oopsie_core::{
    Backtrace, ErrorCode, GenerateImplicitData, HelpText, IntoError, NoneError, Spantrace,
};
