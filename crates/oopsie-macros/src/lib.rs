mod derive;
mod traced;
pub(crate) mod utils;

/// Derive macro that generates context selectors, `Display`, and `Error` impls
/// for a struct or enum.
///
/// This is the low-level building block. Use [`#[traced]`](macro@traced) on top of it
/// for the batteries-included experience (automatic backtrace/spantrace injection).
///
/// # Usage
///
/// ```rust,ignore
/// #[derive(Debug, Oopsie)]
/// pub enum MyError {
///     #[oopsie("Connection to {host} failed")]
///     Connect { host: String, source: std::io::Error },
///
///     #[oopsie("Not found")]
///     NotFound,
/// }
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

/// Attribute macro that injects diagnostic fields (backtrace, spantrace) into
/// every variant/struct and then delegates to `#[derive(Oopsie)]`.
///
/// **Must be placed above `#[derive(Debug, Oopsie)]`.**
///
/// # Usage
///
/// ```rust,ignore
/// #[traced]                  // inject backtrace + spantrace (default)
/// #[derive(Debug, Oopsie)]
/// pub enum MyError {
///     #[oopsie("Connection to {host} failed")]
///     Connect { host: String, source: std::io::Error },
/// }
/// ```
///
/// ## Parameters
///
/// | Parameter | Effect |
/// |-----------|--------|
/// | *(bare)* | Inject backtrace + spantrace |
/// | `backtrace` | Inject backtrace only |
/// | `spantrace` | Inject spantrace only |
/// | `timestamp` | Inject timestamp |
/// | `code = false` | Disable error-code injection |
/// | `path = "my_crate::oopsie"` | Custom path to the `oopsie` crate |
///
/// ## Module naming
///
/// For enums, selectors are wrapped in a module named
/// `{snake_case(strip_suffix("Error", TypeName))}_oopsies`.
/// For example, `AppError` → `app_oopsies`, `ConnError` → `conn_oopsies`.
#[proc_macro_attribute]
pub fn traced(
    attrs: proc_macro::TokenStream,
    element: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match traced::expand(attrs.into(), element.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
