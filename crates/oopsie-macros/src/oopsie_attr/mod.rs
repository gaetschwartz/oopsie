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
use crate::traced::args::TracedArgs;
use crate::utils::{FieldSetting, TracedDefaults};

#[derive(Debug, darling::FromMeta)]
pub struct OopsieAttrArgs {
    pub traced: Option<FieldSetting<true, TracedArgs>>,
    pub debug: crate::utils::BetterFlag<true>,
    pub path: Option<syn::Path>,
}

/// Trace options that only exist nested inside `traced(...)`; spelled at the
/// top level they get a targeted error instead of darling's generic
/// unknown-field one.
const NESTED_ONLY_KEYS: &[&str] = &[
    "backtrace",
    "spantrace",
    "timestamp",
    "location",
    "packed",
    "boxed",
    "code",
];

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
    let (traced_defaults, manifest_err) = crate::utils::manifest_traced();
    // Precedence: a per-attribute `traced(...)` (including `traced = false`)
    // wins; otherwise the manifest `traced` default decides. When the manifest
    // turns tracing on, every sub-toggle starts from its hardcoded default and
    // falls through to the manifest in `resolve`.
    let effective_traced: Option<std::borrow::Cow<'_, TracedArgs>> = match &args.traced {
        Some(setting) if setting.is_enabled() => Some(setting.settings()),
        Some(_) => None,
        None if traced_defaults.traced == Some(true) => {
            Some(std::borrow::Cow::Owned(TracedArgs::default()))
        }
        None => None,
    };
    let keywords = crate::keyword_docs::collect_attr_keywords(&meta);

    match syn::parse2::<syn::Item>(input)? {
        syn::Item::Enum(item_enum) => expand_enum(
            &args,
            effective_traced.as_deref(),
            &traced_defaults,
            &manifest_err,
            &keywords,
            item_enum,
        ),
        syn::Item::Struct(item_struct) => expand_struct(
            &args,
            effective_traced.as_deref(),
            &traced_defaults,
            &manifest_err,
            &keywords,
            item_struct,
        ),
        other => Err(syn::Error::new_spanned(
            other,
            "`#[oopsie]` can only be applied to enums or structs",
        )),
    }
}

fn expand_enum(
    args: &OopsieAttrArgs,
    traced: Option<&TracedArgs>,
    defaults: &TracedDefaults,
    manifest_err: &TokenStream2,
    keywords: &(Vec<syn::Ident>, Vec<syn::Ident>),
    mut item: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let span = item.span();
    let oopsie_path: syn::Path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    // Step 1: inject diagnostic fields in place if requested.
    if let Some(traced) = traced {
        crate::traced::expand_enum::expand_enum(traced, defaults, &oopsie_path, span, &mut item)?;
    }

    // Step 2: generate Oopsie impls from the injected item. The derive layer
    // needs the helper attrs still present, so it reads a copy taken before the
    // strip below.
    let derive_input = syn::DeriveInput::from(item.clone());
    let mut container_attrs = derive::parse::EnumContainerAttrs::from_attrs(&derive_input.attrs)?;
    // The macro-level `path` governs every generated impl; an explicit
    // container-attr `path` still wins.
    if container_attrs.inner.path.is_none() {
        container_attrs.inner.path.clone_from(&args.path);
    }
    let impls = derive::expand_enum(&derive_input, &container_attrs, traced.is_some())?;

    let (attr_kws, traced_kws) = keywords;
    let keyword_docs = crate::keyword_docs::gen_use_block(
        &container_attrs.oopsie_path(),
        &[("attr", attr_kws), ("traced", traced_kws)],
    );

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    fix_derives(&mut item.attrs, args.debug.is_enabled());
    strip_oopsie_attrs(&mut item.attrs);
    for variant in &mut item.variants {
        strip_oopsie_attrs(&mut variant.attrs);
        for field in &mut variant.fields {
            strip_oopsie_attrs(&mut field.attrs);
        }
    }

    Ok(quote! {
        #item
        #impls
        #keyword_docs
        #manifest_err
    })
}

fn expand_struct(
    args: &OopsieAttrArgs,
    traced: Option<&TracedArgs>,
    defaults: &TracedDefaults,
    manifest_err: &TokenStream2,
    keywords: &(Vec<syn::Ident>, Vec<syn::Ident>),
    mut item: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let span = item.span();
    let oopsie_path: syn::Path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    // Step 1: inject diagnostic fields in place if requested.
    if let Some(traced) = traced {
        crate::traced::expand_struct::expand_struct(
            traced,
            defaults,
            &oopsie_path,
            span,
            &mut item,
        )?;
    }

    // Step 2: generate Oopsie impls from the injected item. The derive layer
    // needs the helper attrs still present, so it reads a copy taken before the
    // strip below.
    let derive_input = syn::DeriveInput::from(item.clone());
    let mut container_attrs = derive::parse::StructAttrs::from_attrs(&derive_input.attrs)?;
    // The macro-level `path` governs every generated impl; an explicit
    // container-attr `path` still wins.
    if container_attrs.container.path.is_none() {
        container_attrs.container.path.clone_from(&args.path);
    }
    let impls = derive::expand_struct(&derive_input, &container_attrs)?;

    let (attr_kws, traced_kws) = keywords;
    let keyword_docs = crate::keyword_docs::gen_use_block(
        &container_attrs.container.oopsie_path(),
        &[("attr", attr_kws), ("traced", traced_kws)],
    );

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    fix_derives(&mut item.attrs, args.debug.is_enabled());
    strip_oopsie_attrs(&mut item.attrs);
    for field in &mut item.fields {
        strip_oopsie_attrs(&mut field.attrs);
    }

    Ok(quote! {
        #item
        #impls
        #keyword_docs
        #manifest_err
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
    fn bare_enum() {
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
    fn bare_struct() {
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

    #[cfg(not(feature = "unstable-error-generic-member-access"))]
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
    fn top_level_code_points_at_nested_form() {
        let err = expand(quote! { code = false }, quote! { pub struct S { x: u32 } }).unwrap_err();
        assert!(err.to_string().contains("traced(code = false)"), "{err}");
    }

    #[test]
    fn top_level_location_points_at_nested_form() {
        let err = expand(quote! { location }, quote! { pub struct S { x: u32 } }).unwrap_err();
        assert!(err.to_string().contains("traced(location)"), "{err}");
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
        // The keyword-doc `use` block must resolve through the same effective
        // path as the impls, or it becomes an unresolvable import.
        assert!(
            out.contains("attr_oopsie :: __private :: documented :: attr :: path"),
            "{out}"
        );
        assert!(!out.contains("macro_oopsie :: __private"), "{out}");
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
}
