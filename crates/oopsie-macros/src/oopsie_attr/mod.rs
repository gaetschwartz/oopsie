//! The `#[oopsie]` attribute macro implementation.
//!
//! This is the ergonomic primary path for defining error types. It handles:
//! - Generating all Oopsie impls (same codegen as `#[derive(Oopsie)]`)
//! - Injecting diagnostic fields when tracing options are requested
//! - Generating `Debug` automatically (no need to write `#[derive(Debug)]`)

#![allow(clippy::ref_patterns, reason = "darling's FromMeta derive emits them")]

use darling::FromMeta as _;
use darling::ast::NestedMeta;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse_quote;
use syn::spanned::Spanned as _;

use crate::derive;
use crate::traced::args::TracedArgs;
use crate::utils::FieldSetting;

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
        NestedMeta::Lit(_) | NestedMeta::Meta(_) | NestedMeta::NameValueInvalidExpr(_) => None,
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
    // Spans the coherence errors `validate` raises (e.g. packed/boxed conflicts) at the
    // `traced(...)` meta itself, falling back to the item when tracing comes from the
    // manifest default and there's no attribute token to point at.
    let traced_span = meta.iter().find_map(|m| match m {
        NestedMeta::Meta(inner) if inner.path().is_ident("traced") => Some(inner.span()),
        NestedMeta::Meta(_) | NestedMeta::Lit(_) | NestedMeta::NameValueInvalidExpr(_) => None,
    });
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
    let (attr_keywords, traced_keywords) = crate::keyword_docs::collect_attr_keywords(&meta);
    let traced = effective_traced.as_deref();
    let oopsie_path: syn::Path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    let mut item = syn::parse2::<syn::Item>(input)?;
    // The derive resolves through the same crate path as the impls it
    // generates; an unparsable container attr falls back and is reported by the
    // derive.
    let container_path = match &item {
        syn::Item::Enum(e) => derive::parse::EnumContainerAttrs::from_attrs(&e.attrs)
            .ok()
            .and_then(|c| c.inner.path),
        syn::Item::Struct(s) => derive::parse::StructAttrs::from_attrs(&s.attrs)
            .ok()
            .and_then(|c| c.container.path),
        _ => None,
    };
    let derive_path = container_path.as_ref().unwrap_or(&oopsie_path);
    let item_attrs = match &mut item {
        syn::Item::Enum(item_enum) => {
            if let Some(traced) = traced {
                let span = traced_span.unwrap_or_else(|| item_enum.span());
                crate::traced::expand_enum::expand_enum(
                    traced,
                    &traced_defaults,
                    &oopsie_path,
                    span,
                    item_enum,
                )?;
            }
            &mut item_enum.attrs
        }
        syn::Item::Struct(item_struct) => {
            if let Some(traced) = traced {
                let span = traced_span.unwrap_or_else(|| item_struct.span());
                crate::traced::expand_struct::expand_struct(
                    traced,
                    &traced_defaults,
                    &oopsie_path,
                    span,
                    item_struct,
                )?;
            }
            &mut item_struct.attrs
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "`#[oopsie]` can only be applied to enums or structs",
            ));
        }
    };

    fix_derives(item_attrs, args.debug.is_enabled());
    let marker = Marker {
        tracing_active: traced.is_some(),
        path: args.path.clone(),
        attr_keywords,
        traced_keywords,
    };
    // The derive must come first: a helper attribute ahead of the derive that
    // registers it trips `legacy_derive_helpers`.
    item_attrs.splice(
        0..0,
        [
            parse_quote! { #[derive(#derive_path::__private::OopsieAttrImpl)] },
            marker.to_attr(),
        ],
    );

    Ok(quote! {
        #item
        #manifest_err
    })
}

const MARKER: &str = "__oopsie_attr";

/// What the attribute knows that the item alone doesn't, handed to
/// [`expand_impls`] through a `#[__oopsie_attr(...)]` helper attribute.
struct Marker {
    tracing_active: bool,
    path: Option<syn::Path>,
    attr_keywords: Vec<syn::Ident>,
    traced_keywords: Vec<syn::Ident>,
}

