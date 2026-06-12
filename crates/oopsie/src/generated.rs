//! Registry of `#[oopsie]` macro invocation sites.
//!
//! Macro-generated code carries the span of its invocation, so its frames
//! resolve to the attribute's own file and line. Each expansion registers
//! that location here; the fancy renderer hides matching frames the same
//! way it hides its own capture machinery.

use linkme::distributed_slice;

/// One `#[oopsie]` / `derive(Oopsie)` expansion site.
#[doc(hidden)]
pub struct GeneratedSite {
    /// `CARGO_CRATE_NAME` of the expanding crate — the ident form symbols
    /// carry.
    pub krate: &'static str,
    /// `file!()` at the invocation: the compiler-relative path.
    pub file: &'static str,
    /// `line!()` at the invocation.
    pub line: u32,
}

/// Every `#[oopsie]` expansion in the linked program.
#[doc(hidden)]
#[distributed_slice]
pub static GENERATED_SITES: [GeneratedSite];
