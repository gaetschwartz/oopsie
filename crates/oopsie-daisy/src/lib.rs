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
    BackTrace, ErrorCode, GenerateImplicitData, HelpText, IntoError, NoneError, SpanTrace,
};

#[inline]
pub(crate) fn extract_from_error_ref<T: 'static>(err: &dyn std::error::Error) -> Option<&T> {
    #[cfg(feature = "unstable")]
    {
        std::error::request_ref::<T>(err)
    }
    #[cfg(not(feature = "unstable"))]
    {
        _ = err;
        None
    }
}
