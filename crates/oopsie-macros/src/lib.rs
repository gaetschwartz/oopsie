#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    doc(test(attr(feature(error_generic_member_access))))
)]
#![warn(missing_docs)]
//! Procedural macros backing the `oopsie` error-handling crate.
//!
//! This crate provides the [`#[oopsie]`](macro@oopsie) attribute — the canonical
//! way to define an error type — and the underlying [`Oopsie`](derive@Oopsie)
//! derive. Both generate context selectors plus `Display` and `Error` impls; the
//! attribute additionally derives `Debug` and can inject diagnostic fields such
//! as backtraces, span traces, timestamps, and caller locations.
//!
//! Depend on the `oopsie` facade rather than this crate directly; the macros are
//! re-exported from there alongside the runtime they expand against.

pub(crate) mod derive;
pub(crate) mod keyword_docs;
mod oopsie_attr;
pub(crate) mod traced;
pub(crate) mod utils;

/// Derive macro that generates context selectors, `Display`, and `Error` impls
/// for a struct or enum.
///
/// This is the engine the [`#[oopsie]`](macro@oopsie) attribute drives, and the
/// attribute is the canonical entry point. On top of this derive the attribute
/// also derives `Debug` for you and, with `traced`, injects the backtrace,
/// span-trace, and caller-location fields. Reach for the derive directly only
/// when you want to supply those pieces yourself; otherwise prefer the attribute.
///
/// # What gets generated
///
/// For each variant or struct, the derive produces a **context selector** — a struct
/// holding all fields except the source error and `#[oopsie(capture)]` fields.
///
/// - **Leaf** selectors (no source field) expose `.build()` and `.fail()`
///   (shorthand for `Err(self.build())`).
/// - **Source** selectors instead expose `.build_error(source)` via the
///   `Contextual` trait — they have no `.build()`.
///
/// All fields accept `Into<T>`, so `"str"` is accepted for `String` fields.
///
/// # Usage
///
/// ```ignore
/// use oopsie::ResultExt as _;
///
/// #[derive(Debug, oopsie::Oopsie)]
/// #[oopsie(module(false))]
/// pub enum MyError {
///     #[oopsie("Connection to {host} failed")]
///     Connect { host: String, source: std::io::Error },
/// }
///
/// # fn main() {
/// let result: Result<(), std::io::Error> = Err(std::io::Error::other("refused"));
/// let err = result.context(Connect { host: "db.example.com" }).unwrap_err();
/// assert_eq!(err.to_string(), "Connection to db.example.com failed");
/// # }
/// ```
///
/// See the [`oopsie`](https://docs.rs/oopsie) crate docs for the full attribute reference.
#[proc_macro_derive(Oopsie, attributes(oopsie))]
pub fn oopsie_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match derive::expand(input.into()) {
        Ok(tokens) => {
            let dep = utils::manifest_dep_token();
            quote::quote! { #tokens #dep }.into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

/// Attribute macro — the primary way to define an `oopsie` error type.
///
/// Generates context selectors, `Display`, `Error`, and `Debug` impls in one
/// attribute. No need for `#[derive(Debug, Oopsie)]`.
///
/// # Usage
///
/// ```ignore
/// use oopsie::ResultExt as _;
///
/// #[oopsie::oopsie]
/// pub enum MyError {
///     #[oopsie("Connection to {host} failed")]
///     Connect { host: String, source: std::io::Error },
/// }
///
/// # fn main() {
/// let result: Result<(), std::io::Error> = Err(std::io::Error::other("refused"));
/// let err = result.context(my_oopsies::Connect { host: "db.example.com" }).unwrap_err();
/// assert_eq!(err.to_string(), "Connection to db.example.com failed");
/// # }
/// ```
///
/// ## Diagnostics
///
/// Pass `traced` to automatically inject a backtrace field (and a span-trace when
/// the `tracing` feature is enabled):
///
/// ```ignore
/// #[oopsie::oopsie(traced)]
/// pub enum MyError {
///     #[oopsie("Connection failed")]
///     Connect,
/// }
/// # fn main() {}
/// ```
///
/// ## Parameters
///
/// The bare `#[oopsie]` adds no diagnostics; it is equivalent to
/// `#[derive(Debug, Oopsie)]`. The arguments below tune what gets generated;
/// each has its own subsection further down.
///
/// | Parameter | Effect |
/// |-----------|--------|
/// | `traced` | Inject a backtrace field (and a span-trace under the `tracing` feature), plus an auto error code |
/// | `path` | Path to the `oopsie` crate in generated impls |
/// | `debug` | Skip the automatic `Debug` derive |
///
#[doc = include_str!("../keyword_docs/attr/traced.md")]
#[doc = include_str!("../keyword_docs/attr/path.md")]
#[doc = include_str!("../keyword_docs/attr/debug.md")]
///
/// ## `traced(...)` options
///
/// These keys are spelled nested inside `traced(...)`. `backtrace`/`spantrace`
/// take an optional settings block (`r#type`, `boxed`, `enabled`); `timestamp`
/// takes `chrono`/`provide`; `packed`/`boxed` are pair-level layout flags.
///
/// | Option | Effect |
/// |--------|--------|
/// | `backtrace` | Enable/tune the captured backtrace (default on) |
/// | `spantrace` | Enable/tune the captured span trace (default on; captures nothing without the `tracing` feature) |
/// | `timestamp` | Inject an auto-captured timestamp field (default off) |
/// | `location` | Inject an auto-captured call-site `Location` field (default on) |
/// | `packed` | Store both traces as one `(Backtrace, SpanTrace)` field |
/// | `boxed` | Box the injected trace field(s) |
/// | `code` | Tune (or disable) the auto error code |
/// | `chrono` | Use `chrono::DateTime<Local>` timestamps |
/// | `provide` | Expose the timestamp via the provider API |
/// | `r#type` | Override the injected type for this part |
/// | `enabled` | Explicit on/off inside a settings block |
///
#[doc = include_str!("../keyword_docs/traced/backtrace.md")]
#[doc = include_str!("../keyword_docs/traced/spantrace.md")]
#[doc = include_str!("../keyword_docs/traced/timestamp.md")]
#[doc = include_str!("../keyword_docs/traced/location.md")]
#[doc = include_str!("../keyword_docs/traced/packed.md")]
#[doc = include_str!("../keyword_docs/traced/boxed.md")]
#[doc = include_str!("../keyword_docs/traced/code.md")]
#[doc = include_str!("../keyword_docs/traced/chrono.md")]
#[doc = include_str!("../keyword_docs/traced/provide.md")]
#[doc = include_str!("../keyword_docs/traced/type.md")]
#[doc = include_str!("../keyword_docs/traced/enabled.md")]
///
/// With `traced`, every variant (or the struct itself) also gets an automatic
/// error code unless it is `transparent` or carries an explicit
/// `#[oopsie(code = "...")]`. The code is `module_path::Type` for structs and
/// `module_path::Type::Variant` for enum variants; `Report` renders it as
/// `Error[...]:` in the report header.
///
/// Container-level `#[oopsie(...)]` attributes (`module`, `vis`, `size`, etc.)
/// are placed on the type itself, not in the attribute macro's argument list.
#[proc_macro_attribute]
pub fn oopsie(
    attrs: proc_macro::TokenStream,
    element: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match oopsie_attr::expand(attrs.into(), element.into()) {
        Ok(tokens) => {
            let dep = utils::manifest_dep_token();
            quote::quote! { #tokens #dep }.into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}
