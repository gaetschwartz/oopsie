//! Registry of `#[oopsie]` macro invocation sites.
//!
//! Macro-generated code carries the span of its invocation, so its frames
//! resolve to the attribute's own file and line. Each expansion registers
//! that location here; the fancy renderer hides matching frames the same
//! way it hides its own capture machinery. Without the `fancy` feature the
//! registration macro expands to nothing and the registry does not exist.

#[cfg(feature = "fancy")]
use linkme::distributed_slice;

/// One `#[oopsie]` / `derive(Oopsie)` expansion site.
#[doc(hidden)]
#[cfg(feature = "fancy")]
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
#[cfg(feature = "fancy")]
#[distributed_slice]
pub static GENERATED_SITES: [GeneratedSite];

/// Registers an `#[oopsie]` expansion site for generated-frame hiding.
///
/// The location builtins resolve to the outermost macro invocation, so when
/// this expands from a proc-macro emission spanned at the user's attribute,
/// `file!()`/`line!()` land on that attribute — the same location the
/// generated frames carry. `env!` reads the expanding crate's environment,
/// yielding the user's crate name.
#[doc(hidden)]
#[cfg(feature = "fancy")]
#[macro_export]
macro_rules! __register_generated_site {
    () => {
        const _: () = {
            #[$crate::__private::linkme::distributed_slice($crate::__private::GENERATED_SITES)]
            #[linkme(crate = $crate::__private::linkme)]
            static SITE: $crate::__private::GeneratedSite = $crate::__private::GeneratedSite {
                krate: ::core::env!("CARGO_CRATE_NAME"),
                file: ::core::file!(),
                line: ::core::line!(),
            };
        };
    };
}

/// Without `fancy` there is no renderer to consume the registry.
#[doc(hidden)]
#[cfg(not(feature = "fancy"))]
#[macro_export]
macro_rules! __register_generated_site {
    () => {};
}
