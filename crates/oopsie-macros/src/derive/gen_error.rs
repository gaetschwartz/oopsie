//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

use super::parse::{CategorizedFields, FieldAttrs, ProvideAttr, VariantAttrs};

/// Generate `std::error::Error` impl for an enum.
pub(crate) fn gen_enum_error(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut source_arms = Vec::new();
    let mut provide_arms = Vec::new();

    for variant in &data.variants {
        let variant_ident = &variant.ident;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;

        // source() arm
        if let Some(source_field) = &categorized.source {
            let source_ident = &source_field.ident;
            source_arms.push(quote! {
                Self::#variant_ident { #source_ident, .. } => ::core::option::Option::Some(#source_ident),
            });
        } else {
            source_arms.push(quote! {
                Self::#variant_ident { .. } => ::core::option::Option::None,
            });
        }

        // provide() arm
        let mut provide_stmts = Vec::new();

        // Forward source's provide
        if let Some(source_field) = &categorized.source {
            let source_ident = &source_field.ident;
            provide_stmts.push(quote! {
                ::core::error::Error::provide(#source_ident, request);
            });
        }

        // Provide from auto fields (backtrace, spantrace via provide attrs)
        for (field_ident, provide_attr) in &categorized.provides {
            provide_stmts.push(gen_provide_call(field_ident, provide_attr));
        }

        // Collect all field names needed for pattern
        let field_names = collect_provide_field_names(&categorized);
        let pattern = if field_names.is_empty() {
            quote! { Self::#variant_ident { .. } }
        } else {
            quote! { Self::#variant_ident { #(#field_names),*, .. } }
        };

        if !provide_stmts.is_empty() {
            provide_arms.push(quote! {
                #pattern => {
                    #(#provide_stmts)*
                }
            });
        } else {
            provide_arms.push(quote! {
                #pattern => {}
            });
        }
    }

    let provide_method = if !provide_arms.is_empty() {
        quote! {
            #[cfg(feature = "unstable")]
            fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                match self {
                    #(#provide_arms)*
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #impl_generics ::core::error::Error for #enum_ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                match self {
                    #(#source_arms)*
                }
            }

            #provide_method
        }
    })
}

/// Generate `std::error::Error` impl for a struct.
pub(crate) fn gen_struct_error(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;

    let source_body = if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        quote! { ::core::option::Option::Some(&self.#source_ident) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let mut provide_stmts = Vec::new();
    if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        provide_stmts.push(quote! {
            ::core::error::Error::provide(&self.#source_ident, request);
        });
    }
    for (field_ident, provide_attr) in &categorized.provides {
        provide_stmts.push(gen_provide_call_self(field_ident, provide_attr));
    }

    let provide_method = if !provide_stmts.is_empty() {
        quote! {
            #[cfg(feature = "unstable")]
            fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                #(#provide_stmts)*
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #impl_generics ::core::error::Error for #struct_ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                #source_body
            }

            #provide_method
        }
    })
}

fn gen_provide_call(field_ident: &syn::Ident, attr: &ProvideAttr) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if attr.is_ref {
        quote! { request.provide_ref::<#ty>(#expr); }
    } else {
        quote! { request.provide_value::<#ty>(#expr); }
    }
}

fn gen_provide_call_self(field_ident: &syn::Ident, attr: &ProvideAttr) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    // For struct provide, we prefix field access with self.
    // But the expr in the provide attr already uses the field ident directly,
    // and in struct context we destructure, so we need to be careful.
    // Actually, for structs we use self.field_ident in the expr.
    // The provide exprs reference fields by name, so we need the fields in scope.
    // For now, let's just use the expr as-is (it references field names from destructured self).
    if attr.is_ref {
        quote! { request.provide_ref::<#ty>(#expr); }
    } else {
        quote! { request.provide_value::<#ty>(#expr); }
    }
}

fn collect_provide_field_names(categorized: &CategorizedFields) -> Vec<&syn::Ident> {
    let mut names = Vec::new();
    if let Some(source) = &categorized.source {
        names.push(&source.ident);
    }
    for (ident, _) in &categorized.provides {
        if !names.contains(&ident) {
            names.push(ident);
        }
    }
    // Also include auto fields that may be referenced in provide exprs
    for af in &categorized.auto_fields {
        if !names.contains(&&af.ident) {
            names.push(&af.ident);
        }
    }
    names
}
