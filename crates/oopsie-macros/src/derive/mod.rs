//! `#[derive(Oopsie)]` implementation.

mod gen_selectors;
pub(crate) mod parse;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, parse_quote};

use self::gen_selectors::{gen_enum_selectors, gen_struct_selector};
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

fn oopsie_path(container: &ContainerAttrs) -> syn::Path {
    container
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie })
}

fn expand_enum(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selectors = gen_enum_selectors(input, container_attrs, &path)?;

    Ok(quote! {
        #(#selectors)*
    })
}

fn expand_struct(
    input: &DeriveInput,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream2> {
    let path = oopsie_path(container_attrs);
    let selector = gen_struct_selector(input, container_attrs, &path)?;

    Ok(quote! {
        #selector
    })
}
