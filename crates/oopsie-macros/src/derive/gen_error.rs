//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Expr, Token, Type};

use super::parse::{CategorizedFields, ProvideAttr};

/// Generate `std::error::Error` impl for an enum.
pub fn gen_enum_error(input: &DeriveInput) -> syn::Result<TokenStream2> {
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

        // Provide from field-level provide attrs
        for (_field_ident, provide_attr) in &categorized.provides {
            provide_stmts.push(gen_provide_call(provide_attr));
        }

        // Provide from variant-level provide attrs (backtrace, spantrace, error code, help text)
        let variant_provides = parse_item_level_provides(&variant.attrs)?;
        for provide_attr in &variant_provides {
            let ty = &provide_attr.provided_type;
            let expr = &provide_attr.expr;
            if provide_attr.is_ref {
                provide_stmts.push(quote! { request.provide_ref::<#ty>(#expr); });
            } else {
                provide_stmts.push(quote! { request.provide_value::<#ty>(#expr); });
            }
        }

        // Collect all field names needed for pattern
        let field_names = collect_provide_field_names(&categorized);
        let pattern = if field_names.is_empty() {
            quote! { Self::#variant_ident { .. } }
        } else {
            quote! { Self::#variant_ident { #(#field_names),*, .. } }
        };

        if provide_stmts.is_empty() {
            provide_arms.push(quote! {
                #pattern => {}
            });
        } else {
            provide_arms.push(quote! {
                #pattern => {
                    #(#provide_stmts)*
                }
            });
        }
    }

    let provide_method = if provide_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            #[cfg(feature = "unstable")]
            fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                match self {
                    #(#provide_arms)*
                }
            }
        }
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
pub fn gen_struct_error(input: &DeriveInput) -> syn::Result<TokenStream2> {
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
    // Parse provides from both field-level and struct-level attrs
    for (_field_ident, provide_attr) in &categorized.provides {
        provide_stmts.push(gen_provide_call_self(provide_attr));
    }
    // Also parse struct-level provide attrs
    let struct_provides = parse_item_level_provides(&input.attrs)?;
    for provide_attr in &struct_provides {
        let ty = &provide_attr.provided_type;
        let expr = &provide_attr.expr;
        if provide_attr.is_ref {
            provide_stmts.push(quote! { request.provide_ref::<#ty>(&self.#expr); });
        } else {
            provide_stmts.push(quote! { request.provide_value::<#ty>(#expr); });
        }
    }

    let provide_method = if provide_stmts.is_empty() {
        quote! {}
    } else {
        quote! {
            #[cfg(feature = "unstable")]
            fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                #(#provide_stmts)*
            }
        }
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

fn gen_provide_call(attr: &ProvideAttr) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if attr.is_ref {
        quote! { request.provide_ref::<#ty>(#expr); }
    } else {
        quote! { request.provide_value::<#ty>(#expr); }
    }
}

fn gen_provide_call_self(attr: &ProvideAttr) -> TokenStream2 {
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

/// Parse `#[oopsie(provide(...))]` attributes from struct/variant-level attributes.
///
/// These are emitted by the `#[oopsie]` attribute macro for backtrace, spantrace,
/// error code, and help text. The format is:
/// - `#[oopsie(provide(ref, Type => expr))]` for ref provides
/// - `#[oopsie(provide(Type => expr))]` for value provides
fn parse_item_level_provides(attrs: &[syn::Attribute]) -> syn::Result<Vec<ProvideAttr>> {
    let mut provides = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("oopsie") {
            continue;
        }
        // Try to parse the attr content. Skip if it doesn't parse as meta items
        // (e.g., short display form like `#[oopsie("display string")]`).
        let Ok(nested) = attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        else {
            continue;
        };
        for meta in &nested {
            if let syn::Meta::List(list) = meta
                && list.path.is_ident("provide")
            {
                let provide: ProvideContent = syn::parse2(list.tokens.clone())?;
                provides.push(provide.0);
            }
        }
    }
    Ok(provides)
}

/// Helper to parse the content of `provide(...)`.
struct ProvideContent(ProvideAttr);

impl Parse for ProvideContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Check for `ref` keyword. Note: `ref` is a Rust keyword, so we must
        // peek for Token![ref] rather than Ident.
        let is_ref = if input.peek(Token![ref]) {
            let _: Token![ref] = input.parse()?;
            let _: Token![,] = input.parse()?;
            true
        } else {
            false
        };
        let provided_type: Type = input.parse()?;
        let _: Token![=>] = input.parse()?;
        let expr: Expr = input.parse()?;
        Ok(Self(ProvideAttr {
            is_ref,
            provided_type,
            expr,
        }))
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
