//! Display impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt as _;

use super::model::{ResolvedEnum, ResolvedStruct, field_binding_pats};
use super::parse::DisplayAttr;

/// Generate a `Display` impl for an enum.
pub fn gen_enum_display(resolved: &ResolvedEnum) -> TokenStream2 {
    let input = resolved.input;
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Mangled formatter binding: the destructure binds every field name (so the
    // format string can interpolate `{field}`), which would shadow a `Formatter`
    // parameter named `f` if a user field is also named `f`.
    let fmtr = formatter(&input.data);

    let mut arms = Vec::new();
    for v in &resolved.variants {
        let variant_ident = v.ident();
        let cfg_attrs = &v.cfg_attrs;
        let field_binds = field_binding_pats(&v.variant.fields);

        let write_call = if let Some(display) = &v.attrs.display {
            gen_write_call(display, &fmtr)
        } else if let (true, Some(source)) = (v.attrs.transparent, &v.fields.source) {
            // `transparent` delegates Display to the source (thiserror parity).
            let source_ident = &source.ident;
            quote! { ::core::fmt::Display::fmt(#source_ident, #fmtr) }
        } else {
            // Default: use variant name as display string
            let name = variant_ident.unraw().to_string();
            quote! { ::core::write!(#fmtr, #name) }
        };

        arms.push(quote! {
            #(#cfg_attrs)*
            #[allow(unused_variables)]
            Self::#variant_ident { #(#field_binds)* .. } => #write_call,
        });
    }

    let body = if arms.is_empty() {
        quote! { match *self {} }
    } else if resolved.any_variant_cfg {
        quote! { match self { #(#arms)* _ => ::core::unreachable!() } }
    } else {
        quote! { match self { #(#arms)* } }
    };

    quote! {
        impl #impl_generics ::core::fmt::Display for #enum_ident #ty_generics #where_clause {
            fn fmt(&self, #fmtr: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
    }
}

/// Generate a `Display` impl for a struct.
pub fn gen_struct_display(resolved: &ResolvedStruct) -> TokenStream2 {
    let input = resolved.input;
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let attrs = resolved.attrs;

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let field_binds = field_binding_pats(&data.fields);

    let destructure = if field_binds.is_empty() {
        quote! {}
    } else {
        quote! {
            #[allow(unused_variables)]
            let Self { #(#field_binds)* .. } = self;
        }
    };

    // See `gen_enum_display`: the field destructure would shadow a `Formatter`
    // parameter named `f` when a user field is also named `f`.
    let fmtr = formatter(&input.data);

    let write_call = if let Some(display) = &attrs.display {
        gen_write_call(display, &fmtr)
    } else if let (true, Some(source)) = (attrs.transparent, &resolved.fields.source) {
        // `transparent` delegates Display to the source (thiserror parity).
        let source_ident = &source.ident;
        quote! { ::core::fmt::Display::fmt(#source_ident, #fmtr) }
    } else {
        let name = struct_ident.unraw().to_string();
        quote! { ::core::write!(#fmtr, #name) }
    };

    quote! {
        impl #impl_generics ::core::fmt::Display for #struct_ident #ty_generics #where_clause {
            fn fmt(&self, #fmtr: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #destructure
                #write_call
            }
        }
    }
}

fn gen_write_call(display: &DisplayAttr, fmtr: &syn::Ident) -> TokenStream2 {
    let fmt = &display.format_str;
    let args = display.args.iter();
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
            .filter_map(|v| match &v.fields {
                syn::Fields::Named(f) => Some(f.named.iter().filter_map(|f| f.ident.as_ref())),
                syn::Fields::Unnamed(_) | syn::Fields::Unit => None,
            })
            .flatten()
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => unreachable!(),
    };

    let mut candidate = format_ident!("__oopsie_fmt");
    let mut n = 0u32;
    while field_names.contains(&&candidate) {
        candidate = format_ident!("__oopsie_fmt_{n}");
        n += 1;
    }
    candidate
}
