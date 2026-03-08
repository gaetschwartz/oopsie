//! `#[derive(Oopsie)]` implementation.

pub(crate) mod parse;

use proc_macro2::TokenStream as TokenStream2;
use syn::DeriveInput;

use self::parse::ContainerAttrs;

pub fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let input: DeriveInput = syn::parse2(input)?;
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;

    match &input.data {
        syn::Data::Enum(_) => expand_enum(&input, &container_attrs),
        syn::Data::Struct(_) => expand_struct(&input, &container_attrs),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "#[derive(Oopsie)] cannot be applied to unions",
        )),
    }
}

fn expand_enum(input: &DeriveInput, _container_attrs: &ContainerAttrs) -> syn::Result<TokenStream2> {
    let _ = input;
    Ok(TokenStream2::new())
}

fn expand_struct(input: &DeriveInput, _container_attrs: &ContainerAttrs) -> syn::Result<TokenStream2> {
    let _ = input;
    Ok(TokenStream2::new())
}
