//! The `#[oopsie]` attribute macro implementation.
//!
//! This is the ergonomic primary path for defining error types. It handles:
//! - Generating all Oopsie impls (same codegen as `#[derive(Oopsie)]`)
//! - Injecting diagnostic fields when tracing options are requested
//! - Generating `Debug` automatically (no need to write `#[derive(Debug)]`)

use darling::FromMeta as _;
use darling::ast::NestedMeta;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse_quote;
use syn::spanned::Spanned as _;

use crate::derive;
use crate::traced::args::{CodeSettings, TracedArgs};
use crate::utils::FieldSetting;

#[derive(Debug, darling::FromMeta)]
pub struct OopsieAttrArgs {
    pub traced: Option<FieldSetting<true, TracedArgs>>,
    pub code: FieldSetting<true, CodeSettings>,
    pub debug: crate::utils::BetterFlag<true>,
    pub path: Option<syn::Path>,
}

/// Trace options that only exist nested inside `traced(...)`; spelled at the
/// top level they get a targeted error instead of darling's generic
/// unknown-field one.
const NESTED_ONLY_KEYS: &[&str] = &["backtrace", "spantrace", "timestamp", "packed", "boxed"];

pub fn expand(attrs: TokenStream2, input: TokenStream2) -> syn::Result<TokenStream2> {
    let meta = NestedMeta::parse_meta_list(attrs)?;

    if let Some(lit) = meta.iter().find_map(|m| match m {
        NestedMeta::Lit(lit @ syn::Lit::Str(_)) => Some(lit),
        NestedMeta::Lit(_) | NestedMeta::Meta(_) => None,
    }) {
        return Err(syn::Error::new_spanned(
            lit,
            "display strings don't go in the macro arguments; put them in a \
             separate `#[oopsie(\"...\")]` attribute on the struct, or on each \
             enum variant",
        ));
    }

    for m in &meta {
        let NestedMeta::Meta(inner) = m else { continue };
        let Some(ident) = inner.path().get_ident() else {
            continue;
        };
        if NESTED_ONLY_KEYS.iter().any(|k| ident == k) {
            let suggestion = quote::quote!(#inner).to_string();
            return Err(syn::Error::new_spanned(
                inner.path(),
                format!(
                    "`{ident}` must be nested inside `traced(...)`, \
                     e.g. `#[oopsie::oopsie(traced({suggestion}))]`"
                ),
            ));
        }
    }

    let args = OopsieAttrArgs::from_list(&meta)?;
    let needs_tracing = args.traced.as_ref().is_some_and(FieldSetting::is_enabled);

    // The resolved `spantrace` setting can't tell an explicit request apart from
    // the omitted default — both resolve to the same `Flag(true)` — so an
    // explicit-only rejection has to scan the raw tokens before darling folds them.
    #[cfg(not(feature = "tracing"))]
    {
        let names_spantrace = meta.iter().any(|m| {
            let NestedMeta::Meta(syn::Meta::List(list)) = m else {
                return false;
            };
            if !list.path.is_ident("traced") {
                return false;
            }
            let Ok(inner) = list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            inner.iter().any(|nm| nm.path().is_ident("spantrace"))
        });
        if names_spantrace {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`spantrace` requires the `tracing` feature of `oopsie`",
            ));
        }
    }

    match syn::parse2::<syn::Item>(input)? {
        syn::Item::Enum(item_enum) => expand_enum(&args, needs_tracing, item_enum),
        syn::Item::Struct(item_struct) => expand_struct(&args, needs_tracing, item_struct),
        other => Err(syn::Error::new_spanned(
            other,
            "`#[oopsie]` can only be applied to enums or structs",
        )),
    }
}

