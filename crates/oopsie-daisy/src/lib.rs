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

/// Snapshot redaction helpers for tests. Centralises filter patterns so they
/// don't have to be repeated at every `insta::with_settings!` call site.
#[cfg(test)]
macro_rules! redact {
    (backtrace_json, $bl:block) => {
        insta::with_settings! {
            { filters => [
                (r#""line": \d+"#, r#""line": [LINE]"#),
                (r#""filename": "[^"]+""#, r#""filename": "[FILE]""#),
                (r"\[[0-9a-f]{7,16}\]", "[PTR]"),
            ] }, $bl
        }
    };
    (backtrace, $bl:block) => {
        insta::with_settings! {
            { filters => [
                (r"\[[0-9a-f]{7,16}\]", "[PTR]"),
                (r"rs:\d+:\d+", "rs:[LOC]"),
                (r"\/rustc\/[a-f0-9]+\/", "/rustc/[COMMIT]/"),
                (&env!("CARGO_MANIFEST_DIR").replace("/", r"\/"), "[CRATE_DIR]"),
            ] }, $bl
        }
    };
}

#[cfg(test)]
pub(crate) use redact;

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