impl Marker {
    fn to_attr(&self) -> syn::Attribute {
        let tracing_active = self.tracing_active.then(|| quote! { tracing_active, });
        let path = self.path.as_ref().map(|p| quote! { path(#p), });
        let attr_keywords = &self.attr_keywords;
        let traced_keywords = &self.traced_keywords;
        parse_quote! {
            #[__oopsie_attr(
                #tracing_active
                #path
                attr_keywords(#(#attr_keywords),*),
                traced_keywords(#(#traced_keywords),*)
            )]
        }
    }

    fn from_attr(attr: &syn::Attribute) -> syn::Result<Self> {
        use syn::ext::IdentExt as _;
        let idents = |list: &syn::MetaList| -> syn::Result<Vec<syn::Ident>> {
            list.parse_args_with(|input: syn::parse::ParseStream<'_>| {
                syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated_with(
                    input,
                    syn::Ident::parse_any,
                )
            })
            .map(|p| p.into_iter().collect())
        };
        let mut marker = Self {
            tracing_active: false,
            path: None,
            attr_keywords: Vec::new(),
            traced_keywords: Vec::new(),
        };
        let metas = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        )?;
        for meta in &metas {
            match meta {
                syn::Meta::Path(p) if p.is_ident("tracing_active") => marker.tracing_active = true,
                syn::Meta::List(l) if l.path.is_ident("path") => {
                    marker.path = Some(l.parse_args()?);
                }
                syn::Meta::List(l) if l.path.is_ident("attr_keywords") => {
                    marker.attr_keywords = idents(l)?;
                }
                syn::Meta::List(l) if l.path.is_ident("traced_keywords") => {
                    marker.traced_keywords = idents(l)?;
                }
                syn::Meta::Path(_) | syn::Meta::List(_) | syn::Meta::NameValue(_) => {
                    return Err(syn::Error::new_spanned(meta, "unknown `__oopsie_attr` key"));
                }
            }
        }
        Ok(marker)
    }
}

/// Generate the impls for an item rewritten by [`expand`].
pub fn expand_impls(input: TokenStream2) -> syn::Result<TokenStream2> {
    let mut input: syn::DeriveInput = syn::parse2(input)?;
    let marker_idx = input
        .attrs
        .iter()
        .position(|a| a.path().is_ident(MARKER))
        .ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "`OopsieAttrImpl` is an implementation detail of `#[oopsie::oopsie]`; \
                 use `#[oopsie::oopsie]` or `#[derive(Oopsie)]` instead",
            )
        })?;
    let marker = Marker::from_attr(&input.attrs.remove(marker_idx))?;

    let (impls, path) = match &input.data {
        syn::Data::Enum(_) => {
            let mut container = derive::parse::EnumContainerAttrs::from_attrs(&input.attrs)?;
            // An explicit container-attr `path` wins over the macro-level one.
            if container.inner.path.is_none() {
                container.inner.path.clone_from(&marker.path);
            }
            let impls = derive::expand_enum(&input, &container, marker.tracing_active)?;
            (impls, container.oopsie_path())
        }
        syn::Data::Struct(_) => {
            let mut container = derive::parse::StructAttrs::from_attrs(&input.attrs)?;
            if container.container.path.is_none() {
                container.container.path.clone_from(&marker.path);
            }
            let impls = derive::expand_struct(&input, &container)?;
            (impls, container.container.oopsie_path())
        }
        syn::Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`#[oopsie]` can only be applied to enums or structs",
            ));
        }
    };

    let keyword_docs = crate::keyword_docs::gen_use_block(
        &path,
        &[
            ("attr", &marker.attr_keywords),
            ("traced", &marker.traced_keywords),
        ],
    );

    Ok(quote! {
        #impls
        #keyword_docs
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    /// Both halves of the attribute: the rewritten item, then the impls the
    /// hidden derive generates from it (no `cfg` to evaluate in these inputs).
    fn expand_full(attrs: TokenStream2, input: TokenStream2) -> syn::Result<TokenStream2> {
        let mut file: syn::File = syn::parse2(expand(attrs, input)?)?;
        let item = file.items.remove(0);
        let impls = expand_impls(quote! { #item })?;
        let rest = file.items;
        Ok(quote! { #item #impls #(#rest)* })
    }

    // Output includes the `provide` impl when the unstable feature is on,
    // so the snapshot only matches in the default-features build.
    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn bare_enum() {
        let result = expand_full(
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
        let result = expand_full(
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
        let result = expand_full(
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
        let result = expand_full(
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
        let out = expand_full(
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
        let out = expand_full(
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

    #[test]
    fn cfg_attr_traced_false_gates_injection_on_the_predicate() {
        let output = expand(
            quote! { traced },
            quote! {
                pub enum AppError {
                    #[cfg_attr(feature = "quiet", oopsie(traced = false))]
                    #[oopsie("boom")]
                    Boom { info: String },
                }
            },
        )
        .unwrap()
        .to_string();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn cfg_gated_trace_field_gets_a_mirror() {
        let output = expand(
            quote! { traced },
            quote! {
                pub struct S {
                    info: String,
                    #[cfg(feature = "bt")]
                    backtrace: Backtrace,
                }
            },
        )
        .unwrap()
        .to_string();
        insta::assert_snapshot!(output);
    }
}
