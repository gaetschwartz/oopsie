//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, Type};

use super::parse::{CategorizedFields, DisplayAttr, ProvideAttr, StructAttrs, VariantAttrs};

/// Generate `std::error::Error` impl for an enum.
pub fn gen_enum_error(input: &DeriveInput, oopsie_path: &syn::Path) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut source_arms = Vec::new();
    let mut provide_arms = Vec::new();

    // Diagnostic arms
    let mut bt_arms = Vec::new();
    let mut st_arms = Vec::new();
    let mut code_arms = Vec::new();
    let mut help_arms = Vec::new();

    for variant in &data.variants {
        let variant_ident = &variant.ident;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let cfg_attrs: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();

        // source() arm
        if let Some(source_field) = &categorized.source {
            let source_ident = &source_field.ident;
            source_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #source_ident, .. } => ::core::option::Option::Some(#source_ident.as_error_source()),
            });
        } else {
            source_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { .. } => ::core::option::Option::None,
            });
        }

        // provide() arm (nightly only)
        let mut provide_stmts = Vec::new();

        // Forward source's provide. Use `as_error_source()` to obtain a
        // `&dyn Error` so the call doesn't need `Box<dyn Error + …>: Error`
        // (which fails for unsized-content boxes — same reason `source()`
        // needs the same trick).
        if let Some(source_field) = &categorized.source {
            let source_ident = &source_field.ident;
            provide_stmts.push(quote! {
                ::core::error::Error::provide(#source_ident.as_error_source(), request);
            });
        }

        // Provide from field-level provide attrs
        for (_field_ident, provide_attr) in &categorized.provides {
            provide_stmts.push(gen_provide_call(provide_attr));
        }

        // Provide backtrace/spantrace refs from detected fields
        if let Some(tf) = &categorized.traces_field {
            provide_stmts.push(quote! {
                request.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
            });
            provide_stmts.push(quote! {
                request.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
            });
        } else {
            if let Some(bt_field) = &categorized.backtrace_field {
                provide_stmts.push(quote! {
                    request.provide_ref::<#oopsie_path::Backtrace>(
                        ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field)
                    );
                });
            }
            if let Some(st_field) = &categorized.spantrace_field {
                provide_stmts.push(quote! {
                    request.provide_ref::<#oopsie_path::SpanTrace>(
                        ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field)
                    );
                });
            }
        }

        // Provide from variant-level provide attrs (including auto error code from trace injection)
        for provide_attr in &variant_attrs.provides {
            provide_stmts.push(gen_provide_call(provide_attr));
        }

        // Provide from help/code in VariantAttrs
        if let Some(help) = &variant_attrs.help {
            provide_stmts.push(gen_help_provide(help, oopsie_path));
        }
        if let Some(code) = &variant_attrs.code {
            provide_stmts.push(quote! {
                request.provide_value_with::<#oopsie_path::ErrorCode>(|| #oopsie_path::ErrorCode::from(#code));
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
                #(#cfg_attrs)*
                #pattern => {}
            });
        } else {
            provide_arms.push(quote! {
                #(#cfg_attrs)*
                #pattern => {
                    #(#provide_stmts)*
                }
            });
        }

        // ── Diagnostic arms ──

        // Backtrace
        if let Some(tf) = &categorized.traces_field {
            bt_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #tf, .. } => ::core::option::Option::Some(&#tf.0),
            });
        } else if let Some(bt_field) = &categorized.backtrace_field {
            bt_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #bt_field, .. } => ::core::option::Option::Some(
                    ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field)
                ),
            });
        }

        // Spantrace
        if let Some(tf) = &categorized.traces_field {
            st_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #tf, .. } => ::core::option::Option::Some(&#tf.1),
            });
        } else if let Some(st_field) = &categorized.spantrace_field {
            st_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #st_field, .. } => ::core::option::Option::Some(
                    ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field)
                ),
            });
        }

        // Error code: check for user-specified code first, then auto-generated from provide attrs
        if let Some(code) = &variant_attrs.code {
            code_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { .. } => ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#code)),
            });
        } else {
            // Check for auto-generated code from trace-injection provide attrs
            for provide_attr in &variant_attrs.provides {
                if is_error_code_provide(provide_attr) {
                    let expr = &provide_attr.expr;
                    code_arms.push(quote! {
                        #(#cfg_attrs)*
                        Self::#variant_ident { .. } => ::core::option::Option::Some(#expr),
                    });
                    break;
                }
            }
        }

        // Help text: dynamic field takes precedence over static attribute
        if let Some(help_field) = &categorized.help_field {
            help_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #help_field, .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from(#help_field.to_string())),
            });
        } else if let Some(help) = &variant_attrs.help {
            let fmt = &help.format_str;
            let args = &help.args;
            if args.is_empty() {
                help_arms.push(quote! {
                    #(#cfg_attrs)*
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from_static(#fmt)),
                });
            } else {
                help_arms.push(quote! {
                    #(#cfg_attrs)*
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from(::std::format!(#fmt, #(#args),*))),
                });
            }
        }
    }

    let provide_method =
        if provide_arms.is_empty() || !cfg!(feature = "unstable-error-generic-member-access") {
            quote! {}
        } else {
            quote! {
                #[allow(unused_variables)]
                fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                    use #oopsie_path::AsErrorSource as _;
                    match self {
                        #(#provide_arms)*
                    }
                }
            }
        };

    // Generate Diagnostic methods
    let bt_method = if bt_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> {
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
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> {
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
            fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
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
            fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
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
                // Bring `as_error_source` into scope so method-call autoderef
                // can pick the `dyn Error + Send + Sync + 'static` impl for
                // `Box<dyn Error + …>` fields.
                use #oopsie_path::AsErrorSource as _;
                match self {
                    #(#source_arms)*
                }
            }

            #provide_method
        }

        impl #impl_generics #oopsie_path::Diagnostic for #enum_ident #ty_generics #where_clause {
            #bt_method
            #st_method
            #code_method
            #help_method
        }
    })
}

