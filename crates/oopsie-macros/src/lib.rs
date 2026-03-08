mod error;
pub(crate) mod utils;

#[proc_macro_attribute]
pub fn oopsie(
    attrs: proc_macro::TokenStream,
    element: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match error::expand(attrs.into(), element.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
