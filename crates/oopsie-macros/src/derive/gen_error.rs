//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, Type};

use super::parse::{CategorizedFields, DisplayAttr, ProvideAttr, StructAttrs, VariantAttrs};

/// Build the body of a `oopsie_backtrace`/`oopsie_spantrace` accessor that
/// surfaces the deepest *captured* trace: prefer the source's (origin-most)
/// trace, falling back to this layer's own field. This mirrors the source-first
/// ordering of the generated `provide()` (plus std's first-wins `Request`) so
/// the stable accessor and the provider path return the same trace. Without the
/// `unstable-error-generic-member-access` feature the source lookup yields
/// `None`, so the body degrades to the own field. Returns `None` when the layer
/// has neither a source nor an own trace.
///
/// `source_fn` names the skip-empty `__private` lookup (`source_backtrace` /
/// `source_spantrace`), so an empty source trace never shadows a captured one.
///
/// `probe` is the stable `DiagProbe` forwarding expression, supplied only for
/// `transparent` layers. It sits between the (nightly-only) provider path and
/// the own field, so a transparent wrapper forwards its source's trace on
/// stable too — degrading to the own field when the source carries none.
fn trace_accessor_body(
    own: Option<TokenStream2>,
    source_access: Option<TokenStream2>,
    probe: Option<TokenStream2>,
    source_fn: &syn::Ident,
    oopsie_path: &syn::Path,
) -> Option<TokenStream2> {
    match (own, source_access) {
        (Some(own), Some(src)) => {
            let head = provider_then_probe(&src, probe, source_fn, oopsie_path);
            Some(quote! { #head.or(::core::option::Option::Some(#own)) })
        }
        (Some(own), None) => Some(quote! { ::core::option::Option::Some(#own) }),
        (None, Some(src)) => Some(provider_then_probe(&src, probe, source_fn, oopsie_path)),
        (None, None) => None,
    }
}

/// The provider-API trace lookup, with the (transparent-only) stable `DiagProbe`
/// forwarder OR-ed in after it when present.
fn provider_then_probe(
    src: &TokenStream2,
    probe: Option<TokenStream2>,
    source_fn: &syn::Ident,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let base = quote! { #oopsie_path::__private::#source_fn(#src) };
    match probe {
        Some(probe) => quote! { #base.or(#probe) },
        None => base,
    }
}

/// Idents an accessor match arm must bind: the own-trace field and/or the
/// source field, in that order.
fn accessor_pattern_binds(
    own: Option<&syn::Ident>,
    source: Option<&syn::Ident>,
) -> Vec<syn::Ident> {
    let mut binds = Vec::new();
    if let Some(own) = own {
        binds.push(own.clone());
    }
    if let Some(source) = source {
        binds.push(source.clone());
    }
    binds
}

/// Build a `DiagProbe` forwarding call for a transparent layer: forward one
/// `Diagnostic` accessor (`method`) to the source if it implements `Diagnostic`,
/// else `None`. `target` is the `&Source` reference to probe (a by-ref binding
/// in enum arms, `&self.field` in structs). Mirrors the autoref dispatch used by
/// `CaptureProbe` in `gen_selectors`.
fn gen_diag_forward(
    target: impl quote::ToTokens,
    method: &str,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let method = format_ident!("{method}");
    quote! {
        {
            use #oopsie_path::__private::{DiagForwardExt as _, DiagForwardFallback as _};
            (&#oopsie_path::__private::DiagProbe(#target)).#method()
        }
    }
}

/// Generate `std::error::Error` impl for an enum.
pub fn gen_enum_error(input: &DeriveInput, oopsie_path: &syn::Path) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    // Mangled `provide` parameter: the arm destructures every field name (so
    // provide exprs can reference fields), which would shadow a parameter named
    // `request` if a user field is also named `request`.
    let req = format_ident!("__request");

    let mut source_arms = Vec::new();
    let mut provide_arms = Vec::new();

    // Diagnostic arms
    let mut bt_arms = Vec::new();
    let mut st_arms = Vec::new();
    let mut code_arms = Vec::new();
    let mut help_arms = Vec::new();
    let mut accessor_uses_source = false;

    for variant in &data.variants {
        let variant_ident = &variant.ident;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        if let (Some(help_field), Some(_)) = (&categorized.help_field, &variant_attrs.help) {
            return Err(syn::Error::new_spanned(
                help_field,
                "ambiguous help: this `#[oopsie(help)]` field conflicts with the \
                 `help = ...` attribute; remove one",
            ));
        }
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
                ::core::error::Error::provide(#source_ident.as_error_source(), #req);
            });
        }

        // Provide from field-level provide attrs
        for (_field_ident, provide_attr) in &categorized.provides {
            provide_stmts.push(gen_provide_call(provide_attr, &req));
        }

        // Provide backtrace/spantrace refs from detected fields. An empty
        // trace is never provided, so it cannot shadow a captured one further
        // out (`Request` is first-wins).
        if let Some(tf) = &categorized.traces_field {
            provide_stmts.push(quote! {
                if #tf.0.is_captured() {
                    #req.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
                }
            });
            if cfg!(feature = "tracing") {
                provide_stmts.push(quote! {
                    if #tf.1.is_captured() {
                        #req.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
                    }
                });
            }
        } else {
            if let Some(bt_field) = &categorized.backtrace_field {
                provide_stmts.push(quote! {
                    {
                        let __bt = ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field);
                        if __bt.is_captured() {
                            #req.provide_ref::<#oopsie_path::Backtrace>(__bt);
                        }
                    }
                });
            }
            // Guard against naming `SpanTrace` when the feature (and thus the type) is absent.
            if cfg!(feature = "tracing")
                && let Some(st_field) = &categorized.spantrace_field
            {
                provide_stmts.push(quote! {
                    {
                        let __st = ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field);
                        if __st.is_captured() {
                            #req.provide_ref::<#oopsie_path::SpanTrace>(__st);
                        }
                    }
                });
            }
        }

        // Provide from variant-level provide attrs (including auto error code from trace injection)
        for provide_attr in &variant_attrs.provides {
            provide_stmts.push(gen_provide_call(provide_attr, &req));
        }

        // Dynamic help field takes precedence; the provide path and the stable accessor must agree.
        if let Some(help_field) = &categorized.help_field {
            provide_stmts.push(quote! {
                #req.provide_value_with::<#oopsie_path::HelpText>(
                    || #oopsie_path::HelpText::from(#help_field.to_string())
                );
            });
        } else if let Some(help) = &variant_attrs.help {
            provide_stmts.push(gen_help_provide(help, oopsie_path, &req)?);
        }
        if let Some(code) = &variant_attrs.code {
            provide_stmts.push(gen_code_provide(code, oopsie_path, &req)?);
        }

        let field_names = collect_provide_field_names(&categorized);
        provide_arms.push(quote! {
            #(#cfg_attrs)*
            Self::#variant_ident { #(#field_names,)* .. } => {
                #(#provide_stmts)*
            }
        });

        // ── Diagnostic arms ──

        let source_ident = categorized.source.as_ref().map(|s| &s.ident);
        let src_access = source_ident.map(|s| quote! { #s.as_error_source() });
        // Transparent layers forward the source's trace on stable via `DiagProbe`
        // (the provider path is nightly-only). `#s` is bound by ref in the arm.
        let bt_probe = source_ident
            .filter(|_| variant_attrs.transparent)
            .map(|s| gen_diag_forward(s, "fwd_backtrace", oopsie_path));
        let st_probe = source_ident
            .filter(|_| cfg!(feature = "tracing") && variant_attrs.transparent)
            .map(|s| gen_diag_forward(s, "fwd_spantrace", oopsie_path));
        let bt_fn = format_ident!("source_backtrace");
        let st_fn = format_ident!("source_spantrace");

        // Backtrace
        let (bt_own, bt_bind) = if let Some(tf) = &categorized.traces_field {
            (Some(quote! { &#tf.0 }), Some(tf))
        } else if let Some(bt_field) = &categorized.backtrace_field {
            (
                Some(quote! {
                    ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field)
                }),
                Some(bt_field),
            )
        } else {
            (None, None)
        };
        if let Some(body) =
            trace_accessor_body(bt_own, src_access.clone(), bt_probe, &bt_fn, oopsie_path)
        {
            let binds = accessor_pattern_binds(bt_bind, source_ident);
            bt_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #(#binds,)* .. } => #body,
            });
            accessor_uses_source |= source_ident.is_some();
        }

        // Spantrace
        let (st_own, st_bind) = if let Some(tf) = &categorized.traces_field {
            (Some(quote! { &#tf.1 }), Some(tf))
        } else if let Some(st_field) = &categorized.spantrace_field {
            (
                Some(quote! {
                    ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field)
                }),
                Some(st_field),
            )
        } else {
            (None, None)
        };
        if let Some(body) =
            trace_accessor_body(st_own, src_access.clone(), st_probe, &st_fn, oopsie_path)
        {
            let binds = accessor_pattern_binds(st_bind, source_ident);
            st_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #(#binds,)* .. } => #body,
            });
            accessor_uses_source |= source_ident.is_some();
        }

        // Error code: user-specified, then auto-generated from provide attrs,
        // then (for `transparent`) forwarded from the source.
        if let Some(code) = &variant_attrs.code {
            if code.is_static() {
                let lit = code.static_lit()?;
                code_arms.push(quote! {
                    #(#cfg_attrs)*
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#lit)),
                });
            } else {
                let fmt = &code.format_str;
                let args = code.args.iter();
                let code_field_names: Vec<&syn::Ident> = match &variant.fields {
                    syn::Fields::Named(f) => {
                        f.named.iter().filter_map(|f| f.ident.as_ref()).collect()
                    }
                    syn::Fields::Unnamed(_) | syn::Fields::Unit => Vec::new(),
                };
                code_arms.push(quote! {
                    #(#cfg_attrs)*
                    #[allow(unused_variables)]
                    Self::#variant_ident { #(#code_field_names,)* .. } => ::core::option::Option::Some(#oopsie_path::ErrorCode::from(::std::format!(#fmt #(, #args)*))),
                });
            }
        } else if let Some(provide_attr) = variant_attrs
            .provides
            .iter()
            .find(|p| is_error_code_provide(p))
        {
            let expr = &provide_attr.expr;
            // The provide expr may reference fields (it gets the same bindings
            // inside the generated `provide()`), so bind them here too.
            let binds = collect_provide_field_names(&categorized);
            // A ref-form provide evaluates to `&ErrorCode`; the accessor
            // returns it by value.
            let value = if provide_attr.is_ref() {
                quote! { ::core::option::Option::Some(::core::clone::Clone::clone(#expr)) }
            } else {
                quote! { ::core::option::Option::Some(#expr) }
            };
            code_arms.push(quote! {
                #(#cfg_attrs)*
                #[allow(unused_variables)]
                Self::#variant_ident { #(#binds,)* .. } => #value,
            });
        } else if let (true, Some(source_field)) = (variant_attrs.transparent, &categorized.source)
        {
            let source_ident = &source_field.ident;
            let fwd = gen_diag_forward(source_ident, "fwd_code", oopsie_path);
            code_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #source_ident, .. } => #fwd,
            });
        }

        // Help text: dynamic field, then static attribute, then (for
        // `transparent`) forwarded from the source.
        if let Some(help_field) = &categorized.help_field {
            help_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #help_field, .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from(#help_field.to_string())),
            });
        } else if let Some(help) = &variant_attrs.help {
            if help.is_static() {
                let lit = help.static_lit()?;
                help_arms.push(quote! {
                    #(#cfg_attrs)*
                    Self::#variant_ident { .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from_static(#lit)),
                });
            } else {
                let fmt = &help.format_str;
                let args = help.args.iter();
                // The format string may reference variant fields — positional
                // args or inline `{field}` capture — so bind them in the
                // pattern (mirroring the `display` arm).
                let help_field_names: Vec<&syn::Ident> = match &variant.fields {
                    syn::Fields::Named(f) => {
                        f.named.iter().filter_map(|f| f.ident.as_ref()).collect()
                    }
                    syn::Fields::Unnamed(_) | syn::Fields::Unit => Vec::new(),
                };
                help_arms.push(quote! {
                    #(#cfg_attrs)*
                    #[allow(unused_variables)]
                    Self::#variant_ident { #(#help_field_names,)* .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from(::std::format!(#fmt #(, #args)*))),
                });
            }
        } else if let (true, Some(source_field)) = (variant_attrs.transparent, &categorized.source)
        {
            let source_ident = &source_field.ident;
            let fwd = gen_diag_forward(source_ident, "fwd_help", oopsie_path);
            help_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #source_ident, .. } => #fwd,
            });
        }
    }

    let provide_method =
        if provide_arms.is_empty() || !cfg!(feature = "unstable-error-generic-member-access") {
            quote! {}
        } else {
            quote! {
                #[allow(unused_variables)]
                fn provide<'__a>(&'__a self, #req: &mut ::core::error::Request<'__a>) {
                    use #oopsie_path::AsErrorSource as _;
                    match self {
                        #(#provide_arms)*
                    }
                }
            }
        };

    // Bring `as_error_source` into scope for arms that descend into a source.
    let accessor_use_aes = if accessor_uses_source {
        quote! { use #oopsie_path::AsErrorSource as _; }
    } else {
        quote! {}
    };

    // Generate Diagnostic methods
    let bt_method = if bt_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> {
                #accessor_use_aes
                match self {
                    #(#bt_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let st_method = if !cfg!(feature = "tracing") || st_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> {
                #accessor_use_aes
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

    let source_body = if source_arms.is_empty() {
        quote! { match *self {} }
    } else {
        quote! {
            // Bring `as_error_source` into scope so method-call autoderef
            // can pick the `dyn Error + Send + Sync + 'static` impl for
            // `Box<dyn Error + …>` fields.
            use #oopsie_path::AsErrorSource as _;
            match self {
                #(#source_arms)*
            }
        }
    };

    Ok(quote! {
        impl #impl_generics ::core::error::Error for #enum_ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                #source_body
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
    if let (Some(help_field), Some(_)) = (&categorized.help_field, &variant_attrs.help) {
        return Err(syn::Error::new_spanned(
            help_field,
            "ambiguous help: this `#[oopsie(help)]` field conflicts with the \
             `help = ...` attribute; remove one",
        ));
    }

    // Mangled `provide` parameter (see `gen_enum_error`): the destructure binds
    // every field name, which would shadow a parameter named `request`.
    let req = format_ident!("__request");

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
            ::core::error::Error::provide(#source_ident.as_error_source(), #req);
        });
    }

    // Field-level provides
    for (_field_ident, provide_attr) in &categorized.provides {
        provide_stmts.push(gen_provide_call(provide_attr, &req));
    }

    // Provide backtrace/spantrace refs from detected fields. An empty trace
    // is never provided, so it cannot shadow a captured one further out
    // (`Request` is first-wins).
    if let Some(tf) = &categorized.traces_field {
        provide_stmts.push(quote! {
            if #tf.0.is_captured() {
                #req.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
            }
        });
        if cfg!(feature = "tracing") {
            provide_stmts.push(quote! {
                if #tf.1.is_captured() {
                    #req.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
                }
            });
        }
    } else {
        if let Some(bt_field) = &categorized.backtrace_field {
            provide_stmts.push(quote! {
                {
                    let __bt = ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field);
                    if __bt.is_captured() {
                        #req.provide_ref::<#oopsie_path::Backtrace>(__bt);
                    }
                }
            });
        }
        // Guard against naming `SpanTrace` when the feature (and thus the type) is absent.
        if cfg!(feature = "tracing")
            && let Some(st_field) = &categorized.spantrace_field
        {
            provide_stmts.push(quote! {
                {
                    let __st = ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field);
                    if __st.is_captured() {
                        #req.provide_ref::<#oopsie_path::SpanTrace>(__st);
                    }
                }
            });
        }
    }

    // Struct-level provides (from #[oopsie(provide(...))] on the struct)
    for provide_attr in &attrs.provides {
        provide_stmts.push(gen_provide_call(provide_attr, &req));
    }

    // Dynamic help field takes precedence; the provide path and the stable accessor must agree.
    if let Some(help_field) = &categorized.help_field {
        provide_stmts.push(quote! {
            #req.provide_value_with::<#oopsie_path::HelpText>(
                || #oopsie_path::HelpText::from(#help_field.to_string())
            );
        });
    } else if let Some(help) = &variant_attrs.help {
        provide_stmts.push(gen_help_provide(help, oopsie_path, &req)?);
    }
    if let Some(code) = &variant_attrs.code {
        provide_stmts.push(gen_code_provide(code, oopsie_path, &req)?);
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
                fn provide<'__a>(&'__a self, #req: &mut ::core::error::Request<'__a>) {
                    use #oopsie_path::AsErrorSource as _;
                    #destructure
                    #(#provide_stmts)*
                }
            }
        };

    // ── Diagnostic impl for struct ──

    let struct_source = categorized.source.as_ref().map(|s| &s.ident);
    let struct_src_access = struct_source.map(|s| quote! { self.#s.as_error_source() });
    let struct_use_aes = if struct_source.is_some() {
        quote! { use #oopsie_path::AsErrorSource as _; }
    } else {
        quote! {}
    };
    // Transparent structs forward the source's trace on stable via `DiagProbe`.
    let bt_probe = struct_source
        .filter(|_| variant_attrs.transparent)
        .map(|s| gen_diag_forward(quote! { &self.#s }, "fwd_backtrace", oopsie_path));
    let st_probe = struct_source
        .filter(|_| cfg!(feature = "tracing") && variant_attrs.transparent)
        .map(|s| gen_diag_forward(quote! { &self.#s }, "fwd_spantrace", oopsie_path));
    let bt_fn = format_ident!("source_backtrace");
    let st_fn = format_ident!("source_spantrace");

    let bt_own = if let Some(tf) = &categorized.traces_field {
        Some(quote! { &self.#tf.0 })
    } else {
        categorized.backtrace_field.as_ref().map(|bt_field| {
            quote! {
                ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(&self.#bt_field)
            }
        })
    };
    let bt_method = match trace_accessor_body(
        bt_own,
        struct_src_access.clone(),
        bt_probe,
        &bt_fn,
        oopsie_path,
    ) {
        Some(body) => quote! {
            fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> {
                #struct_use_aes
                #body
            }
        },
        None => quote! {},
    };

    let st_own = if let Some(tf) = &categorized.traces_field {
        Some(quote! { &self.#tf.1 })
    } else {
        categorized.spantrace_field.as_ref().map(|st_field| {
            quote! {
                ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(&self.#st_field)
            }
        })
    };
    let st_method =
        match trace_accessor_body(st_own, struct_src_access, st_probe, &st_fn, oopsie_path) {
            Some(body) if cfg!(feature = "tracing") => quote! {
                fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> {
                    #struct_use_aes
                    #body
                }
            },
            Some(_) | None => quote! {},
        };

    let code_method = if let Some(code) = &variant_attrs.code {
        if code.is_static() {
            let lit = code.static_lit()?;
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#lit))
                }
            }
        } else {
            let fmt = &code.format_str;
            let args = code.args.iter();
            let field_names: Vec<&syn::Ident> = match &data.fields {
                syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
                syn::Fields::Unnamed(_) | syn::Fields::Unit => Vec::new(),
            };
            let destructure = if field_names.is_empty() {
                quote! {}
            } else {
                quote! { #[allow(unused_variables)] let Self { #(#field_names),*, .. } = self; }
            };
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    #destructure
                    ::core::option::Option::Some(#oopsie_path::ErrorCode::from(::std::format!(#fmt #(, #args)*)))
                }
            }
        }
    } else {
        // Check for auto-generated code from trace-injection provide attrs
        let code_provide = attrs.provides.iter().find(|p| is_error_code_provide(p));
        if let Some(provide_attr) = code_provide {
            let expr = &provide_attr.expr;
            // The provide expr may reference fields (it gets the same bindings
            // inside the generated `provide()`), so destructure them here too.
            let field_names = collect_provide_field_names(&categorized);
            let code_destructure = if field_names.is_empty() {
                quote! {}
            } else {
                quote! { #[allow(unused_variables)] let Self { #(#field_names),*, .. } = self; }
            };
            // A ref-form provide evaluates to `&ErrorCode`; the accessor
            // returns it by value.
            let value = if provide_attr.is_ref() {
                quote! { ::core::option::Option::Some(::core::clone::Clone::clone(#expr)) }
            } else {
                quote! { ::core::option::Option::Some(#expr) }
            };
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    #code_destructure
                    #value
                }
            }
        } else if let (true, Some(s)) = (variant_attrs.transparent, struct_source) {
            let fwd = gen_diag_forward(quote! { &self.#s }, "fwd_code", oopsie_path);
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    #fwd
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
        if help.is_static() {
            let lit = help.static_lit()?;
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                    ::core::option::Option::Some(#oopsie_path::HelpText::from_static(#lit))
                }
            }
        } else {
            let fmt = &help.format_str;
            let args = help.args.iter();
            // The format string may reference fields via inline `{field}`
            // capture, so destructure them into locals (mirroring the `Display`
            // impl). `#[allow(unused_variables)]` covers fields no arg uses.
            let field_names: Vec<&syn::Ident> = match &data.fields {
                syn::Fields::Named(f) => f.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
                syn::Fields::Unnamed(_) | syn::Fields::Unit => Vec::new(),
            };
            let destructure = if field_names.is_empty() {
                quote! {}
            } else {
                quote! { #[allow(unused_variables)] let Self { #(#field_names),*, .. } = self; }
            };
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                    #destructure
                    ::core::option::Option::Some(#oopsie_path::HelpText::from(::std::format!(#fmt #(, #args)*)))
                }
            }
        }
    } else if let (true, Some(s)) = (variant_attrs.transparent, struct_source) {
        let fwd = gen_diag_forward(quote! { &self.#s }, "fwd_help", oopsie_path);
        quote! {
            fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                #fwd
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

fn gen_help_provide(
    help: &DisplayAttr,
    oopsie_path: &syn::Path,
    req: &syn::Ident,
) -> syn::Result<TokenStream2> {
    if help.is_static() {
        let lit = help.static_lit()?;
        Ok(quote! {
            #req.provide_value_with::<#oopsie_path::HelpText>(|| #oopsie_path::HelpText::from_static(#lit));
        })
    } else {
        let fmt = &help.format_str;
        let args = help.args.iter();
        // The enclosing `provide` method already destructures every field, so
        // an inline `{field}` capture in the format string resolves here.
        Ok(quote! {
            #req.provide_value_with::<#oopsie_path::HelpText>(|| #oopsie_path::HelpText::from(::std::format!(#fmt #(, #args)*)));
        })
    }
}

fn gen_code_provide(
    code: &DisplayAttr,
    oopsie_path: &syn::Path,
    req: &syn::Ident,
) -> syn::Result<TokenStream2> {
    if code.is_static() {
        let lit = code.static_lit()?;
        Ok(quote! {
            #req.provide_value_with::<#oopsie_path::ErrorCode>(|| #oopsie_path::ErrorCode::from(#lit));
        })
    } else {
        let fmt = &code.format_str;
        let args = code.args.iter();
        // The enclosing `provide` method already destructures every field, so
        // an inline `{field}` capture in the format string resolves here.
        Ok(quote! {
            #req.provide_value_with::<#oopsie_path::ErrorCode>(|| #oopsie_path::ErrorCode::from(::std::format!(#fmt #(, #args)*)));
        })
    }
}

fn gen_provide_call(attr: &ProvideAttr, req: &syn::Ident) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if attr.is_ref() {
        quote! { #req.provide_ref_with::<#ty>(|| #expr); }
    } else {
        quote! { #req.provide_value_with::<#ty>(|| #expr); }
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
