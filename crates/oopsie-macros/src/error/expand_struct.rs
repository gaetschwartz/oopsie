//! Struct expansion for `#[oopsie]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::ErrorArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, extract_and_strip_attr, inject_fields,
};
use super::type_check::ensure_derive_oopsie;

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

    // 1. Ensure #[derive(Oopsie)] is present
    ensure_derive_oopsie(&mut input.attrs);

    // 2. Build config
    let magnetite_utils_path_for_impl = magnetite_utils_path.clone();
    let config = FieldInjectorConfig::new(args, magnetite_utils_path);

    // 3. Check existing fields and inject
    let existence = check_existing_fields(&input.fields, &config.timestamp_type);
    let to_inject = FieldsToInject {
        backtrace: args.backtrace.is_enabled() && !existence.has_backtrace,
        spantrace: args.spantrace.is_enabled() && !existence.has_spantrace,
        timestamp: args.timestamp.is_enabled() && !existence.has_timestamp,
    };

    inject_fields(&mut input.fields, &config, &to_inject)?;

    // 4. Extract #[help("...")] and #[code("...")] before passing to add_provide_attrs
    let help_text = extract_and_strip_attr(&mut input.attrs, "help")?;
    let code_override = extract_and_strip_attr(&mut input.attrs, "code")?;

    // 5. Add struct-level provide attrs (no variant name for structs)
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

    // 6. Add visibility
    input
        .attrs
        .push(parse_quote! { #[oopsie(vis = pub(crate))] });

    // 7. Generate LowerExp impl for fancy error reporting via {:e} format
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
