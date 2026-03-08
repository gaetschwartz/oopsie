//! Display impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

use super::parse::{DisplayAttr, VariantAttrs};

/// Generate a `Display` impl for an enum.
pub(crate) fn gen_enum_display(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut arms = Vec::new();
    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_ident = &variant.ident;

        // Collect all field names for destructuring
        let field_names: Vec<_> = match &variant.fields {
            syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
            _ => Vec::new(),
        };

        let pattern = if field_names.is_empty() {
            quote! { Self::#variant_ident { .. } }
        } else {
            quote! { Self::#variant_ident { #(#field_names),*, .. } }
        };

        let write_call = if let Some(display) = &variant_attrs.display {
            gen_write_call(display)
        } else {
            // Default: use variant name as display string
            let name = variant_ident.to_string();
            quote! { ::core::write!(f, #name) }
        };

        arms.push(quote! {
            #pattern => #write_call,
        });
    }

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #enum_ident #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#arms)*
                }
            }
        }
    })
}

/// Generate a `Display` impl for a struct.
pub(crate) fn gen_struct_display(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let variant_attrs = VariantAttrs::from_attrs(&input.attrs)?;

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    // Collect field names for destructuring
    let field_names: Vec<_> = match &data.fields {
        syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
        _ => Vec::new(),
    };

    let destructure = if field_names.is_empty() {
        quote! {}
    } else {
        quote! { let Self { #(#field_names),*, .. } = self; }
    };

    let write_call = if let Some(display) = &variant_attrs.display {
        gen_write_call(display)
    } else {
        let name = struct_ident.to_string();
        quote! { ::core::write!(f, #name) }
    };

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #struct_ident #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #destructure
                #write_call
            }
        }
    })
}

fn gen_write_call(display: &DisplayAttr) -> TokenStream2 {
    let fmt = &display.format_str;
    let args = &display.args;
    if args.is_empty() {
        quote! { ::core::write!(f, #fmt) }
    } else {
        quote! { ::core::write!(f, #fmt, #(#args),*) }
    }
}
