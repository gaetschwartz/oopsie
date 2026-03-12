#![cfg_attr(
    feature = "unstable",
    feature(error_generic_member_access, try_trait_v2)
)]

mod erased;
mod fancy_report;

pub use erased::{Diagnostics, ErasedError};
pub use fancy_report::{FancyReport, error_backtrace_frame_filter};

#[inline]
pub(crate) fn extract_value_from_error<T: 'static>(err: &dyn std::error::Error) -> Option<T> {
    #[cfg(feature = "unstable")]
    {
        std::error::request_value::<T>(err)
    }
    #[cfg(not(feature = "unstable"))]
    {
        _ = err;
        None
    }
}