/// Generate `std::error::Error` impl for a struct.
pub fn gen_struct_error(
    input: &DeriveInput,
    attrs: &StructAttrs,
    oopsie_path: &syn::Path,
) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;
    let variant_attrs = attrs;

    let source_body = if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        quote! {
            {
                use #oopsie_path::AsErrorSource as _;
                ::core::option::Option::Some(self.#source_ident.as_error_source())
            }
        }
    } else {
        quote! { ::core::option::Option::None }
    };

    let mut provide_stmts = Vec::new();

    // Forward source's provide (uses destructured field name). Same
    // `as_error_source()` trick as `source()` so boxed-dyn fields compile.
    if let Some(source_field) = &categorized.source {
        let source_ident = &source_field.ident;
        provide_stmts.push(quote! {
            ::core::error::Error::provide(#source_ident.as_error_source(), request);
        });
    }

    // Field-level provides
    for (_field_ident, provide_attr) in &categorized.provides {
        provide_stmts.push(gen_provide_call(provide_attr));
    }

    // Provide backtrace/spantrace refs from detected fields
    if let Some(tf) = &categorized.traces_field {
        provide_stmts.push(quote! {
            request.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
        });
        provide_stmts.push(quote! {
            request.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
        });
    } else {
        if let Some(bt_field) = &categorized.backtrace_field {
            provide_stmts.push(quote! {
                request.provide_ref::<#oopsie_path::Backtrace>(
                    ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field)
                );
            });
        }
        if let Some(st_field) = &categorized.spantrace_field {
            provide_stmts.push(quote! {
                request.provide_ref::<#oopsie_path::SpanTrace>(
                    ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field)
                );
            });
        }
    }

    // Struct-level provides (from #[oopsie(provide(...))] on the struct)
    for provide_attr in &attrs.provides {
        provide_stmts.push(gen_provide_call(provide_attr));
    }

    // Help/code from VariantAttrs (user-specified via #[oopsie(help = "...", code = "...")])
    if let Some(help) = &variant_attrs.help {
        provide_stmts.push(gen_help_provide(help, oopsie_path));
    }
    if let Some(code) = &variant_attrs.code {
        provide_stmts.push(quote! {
            request.provide_value_with::<#oopsie_path::ErrorCode>(|| #oopsie_path::ErrorCode::from(#code));
        });
    }

    // Destructure self to bring field names into scope (same pattern as enum match arms)
    let provide_field_names = collect_provide_field_names(&categorized);
    let destructure = if provide_stmts.is_empty() || provide_field_names.is_empty() {
        quote! {}
    } else {
        quote! { let Self { #(#provide_field_names),*, .. } = self; }
    };

    let provide_method =
        if provide_stmts.is_empty() || !cfg!(feature = "unstable-error-generic-member-access") {
            quote! {}
        } else {
            quote! {
                #[allow(unused_variables)]
                fn provide<'__a>(&'__a self, request: &mut ::core::error::Request<'__a>) {
                    use #oopsie_path::AsErrorSource as _;
                    #destructure
                    #(#provide_stmts)*
                }
            }
        };

    // ── Diagnostic impl for struct ──

    let bt_method = if let Some(tf) = &categorized.traces_field {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> {
                ::core::option::Option::Some(&self.#tf.0)
            }
        }
    } else if let Some(bt_field) = &categorized.backtrace_field {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> {
                ::core::option::Option::Some(
                    ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(&self.#bt_field)
                )
            }
        }
    } else {
        quote! {}
    };

    let st_method = if let Some(tf) = &categorized.traces_field {
        quote! {
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> {
                ::core::option::Option::Some(&self.#tf.1)
            }
        }
    } else if let Some(st_field) = &categorized.spantrace_field {
        quote! {
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> {
                ::core::option::Option::Some(
                    ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(&self.#st_field)
                )
            }
        }
    } else {
        quote! {}
    };

    let code_method = if let Some(code) = &variant_attrs.code {
        quote! {
            fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#code))
            }
        }
    } else {
        // Check for auto-generated code from trace-injection provide attrs
        let mut code_expr = None;
        for provide_attr in &attrs.provides {
            if is_error_code_provide(provide_attr) {
                code_expr = Some(&provide_attr.expr);
                break;
            }
        }
        if let Some(expr) = code_expr {
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
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
            fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                ::core::option::Option::Some(#oopsie_path::HelpText::from(self.#help_field.to_string()))
            }
        }
    } else if let Some(help) = &variant_attrs.help {
        let fmt = &help.format_str;
        let args = &help.args;
        if args.is_empty() {
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                    ::core::option::Option::Some(#oopsie_path::HelpText::from_static(#fmt))
                }
            }
        } else {
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                    ::core::option::Option::Some(#oopsie_path::HelpText::from(::std::format!(#fmt, #(#args),*)))
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

        impl #impl_generics #oopsie_path::Diagnostic for #struct_ident #ty_generics #where_clause {
            #bt_method
            #st_method
            #code_method
            #help_method
        }
    })
}

