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
/// ```
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

/// Attribute macro that injects diagnostic fields (backtrace, spantrace) into
/// every variant/struct and then delegates to `#[derive(Oopsie)]`.
///
/// **Must be placed above `#[derive(Debug, Oopsie)]`.**
///
/// # Usage
///
/// ```
/// use oopsie::ResultExt as _;
///
/// #[oopsie::traced]
/// #[derive(Debug, oopsie::Oopsie)]
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