fn expand_enum(
    args: &OopsieAttrArgs,
    needs_tracing: bool,
    item: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let span = item.span();
    let oopsie_path: syn::Path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    // Step 1: inject diagnostic fields if requested.
    let injected_ts = if needs_tracing {
        let traced = args
            .traced
            .as_ref()
            .expect("needs_tracing implies traced is present")
            .settings();
        crate::traced::expand_enum::expand_enum(&traced, &args.code, &oopsie_path, span, item)?
    } else {
        quote! { #item }
    };

    // Step 2: generate Oopsie impls from the (possibly modified) item.
    let derive_input: syn::DeriveInput = syn::parse2(injected_ts.clone())?;
    let mut container_attrs = derive::parse::EnumContainerAttrs::from_attrs(&derive_input.attrs)?;
    // The macro-level `path` governs every generated impl; an explicit
    // container-attr `path` still wins.
    if container_attrs.inner.path.is_none() {
        container_attrs.inner.path.clone_from(&args.path);
    }
    let impls = derive::expand_enum(&derive_input, &container_attrs)?;

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    let mut out_item: syn::ItemEnum = syn::parse2(injected_ts)?;
    fix_derives(&mut out_item.attrs, args.debug.is_enabled());
    strip_oopsie_attrs(&mut out_item.attrs);
    for variant in &mut out_item.variants {
        strip_oopsie_attrs(&mut variant.attrs);
        for field in &mut variant.fields {
            strip_oopsie_attrs(&mut field.attrs);
        }
    }

    Ok(quote! {
        #out_item
        #impls
    })
}

fn expand_struct(
    args: &OopsieAttrArgs,
    needs_tracing: bool,
    item: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let span = item.span();
    let oopsie_path: syn::Path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    // Step 1: inject diagnostic fields if requested.
    let injected_ts = if needs_tracing {
        let traced = args
            .traced
            .as_ref()
            .expect("needs_tracing implies traced is present")
            .settings();
        crate::traced::expand_struct::expand_struct(&traced, &args.code, &oopsie_path, span, item)?
    } else {
        quote! { #item }
    };

    // Step 2: generate Oopsie impls from the (possibly modified) item.
    let derive_input: syn::DeriveInput = syn::parse2(injected_ts.clone())?;
    let mut container_attrs = derive::parse::StructAttrs::from_attrs(&derive_input.attrs)?;
    // The macro-level `path` governs every generated impl; an explicit
    // container-attr `path` still wins.
    if container_attrs.container.path.is_none() {
        container_attrs.container.path.clone_from(&args.path);
    }
    let impls = derive::expand_struct(&derive_input, &container_attrs)?;

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    let mut out_item: syn::ItemStruct = syn::parse2(injected_ts)?;
    fix_derives(&mut out_item.attrs, args.debug.is_enabled());
    strip_oopsie_attrs(&mut out_item.attrs);
    for field in &mut out_item.fields {
        strip_oopsie_attrs(&mut field.attrs);
    }

    Ok(quote! {
        #out_item
        #impls
    })
}

/// Whether a derive path names `ident` in its final segment, so qualified
/// spellings (`::core::fmt::Debug`, `oopsie::Oopsie`) are recognized the same as
/// the bare ident. `is_ident` only matches single-segment paths.
fn derive_path_is(path: &syn::Path, ident: &str) -> bool {
    path.segments.last().is_some_and(|seg| seg.ident == ident)
}

/// Whether a `#[cfg_attr(<pred>, ..., derive(...))]` attribute conditionally
/// derives `ident`. Injection must not race a conditional derive: the injected
/// impl conflicts whenever the cfg is active.
fn cfg_attr_derives(attr: &syn::Attribute, ident: &str) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(metas) = attr.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };
    // First meta is the cfg predicate; the rest are the gated attributes.
    metas.iter().skip(1).any(|meta| match meta {
        syn::Meta::List(list) if list.path.is_ident("derive") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|paths| paths.iter().any(|p| derive_path_is(p, ident))),
        syn::Meta::List(_) | syn::Meta::Path(_) | syn::Meta::NameValue(_) => false,
    })
}

