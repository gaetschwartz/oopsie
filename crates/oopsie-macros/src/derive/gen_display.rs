//! Display impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::DeriveInput;

use super::parse::{CategorizedFields, DisplayAttr, StructAttrs, VariantAttrs};

/// Generate a `Display` impl for an enum.
pub fn gen_enum_display(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    // Mangled formatter binding: the destructure binds every field name (so the
    // format string can interpolate `{field}`), which would shadow a `Formatter`
    // parameter named `f` if a user field is also named `f`.
    let fmtr = formatter(&input.data);

    let mut arms = Vec::new();
    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;
        let variant_ident = &variant.ident;
        let cfg_attrs: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();

        // Collect all field names for destructuring
        let field_names: Vec<_> = match &variant.fields {
            syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
            _ => Vec::new(),
        };

        let write_call = if let Some(display) = &variant_attrs.display {
            gen_write_call(display, &fmtr)
        } else if let (true, Some(source)) = (variant_attrs.transparent, &categorized.source) {
            // `transparent` delegates Display to the source (thiserror parity).
            let source_ident = &source.ident;
            quote! { ::core::fmt::Display::fmt(#source_ident, #fmtr) }
        } else {
            // Default: use variant name as display string
            let name = variant_ident.to_string();
            quote! { ::core::write!(#fmtr, #name) }
        };

        arms.push(quote! {
            #(#cfg_attrs)*
            #[allow(unused_variables)]
            Self::#variant_ident { #(#field_names,)* .. } => #write_call,
        });
    }

    let body = if arms.is_empty() {
        quote! { match *self {} }
    } else {
        quote! { match self { #(#arms)* } }
    };

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #enum_ident #ty_generics #where_clause {
            fn fmt(&self, #fmtr: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
    })
}

/// Generate a `Display` impl for a struct.
pub fn gen_struct_display(input: &DeriveInput, attrs: &StructAttrs) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let variant_attrs = attrs;

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;

    // Collect field names for destructuring
    let field_names: Vec<_> = match &data.fields {
        syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
        _ => Vec::new(),
    };

    let destructure = if field_names.is_empty() {
        quote! {}
    } else {
        quote! {
            #[allow(unused_variables)]
            let Self { #(#field_names),*, .. } = self;
        }
    };

    // See `gen_enum_display`: the field destructure would shadow a `Formatter`
    // parameter named `f` when a user field is also named `f`.
    let fmtr = formatter(&input.data);

    let write_call = if let Some(display) = &variant_attrs.display {
        gen_write_call(display, &fmtr)
    } else if let (true, Some(source)) = (variant_attrs.transparent, &categorized.source) {
        // `transparent` delegates Display to the source (thiserror parity).
        let source_ident = &source.ident;
        quote! { ::core::fmt::Display::fmt(#source_ident, #fmtr) }
    } else {
        let name = struct_ident.to_string();
        quote! { ::core::write!(#fmtr, #name) }
    };

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #struct_ident #ty_generics #where_clause {
            fn fmt(&self, #fmtr: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #destructure
                #write_call
            }
        }
    })
}

fn gen_write_call(display: &DisplayAttr, fmtr: &syn::Ident) -> TokenStream2 {
    let fmt = &display.format_str;
    let args = &display.args;
    quote! { ::core::write!(#fmtr, #fmt #(, #args)*) }
}

fn formatter(data: &syn::Data) -> syn::Ident {
    let field_names = match data {
        syn::Data::Struct(ds) => ds
            .fields
            .iter()
            .filter_map(|f| f.ident.as_ref())
            .collect::<Vec<_>>(),
        syn::Data::Enum(de) => de
            .variants
            .iter()
            .flat_map(|v| match &v.fields {
                syn::Fields::Named(f) => Some(f.named.iter().filter_map(|f| f.ident.as_ref())),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => unreachable!(),
    };

    let mut candidate = format_ident!("__oopsie_fmt");
    let mut n = 0u32;
    while field_names.iter().any(|id| *id == &candidate) {
        candidate = format_ident!("__oopsie_fmt_{n}");
        n += 1;
    }
    candidate
}
