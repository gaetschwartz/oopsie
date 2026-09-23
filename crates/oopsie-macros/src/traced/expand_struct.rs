//! Struct trace-field injection for `#[oopsie(traced)]`.

use proc_macro2::Span;
use syn::ext::IdentExt as _;

use super::args::TracedArgs;
use super::config::FieldInjectorConfig;
use super::inject::{ItemFacts, add_provide_attrs, inject_fields, plan_injection};
use crate::derive::parse::StructAttrs;
use crate::utils::TracedDefaults;

pub fn expand_struct(
    args: &TracedArgs,
    defaults: &TracedDefaults,
    oopsie_path: &syn::Path,
    args_span: Span,
    input: &mut syn::ItemStruct,
) -> syn::Result<()> {
    let struct_name = input.ident.unraw().to_string();

    let resolved = args.resolve(defaults);
    resolved.validate(args_span)?;
    let config = FieldInjectorConfig::new(&resolved, args.code.inner(), oopsie_path);

    // Auto-code suppression facts come from the same parser the derive layer
    // uses, so injection and codegen can't disagree on what counts as a user
    // `code` or a `transparent` struct.
    let (to_inject, auto_code) = plan_injection(
        &resolved,
        &config,
        &input.attrs,
        &input.fields,
        input.ident.span(),
        |attrs| {
            let attrs = StructAttrs::from_attrs(attrs)?;
            Ok(ItemFacts {
                traced: true,
                has_user_code: attrs.code.is_some(),
                transparent: attrs.transparent,
            })
        },
    )?;

    inject_fields(&mut input.fields, &config, &to_inject)?;
    add_provide_attrs(&mut input.attrs, &config, &struct_name, None, &auto_code);

    Ok(())
}
