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
use syn::spanned::Spanned as _;

use crate::derive;
use crate::traced::args::TracedArgs;

pub fn expand(attrs: TokenStream2, input: TokenStream2) -> syn::Result<TokenStream2> {
    let meta = NestedMeta::parse_meta_list(attrs)?;

    // Separate the `traced` shorthand flag from the rest of the args.
    // `traced` is not in `TracedArgs` — it's a convenience alias for default
    // backtrace+spantrace. The remaining args are forwarded directly to `TracedArgs`.
    let mut traced_flag = false;
    let mut remaining: Vec<NestedMeta> = Vec::new();
    for m in meta {
        if matches!(&m, NestedMeta::Meta(syn::Meta::Path(p)) if p.is_ident("traced")) {
            traced_flag = true;
        } else {
            remaining.push(m);
        }
    }

    let trace_args = TracedArgs::from_list(&remaining)?;

    let needs_tracing = traced_flag
        || trace_args.backtrace.is_some()
        || trace_args.spantrace.is_some()
        || trace_args.timestamp.is_some();

    match syn::parse2::<syn::Item>(input)? {
        syn::Item::Enum(item_enum) => expand_enum(&trace_args, needs_tracing, item_enum),
        syn::Item::Struct(item_struct) => expand_struct(&trace_args, needs_tracing, item_struct),
        other => Err(syn::Error::new_spanned(
            other,
            "`#[oopsie]` can only be applied to enums or structs",
        )),
    }
}

fn expand_enum(
    trace_args: &TracedArgs,
    needs_tracing: bool,
    item: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let span = item.span();

    // Step 1: inject diagnostic fields if requested.
    let injected_ts = if needs_tracing {
        crate::traced::expand_enum::expand_enum(trace_args, span, item)?
    } else {
        quote! { #item }
    };

    // Step 2: generate Oopsie impls from the (possibly modified) item.
    let derive_input: syn::DeriveInput = syn::parse2(injected_ts.clone())?;
    let container_attrs = derive::parse::ContainerAttrs::from_attrs(&derive_input.attrs)?;
    let impls = derive::expand_enum(&derive_input, &container_attrs)?;

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    let mut out_item: syn::ItemEnum = syn::parse2(injected_ts)?;
    fix_derives(&mut out_item.attrs);
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
    trace_args: &TracedArgs,
    needs_tracing: bool,
    item: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let span = item.span();

    // Step 1: inject diagnostic fields if requested.
    let injected_ts = if needs_tracing {
        crate::traced::expand_struct::expand_struct(trace_args, span, item)?
    } else {
        quote! { #item }
    };

    // Step 2: generate Oopsie impls from the (possibly modified) item.
    let derive_input: syn::DeriveInput = syn::parse2(injected_ts.clone())?;
    let container_attrs = derive::parse::ContainerAttrs::from_attrs(&derive_input.attrs)?;
    let impls = derive::expand_struct(&derive_input, &container_attrs)?;

    // Step 3: emit the item with Debug added, Oopsie removed from derives,
    // and all #[oopsie(...)] helper attrs stripped (they've been consumed).
    let mut out_item: syn::ItemStruct = syn::parse2(injected_ts)?;
    fix_derives(&mut out_item.attrs);
    strip_oopsie_attrs(&mut out_item.attrs);
    for field in &mut out_item.fields {
        strip_oopsie_attrs(&mut field.attrs);
    }

    Ok(quote! {
        #out_item
        #impls
    })
}

/// Adjust the `#[derive(...)]` attributes on an item:
/// - Remove `Oopsie` (the attr macro handles code generation itself).
/// - Ensure `Debug` is present (required by `std::error::Error`).
fn fix_derives(attrs: &mut Vec<syn::Attribute>) {
    let mut has_debug = false;
    let mut new_attrs: Vec<syn::Attribute> = Vec::with_capacity(attrs.len() + 1);

    for attr in attrs.drain(..) {
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
            .filter(|p| !p.is_ident("Oopsie"))
            .collect();

        has_debug = has_debug || filtered.iter().any(|p| p.is_ident("Debug"));

        if !filtered.is_empty() {
            new_attrs.push(syn::parse_quote! { #[derive(#(#filtered),*)] });
        }
    }

    if !has_debug {
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

    #[cfg(not(feature = "unstable-error-generic-member-access"))]
    #[test]
    fn backtrace_only_struct() {
        let result = expand(
            quote! { backtrace },
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
    fn rejects_union() {
        let result = expand(
            quote! {},
            quote! {
                pub union Foo { x: i32, y: f32 }
            },
        );
        result.unwrap_err();
    }
}
