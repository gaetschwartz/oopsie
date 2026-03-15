//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Expr, Token, Type};

use super::parse::{CategorizedFields, DisplayAttr, ProvideAttr, VariantAttrs};

/// Generate `std::error::Error` impl for an enum.
pub fn gen_enum_error(input: &DeriveInput, crate_path: &syn::Path) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut source_arms = Vec::new();
    let mut provide_arms = Vec::new();

    // ErrorExt arms
    let mut bt_arms = Vec::new();
    let mut st_arms = Vec::new();
    let mut code_arms = Vec::new();
    let mut help_arms = Vec::new();

    for variant in &data.variants {
        let variant_ident = &variant.ident;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;

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

        // provide() arm (nightly only)
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

        // Provide backtrace/spantrace refs from detected fields
        if let Some(bt_field) = &categorized.backtrace_field {
            provide_stmts.push(quote! {
                request.provide_ref::<#crate_path::BackTrace>(#bt_field.as_ref());
            });
        }
        if let Some(st_field) = &categorized.spantrace_field {
            provide_stmts.push(quote! {
                request.provide_ref::<#crate_path::SpanTrace>(#st_field.as_ref());
            });
        }

        // Provide from variant-level provide attrs (auto error code from #[traced])
        let variant_provides = parse_item_level_provides(&variant.attrs)?;
        for provide_attr in &variant_provides {
            provide_stmts.push(gen_provide_call(provide_attr));
        }

        // Provide from help/code in VariantAttrs
        if let Some(help) = &variant_attrs.help {
            provide_stmts.push(gen_help_provide(help, crate_path));
        }
        if let Some(code) = &variant_attrs.code {
            provide_stmts.push(quote! {
                request.provide_value_with::<#crate_path::ErrorCode>(|| #crate_path::ErrorCode::from(#code));
            });
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

        // ── ErrorExt arms ──

        // Backtrace
        if let Some(bt_field) = &categorized.backtrace_field {
            bt_arms.push(quote! {
                Self::#variant_ident { #bt_field, .. } => ::core::option::Option::Some(#bt_field.as_ref()),
            });
        }

        // Spantrace
        if let Some(st_field) = &categorized.spantrace_field {
            st_arms.push(quote! {
                Self::#variant_ident { #st_field, .. } => ::core::option::Option::Some(#st_field.as_ref()),
            });
        }

        // Error code: check for user-specified code first, then auto-generated from provide attrs
        if let Some(code) = &variant_attrs.code {
            code_arms.push(quote! {
                Self::#variant_ident { .. } => ::core::option::Option::Some(#crate_path::ErrorCode::from(#code)),
            });
        } else {
            // Check for auto-generated code from #[traced] provide attrs
            for provide_attr in &variant_provides {
                if is_error_code_provide(provide_attr, crate_path) {
                    let expr = &provide_attr.expr;
                    code_arms.push(quote! {
                        Self::#variant_ident { .. } => ::core::option::Option::Some(#expr),
                    });
                    break;
                }
            }
        }

        // Help text: dynamic field takes precedence over static attribute
        if let Some(help_field) = &categorized.help_field {
            help_arms.push(quote! {
                Self::#variant_ident { #help_field, .. } => ::core::option::Option::Some(#crate_path::HelpText::from(#help_field.to_string())),
            });
        } else if let Some(help) = &variant_attrs.help {
            let fmt = &help.format_str;
            let args = &help.args;
            if args.is_empty() {
                help_arms.push(quote! {
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#crate_path::HelpText(::std::borrow::Cow::Borrowed(#fmt))),
                });
            } else {
                help_arms.push(quote! {
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#crate_path::HelpText(::std::borrow::Cow::Owned(::std::format!(#fmt, #(#args),*)))),
                });
            }
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

    // Generate ErrorExt methods
    let bt_method = if bt_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#crate_path::BackTrace> {
                match self {
                    #(#bt_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let st_method = if st_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#crate_path::SpanTrace> {
                match self {
                    #(#st_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let code_method = if code_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_error_code(&self) -> ::core::option::Option<#crate_path::ErrorCode> {
                match self {
                    #(#code_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let help_method = if help_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_help_text(&self) -> ::core::option::Option<#crate_path::HelpText> {
                match self {
                    #(#help_arms)*
                    _ => ::core::option::Option::None,
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

        impl #impl_generics #crate_path::ErrorExt for #enum_ident #ty_generics #where_clause {
            #bt_method
            #st_method
            #code_method
            #help_method
        }
    })
}

/// Generate `std::error::Error` impl for a struct.
pub fn gen_struct_error(input: &DeriveInput, crate_path: &syn::Path) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;
    let variant_attrs = VariantAttrs::from_attrs(&input.attrs)?;

    let source_body = if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        quote! { ::core::option::Option::Some(&self.#source_ident) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let mut provide_stmts = Vec::new();

    // Forward source's provide (uses destructured field name)
    if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        provide_stmts.push(quote! {
            ::core::error::Error::provide(#source_ident, request);
        });
    }

    // Field-level provides
    for (_field_ident, provide_attr) in &categorized.provides {
        provide_stmts.push(gen_provide_call(provide_attr));
    }

    // Provide backtrace/spantrace refs from detected fields
    if let Some(bt_field) = &categorized.backtrace_field {
        provide_stmts.push(quote! {
            request.provide_ref::<#crate_path::BackTrace>(#bt_field.as_ref());
        });
    }
    if let Some(st_field) = &categorized.spantrace_field {
        provide_stmts.push(quote! {
            request.provide_ref::<#crate_path::SpanTrace>(#st_field.as_ref());
        });
    }

    // Struct-level provides (from #[oopsie(provide(...))] on the struct)
    let struct_provides = parse_item_level_provides(&input.attrs)?;
    for provide_attr in &struct_provides {
        provide_stmts.push(gen_provide_call(provide_attr));
    }

    // Help/code from VariantAttrs (user-specified via #[oopsie(help = "...", code = "...")])
    if let Some(help) = &variant_attrs.help {
        provide_stmts.push(gen_help_provide(help, crate_path));
    }
    if let Some(code) = &variant_attrs.code {
        provide_stmts.push(quote! {
            request.provide_value_with::<#crate_path::ErrorCode>(|| #crate_path::ErrorCode::from(#code));
        });
    }

    // Destructure self to bring field names into scope (same pattern as enum match arms)
    let provide_field_names = collect_provide_field_names(&categorized);
    let destructure = if provide_stmts.is_empty() || provide_field_names.is_empty() {
        quote! {}
    } else {
        quote! { let Self { #(#provide_field_names),*, .. } = self; }
    };

    let provide_method = if provide_stmts.is_empty() {
        quote! {}
    } else {
        quote! {
            #[cfg(feature = "unstable")]
            fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                #destructure
                #(#provide_stmts)*
            }
        }
    };

    // ── ErrorExt impl for struct ──

    let bt_method = if let Some(bt_field) = &categorized.backtrace_field {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#crate_path::BackTrace> {
                ::core::option::Option::Some(self.#bt_field.as_ref())
            }
        }
    } else {
        quote! {}
    };

    let st_method = if let Some(st_field) = &categorized.spantrace_field {
        quote! {
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#crate_path::SpanTrace> {
                ::core::option::Option::Some(self.#st_field.as_ref())
            }
        }
    } else {
        quote! {}
    };

    let code_method = if let Some(code) = &variant_attrs.code {
        quote! {
            fn oopsie_error_code(&self) -> ::core::option::Option<#crate_path::ErrorCode> {
                ::core::option::Option::Some(#crate_path::ErrorCode::from(#code))
            }
        }
    } else {
        // Check for auto-generated code from #[traced] provide attrs
        let mut code_expr = None;
        for provide_attr in &struct_provides {
            if is_error_code_provide(provide_attr, crate_path) {
                code_expr = Some(&provide_attr.expr);
                break;
            }
        }
        if let Some(expr) = code_expr {
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#crate_path::ErrorCode> {
                    ::core::option::Option::Some(#expr)
                }
            }
        } else {
            quote! {}
        }
    };

    // Dynamic help field takes precedence over static attribute
    let help_method = if let Some(help_field) = &categorized.help_field {
        quote! {
            fn oopsie_help_text(&self) -> ::core::option::Option<#crate_path::HelpText> {
                ::core::option::Option::Some(#crate_path::HelpText::from(self.#help_field.to_string()))
            }
        }
    } else if let Some(help) = &variant_attrs.help {
        let fmt = &help.format_str;
        let args = &help.args;
        if args.is_empty() {
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#crate_path::HelpText> {
                    ::core::option::Option::Some(#crate_path::HelpText(::std::borrow::Cow::Borrowed(#fmt)))
                }
            }
        } else {
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#crate_path::HelpText> {
                    ::core::option::Option::Some(#crate_path::HelpText(::std::borrow::Cow::Owned(::std::format!(#fmt, #(#args),*))))
                }
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

        impl #impl_generics #crate_path::ErrorExt for #struct_ident #ty_generics #where_clause {
            #bt_method
            #st_method
            #code_method
            #help_method
        }
    })
}

fn gen_help_provide(help: &DisplayAttr, crate_path: &syn::Path) -> TokenStream2 {
    let fmt = &help.format_str;
    let args = &help.args;
    if args.is_empty() {
        quote! {
            request.provide_value_with::<#crate_path::HelpText>(|| #crate_path::HelpText(::std::borrow::Cow::Borrowed(#fmt)));
        }
    } else {
        quote! {
            request.provide_value_with::<#crate_path::HelpText>(|| #crate_path::HelpText(::std::borrow::Cow::Owned(::std::format!(#fmt, #(#args),*))));
        }
    }
}

fn gen_provide_call(attr: &ProvideAttr) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if attr.is_ref {
        quote! { request.provide_ref_with::<#ty>(|| #expr); }
    } else {
        quote! { request.provide_value_with::<#ty>(|| #expr); }
    }
}

/// Check if a provide attr is for ErrorCode (used to detect auto-generated code from #[traced]).
fn is_error_code_provide(attr: &ProvideAttr, _crate_path: &syn::Path) -> bool {
    // Check if the provided type ends with "ErrorCode"
    if let Type::Path(type_path) = &attr.provided_type
        && let Some(last_seg) = type_path.path.segments.last()
    {
        return last_seg.ident == "ErrorCode";
    }
    false
}

/// Parse `#[oopsie(provide(...))]` attributes from struct/variant-level attributes.
///
/// These are emitted by the `#[traced]` attribute macro for backtrace, spantrace,
/// and auto-generated error codes. The format is:
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
