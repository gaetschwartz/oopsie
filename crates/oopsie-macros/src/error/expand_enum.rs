//! Enum expansion for `#[oopsie]`.

use convert_case::{Case, Casing as _};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::ErrorArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, extract_and_strip_attr, inject_fields,
};
use super::snafu_attrs::{SnafuAttrs, extract_snafu_attr};
use super::type_check::check_derive_snafu;

pub(super) fn expand_enum(
    args: &ErrorArgs,
    _args_span: Span,
    mut input: syn::ItemEnum,
) -> syn::Result<TokenStream2> {
    let enum_name = input.ident.to_string();
    let magnetite_utils_path = args
        .path
        .clone()
        .unwrap_or_else(|| parse_quote! { ::oopsie });

    // 1. Validate derives
    check_derive_snafu(&input.attrs)?;

    // 2. Extract and validate SNAFU attributes
    let snafu_attr = match extract_snafu_attr(&input.attrs) {
        Ok(attr) => attr,
        Err(e) => {
            return Ok(e.write_errors());
        }
    };
    validate_no_snafu_module_or_context(&snafu_attr)?;

    // 3. Build config
    let magnetite_utils_path_for_impl = magnetite_utils_path.clone();
    let config = FieldInjectorConfig::new(args, magnetite_utils_path);

    // 4. Process variants
    for variant in &mut input.variants {
        let existence = check_existing_fields(&variant.fields, &config.timestamp_type);
        let to_inject = FieldsToInject {
            backtrace: args.backtrace.is_enabled() && !existence.has_backtrace,
            spantrace: args.spantrace.is_enabled() && !existence.has_spantrace,
            timestamp: args.timestamp.is_enabled() && !existence.has_timestamp,
        };

        inject_fields(&mut variant.fields, &config, &to_inject)?;

        // Extract #[help("...")] and #[code("...")] before passing to add_provide_attrs
        let help_text = extract_and_strip_attr(&mut variant.attrs, "help")?;
        let code_override = extract_and_strip_attr(&mut variant.attrs, "code")?;

        let variant_name = variant.ident.to_string();
        add_provide_attrs(
            &mut variant.attrs,
            &config,
            args,
            &enum_name,
            Some(&variant_name),
            to_inject.backtrace,
            to_inject.spantrace,
            help_text.as_ref(),
            code_override.as_ref(),
        );
    }

    // 5. Add enum-level SNAFU attributes
    apply_enum_snafu_attrs(
        &mut input.attrs,
        args,
        &snafu_attr,
        &enum_name,
        input.ident.span(),
    );

    // 6. Generate LowerExp impl for fancy error reporting via {:e} format
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    #[cfg(feature = "daisy")]
    let lower_exp_impl = quote! {
        impl #impl_generics ::std::fmt::LowerExp for #ident #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&#magnetite_utils_path_for_impl::FancyReport::from_std(self), f)
            }
        }
    };
    #[cfg(not(feature = "daisy"))]
    let lower_exp_impl = quote! {};

    Ok(quote! {
        #input
        #lower_exp_impl
    })
}

/// Validate that `#[snafu(module)]` and `#[snafu(context)]` are not used with enums.
fn validate_no_snafu_module_or_context(snafu_attr: &SnafuAttrs) -> syn::Result<()> {
    if let Some(module) = &snafu_attr.module {
        return Err(syn::Error::new(
            module.span,
            "`#[snafu(module)]` cannot be used when using #[oopsie] and should be removed. The #[oopsie] macro already manages module generation automatically.",
        ));
    }
    if let Some(context) = &snafu_attr.context {
        return Err(syn::Error::new(
            context.span(),
            "`#[snafu(context)]` cannot be used when using #[oopsie] and should be removed. The #[oopsie] macro already manages module generation automatically.",
        ));
    }
    Ok(())
}

/// Apply SNAFU attributes to an enum (module, suffix, visibility).
fn apply_enum_snafu_attrs(
    attrs: &mut Vec<syn::Attribute>,
    args: &ErrorArgs,
    snafu_attr: &SnafuAttrs,
    enum_name: &str,
    span: Span,
) {
    if args.module.is_enabled() {
        let mut module_name = enum_name.trim_end_matches("Error").to_case(Case::Snake);
        if !module_name.is_empty() {
            module_name.push('_');
        }
        module_name.push_str("snafus");
        let module_ident = syn::Ident::new(&module_name, span);
        attrs.push(parse_quote! { #[snafu(module(#module_ident))] });
    }
    if !args.no_suffix.is_enabled() {
        attrs.push(parse_quote! { #[snafu(context(suffix(false)))] });
    }
    if snafu_attr.visibility.is_none() {
        attrs.push(parse_quote! { #[snafu(visibility(pub(crate)))] });
    }
}
