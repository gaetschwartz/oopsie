//! Struct expansion for `#[oopsie]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::ErrorArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, extract_and_strip_attr, inject_fields,
};
use super::snafu_attrs::extract_snafu_attr;
use super::type_check::check_derive_snafu;

pub(super) fn expand_struct(
    args: &ErrorArgs,
    _args_span: Span,
    mut input: syn::ItemStruct,
) -> syn::Result<TokenStream2> {
    let struct_name = input.ident.to_string();
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

    // module attr is not applicable for structs (no variants to generate context selectors for)
    if let Some(module) = &snafu_attr.module {
        return Err(syn::Error::new(
            module.span,
            "`#[snafu(module)]` is not applicable for struct errors",
        ));
    }

    // 3. Build config
    let magnetite_utils_path_for_impl = magnetite_utils_path.clone();
    let config = FieldInjectorConfig::new(args, magnetite_utils_path);

    // 4. Check existing fields and inject
    let existence = check_existing_fields(&input.fields, &config.timestamp_type);
    let to_inject = FieldsToInject {
        backtrace: args.backtrace.is_enabled() && !existence.has_backtrace,
        spantrace: args.spantrace.is_enabled() && !existence.has_spantrace,
        timestamp: args.timestamp.is_enabled() && !existence.has_timestamp,
    };

    inject_fields(&mut input.fields, &config, &to_inject)?;

    // 5. Extract #[help("...")] and #[code("...")] before passing to add_provide_attrs
    let help_text = extract_and_strip_attr(&mut input.attrs, "help")?;
    let code_override = extract_and_strip_attr(&mut input.attrs, "code")?;

    // 6. Add struct-level provide attrs (no variant name for structs)
    add_provide_attrs(
        &mut input.attrs,
        &config,
        args,
        &struct_name,
        None,
        to_inject.backtrace,
        to_inject.spantrace,
        help_text.as_ref(),
        code_override.as_ref(),
    );

    // 7. Add visibility if not specified
    if snafu_attr.visibility.is_none() {
        input
            .attrs
            .push(parse_quote! { #[snafu(visibility(pub(crate)))] });
    }

    // 8. Generate LowerExp impl for fancy error reporting via {:e} format
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
