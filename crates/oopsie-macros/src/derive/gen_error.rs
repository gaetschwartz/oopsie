//! Error trait impl generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Type;

use super::model::{ResolvedEnum, ResolvedStruct, field_binding_pats};
use super::parse::{CategorizedFields, DisplayAttr, ExitCodeAttr, ProvideAttr};

/// A `NonZeroU8` expression for a parse-validated exit code, spanned at the
/// user's number so any diagnostic on it points back to the source.
fn exit_code_nonzero(exit: ExitCodeAttr, oopsie_path: &syn::Path) -> TokenStream2 {
    let lit = proc_macro2::Literal::u8_unsuffixed(exit.value);
    let lit = quote::quote_spanned! { exit.span => #lit };
    quote! { #oopsie_path::__private::nonzero_u8_unchecked(#lit) }
}

/// The same exit code wrapped in `Some`, for an accessor body.
fn exit_code_some(exit: ExitCodeAttr, oopsie_path: &syn::Path) -> TokenStream2 {
    let nonzero = exit_code_nonzero(exit, oopsie_path);
    quote! { ::core::option::Option::Some(#nonzero) }
}

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
/// `probe` is the stable `DiagProbe` forwarding expression. It sits between the
/// (nightly-only) provider path and the own field, so a forwarding wrapper
/// surfaces its source's trace on stable too — degrading to the own field when
/// the source carries none.
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

/// Build the body of the `oopsie_location` accessor. Location has no
/// Provider-API path: the origin-most value is captured at construction (the
/// `CaptureProbe` extracts the source's via `capture_or_extract`), so a layer's
/// own field already holds the deepest location. A `transparent` layer keeps no
/// own location and forwards to the source on stable via `DiagProbe` (`probe`),
/// preferring it over any own field so the origin-most still wins. Returns
/// `None` when the layer has neither.
fn location_accessor_body(
    own: Option<TokenStream2>,
    probe: Option<TokenStream2>,
) -> Option<TokenStream2> {
    match (own, probe) {
        (Some(own), Some(probe)) => Some(quote! { #probe.or(::core::option::Option::Some(#own)) }),
        (Some(own), None) => Some(quote! { ::core::option::Option::Some(#own) }),
        (None, Some(probe)) => Some(probe),
        (None, None) => None,
    }
}

/// The provider-API trace lookup, with the stable `DiagProbe` source-forwarder
/// OR-ed in after it when present.
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

/// Build a `DiagProbe` forwarding call: forward one `Diagnostic` accessor
/// (`method`) to the source if it implements `Diagnostic`, else `None`. `target`
/// is the `&Source` reference to probe (a by-ref binding in enum arms,
/// `&self.field` in structs). Mirrors the autoref dispatch used by `CaptureProbe`
/// in `gen_selectors`.
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

/// The `Diagnostic` impl's `where` clause: the type's own predicates plus a
/// `<source_ty>: Diagnostic` bound per forwarded generic source. The stable
/// `DiagProbe` autoref needs that bound to select the forwarding impl over the
/// `None` fallback at the generic level; without it forwarding silently breaks
/// for a bare `S: Error`. (Concrete sources are filtered out upstream.)
fn diagnostic_where_clause(
    generics: &syn::Generics,
    forwarded_generic_sources: &[&Type],
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let mut predicates: Vec<TokenStream2> = generics
        .where_clause
        .iter()
        .flat_map(|wc| wc.predicates.iter())
        .map(|pred| quote! { #pred })
        .collect();
    let mut seen: Vec<String> = Vec::new();
    for ty in forwarded_generic_sources {
        let key = quote! { #ty }.to_string();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        predicates.push(quote! { #ty: #oopsie_path::Diagnostic });
    }
    if predicates.is_empty() {
        quote! {}
    } else {
        quote! { where #(#predicates),* }
    }
}

/// A source field's declared type when it names a generic parameter of `generics`
/// and its diagnostics are forwarded (so the `DiagProbe` bound is needed), else
/// `None`. A `transparent` error forwards diagnostics through the probe just like
/// an explicit `forward(...)`, so it needs the bound too. A concrete type names no
/// parameter and must keep yielding `None`, so it is skipped.
fn forwarded_generic_source_ty<'a>(
    source: Option<&'a super::parse::SourceField>,
    transparent: bool,
    generics: &syn::Generics,
) -> Option<&'a Type> {
    let source = source?;
    if !transparent && !source.forward.any() {
        return None;
    }
    let declared = super::generics::DeclaredParams::from_generics(generics);
    declared
        .type_references_param(&source.ty)
        .then_some(&source.ty)
}

/// The mangled `Error::provide` parameter name, probed against every field
/// name across all variants (mirroring `gen_display::formatter`'s probe for
/// `f`) so a field named `__request` cannot shadow it.
fn provide_param(data: &syn::Data) -> syn::Ident {
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

    let mut candidate = format_ident!("__request");
    let mut n = 0u32;
    while field_names.contains(&&candidate) {
        candidate = format_ident!("__request_{n}");
        n += 1;
    }
    candidate
}

/// The named lifetime tying `&self` to `Request<'…>` in the generated
/// `provide` (elision can't substitute: `Request` is invariant in its
/// lifetime), probed against the type's declared lifetimes — mirroring
/// [`provide_param`]'s probe for `__request` — so a user lifetime literally
/// named `'__a` cannot collide with it (E0403/E0496).
fn provide_lifetime(generics: &syn::Generics) -> syn::Lifetime {
    let mut candidate = format_ident!("__a");
    let mut n = 0u32;
    while generics
        .lifetimes()
        .any(|lt| lt.lifetime.ident == candidate)
    {
        candidate = format_ident!("__a_{n}");
        n += 1;
    }
    syn::Lifetime::new(&format!("'{candidate}"), proc_macro2::Span::call_site())
}

/// Generate `std::error::Error` impl for an enum.
pub fn gen_enum_error(
    resolved: &ResolvedEnum,
    oopsie_path: &syn::Path,
) -> syn::Result<TokenStream2> {
    let input = resolved.input;
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Mangled `provide` parameter: the arm destructures every field name (so
    // provide exprs can reference fields), which would shadow a parameter named
    // `request` if a user field is also named `request`.
    let req = provide_param(&input.data);

    let mut source_arms = Vec::new();
    let mut provide_arms = Vec::new();

    // Diagnostic arms
    let mut bt_arms = Vec::new();
    let mut st_arms = Vec::new();
    let mut loc_arms = Vec::new();
    let mut code_arms = Vec::new();
    let mut help_arms = Vec::new();
    let mut exit_arms = Vec::new();
    let mut accessor_uses_source = false;
    let mut provide_probes = MetaSet::default();
    let mut accessor_probes = MetaSet::default();

    // Container-level `exit_code` is the default for every variant; a variant's
    // own `exit_code` overrides it.
    let container_exit = resolved.container.exit_code;

    for v in &resolved.variants {
        let variant = v.variant;
        let variant_ident = v.ident();
        let categorized = &v.fields;
        let variant_attrs = &v.attrs;
        let cfg_attrs = &v.cfg_attrs;

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

        // provide() arm (nightly only). `Request` is first-wins, so this
        // layer's own values go ahead of the source's and its own traces
        // after them (the deepest captured trace wins).
        let mut provide_stmts = Vec::new();

        // Dynamic help field takes precedence; the provide path and the stable accessor must agree.
        if let Some(help_field) = &categorized.help_field {
            let help_cfg = field_cfg_for(categorized, help_field);
            let help_value = help_text_from_field(help_field, oopsie_path);
            provide_stmts.push(quote! {
                #(#help_cfg)*
                #req.provide_value_with::<#oopsie_path::HelpText>(|| #help_value);
            });
        } else if let Some(help) = &variant_attrs.help {
            provide_stmts.push(gen_help_provide(help, oopsie_path, &req)?);
        }
        if let Some(code) = &variant_attrs.code {
            provide_stmts.push(gen_code_provide(code, oopsie_path, &req)?);
        }
        // Provide the declared exit code (variant override, else container
        // default) so it reaches a wrapper's accessor through the Provider API.
        if let Some(exit) = variant_attrs.exit_code.or(container_exit) {
            provide_stmts.push(gen_exit_code_provide(exit, oopsie_path, &req));
        }

        let own = MetaSet {
            code: variant_attrs.code.is_some(),
            help: categorized.help_field.is_some() || variant_attrs.help.is_some(),
        };
        // Provide from field-level provide attrs. A cfg-stripped field's stmt
        // must drop with it (its destructure binding already does).
        for (field_ident, provide_attr) in &categorized.provides {
            let field_cfg = field_cfg_for(categorized, field_ident);
            let call = gen_provide_call(provide_attr, &req, oopsie_path, own, &mut provide_probes);
            provide_stmts.push(quote! {
                #(#field_cfg)*
                #call
            });
        }

        // Provide from variant-level provide attrs (including auto error code from trace injection)
        for provide_attr in &variant_attrs.provides {
            provide_stmts.push(gen_provide_call(
                provide_attr,
                &req,
                oopsie_path,
                own,
                &mut provide_probes,
            ));
        }

        if let Some(source_field) = &categorized.source {
            provide_stmts.push(gen_source_provide_forward(
                &source_field.ident,
                variant_attrs.transparent,
                &req,
                oopsie_path,
            ));
        }

        // Provide backtrace/spantrace refs from detected fields. An empty
        // trace is never provided, so it cannot shadow a captured one further
        // out (`Request` is first-wins). The statement references the field by
        // name, so a cfg-stripped trace field's stmt must drop with it (the
        // destructure binding already does).
        if let Some(tf) = &categorized.traces_field {
            let field_cfg = trace_field_cfg(categorized, Some(tf));
            provide_stmts.push(quote! {
                #(#field_cfg)*
                if #tf.0.is_captured() {
                    #req.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
                }
            });
            provide_stmts.push(quote! {
                #(#field_cfg)*
                if #tf.1.is_captured() {
                    #req.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
                }
            });
        } else {
            if let Some(bt_field) = &categorized.backtrace_field {
                let field_cfg = trace_field_cfg(categorized, Some(bt_field));
                provide_stmts.push(quote! {
                    #(#field_cfg)*
                    {
                        let __bt = ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field);
                        if __bt.is_captured() {
                            #req.provide_ref::<#oopsie_path::Backtrace>(__bt);
                        }
                    }
                });
            }
            if let Some(st_field) = &categorized.spantrace_field {
                let field_cfg = trace_field_cfg(categorized, Some(st_field));
                provide_stmts.push(quote! {
                    #(#field_cfg)*
                    {
                        let __st = ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field);
                        if __st.is_captured() {
                            #req.provide_ref::<#oopsie_path::SpanTrace>(__st);
                        }
                    }
                });
            }
        }

        let field_binds = collect_provide_field_binds(categorized);
        provide_arms.push(quote! {
            #(#cfg_attrs)*
            Self::#variant_ident { #(#field_binds)* .. } => {
                #(#provide_stmts)*
            }
        });

        // ── Diagnostic arms ──

        let source_ident = categorized.source.as_ref().map(|s| &s.ident);
        let forward = categorized
            .source
            .as_ref()
            .map(|s| s.forward)
            .unwrap_or_default();
        let src_access = source_ident.map(|s| quote! { #s.as_error_source() });
        // `transparent` or `forward(...)` surfaces the source's trace on stable
        // via `DiagProbe` (the provider path is nightly-only). `#s` is bound by
        // ref in the arm.
        let bt_probe = source_ident
            .filter(|_| variant_attrs.transparent || forward.backtrace)
            .map(|s| gen_diag_forward(s, "fwd_backtrace", oopsie_path));
        let st_probe = source_ident
            .filter(|_| variant_attrs.transparent || forward.spantrace)
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
        let bt_stripped = trace_accessor_body(
            None,
            src_access.clone(),
            bt_probe.clone(),
            &bt_fn,
            oopsie_path,
        );
        if let Some(body) =
            trace_accessor_body(bt_own, src_access.clone(), bt_probe, &bt_fn, oopsie_path)
        {
            let binds = accessor_pattern_binds(bt_bind, source_ident);
            // Auto-injected (mangled) trace fields carry no user cfg, so this is
            // empty there.
            let field_cfg = trace_field_cfg(categorized, bt_bind);
            bt_arms.push(enum_accessor_arms(
                cfg_attrs,
                variant_ident,
                field_cfg,
                (&binds, body),
                source_ident.zip(bt_stripped),
            ));
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
        let st_stripped = trace_accessor_body(
            None,
            src_access.clone(),
            st_probe.clone(),
            &st_fn,
            oopsie_path,
        );
        if let Some(body) =
            trace_accessor_body(st_own, src_access.clone(), st_probe, &st_fn, oopsie_path)
        {
            let binds = accessor_pattern_binds(st_bind, source_ident);
            let field_cfg = trace_field_cfg(categorized, st_bind);
            st_arms.push(enum_accessor_arms(
                cfg_attrs,
                variant_ident,
                field_cfg,
                (&binds, body),
                source_ident.zip(st_stripped),
            ));
            accessor_uses_source |= source_ident.is_some();
        }

        // Location: the origin-most location is captured at construction (the
        // `CaptureProbe` extracts the source's via `capture_or_extract`), so the
        // accessor returns this layer's own field. A `transparent` or
        // `forward(location)` layer keeps no own field and forwards to the source
        // on stable via `DiagProbe`.
        let loc_own = categorized
            .location_field
            .as_ref()
            .map(|lf| quote! { *#lf });
        let loc_source = source_ident.filter(|_| variant_attrs.transparent || forward.location);
        let loc_probe = loc_source.map(|s| gen_diag_forward(s, "fwd_location", oopsie_path));
        let loc_stripped = location_accessor_body(None, loc_probe.clone());
        if let Some(body) = location_accessor_body(loc_own, loc_probe) {
            let binds = accessor_pattern_binds(categorized.location_field.as_ref(), loc_source);
            let field_cfg = trace_field_cfg(categorized, categorized.location_field.as_ref());
            loc_arms.push(enum_accessor_arms(
                cfg_attrs,
                variant_ident,
                field_cfg,
                (&binds, body),
                loc_source.zip(loc_stripped),
            ));
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
                let code_field_binds = field_binding_pats(&variant.fields);
                code_arms.push(quote! {
                    #(#cfg_attrs)*
                    #[allow(unused_variables)]
                    Self::#variant_ident { #(#code_field_binds)* .. } => ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*))),
                });
            }
        } else if let Some(arm) = enum_provided_meta_arm(
            variant_ident,
            categorized,
            &variant_attrs.provides,
            variant_attrs.transparent,
            MetaType::ErrorCode,
            &mut accessor_probes,
            oopsie_path,
        ) {
            code_arms.push(quote! {
                #(#cfg_attrs)*
                #arm
            });
        }

        // Help text: dynamic field, then static attribute, then a `HelpText`
        // provide, then (for `transparent`) forwarded from the source.
        if let Some(help_field) = &categorized.help_field {
            // The body interpolates the help field, so a cfg-stripped help field
            // takes the whole arm with it; the trailing `_ => None` arm covers
            // the variant then.
            let field_cfg = field_cfg_for(categorized, help_field);
            let help_value = help_text_from_field(help_field, oopsie_path);
            help_arms.push(quote! {
                #(#cfg_attrs)*
                #(#field_cfg)*
                Self::#variant_ident { #help_field, .. } => ::core::option::Option::Some(#help_value),
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
                let help_field_binds = field_binding_pats(&variant.fields);
                help_arms.push(quote! {
                    #(#cfg_attrs)*
                    #[allow(unused_variables)]
                    Self::#variant_ident { #(#help_field_binds)* .. } => ::core::option::Option::Some(#oopsie_path::HelpText::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*))),
                });
            }
        } else if let Some(arm) = enum_provided_meta_arm(
            variant_ident,
            categorized,
            &variant_attrs.provides,
            variant_attrs.transparent,
            MetaType::HelpText,
            &mut accessor_probes,
            oopsie_path,
        ) {
            help_arms.push(quote! {
                #(#cfg_attrs)*
                #arm
            });
        }

        // Exit code: variant override, then container default, then the
        // nearest one declared down the source chain.
        if let Some(exit) = variant_attrs.exit_code.or(container_exit) {
            let some = exit_code_some(exit, oopsie_path);
            exit_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { .. } => #some,
            });
        } else if let Some(source_field) = &categorized.source {
            let source_ident = &source_field.ident;
            let fwd = gen_source_meta_forward(
                source_ident,
                &quote! { #source_ident.as_error_source() },
                "source_exit_code",
                "fwd_exit_code",
                oopsie_path,
            );
            exit_arms.push(quote! {
                #(#cfg_attrs)*
                Self::#variant_ident { #source_ident, .. } => #fwd,
            });
        }
    }

    // An all-stripped enum reaches the generators with every arm gated out (the
    // attribute-macro path runs before cfg-stripping), so matches that bind one
    // arm per variant need a wildcard fallback to stay exhaustive.
    let cfg_fallback_arm = if resolved.any_variant_cfg {
        quote! { _ => ::core::unreachable!() }
    } else {
        quote! {}
    };

    // `Error::provide` only compiles when the consumer enables the
    // `error_generic_member_access` language feature, so it can't be emitted
    // unconditionally (no stub is possible for a language feature).
    let provide_method =
        if provide_arms.is_empty() || !cfg!(feature = "unstable-error-generic-member-access") {
            quote! {}
        } else {
            let lt = provide_lifetime(&input.generics);
            let probe_items = provide_probes.probe_items(oopsie_path);
            quote! {
                #[allow(unused_variables)]
                fn provide<#lt>(&#lt self, #req: &mut ::core::error::Request<#lt>) {
                    use #oopsie_path::AsErrorSource as _;
                    #probe_items
                    match self {
                        #(#provide_arms)*
                        #cfg_fallback_arm
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

    let st_method = if st_arms.is_empty() {
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

    let loc_method = if loc_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_location(&self) -> ::core::option::Option<&'static ::core::panic::Location<'static>> {
                match self {
                    #(#loc_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let code_probe_items = MetaSet {
        help: false,
        ..accessor_probes
    }
    .probe_items(oopsie_path);
    let help_probe_items = MetaSet {
        code: false,
        ..accessor_probes
    }
    .probe_items(oopsie_path);
    let code_method = if code_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                #code_probe_items
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
                #help_probe_items
                match self {
                    #(#help_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let exit_method = if exit_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            fn oopsie_exit_code(&self) -> ::core::option::Option<::core::num::NonZeroU8> {
                match self {
                    #(#exit_arms)*
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
                #cfg_fallback_arm
            }
        }
    };

    let forwarded_generic_sources: Vec<&Type> = resolved
        .variants
        .iter()
        .filter_map(|v| {
            forwarded_generic_source_ty(
                v.fields.source.as_ref(),
                v.attrs.transparent,
                &input.generics,
            )
        })
        .collect();
    let diag_where =
        diagnostic_where_clause(&input.generics, &forwarded_generic_sources, oopsie_path);

    Ok(quote! {
        impl #impl_generics ::core::error::Error for #enum_ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                #source_body
            }

            #provide_method
        }

        impl #impl_generics #oopsie_path::Diagnostic for #enum_ident #ty_generics #diag_where {
            #bt_method
            #st_method
            #loc_method
            #code_method
            #help_method
            #exit_method
        }
    })
}

/// Generate `std::error::Error` impl for a struct.
pub fn gen_struct_error(
    resolved: &ResolvedStruct,
    oopsie_path: &syn::Path,
) -> syn::Result<TokenStream2> {
    let input = resolved.input;
    let attrs = resolved.attrs;
    let struct_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = &resolved.fields;
    let variant_attrs = attrs;

    // Mangled `provide` parameter (see `gen_enum_error`): the destructure binds
    // every field name, which would shadow a parameter named `request`.
    let req = provide_param(&input.data);

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

    // Own values, then the source's, then own traces (see the enum arm).
    let mut provide_stmts = Vec::new();

    // Dynamic help field takes precedence; the provide path and the stable accessor must agree.
    if let Some(help_field) = &categorized.help_field {
        let help_cfg = field_cfg_for(categorized, help_field);
        let help_value = help_text_from_field(help_field, oopsie_path);
        provide_stmts.push(quote! {
            #(#help_cfg)*
            #req.provide_value_with::<#oopsie_path::HelpText>(|| #help_value);
        });
    } else if let Some(help) = &variant_attrs.help {
        provide_stmts.push(gen_help_provide(help, oopsie_path, &req)?);
    }
    if let Some(code) = &variant_attrs.code {
        provide_stmts.push(gen_code_provide(code, oopsie_path, &req)?);
    }
    if let Some(exit) = attrs.container.exit_code {
        provide_stmts.push(gen_exit_code_provide(exit, oopsie_path, &req));
    }

    let own = MetaSet {
        code: variant_attrs.code.is_some(),
        help: categorized.help_field.is_some() || variant_attrs.help.is_some(),
    };
    let mut provide_probes = MetaSet::default();
    // Field-level provides. A cfg-stripped field's stmt must drop with it (its
    // destructure binding already does).
    for (field_ident, provide_attr) in &categorized.provides {
        let field_cfg = field_cfg_for(categorized, field_ident);
        let call = gen_provide_call(provide_attr, &req, oopsie_path, own, &mut provide_probes);
        provide_stmts.push(quote! {
            #(#field_cfg)*
            #call
        });
    }

    // Struct-level provides (from #[oopsie(provide(...))] on the struct)
    for provide_attr in &attrs.provides {
        provide_stmts.push(gen_provide_call(
            provide_attr,
            &req,
            oopsie_path,
            own,
            &mut provide_probes,
        ));
    }

    if let Some(source_field) = &categorized.source {
        provide_stmts.push(gen_source_provide_forward(
            &source_field.ident,
            variant_attrs.transparent,
            &req,
            oopsie_path,
        ));
    }

    // Provide backtrace/spantrace refs from detected fields. An empty trace
    // is never provided, so it cannot shadow a captured one further out
    // (`Request` is first-wins). The statement references the field by name, so
    // a cfg-stripped trace field's stmt must drop with it (the destructure
    // binding already does).
    if let Some(tf) = &categorized.traces_field {
        let field_cfg = trace_field_cfg(categorized, Some(tf));
        provide_stmts.push(quote! {
            #(#field_cfg)*
            if #tf.0.is_captured() {
                #req.provide_ref::<#oopsie_path::Backtrace>(&#tf.0);
            }
        });
        provide_stmts.push(quote! {
            #(#field_cfg)*
            if #tf.1.is_captured() {
                #req.provide_ref::<#oopsie_path::SpanTrace>(&#tf.1);
            }
        });
    } else {
        if let Some(bt_field) = &categorized.backtrace_field {
            let field_cfg = trace_field_cfg(categorized, Some(bt_field));
            provide_stmts.push(quote! {
                #(#field_cfg)*
                {
                    let __bt = ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(#bt_field);
                    if __bt.is_captured() {
                        #req.provide_ref::<#oopsie_path::Backtrace>(__bt);
                    }
                }
            });
        }
        if let Some(st_field) = &categorized.spantrace_field {
            let field_cfg = trace_field_cfg(categorized, Some(st_field));
            provide_stmts.push(quote! {
                #(#field_cfg)*
                {
                    let __st = ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(#st_field);
                    if __st.is_captured() {
                        #req.provide_ref::<#oopsie_path::SpanTrace>(__st);
                    }
                }
            });
        }
    }

    // Destructure self to bring field names into scope (same pattern as enum match arms)
    let provide_field_binds = collect_provide_field_binds(categorized);
    let destructure = if provide_stmts.is_empty() || provide_field_binds.is_empty() {
        quote! {}
    } else {
        quote! { let Self { #(#provide_field_binds)* .. } = self; }
    };

    // `Error::provide` only compiles when the consumer enables the
    // `error_generic_member_access` language feature, so it can't be emitted
    // unconditionally (no stub is possible for a language feature).
    let provide_method =
        if provide_stmts.is_empty() || !cfg!(feature = "unstable-error-generic-member-access") {
            quote! {}
        } else {
            let lt = provide_lifetime(&input.generics);
            let probe_items = provide_probes.probe_items(oopsie_path);
            quote! {
                #[allow(unused_variables)]
                fn provide<#lt>(&#lt self, #req: &mut ::core::error::Request<#lt>) {
                    use #oopsie_path::AsErrorSource as _;
                    #probe_items
                    #destructure
                    #(#provide_stmts)*
                }
            }
        };

    // ── Diagnostic impl for struct ──

    let struct_source = categorized.source.as_ref().map(|s| &s.ident);
    let forward = categorized
        .source
        .as_ref()
        .map(|s| s.forward)
        .unwrap_or_default();
    let struct_src_access = struct_source.map(|s| quote! { self.#s.as_error_source() });
    let struct_use_aes = if struct_source.is_some() {
        quote! { use #oopsie_path::AsErrorSource as _; }
    } else {
        quote! {}
    };
    // A `transparent` or `forward(...)` struct surfaces the source's trace on
    // stable via `DiagProbe`.
    let bt_probe = struct_source
        .filter(|_| variant_attrs.transparent || forward.backtrace)
        .map(|s| gen_diag_forward(quote! { &self.#s }, "fwd_backtrace", oopsie_path));
    let st_probe = struct_source
        .filter(|_| variant_attrs.transparent || forward.spantrace)
        .map(|s| gen_diag_forward(quote! { &self.#s }, "fwd_spantrace", oopsie_path));
    let bt_fn = format_ident!("source_backtrace");
    let st_fn = format_ident!("source_spantrace");

    // The own-trace field whose cfg gates the accessor: the packed `traces`
    // field if present, else the standalone backtrace/spantrace field.
    let bt_field_ident = categorized
        .traces_field
        .as_ref()
        .or(categorized.backtrace_field.as_ref());
    let st_field_ident = categorized
        .traces_field
        .as_ref()
        .or(categorized.spantrace_field.as_ref());

    let bt_own = if let Some(tf) = &categorized.traces_field {
        Some(quote! { &self.#tf.0 })
    } else {
        categorized.backtrace_field.as_ref().map(|bt_field| {
            quote! {
                ::core::borrow::Borrow::<#oopsie_path::Backtrace>::borrow(&self.#bt_field)
            }
        })
    };
    let bt_sig =
        quote! { fn oopsie_backtrace(&self) -> ::core::option::Option<&#oopsie_path::Backtrace> };
    let bt_method = gen_struct_trace_method(
        &bt_sig,
        &struct_use_aes,
        trace_field_cfg(categorized, bt_field_ident),
        bt_own,
        struct_src_access.clone(),
        bt_probe,
        &bt_fn,
        oopsie_path,
    );

    let st_own = if let Some(tf) = &categorized.traces_field {
        Some(quote! { &self.#tf.1 })
    } else {
        categorized.spantrace_field.as_ref().map(|st_field| {
            quote! {
                ::core::borrow::Borrow::<#oopsie_path::SpanTrace>::borrow(&self.#st_field)
            }
        })
    };
    let st_sig =
        quote! { fn oopsie_spantrace(&self) -> ::core::option::Option<&#oopsie_path::SpanTrace> };
    let st_method = gen_struct_trace_method(
        &st_sig,
        &struct_use_aes,
        trace_field_cfg(categorized, st_field_ident),
        st_own,
        struct_src_access,
        st_probe,
        &st_fn,
        oopsie_path,
    );

    let loc_own = categorized
        .location_field
        .as_ref()
        .map(|lf| quote! { self.#lf });
    let loc_probe = struct_source
        .filter(|_| variant_attrs.transparent || forward.location)
        .map(|s| gen_diag_forward(quote! { &self.#s }, "fwd_location", oopsie_path));
    let loc_sig = quote! {
        fn oopsie_location(&self) -> ::core::option::Option<&'static ::core::panic::Location<'static>>
    };
    let loc_field_cfg = trace_field_cfg(categorized, categorized.location_field.as_ref());
    let loc_method = match location_accessor_body(loc_own, loc_probe.clone()) {
        Some(body) => {
            let full = quote! { #loc_sig { #body } };
            match negated_existence_cfg(loc_field_cfg) {
                Some(not_cfg) => {
                    let stripped = location_accessor_body(None, loc_probe)
                        .map(|b| quote! { #not_cfg #loc_sig { #b } });
                    quote! { #(#loc_field_cfg)* #full #stripped }
                }
                None => full,
            }
        }
        None => quote! {},
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
            let field_binds = field_binding_pats(&data.fields);
            let destructure = if field_binds.is_empty() {
                quote! {}
            } else {
                quote! { #[allow(unused_variables)] let Self { #(#field_binds)* .. } = self; }
            };
            quote! {
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    #destructure
                    ::core::option::Option::Some(#oopsie_path::ErrorCode::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*)))
                }
            }
        }
    } else {
        struct_provided_meta_body(
            categorized,
            &attrs.provides,
            variant_attrs.transparent,
            MetaType::ErrorCode,
            oopsie_path,
        )
        .map(|body| {
            let field_cfg = meta_field_cfg(categorized, MetaType::ErrorCode);
            quote! {
                #(#field_cfg)*
                fn oopsie_error_code(&self) -> ::core::option::Option<#oopsie_path::ErrorCode> {
                    #body
                }
            }
        })
        .unwrap_or_default()
    };

    // Dynamic help field takes precedence over static attribute
    let help_method = if let Some(help_field) = &categorized.help_field {
        // The body names `self.<field>`, so a cfg-stripped help field takes the
        // whole accessor with it, degrading to the trait default (`None`) — the
        // struct analogue of the enum arm dropping to `_ => None`.
        let help_cfg = field_cfg_for(categorized, help_field);
        let help_value = help_text_from_field(quote! { self.#help_field }, oopsie_path);
        quote! {
            #(#help_cfg)*
            fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                ::core::option::Option::Some(#help_value)
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
            let field_binds = field_binding_pats(&data.fields);
            let destructure = if field_binds.is_empty() {
                quote! {}
            } else {
                quote! { #[allow(unused_variables)] let Self { #(#field_binds)* .. } = self; }
            };
            quote! {
                fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                    #destructure
                    ::core::option::Option::Some(#oopsie_path::HelpText::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*)))
                }
            }
        }
    } else if let Some(body) = struct_provided_meta_body(
        categorized,
        &attrs.provides,
        variant_attrs.transparent,
        MetaType::HelpText,
        oopsie_path,
    ) {
        let field_cfg = meta_field_cfg(categorized, MetaType::HelpText);
        quote! {
            #(#field_cfg)*
            fn oopsie_help_text(&self) -> ::core::option::Option<#oopsie_path::HelpText> {
                #body
            }
        }
    } else {
        quote! {}
    };

    // A struct's single `#[oopsie(...)]` list mixes container and variant roles,
    // so `exit_code` is parsed on the flattened container; a struct with no own
    // code forwards the nearest one down the source chain.
    let exit_method = if let Some(exit) = attrs.container.exit_code {
        let some = exit_code_some(exit, oopsie_path);
        quote! {
            fn oopsie_exit_code(&self) -> ::core::option::Option<::core::num::NonZeroU8> {
                #some
            }
        }
    } else if let Some(s) = struct_source {
        let fwd = gen_source_meta_forward(
            quote! { &self.#s },
            &quote! { self.#s.as_error_source() },
            "source_exit_code",
            "fwd_exit_code",
            oopsie_path,
        );
        quote! {
            fn oopsie_exit_code(&self) -> ::core::option::Option<::core::num::NonZeroU8> {
                #fwd
            }
        }
    } else {
        quote! {}
    };

    let forwarded_generic_sources: Vec<&Type> = forwarded_generic_source_ty(
        categorized.source.as_ref(),
        attrs.transparent,
        &input.generics,
    )
    .into_iter()
    .collect();
    let diag_where =
        diagnostic_where_clause(&input.generics, &forwarded_generic_sources, oopsie_path);

    Ok(quote! {
        impl #impl_generics ::core::error::Error for #struct_ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                #source_body
            }

            #provide_method
        }

        impl #impl_generics #oopsie_path::Diagnostic for #struct_ident #ty_generics #diag_where {
            #bt_method
            #st_method
            #loc_method
            #code_method
            #help_method
            #exit_method
        }
    })
}

/// `HelpText::from(<field>.to_string())` with `ToString` named through the
/// `alloc` facade: a method-call `.to_string()` resolves via the std prelude,
/// which a `no_std` consumer doesn't have (E0599).
fn help_text_from_field(field: impl quote::ToTokens, oopsie_path: &syn::Path) -> TokenStream2 {
    quote! {
        #oopsie_path::HelpText::from(
            #oopsie_path::__private::alloc::string::ToString::to_string(&#field)
        )
    }
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
            #req.provide_value_with::<#oopsie_path::HelpText>(|| #oopsie_path::HelpText::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*)));
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
            #req.provide_value_with::<#oopsie_path::ErrorCode>(|| #oopsie_path::ErrorCode::from(#oopsie_path::__private::alloc::format!(#fmt #(, #args)*)));
        })
    }
}

/// Forward a `provide` request to the source (through `as_error_source()`, so
/// a boxed-dyn field compiles as in `source()`). A layer that renders its own
/// message keeps the source's code and help out of the request: they would
/// label a message this layer replaced. Everything else, including traces and
/// user `provide(...)` values, still passes through.
fn gen_source_provide_forward(
    source_ident: &syn::Ident,
    transparent: bool,
    req: &syn::Ident,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let forward = quote! {
        ::core::error::Error::provide(#source_ident.as_error_source(), #req);
    };
    if transparent {
        forward
    } else {
        quote! {
            if !#oopsie_path::__private::requests_code_or_help(#req) {
                #forward
            }
        }
    }
}

/// A code/help/exit accessor body that defers to the source: the Provider-API
/// lookup (`source_fn`), then the stable `DiagProbe` forwarder (`probe_method`)
/// on `target`, so the accessor agrees with the generated `provide`.
fn gen_source_meta_forward(
    target: impl quote::ToTokens,
    src_access: &TokenStream2,
    source_fn: &str,
    probe_method: &str,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let source_fn = format_ident!("{source_fn}");
    let probe = gen_diag_forward(target, probe_method, oopsie_path);
    quote! {
        {
            use #oopsie_path::AsErrorSource as _;
            #oopsie_path::__private::#source_fn(#src_access).or_else(|| #probe)
        }
    }
}

/// Provide the declared exit code as a `NonZeroU8`, so a type-erased wrapper
/// (e.g. `Welp`) can surface the origin-most code through the Provider API.
fn gen_exit_code_provide(
    exit: ExitCodeAttr,
    oopsie_path: &syn::Path,
    req: &syn::Ident,
) -> TokenStream2 {
    let nonzero = exit_code_nonzero(exit, oopsie_path);
    quote! {
        #req.provide_value::<::core::num::NonZeroU8>(#nonzero);
    }
}

/// A `ref` provide of oopsie's `ErrorCode`/`HelpText` is also provided by
/// value, since the stable accessors and oopsie's own lookups ask for these by
/// value. On a layer with its `own` code/help, that value wins, so a `ref`
/// provide of the same type is dropped.
fn gen_provide_call(
    attr: &ProvideAttr,
    req: &syn::Ident,
    oopsie_path: &syn::Path,
    own: MetaSet,
    probes: &mut MetaSet,
) -> TokenStream2 {
    let ty = &attr.provided_type;
    let expr = &attr.expr;
    if !attr.is_ref() {
        return quote! { #req.provide_value_with::<#ty>(|| #expr); };
    }
    let by_ref = quote! { #req.provide_ref_with::<#ty>(|| #expr); };
    let Some(meta) = MetaType::named_by(attr) else {
        return by_ref;
    };
    probes.mark(meta);
    let probe = meta.probe(ty);
    if own.contains(meta) {
        return quote! {
            if #probe.is_none() {
                #by_ref
            }
        };
    }
    let meta_ty = meta.path(oopsie_path);
    quote! {
        #by_ref
        if let ::core::option::Option::Some(__oopsie_meta) = #probe {
            #req.provide_value_with::<#meta_ty>(|| __oopsie_meta(#expr));
        }
    }
}

/// An oopsie diagnostic type a `provide(...)` can supply, which the stable
/// accessors surface.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MetaType {
    ErrorCode,
    HelpText,
}

impl MetaType {
    const ALL: [Self; 2] = [Self::ErrorCode, Self::HelpText];

    const fn ident(self) -> &'static str {
        match self {
            Self::ErrorCode => "ErrorCode",
            Self::HelpText => "HelpText",
        }
    }

    fn path(self, oopsie_path: &syn::Path) -> TokenStream2 {
        let ident = format_ident!("{}", self.ident());
        quote! { #oopsie_path::#ident }
    }

    /// The meta type whose name `attr`'s type is spelled with. A proc macro
    /// can't resolve types, so this only selects candidates: [`Self::probe`]
    /// decides whether the type really is oopsie's.
    fn named_by(attr: &ProvideAttr) -> Option<Self> {
        let Type::Path(type_path) = &attr.provided_type else {
            return None;
        };
        let last = type_path.path.segments.last()?;
        Self::ALL.into_iter().find(|m| last.ident == m.ident())
    }

    /// `Some(clone fn)` when `ty` is oopsie's type, else `None`, resolved at
    /// the type level; needs [`MetaSet::probe_items`] in scope.
    fn probe(self, ty: &Type) -> TokenStream2 {
        let probe = format_ident!("__OopsieProbe{}", self.ident());
        quote! { #probe::<#ty>(::core::marker::PhantomData).get() }
    }

    /// Block-scope items behind [`Self::probe`]: the inherent `get` on the
    /// probe at oopsie's type outranks the blanket trait fallback, so only that
    /// exact type yields `Some`.
    fn probe_items(self, oopsie_path: &syn::Path) -> TokenStream2 {
        let probe = format_ident!("__OopsieProbe{}", self.ident());
        let fallback = format_ident!("__OopsieProbe{}Fallback", self.ident());
        let ty = self.path(oopsie_path);
        quote! {
            #[allow(dead_code)]
            struct #probe<T: ?::core::marker::Sized>(::core::marker::PhantomData<T>);
            #[allow(dead_code)]
            impl #probe<#ty> {
                fn get(&self) -> ::core::option::Option<fn(&#ty) -> #ty> {
                    ::core::option::Option::Some(<#ty as ::core::clone::Clone>::clone)
                }
            }
            #[allow(dead_code)]
            trait #fallback<T: ?::core::marker::Sized> {
                fn get(&self) -> ::core::option::Option<fn(&T) -> #ty> {
                    ::core::option::Option::None
                }
            }
            impl<T: ?::core::marker::Sized> #fallback<T> for #probe<T> {}
        }
    }
}

/// A set of [`MetaType`]s.
#[derive(Default, Clone, Copy)]
struct MetaSet {
    code: bool,
    help: bool,
}

impl MetaSet {
    const fn mark(&mut self, meta: MetaType) {
        match meta {
            MetaType::ErrorCode => self.code = true,
            MetaType::HelpText => self.help = true,
        }
    }

    const fn contains(self, meta: MetaType) -> bool {
        match meta {
            MetaType::ErrorCode => self.code,
            MetaType::HelpText => self.help,
        }
    }

    /// The probe items of each member (see [`MetaType::probe_items`]).
    fn probe_items(self, oopsie_path: &syn::Path) -> TokenStream2 {
        let code = self
            .code
            .then(|| MetaType::ErrorCode.probe_items(oopsie_path));
        let help = self
            .help
            .then(|| MetaType::HelpText.probe_items(oopsie_path));
        quote! { #code #help }
    }
}

/// Every `provide(...)` named like `meta`: field-level ahead of `outer`
/// (variant/container), mirroring `provide()`'s first-wins ordering.
fn meta_candidates<'a>(
    categorized: &'a CategorizedFields,
    outer: &'a [ProvideAttr],
    meta: MetaType,
) -> Vec<&'a ProvideAttr> {
    categorized
        .provides
        .iter()
        .map(|(_, p)| p)
        .chain(outer)
        .filter(|p| MetaType::named_by(p) == Some(meta))
        .collect()
}

/// An accessor value for `meta`: the first candidate whose type is oopsie's,
/// else `fallback`. `None` when there is nothing to try.
fn provided_meta_body(
    candidates: &[&ProvideAttr],
    meta: MetaType,
    fallback: Option<TokenStream2>,
) -> Option<TokenStream2> {
    let mut tries = candidates
        .iter()
        .map(|p| {
            let probe = meta.probe(&p.provided_type);
            let expr = &p.expr;
            let arg = if p.is_ref() {
                quote! { #expr }
            } else {
                quote! { &(#expr) }
            };
            quote! { #probe.map(|__oopsie_meta| __oopsie_meta(#arg)) }
        })
        .chain(fallback);
    let first = tries.next()?;
    Some(tries.fold(first, |acc, next| quote! { #acc.or_else(|| #next) }))
}

/// The `transparent` forward of `meta` to `source` (a `&Source` expression).
fn meta_source_forward(
    meta: MetaType,
    target: impl quote::ToTokens,
    src_access: &TokenStream2,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let (source_fn, probe_method) = match meta {
        MetaType::ErrorCode => ("source_error_code", "fwd_code"),
        MetaType::HelpText => ("source_help_text", "fwd_help"),
    };
    gen_source_meta_forward(target, src_access, source_fn, probe_method, oopsie_path)
}

/// An enum accessor arm for `meta` from its `provide(...)` candidates, then
/// (for `transparent`) the source's value. `None` when there is neither.
fn enum_provided_meta_arm(
    variant_ident: &syn::Ident,
    categorized: &CategorizedFields,
    outer: &[ProvideAttr],
    transparent: bool,
    meta: MetaType,
    probes: &mut MetaSet,
    oopsie_path: &syn::Path,
) -> Option<TokenStream2> {
    let candidates = meta_candidates(categorized, outer, meta);
    let source_ident = categorized
        .source
        .as_ref()
        .filter(|_| transparent)
        .map(|s| &s.ident);
    let fwd = source_ident
        .map(|s| meta_source_forward(meta, s, &quote! { #s.as_error_source() }, oopsie_path));
    if candidates.is_empty() {
        let (s, fwd) = source_ident.zip(fwd)?;
        return Some(quote! {
            Self::#variant_ident { #s, .. } => #fwd,
        });
    }
    probes.mark(meta);
    // The provide exprs may reference fields, bound as in `provide()`.
    let binds = collect_provide_field_binds(categorized);
    let value = provided_meta_body(&candidates, meta, fwd)?;
    let field_cfg = meta_field_cfg(categorized, meta);
    Some(quote! {
        #(#field_cfg)*
        #[allow(unused_variables)]
        Self::#variant_ident { #(#binds)* .. } => #value,
    })
}

/// The cfg attrs of the first field-level `provide(...)` named like `meta`: a
/// cfg-stripped field takes the accessor arm or method naming it along.
fn meta_field_cfg(categorized: &CategorizedFields, meta: MetaType) -> &[syn::Attribute] {
    let ident = categorized
        .provides
        .iter()
        .find(|(_, p)| MetaType::named_by(p) == Some(meta))
        .map(|(f, _)| f);
    trace_field_cfg(categorized, ident)
}

/// A struct accessor body for `meta`, like [`enum_provided_meta_arm`].
fn struct_provided_meta_body(
    categorized: &CategorizedFields,
    outer: &[ProvideAttr],
    transparent: bool,
    meta: MetaType,
    oopsie_path: &syn::Path,
) -> Option<TokenStream2> {
    let candidates = meta_candidates(categorized, outer, meta);
    let fwd = categorized
        .source
        .as_ref()
        .filter(|_| transparent)
        .map(|s| {
            let s = &s.ident;
            meta_source_forward(
                meta,
                quote! { &self.#s },
                &quote! { self.#s.as_error_source() },
                oopsie_path,
            )
        });
    if candidates.is_empty() {
        return fwd;
    }
    let items = meta.probe_items(oopsie_path);
    let field_binds = collect_provide_field_binds(categorized);
    let destructure = if field_binds.is_empty() {
        quote! {}
    } else {
        quote! { #[allow(unused_variables)] let Self { #(#field_binds)* .. } = self; }
    };
    let value = provided_meta_body(&candidates, meta, fwd)?;
    Some(quote! {
        #items
        #destructure
        #value
    })
}

/// Field-binding patterns (`name,`) for the fields a `provide(...)` expr can
/// reference (source, provide-attr fields, auto, then user fields). Each binding
/// carries its `#[cfg(...)]` attrs, so a stripped field's binding is stripped
/// with it (the destructure's trailing `..` absorbs the gap). The generated
/// `provide` method carries `#[allow(unused_variables)]` for fields no expr uses.
fn collect_provide_field_binds(categorized: &CategorizedFields) -> Vec<TokenStream2> {
    let mut seen: Vec<&syn::Ident> = Vec::new();
    let mut binds = Vec::new();
    if let Some(source) = &categorized.source {
        seen.push(&source.ident);
        let cfg = &source.cfg_attrs;
        let ident = &source.ident;
        binds.push(quote! { #(#cfg)* #ident, });
    }
    for (ident, _) in &categorized.provides {
        if seen.contains(&ident) {
            continue;
        }
        seen.push(ident);
        let cfg = field_cfg_for(categorized, ident);
        binds.push(quote! { #(#cfg)* #ident, });
    }
    for af in &categorized.auto_fields {
        if seen.contains(&&af.ident) {
            continue;
        }
        seen.push(&af.ident);
        let cfg = &af.cfg_attrs;
        let ident = &af.ident;
        binds.push(quote! { #(#cfg)* #ident, });
    }
    for uf in &categorized.user_fields {
        if seen.contains(&&uf.ident) {
            continue;
        }
        seen.push(&uf.ident);
        let cfg = &uf.cfg_attrs;
        let ident = &uf.ident;
        binds.push(quote! { #(#cfg)* #ident, });
    }
    binds
}

/// The cfg attrs of an optional trace-field ident, or `&[]` when absent. A
/// stripped trace field's accessor arm / provide stmt must drop with it, so its
/// cfg rides onto every generated mention exactly as the help field's does.
fn trace_field_cfg<'a>(
    categorized: &'a CategorizedFields,
    ident: Option<&syn::Ident>,
) -> &'a [syn::Attribute] {
    ident.map_or(&[], |ident| field_cfg_for(categorized, ident))
}

/// The `not(...)` complement of a field's existence predicate, for emitting a
/// struct accessor that runs only when the own trace field is stripped. `None`
/// when nothing gates the field's existence, so the accessor is unconditional.
fn negated_existence_cfg(field_cfg: &[syn::Attribute]) -> Option<TokenStream2> {
    let pred = super::parse::existence_pred(field_cfg)?;
    Some(quote! { #[cfg(not(#pred))] })
}

/// An enum accessor arm gated on its own field's cfg, plus the complementary
/// source-only arm (`stripped`: the source binding and its body) when that cfg
/// can strip the field, so the variant keeps forwarding its source's value
/// instead of falling through to `_ => None`. The enum twin of
/// [`gen_struct_trace_method`].
fn enum_accessor_arms(
    cfg_attrs: &[syn::Attribute],
    variant_ident: &syn::Ident,
    field_cfg: &[syn::Attribute],
    (binds, body): (&[syn::Ident], TokenStream2),
    stripped: Option<(&syn::Ident, TokenStream2)>,
) -> TokenStream2 {
    let full = quote! {
        #(#cfg_attrs)*
        #(#field_cfg)*
        Self::#variant_ident { #(#binds,)* .. } => #body,
    };
    let complement =
        negated_existence_cfg(field_cfg)
            .zip(stripped)
            .map(|(not_cfg, (source, stripped_body))| {
                quote! {
                    #(#cfg_attrs)*
                    #not_cfg
                    Self::#variant_ident { #source, .. } => #stripped_body,
                }
            });
    quote! { #full #complement }
}

/// A struct trace accessor that forwards a cfg-stripped own field's arm to a
/// source-only body, mirroring the enum's drop-the-arm-fall-through behavior.
/// With no existence cfg (the common/auto case) it emits exactly the full body,
/// byte-identical to the pre-cfg path. With one, it gates the full body on the
/// field's cfg and, when a source/probe path exists, emits the complementary
/// source-only body so source forwarding survives the field being stripped.
#[expect(
    clippy::too_many_arguments,
    reason = "wraps one accessor's full inputs"
)]
fn gen_struct_trace_method(
    sig: &TokenStream2,
    use_aes: &TokenStream2,
    field_cfg: &[syn::Attribute],
    own: Option<TokenStream2>,
    source_access: Option<TokenStream2>,
    probe: Option<TokenStream2>,
    source_fn: &syn::Ident,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let with_body = |body: TokenStream2| quote! { #sig { #use_aes #body } };
    let full = trace_accessor_body(
        own,
        source_access.clone(),
        probe.clone(),
        source_fn,
        oopsie_path,
    )
    .map(with_body);
    let Some(not_cfg) = negated_existence_cfg(field_cfg) else {
        return full.unwrap_or_default();
    };
    let gated = full.map(|m| quote! { #(#field_cfg)* #m });
    let stripped =
        trace_accessor_body(None, source_access, probe, source_fn, oopsie_path).map(with_body);
    let stripped = stripped.map(|m| quote! { #not_cfg #m });
    quote! { #gated #stripped }
}

/// The cfg attrs of the categorized field named `ident`, or `&[]` if it is not a
/// user/auto/source field (e.g. a provide attr referencing a synthetic name).
fn field_cfg_for<'a>(
    categorized: &'a CategorizedFields,
    ident: &syn::Ident,
) -> &'a [syn::Attribute] {
    if let Some(source) = &categorized.source
        && &source.ident == ident
    {
        return &source.cfg_attrs;
    }
    if let Some(af) = categorized.auto_fields.iter().find(|f| &f.ident == ident) {
        return &af.cfg_attrs;
    }
    if let Some(uf) = categorized.user_fields.iter().find(|f| &f.ident == ident) {
        return &uf.cfg_attrs;
    }
    &[]
}
