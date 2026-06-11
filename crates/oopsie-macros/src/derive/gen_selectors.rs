//! Context selector generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt as _;
use syn::{DeriveInput, Ident, Visibility};

use super::parse::{
    CategorizedFields, EnumContainerAttrs, ModuleSetting, SourceKind, StructAttrs, SuffixSetting,
    UserField, VariantAttrs,
};

/// The generic-parameter list, where-clause, field block, and derive set for a
/// selector struct built from a variant/struct's user fields.
struct SelectorShape {
    /// `<__T0, ...>` for the selector's `Into`-converted fields (empty when all
    /// user fields are cfg-gated, which keeps decl/use positions in sync).
    generic_params: TokenStream2,
    where_clauses: TokenStream2,
    /// The braced field block (`{ ... }`), each field carrying its cfg attrs.
    struct_fields: TokenStream2,
    /// The `#[derive(...)]` line for the selector struct.
    derives: TokenStream2,
}

/// Build a selector's generic params, where-clause, field block, and derive set
/// from its user fields. `doc` renders each field's doc-comment.
///
/// A cfg-gated field cannot ride an `Into` type parameter: cfg attrs are
/// rejected on generic *arguments* (the `Selector<__T>` use position) and
/// unstable in `where` clauses, so a gated `__T` would dangle once the field is
/// stripped. Such fields instead take their own concrete type (no conversion),
/// gated alongside the field; only unconditional fields contribute a parameter.
/// A concrete-typed field also can't ride a blanket `Copy`/`Clone` derive (its
/// type may be neither), so any cfg-gated field drops both from the selector.
fn selector_shape(user_fields: &[UserField], doc: &dyn Fn(&UserField) -> String) -> SelectorShape {
    let mut params = Vec::new();
    let mut bounds = Vec::new();
    let mut fields = Vec::new();
    let mut has_cfg_field = false;
    for (i, uf) in user_fields.iter().enumerate() {
        let field_ident = &uf.ident;
        let field_ty = &uf.ty;
        let cfg = &uf.cfg_attrs;
        let field_doc = doc(uf);
        if cfg.is_empty() {
            let ty_param = format_ident!("__T{}", i);
            params.push(quote! { #ty_param });
            bounds.push(quote! { #ty_param: ::core::convert::Into<#field_ty> });
            fields.push(quote! { #[doc = #field_doc] pub #field_ident: #ty_param });
        } else {
            has_cfg_field = true;
            fields.push(quote! { #(#cfg)* #[doc = #field_doc] pub #field_ident: #field_ty });
        }
    }
    let generic_params = if params.is_empty() {
        quote! {}
    } else {
        quote! { <#(#params),*> }
    };
    let where_clauses = if bounds.is_empty() {
        quote! {}
    } else {
        quote! { where #(#bounds),* }
    };
    let derives = if has_cfg_field {
        quote! { #[derive(Debug)] }
    } else {
        quote! { #[derive(Debug, Copy, Clone)] }
    };
    SelectorShape {
        generic_params,
        where_clauses,
        struct_fields: quote! { { #(#fields),* } },
        derives,
    }
}

/// Per-field initializers for the destination's struct expression, e.g.
/// `name: self.name.into()`. cfg-gated fields keep their concrete type, so they
/// move without `.into()` and carry their cfg attrs so the initializer is
/// stripped together with the field.
fn user_init_exprs(user_fields: &[UserField]) -> Vec<TokenStream2> {
    user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            let cfg = &uf.cfg_attrs;
            if cfg.is_empty() {
                quote! { #ident: self.#ident.into() }
            } else {
                quote! { #(#cfg)* #ident: self.#ident }
            }
        })
        .collect()
}

/// Resolve a selector's visibility from an explicit `vis(...)` override or, in
/// its absence, the error type's own visibility (snafu parity: a library's
/// `pub` error yields selectors its downstream users can name). Both sources go
/// through the same child-module lift, so an explicit override can't drift the
/// way it did when only the default path lifted.
fn resolve_selector_vis(
    explicit: Option<&Visibility>,
    error_vis: &Visibility,
    wrapped_in_module: bool,
) -> Visibility {
    let chosen = explicit.unwrap_or(error_vis);
    if wrapped_in_module {
        lift_into_child_module(chosen)
    } else {
        chosen.clone()
    }
}

/// Re-express a visibility one module level deeper, as seen from a generated
/// child module that holds the selectors. `pub` and crate-absolute paths are
/// position-independent; module-relative ones (`super`, `self`, `in path`) and
/// inherited (private) visibility gain a `super` so they still reach the
/// error's own scope.
fn lift_into_child_module(vis: &Visibility) -> Visibility {
    let restricted = match vis {
        Visibility::Public(_) => return vis.clone(),
        Visibility::Inherited => return syn::parse_quote! { pub(super) },
        Visibility::Restricted(restricted) => restricted,
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
    let vis = resolve_selector_vis(container.visibility(), &input.vis, wrapped_in_module);

    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };

    let mut selectors = Vec::new();
    let mut seen_selectors: std::collections::HashMap<String, Ident> =
        std::collections::HashMap::new();
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
            let Some(source) = &categorized.source else {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "`transparent` requires a source field (a field named `source` \
                     or marked `#[oopsie(from)]`)",
                ));
            };
            if let Some(extra) = categorized.user_fields.first() {
                return Err(syn::Error::new_spanned(
                    &extra.ident,
                    "`transparent` allows no fields besides the source and \
                     auto-captured trace fields; remove this field or drop `transparent`",
                ));
            }
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
                super::parse::SourceKind::No | super::parse::SourceKind::Disabled => {
                    unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
                }
            };
            let auto_inits = gen_auto_inits(&categorized, oopsie_path, true);
            let auto_names = gen_auto_field_inits(&categorized);
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
                            #(#auto_names)*
                        }
                    }
                }
            });
            continue;
        }

        let selector_ident = selector_name(variant_ident, &container.effective_suffix(true))?;
        // cfg-gated variants may legitimately share a selector name under
        // mutually exclusive cfgs, so only unconditional variants participate
        // in the collision check.
        if cfg_attrs.is_empty()
            && let Some(first) =
                seen_selectors.insert(selector_ident.to_string(), variant_ident.clone())
        {
            let mut err = syn::Error::new_spanned(
                variant_ident,
                format!(
                    "variants `{first}` and `{variant_ident}` both generate a selector named \
                     `{selector_ident}` (a trailing `Error` is stripped from variant names); \
                     rename one of the variants"
                ),
            );
            err.combine(syn::Error::new_spanned(
                &first,
                format!("`{first}` also generates selector `{selector_ident}`"),
            ));
            return Err(err);
        }
        // `vis` is already re-anchored to the child module, so the fallback
        // takes no further lift; an explicit variant override goes through the
        // same lift as the container default rather than being emitted verbatim.
        let selector_vis = match variant_attrs.visibility() {
            Some(explicit) => resolve_selector_vis(Some(explicit), &vis, wrapped_in_module),
            None => vis.clone(),
        };

        let has_source = categorized.source.is_some();
        let user_fields = &categorized.user_fields;

        // Generate selector struct
        let SelectorShape {
            generic_params,
            where_clauses,
            struct_fields,
            derives,
        } = selector_shape(user_fields, &|uf| {
            format!(
                "Value for the `{}` field of `{enum_ident}::{variant_ident}`.",
                uf.ident
            )
        });

        let selector_doc = format!("Context selector for `{enum_ident}::{variant_ident}`.");
        let selector_struct = if user_fields.is_empty() {
            quote! {
                #(#cfg_attrs)*
                #[doc = #selector_doc]
                #[derive(Debug, Copy, Clone)]
                #selector_vis struct #selector_ident;
            }
        } else {
            quote! {
                #(#cfg_attrs)*
                #[doc = #selector_doc]
                #derives
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
    let wrapped_in_module = matches!(
        attrs.container.effective_module(false),
        ModuleSetting::On(_)
    );
    let vis = resolve_selector_vis(attrs.visibility(), &input.vis, wrapped_in_module);
    // Variant-level fields are inlined on `StructAttrs` (darling allows only
    // one flatten per derive); aliasing makes downstream field access read
    // naturally as `variant_attrs.transparent` etc.
    let variant_attrs = attrs;

    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };

    let categorized = CategorizedFields::from_fields(&data.fields)?;

    if variant_attrs.transparent {
        let Some(source) = &categorized.source else {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`transparent` requires a source field (a field named `source` \
                 or marked `#[oopsie(from)]`)",
            ));
        };
        if let Some(extra) = categorized.user_fields.first() {
            return Err(syn::Error::new_spanned(
                &extra.ident,
                "`transparent` allows no fields besides the source and \
                 auto-captured trace fields; remove this field or drop `transparent`",
            ));
        }
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
            super::parse::SourceKind::No | super::parse::SourceKind::Disabled => {
                unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
            }
        };
        let auto_inits = gen_auto_inits(&categorized, oopsie_path, true);
        let auto_names = gen_auto_field_inits(&categorized);
        return Ok(quote! {
            impl ::core::convert::From<#param_ty> for #struct_ident {
                #[track_caller]
                fn from(source: #param_ty) -> Self {
                    // Capture probes borrow `&source` before `body_assign`
                    // moves it into the renamed field.
                    #(#auto_inits)*
                    #body_assign
                    Self { #source_ident, #(#auto_names)* }
                }
            }
        });
    }

    let selector_ident = selector_name(struct_ident, &attrs.container.effective_suffix(false))?;
    let has_source = categorized.source.is_some();
    let user_fields = &categorized.user_fields;

    let SelectorShape {
        generic_params,
        where_clauses,
        struct_fields,
        derives,
    } = selector_shape(user_fields, &|uf| {
        format!("Value for the `{}` field of `{struct_ident}`.", uf.ident)
    });

    let selector_doc = format!("Context selector for `{struct_ident}`.");
    let selector_struct = if user_fields.is_empty() {
        quote! {
            #[doc = #selector_doc]
            #[derive(Debug, Copy, Clone)]
            #vis struct #selector_ident;
        }
    } else {
        quote! {
            #[doc = #selector_doc]
            #derives
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

fn selector_name(base: &Ident, suffix: &SuffixSetting) -> syn::Result<Ident> {
    let base_str = base.unraw().to_string();
    let stripped = base_str
        .strip_suffix("Error")
        .filter(|s| !s.is_empty())
        .unwrap_or(&base_str);
    let name = match suffix {
        SuffixSetting::Off => stripped.to_owned(),
        SuffixSetting::Custom(s) => format!("{stripped}{s}"),
    };
    ident_maybe_raw(&name, base.span())
}

/// Derived names can collide with keywords (`r#try` strips to `try`), which
/// `Ident::new` panics on; those become raw idents. The handful of names that
/// cannot be raw either are a real error, not a panic.
fn ident_maybe_raw(name: &str, span: proc_macro2::Span) -> syn::Result<Ident> {
    match syn::parse_str::<Ident>(name) {
        Ok(mut id) => {
            id.set_span(span);
            Ok(id)
        }
        Err(_) if matches!(name, "self" | "Self" | "super" | "crate" | "_") => {
            Err(syn::Error::new(
                span,
                format!(
                    "cannot generate a selector named `{name}`; rename the item or set \
                 `#[oopsie(suffix = \"...\")]`"
                ),
            ))
        }
        Err(_) => Ok(Ident::new_raw(name, span)),
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
            let cfg = &af.cfg_attrs;
            if has_source {
                quote! {
                    #(#cfg)*
                    let #ident = {
                        use #oopsie_path::__private::{CaptureFromExt as _, CaptureFromFallback as _};
                        (&#oopsie_path::__private::CaptureProbe(&source)).resolve::<#ty>()
                    };
                }
            } else {
                quote! { #(#cfg)* let #ident = <#ty as #oopsie_path::Capturable>::capture(); }
            }
        })
        .collect()
}

/// Shorthand struct-expression initializers (`name,`) for auto-captured fields,
/// each carrying its cfg attrs so a stripped field's binding and reference
/// vanish together.
fn gen_auto_field_inits(categorized: &CategorizedFields) -> Vec<TokenStream2> {
    categorized
        .auto_fields
        .iter()
        .map(|af| {
            let ident = &af.ident;
            let cfg = &af.cfg_attrs;
            quote! { #(#cfg)* #ident, }
        })
        .collect()
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
        SourceKind::No | SourceKind::Disabled => {
            unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
        }
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
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields);

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
                    #(#auto_names)*
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
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields);

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

    let build_doc = format!("Builds `{enum_ident}::{variant_ident}` from this selector's fields.");
    let fail_doc = format!("Builds `{enum_ident}::{variant_ident}` and returns it as `Err`.");
    quote! {
        impl #generic_params #selector_ident #generic_params
        #where_clauses
        {
            #[doc = #build_doc]
            #[must_use]
            #[track_caller]
            pub fn build(self) -> #enum_ident {
                #(#auto_inits)*
                #enum_ident::#variant_ident {
                    #(#user_inits,)*
                    #(#auto_names)*
                }
            }

            #[doc = #fail_doc]
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
        SourceKind::No | SourceKind::Disabled => {
            unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
        }
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
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields);

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
                    #(#auto_names)*
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
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields);

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

    let build_doc = format!("Builds `{struct_ident}` from this selector's fields.");
    let fail_doc = format!("Builds `{struct_ident}` and returns it as `Err`.");
    quote! {
        impl #generic_params #selector_ident #generic_params
        #where_clauses
        {
            #[doc = #build_doc]
            #[must_use]
            #[track_caller]
            pub fn build(self) -> #struct_ident {
                #(#auto_inits)*
                #struct_ident {
                    #(#user_inits,)*
                    #(#auto_names)*
                }
            }

            #[doc = #fail_doc]
            #[track_caller]
            pub fn fail<__T>(self) -> ::core::result::Result<__T, #struct_ident> {
                ::core::result::Result::Err(self.build())
            }
        }

        #none_error_impl
    }
}