/// Adjust the `#[derive(...)]` attributes on an item:
/// - Remove `Oopsie` (the attr macro handles code generation itself).
/// - Ensure `Debug` is present (required by `std::error::Error`) when
///   `inject_debug` is set and no `Debug` derive (plain or `cfg_attr`-gated)
///   is already there.
fn fix_derives(attrs: &mut Vec<syn::Attribute>, inject_debug: bool) {
    let mut has_debug = false;
    let mut new_attrs: Vec<syn::Attribute> = Vec::with_capacity(attrs.len() + 1);

    for attr in attrs.drain(..) {
        has_debug = has_debug || cfg_attr_derives(&attr, "Debug");
        if !attr.path().is_ident("derive") {
            new_attrs.push(attr);
            continue;
        }

        let paths = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        );

        let Ok(paths) = paths else {
            // Can't parse — leave attr as-is.
            new_attrs.push(attr);
            continue;
        };

        let filtered: Vec<syn::Path> = paths
            .into_iter()
            .filter(|p| !derive_path_is(p, "Oopsie"))
            .collect();

        has_debug = has_debug || filtered.iter().any(|p| derive_path_is(p, "Debug"));

        if !filtered.is_empty() {
            new_attrs.push(syn::parse_quote! { #[derive(#(#filtered),*)] });
        }
    }

    if inject_debug && !has_debug {
        new_attrs.push(syn::parse_quote! { #[derive(::core::fmt::Debug)] });
    }

    *attrs = new_attrs;
}

/// Strip all `#[oopsie(...)]` attributes. Used to remove processed helper attributes
/// from the output item so Rust doesn't complain about unknown attributes.
fn strip_oopsie_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|a| !a.path().is_ident("oopsie"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    // Output includes the `provide` impl when the unstable feature is on,
    // so the snapshot only matches in the default-features build.
    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn bare_enum_no_tracing() {
        let result = expand(
            quote! {},
            quote! {
                pub enum AppError {
                    #[oopsie("Connection failed")]
                    Connect,
                }
            },
        );
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn bare_struct_no_tracing() {
        let result = expand(
            quote! {},
            quote! {
                pub struct ConnectionFailed {
                    host: String,
                }
            },
        );
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn traced_enum() {
        let result = expand(
            quote! { traced },
            quote! {
                pub enum AppError {
                    #[oopsie("Connection failed")]
                    Connect,
                }
            },
        );
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[cfg(all(
        not(feature = "unstable-error-generic-member-access"),
        feature = "tracing"
    ))]
    #[test]
    fn backtrace_only_struct() {
        let result = expand(
            quote! { traced(spantrace(false)) },
            quote! {
                pub struct ConnectionFailed {
                    host: String,
                }
            },
        );
        let output = result.unwrap().to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn top_level_trace_arg_points_at_nested_form() {
        let err = expand(quote! { backtrace }, quote! { pub struct S { x: u32 } }).unwrap_err();
        assert!(err.to_string().contains("traced(backtrace)"), "{err}");
    }

    #[test]
    fn top_level_packed_points_at_nested_form() {
        let err = expand(
            quote! { traced, packed = false },
            quote! { pub struct S { x: u32 } },
        )
        .unwrap_err();
        assert!(err.to_string().contains("traced(packed = false)"), "{err}");
    }

    #[test]
    fn traced_false_disables_injection() {
        let output = expand(
            quote! { traced = false },
            quote! { pub struct S { x: u32 } },
        )
        .unwrap()
        .to_string();
        assert!(!output.contains("__oopsie_traces"), "{output}");
        assert!(!output.contains("__oopsie_backtrace"), "{output}");
    }

    #[test]
    fn path_reaches_derive_impls() {
        let out = expand(
            quote! { traced, path = "my_oopsie" },
            quote! { pub enum E { #[oopsie("boom")] Boom { info: String } } },
        )
        .unwrap()
        .to_string();
        assert!(out.contains("my_oopsie :: Contextual"), "{out}");
        assert!(!out.contains(":: oopsie :: Contextual"), "{out}");
    }

    #[test]
    fn container_path_attr_wins_over_macro_path() {
        let out = expand(
            quote! { path = "macro_oopsie" },
            quote! {
                #[oopsie(path = "attr_oopsie")]
                pub enum E { #[oopsie("boom")] Boom { info: String } }
            },
        )
        .unwrap()
        .to_string();
        assert!(out.contains("attr_oopsie :: Contextual"), "{out}");
        assert!(!out.contains("macro_oopsie :: Contextual"), "{out}");
    }

    #[test]
    fn strips_oopsie_derive_if_present() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Clone, Oopsie)]
                pub enum AppError {
                    #[oopsie("Fail")]
                    Fail,
                }
            },
        );
        let output = result.unwrap().to_string();
        // Oopsie should not appear inside a derive(...) in the output
        assert!(
            !output.contains("derive (Clone , Oopsie)")
                && !output.contains("derive(Clone, Oopsie)"),
            "Oopsie should be stripped from derive: {output}"
        );
    }

    #[test]
    fn debug_false_skips_injected_debug() {
        let output = expand(
            quote! { debug = false },
            quote! {
                pub enum AppError {
                    #[oopsie("Fail")]
                    Fail,
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(
            !output.contains("core :: fmt :: Debug"),
            "debug = false must not inject Debug:\n{output}"
        );
    }

    #[test]
    fn existing_debug_not_duplicated() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(Debug, Clone)]
                pub enum AppError {
                    #[oopsie("Fail")]
                    Fail,
                }
            },
        );
        let output = result.unwrap().to_string();
        // Since Debug was already present, fix_derives should NOT inject an extra
        // `#[derive(::core::fmt::Debug)]` — the injected form uses the qualified path.
        assert!(
            !output.contains("core :: fmt :: Debug"),
            "fix_derives should not inject extra Debug when already present:\n{output}"
        );
    }

    #[test]
    fn qualified_debug_not_duplicated() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(::core::fmt::Debug, Clone)]
                pub enum AppError {
                    #[oopsie("Fail")]
                    Fail,
                }
            },
        );
        let output = result.unwrap().to_string();
        // The user already qualified Debug; `fix_derives` must recognize it and
        // NOT inject a second `::core::fmt::Debug` (which would be a duplicate
        // impl). Only the user's qualified path should remain.
        assert_eq!(
            output.matches("core :: fmt :: Debug").count(),
            1,
            "qualified Debug must not be duplicated:\n{output}"
        );
    }

    #[test]
    fn qualified_oopsie_stripped() {
        let result = expand(
            quote! {},
            quote! {
                #[derive(oopsie::Oopsie)]
                pub enum AppError {
                    #[oopsie("Fail")]
                    Fail,
                }
            },
        );
        let output = result.unwrap().to_string();
        // The attr macro generates the impls itself, so a qualified
        // `oopsie::Oopsie` derive must be stripped from the emitted item.
        assert!(
            !output.contains("oopsie :: Oopsie"),
            "qualified Oopsie derive should be stripped:\n{output}"
        );
    }

    #[test]
    fn display_string_in_macro_args_gets_targeted_error() {
        let err = expand(
            quote! { "upstream failed: {service}" },
            quote! { pub struct UpstreamError { service: String } },
        )
        .unwrap_err();
        assert!(err.to_string().contains("separate"), "{err}");
    }

    #[test]
    fn display_string_in_non_first_position_gets_targeted_error() {
        let err = expand(
            quote! { traced, "msg" },
            quote! { pub struct UpstreamError { service: String } },
        )
        .unwrap_err();
        assert!(err.to_string().contains("separate"), "{err}");
    }

    #[test]
    fn rejects_union() {
        let result = expand(
            quote! {},
            quote! {
                pub union Foo { x: i32, y: f32 }
            },
        );
        result.unwrap_err();
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn traced_list_form_parses_and_injects() {
        // `traced(packed = false)` must parse (not error on unknown key `traced`)
        // and still trigger field injection.
        let result = expand(
            quote! { traced(packed = false) },
            quote! {
                pub enum AppError {
                    #[oopsie("boom")]
                    Boom { info: String },
                }
            },
        );
        let output = result
            .expect("traced(...) list form must parse")
            .to_string();
        // Unpacked => two separate injected fields.
        assert!(output.contains("__oopsie_backtrace"), "{output}");
        assert!(output.contains("__oopsie_spantrace"), "{output}");
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn traced_list_form_default_is_packed() {
        let result = expand(
            quote! { traced(boxed = false) },
            quote! {
                pub enum AppError {
                    #[oopsie("boom")]
                    Boom { info: String },
                }
            },
        );
        let output = result.expect("traced(boxed=false) must parse").to_string();
        // packed default => single combined field, no separate ones.
        assert!(output.contains("__oopsie_traces"), "{output}");
        assert!(!output.contains("__oopsie_backtrace"), "{output}");
    }

    #[cfg(not(feature = "tracing"))]
    #[test]
    fn traced_injects_backtrace_only_without_tracing() {
        let output = expand(
            quote! { traced },
            quote! {
                pub enum AppError {
                    #[oopsie("boom")]
                    Boom { info: String },
                }
            },
        )
        .unwrap()
        .to_string();
        assert!(output.contains("__oopsie_backtrace"), "{output}");
        assert!(!output.contains("__oopsie_spantrace"), "{output}");
        assert!(!output.contains("__oopsie_traces"), "{output}");
    }
}
