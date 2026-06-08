//! Context selector generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, Visibility};

use super::parse::{
    CategorizedFields, EnumContainerAttrs, ModuleSetting, SourceKind, StructAttrs, SuffixSetting,
    VariantAttrs,
};

/// Default selector visibility when no explicit `vis(...)` is given.
///
/// A selector's `Contextual::Destination` is the error type, so the selector
/// may never be more visible than the error — exposing a narrower error through
/// a wider selector's public associated type fails the private-in-public check.
/// The selector therefore mirrors the error's visibility, capping `pub` at the
/// crate (a `pub(crate)` selector is reachable crate-wide yet never exceeds the
/// destination). When selectors are wrapped in a generated child module, a
/// module-relative visibility gains one `super` so it still reaches the error's
/// own scope.
fn default_selector_vis(error_vis: &Visibility, wrapped_in_module: bool) -> Visibility {
    if matches!(error_vis, Visibility::Public(_)) {
        return syn::parse_quote! { pub(crate) };
    }
    if wrapped_in_module {
        lift_into_child_module(error_vis)
    } else {
        error_vis.clone()
    }
}

/// Re-express a restricted visibility one module level deeper, as seen from a
/// generated child module that holds the selectors. Crate-absolute paths are
/// position-independent; module-relative ones gain a `super`.
fn lift_into_child_module(vis: &Visibility) -> Visibility {
    let Visibility::Restricted(restricted) = vis else {
        return syn::parse_quote! { pub(super) };
    };
    let path = &restricted.path;
    match path.segments.first() {
        Some(seg) if seg.ident == "crate" => vis.clone(),
        Some(seg) if seg.ident == "self" && path.segments.len() == 1 => {
            syn::parse_quote! { pub(super) }
        }
        _ => syn::parse_quote! { pub(in super::#path) },
    }
}

/// Generate context selectors for all variants of an enum.
pub fn gen_enum_selectors(
    input: &DeriveInput,
    container: &EnumContainerAttrs,
    oopsie_path: &syn::Path,
) -> syn::Result<Vec<TokenStream2>> {
    let enum_ident = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let wrapped_in_module = matches!(container.effective_module(true), ModuleSetting::On(_));
    let vis = container
        .visibility()
        .cloned()
        .unwrap_or_else(|| default_selector_vis(&input.vis, wrapped_in_module));

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut selectors = Vec::new();
    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let categorized = CategorizedFields::from_fields(&variant.fields)?;
        let variant_ident = &variant.ident;
        // Extract `#[cfg(...)]` attrs so the generated selector + impl carry
        // the same gating as the variant. Without this, callers behind a
        // disabled feature still see references to types that don't exist.
        let cfg_attrs: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();

        if variant_attrs.transparent {
            // Generate From impl instead of selector
            if let Some(source) = &categorized.source {
                // When the source field uses `from(T, transform)` (or auto-box
                // detected `Box<T>`), the generated `From` impl accepts the
                // pre-transform type `T` and applies the transform internally,
                // matching snafu's `#[snafu(context(false))]` semantics.
                let source_ident = &source.ident;
                let (param_ty, body_assign) = match &source.kind {
                    super::parse::SourceKind::Transformed {
                        source_type,
                        transform,
                    } => (
                        quote! { #source_type },
                        quote! { let #source_ident = (#transform)(source); },
                    ),
                    super::parse::SourceKind::Yes => {
                        let ty = &source.ty;
                        (quote! { #ty }, quote! { let #source_ident = source; })
                    }
                    super::parse::SourceKind::No => {
                        unreachable!("categorized.source set but kind is SourceKind::No")
                    }
                };
                let auto_inits = gen_auto_inits(&categorized, oopsie_path, true);
                let auto_names = gen_auto_field_names(&categorized);
                let user_inits = gen_user_default_inits(&categorized);
                selectors.push(quote! {
                    #(#cfg_attrs)*
                    impl #ty_generics ::core::convert::From<#param_ty> for #enum_ident #ty_generics {
                        #[track_caller]
                        fn from(source: #param_ty) -> Self {
                            // Capture probes borrow `&source` before
                            // `body_assign` moves it into the renamed field.
                            #(#auto_inits)*
                            #body_assign
                            #enum_ident::#variant_ident {
                                #source_ident,
                                #(#user_inits)*
                                #(#auto_names,)*
                            }
                        }
                    }
                });
            }
            continue;
        }

        let selector_ident = selector_name(variant_ident, &container.effective_suffix(true));
        let selector_vis = variant_attrs
            .visibility()
            .cloned()
            .unwrap_or_else(|| vis.clone());

        let has_source = categorized.source.is_some();
        let user_fields = &categorized.user_fields;

        // Generate selector struct
        let (generic_params, _generic_args, where_clauses, struct_fields) =
            if user_fields.is_empty() {
                // Unit struct for source-only or no-field variants
                (quote! {}, quote! {}, quote! {}, quote! {})
            } else {
                let mut params = Vec::new();
                let mut args = Vec::new();
                let mut bounds = Vec::new();
                let mut fields = Vec::new();
                for (i, uf) in user_fields.iter().enumerate() {
                    let ty_param = format_ident!("__T{}", i);
                    let field_ident = &uf.ident;
                    let field_ty = &uf.ty;
                    params.push(quote! { #ty_param });
                    args.push(quote! { #ty_param });
                    bounds.push(quote! { #ty_param: ::core::convert::Into<#field_ty> });
                    fields.push(quote! { pub #field_ident: #ty_param });
                }
                (
                    quote! { <#(#params),*> },
                    quote! { <#(#args),*> },
                    quote! { where #(#bounds),* },
                    quote! { { #(#fields),* } },
                )
            };

        let selector_struct = if user_fields.is_empty() {
            quote! {
                #(#cfg_attrs)*
                #[derive(Debug, Copy, Clone)]
                #selector_vis struct #selector_ident;
            }
        } else {
            quote! {
                #(#cfg_attrs)*
                #[derive(Debug, Copy, Clone)]
                #selector_vis struct #selector_ident #generic_params #struct_fields
            }
        };

        // Generate Contextual or build/fail depending on whether there's a source
        let methods_inner = if has_source {
            gen_build_error(
                &selector_ident,
                enum_ident,
                variant_ident,
                &categorized,
                &generic_params,
                &where_clauses,
                oopsie_path,
            )
        } else {
            gen_build_fail(
                &selector_ident,
                enum_ident,
                variant_ident,
                &categorized,
                &generic_params,
                &where_clauses,
                oopsie_path,
            )
        };
        // Wrap methods in `const _: () = { ... };` so cfg-attrs apply to all
        // impl blocks emitted by gen_build_error / gen_build_fail.
        let methods = if cfg_attrs.is_empty() {
            methods_inner
        } else {
            quote! {
                #(#cfg_attrs)*
                const _: () = {
                    #methods_inner
                };
            }
        };

        selectors.push(quote! {
            #selector_struct
            #methods
        });
    }

    Ok(selectors)
}

/// Generate context selector for a struct error.
pub fn gen_struct_selector(
    input: &DeriveInput,
    attrs: &StructAttrs,
    oopsie_path: &syn::Path,
) -> syn::Result<TokenStream2> {
    let struct_ident = &input.ident;
    let vis = attrs
        .visibility()
        .cloned()
        .unwrap_or_else(|| default_selector_vis(&input.vis, false));
    // Variant-level fields are inlined on `StructAttrs` (darling allows only
    // one flatten per derive); aliasing makes downstream field access read
    // naturally as `variant_attrs.transparent` etc.
    let variant_attrs = attrs;

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;

    if variant_attrs.transparent {
        // Generate From impl for transparent structs
        if let Some(source) = &categorized.source {
            let source_ident = &source.ident;
            let (param_ty, body_assign) = match &source.kind {
                super::parse::SourceKind::Transformed {
                    source_type,
                    transform,
                } => (
                    quote! { #source_type },
                    quote! { let #source_ident = (#transform)(source); },
                ),
                super::parse::SourceKind::Yes => {
                    let ty = &source.ty;
                    (quote! { #ty }, quote! { let #source_ident = source; })
                }
                super::parse::SourceKind::No => {
                    unreachable!("categorized.source set but kind is SourceKind::No")
                }
            };
            let auto_inits = gen_auto_inits(&categorized, oopsie_path, true);
            let auto_names = gen_auto_field_names(&categorized);
            return Ok(quote! {
                impl ::core::convert::From<#param_ty> for #struct_ident {
                    #[track_caller]
                    fn from(source: #param_ty) -> Self {
                        // Capture probes borrow `&source` before `body_assign`
                        // moves it into the renamed field.
                        #(#auto_inits)*
                        #body_assign
                        Self { #source_ident, #(#auto_names,)* }
                    }
                }
            });
        }
        return Ok(TokenStream2::new());
    }

    let selector_ident = selector_name(struct_ident, &attrs.container.effective_suffix(false));
    let has_source = categorized.source.is_some();
    let user_fields = &categorized.user_fields;

    let (generic_params, _generic_args, where_clauses, struct_fields) = if user_fields.is_empty() {
        (quote! {}, quote! {}, quote! {}, quote! {})
    } else {
        let mut params = Vec::new();
        let mut args = Vec::new();
        let mut bounds = Vec::new();
        let mut fields = Vec::new();
        for (i, uf) in user_fields.iter().enumerate() {
            let ty_param = format_ident!("__T{}", i);
            let field_ident = &uf.ident;
            let field_ty = &uf.ty;
            params.push(quote! { #ty_param });
            args.push(quote! { #ty_param });
            bounds.push(quote! { #ty_param: ::core::convert::Into<#field_ty> });
            fields.push(quote! { pub #field_ident: #ty_param });
        }
        (
            quote! { <#(#params),*> },
            quote! { <#(#args),*> },
            quote! { where #(#bounds),* },
            quote! { { #(#fields),* } },
        )
    };

    let selector_struct = if user_fields.is_empty() {
        quote! {
            #[derive(Debug, Copy, Clone)]
            #vis struct #selector_ident;
        }
    } else {
        quote! {
            #[derive(Debug, Copy, Clone)]
            #vis struct #selector_ident #generic_params #struct_fields
        }
    };

    let methods = if has_source {
        gen_build_error_struct(
            &selector_ident,
            struct_ident,
            &categorized,
            &generic_params,
            &where_clauses,
            oopsie_path,
        )
    } else {
        gen_build_fail_struct(
            &selector_ident,
            struct_ident,
            &categorized,
            &generic_params,
            &where_clauses,
            oopsie_path,
        )
    };

    Ok(quote! {
        #selector_struct
        #methods
    })
}

fn selector_name(base: &Ident, suffix: &SuffixSetting) -> Ident {
    let base_str = base.to_string();
    let stripped = base_str
        .strip_suffix("Error")
        .filter(|s| !s.is_empty())
        .unwrap_or(&base_str);
    match suffix {
        SuffixSetting::Off => Ident::new(stripped, base.span()),
        SuffixSetting::Custom(s) => format_ident!("{}{}", stripped, s),
    }
}

fn gen_auto_inits(
    categorized: &CategorizedFields,
    oopsie_path: &syn::Path,
    has_source: bool,
) -> Vec<TokenStream2> {
    categorized
        .auto_fields
        .iter()
        .map(|af| {
            let ident = &af.ident;
            let ty = &af.ty;
            if has_source {
                quote! {
                    let #ident = {
                        use #oopsie_path::__private::{CaptureFromExt as _, CaptureFromFallback as _};
                        (&#oopsie_path::__private::CaptureProbe(&source)).resolve::<#ty>()
                    };
                }
            } else {
                quote! { let #ident = <#ty as #oopsie_path::Capturable>::capture(); }
            }
        })
        .collect()
}

fn gen_user_default_inits(categorized: &CategorizedFields) -> Vec<TokenStream2> {
    categorized
        .user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            quote! { #ident: ::core::default::Default::default(), }
        })
        .collect()
}

fn gen_auto_field_names(categorized: &CategorizedFields) -> Vec<&Ident> {
    categorized.auto_fields.iter().map(|af| &af.ident).collect()
}

/// Generate `Contextual` impl for an enum variant with a source field.
fn gen_build_error(
    selector_ident: &Ident,
    enum_ident: &Ident,
    variant_ident: &Ident,
    categorized: &CategorizedFields,
    generic_params: &TokenStream2,
    where_clauses: &TokenStream2,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let source_field = categorized.source.as_ref().unwrap();
    let source_ident = &source_field.ident;

    let (source_type, source_transform) = match &source_field.kind {
        SourceKind::No => unreachable!(),
        SourceKind::Yes => {
            let ty = &source_field.ty;
            (quote! { #ty }, None)
        }
        SourceKind::Transformed {
            source_type,
            transform,
        } => (quote! { #source_type }, Some(quote! { (#transform) })),
    };

    let auto_inits = gen_auto_inits(categorized, oopsie_path, true);
    let auto_names = gen_auto_field_names(categorized);
    let user_inits: Vec<_> = categorized
        .user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            quote! { #ident: self.#ident.into() }
        })
        .collect();

    let source_assign = if let Some(transform) = source_transform {
        quote! { let #source_ident = #transform(source); }
    } else {
        quote! { let #source_ident = source; }
    };

    quote! {
        impl #generic_params #oopsie_path::Contextual<#source_type> for #selector_ident #generic_params
        #where_clauses
        {
            type Destination = #enum_ident;

            #[track_caller]
            fn build_error(self, source: #source_type) -> #enum_ident {
                // Capture probes borrow `&source`, so they must run before
                // `source_assign` moves `source` into the (possibly renamed)
                // field. They also see the pre-transform value, preserving the
                // source's own trace.
                #(#auto_inits)*
                #source_assign
                #enum_ident::#variant_ident {
                    #(#user_inits,)*
                    #source_ident,
                    #(#auto_names,)*
                }
            }
        }
    }
}

/// Generate `build()` and `fail()` for leaf enum variants (no source).
fn gen_build_fail(
    selector_ident: &Ident,
    enum_ident: &Ident,
    variant_ident: &Ident,
    categorized: &CategorizedFields,
    generic_params: &TokenStream2,
    where_clauses: &TokenStream2,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let auto_inits = gen_auto_inits(categorized, oopsie_path, false);
    let auto_names = gen_auto_field_names(categorized);
    let user_inits: Vec<_> = categorized
        .user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            quote! { #ident: self.#ident.into() }
        })
        .collect();

    // Also implement Contextual with NoSource for OptionExt support
    let none_error_impl = quote! {
        impl #generic_params #oopsie_path::Contextual<#oopsie_path::NoSource> for #selector_ident #generic_params
        #where_clauses
        {
            type Destination = #enum_ident;

            #[track_caller]
            fn build_error(self, _: #oopsie_path::NoSource) -> #enum_ident {
                self.build()
            }
        }
    };

    quote! {
        impl #generic_params #selector_ident #generic_params
        #where_clauses
        {
            #[must_use]
            #[track_caller]
            pub fn build(self) -> #enum_ident {
                #(#auto_inits)*
                #enum_ident::#variant_ident {
                    #(#user_inits,)*
                    #(#auto_names,)*
                }
            }

            #[track_caller]
            pub fn fail<__T>(self) -> ::core::result::Result<__T, #enum_ident> {
                ::core::result::Result::Err(self.build())
            }
        }

        #none_error_impl
    }
}

/// Generate `Contextual` impl for a struct with source.
fn gen_build_error_struct(
    selector_ident: &Ident,
    struct_ident: &Ident,
    categorized: &CategorizedFields,
    generic_params: &TokenStream2,
    where_clauses: &TokenStream2,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let source_field = categorized.source.as_ref().unwrap();
    let source_ident = &source_field.ident;

    let (source_type, source_transform) = match &source_field.kind {
        SourceKind::No => unreachable!(),
        SourceKind::Yes => {
            let ty = &source_field.ty;
            (quote! { #ty }, None)
        }
        SourceKind::Transformed {
            source_type,
            transform,
        } => (quote! { #source_type }, Some(quote! { (#transform) })),
    };

    let auto_inits = gen_auto_inits(categorized, oopsie_path, true);
    let auto_names = gen_auto_field_names(categorized);
    let user_inits: Vec<_> = categorized
        .user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            quote! { #ident: self.#ident.into() }
        })
        .collect();

    let source_assign = if let Some(transform) = source_transform {
        quote! { let #source_ident = #transform(source); }
    } else {
        quote! { let #source_ident = source; }
    };

    quote! {
        impl #generic_params #oopsie_path::Contextual<#source_type> for #selector_ident #generic_params
        #where_clauses
        {
            type Destination = #struct_ident;

            #[track_caller]
            fn build_error(self, source: #source_type) -> #struct_ident {
                // Capture probes borrow `&source` before `source_assign` moves
                // it (see the enum path).
                #(#auto_inits)*
                #source_assign
                #struct_ident {
                    #(#user_inits,)*
                    #source_ident,
                    #(#auto_names,)*
                }
            }
        }
    }
}

/// Generate `build()` and `fail()` for a leaf struct (no source).
fn gen_build_fail_struct(
    selector_ident: &Ident,
    struct_ident: &Ident,
    categorized: &CategorizedFields,
    generic_params: &TokenStream2,
    where_clauses: &TokenStream2,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let auto_inits = gen_auto_inits(categorized, oopsie_path, false);
    let auto_names = gen_auto_field_names(categorized);
    let user_inits: Vec<_> = categorized
        .user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            quote! { #ident: self.#ident.into() }
        })
        .collect();

    let none_error_impl = quote! {
        impl #generic_params #oopsie_path::Contextual<#oopsie_path::NoSource> for #selector_ident #generic_params
        #where_clauses
        {
            type Destination = #struct_ident;

            #[track_caller]
            fn build_error(self, _: #oopsie_path::NoSource) -> #struct_ident {
                self.build()
            }
        }
    };

    quote! {
        impl #generic_params #selector_ident #generic_params
        #where_clauses
        {
            #[must_use]
            #[track_caller]
            pub fn build(self) -> #struct_ident {
                #(#auto_inits)*
                #struct_ident {
                    #(#user_inits,)*
                    #(#auto_names,)*
                }
            }

            #[track_caller]
            pub fn fail<__T>(self) -> ::core::result::Result<__T, #struct_ident> {
                ::core::result::Result::Err(self.build())
            }
        }

        #none_error_impl
    }
}
