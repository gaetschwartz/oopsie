#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![cfg_attr(feature = "unstable-try-trait-v2", feature(try_trait_v2))]

// Re-export the #[traced] proc-macro attribute and #[derive(Oopsie)].
pub use oopsie_macros::Oopsie;
pub use oopsie_macros::traced;

// Re-export all core types.
pub use oopsie_core::*;

#[cfg(feature = "fancy")]
mod color;
#[cfg(feature = "fancy")]
pub use color::{ColorConfig, get_color_mode, set_color_mode};

#[cfg(feature = "fancy")]
mod report;
#[cfg(feature = "fancy")]
pub use report::Report;

#[cfg(feature = "fancy")]
pub mod trace_printer;
