#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    doc(test(attr(feature(error_generic_member_access))))
)]

pub(crate) mod derive;
mod oopsie_attr;
pub(crate) mod traced;
pub(crate) mod utils;

/// Derive macro that generates context selectors, `Display`, and `Error` impls
/// for a struct or enum.
///
/// This is the low-level building block. For the batteries-included experience
/// (automatic `Debug` generation, optional tracing/diagnostics), prefer
/// [`#[oopsie]`](macro@oopsie) instead.
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
        Ok(tokens) => tokens.into(),
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
/// Pass `traced` to automatically inject backtrace and spantrace fields:
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
/// | Parameter | Effect |
/// |-----------|--------|
/// | *(bare)* | No diagnostics; equivalent to `#[derive(Debug, Oopsie)]` |
/// | `traced` | Inject backtrace + spantrace, plus an auto error code (`module_path::Type::Variant`) |
/// | `traced(timestamp)` | …plus an auto-captured timestamp |
/// | `traced(backtrace(false))` | Disable one part (any of `backtrace`/`spantrace`/`timestamp`) |
/// | `traced(timestamp(chrono = true))` | `chrono::DateTime<Local>` timestamps (needs oopsie's `chrono` feature) |
/// | `traced(packed = false, boxed = false)` | Trace field layout tuning |
/// | `code = false` | Disable the auto error code (only meaningful with `traced`) |
/// | `debug = false` | Skip the automatic `Debug` derive (for hand-written `impl Debug`) |
/// | `path = "my_crate::oopsie"` | Custom path to the `oopsie` crate (all generated impls) |
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
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