fn gen_help_provide(help: &DisplayAttr, oopsie_path: &syn::Path) -> TokenStream2 {
    let fmt = &help.format_str;
    let args = &help.args;
    if args.is_empty() {
        quote! {
            request.provide_value_with::<#oopsie_path::HelpText>(|| #oopsie_path::HelpText::from_static(#fmt));
        }
    } else {
        quote! {
            request.provide_value_with::<#oopsie_path::HelpText>(|| #oopsie_path::HelpText::from(::std::format!(#fmt, #(#args),*)));
        }
    }
}

fn gen_provide_call(attr: &ProvideAttr) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if attr.is_ref() {
        quote! { request.provide_ref_with::<#ty>(|| #expr); }
    } else {
        quote! { request.provide_value_with::<#ty>(|| #expr); }
    }
}

/// Check if a provide attr is for oopsie's `ErrorCode` (used to surface the
/// auto-generated code from trace injection, and a user's own ErrorCode
/// provide, through `oopsie_error_code()`).
///
/// Matches a bare `ErrorCode` (the form trace injection emits) or one
/// qualified by `oopsie`/`oopsie_core`. A foreign `my_crate::ErrorCode` is
/// deliberately not matched, so it is not hijacked into `oopsie_error_code()`.
fn is_error_code_provide(attr: &ProvideAttr) -> bool {
    let Type::Path(type_path) = &attr.provided_type else {
        return false;
    };
    let segments = &type_path.path.segments;
    let Some(last) = segments.last() else {
        return false;
    };
    if last.ident != "ErrorCode" {
        return false;
    }
    match segments.len() {
        1 => true,
        n => {
            let qualifier = &segments[n - 2].ident;
            qualifier == "oopsie" || qualifier == "oopsie_core"
        }
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
    // User fields may be referenced by a variant/struct-level `provide(...)`
    // expr; bind them so those exprs resolve. The generated `provide` method
    // carries `#[allow(unused_variables)]` for fields no expr references.
    for uf in &categorized.user_fields {
        if !names.contains(&&uf.ident) {
            names.push(&uf.ident);
        }
    }
    names
}
