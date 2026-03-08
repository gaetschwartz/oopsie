// Re-export the #[oopsie] proc-macro attribute.
pub use oopsie_macros::oopsie;

// Re-export all core types.
pub use oopsie_core::*;

// Re-export oopsie-daisy when the "daisy" feature is enabled.
#[cfg(feature = "daisy")]
pub use oopsie_daisy::*;
