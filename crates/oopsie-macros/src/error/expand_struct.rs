//! Struct expansion for `#[oopsie]`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse_quote;

use super::args::ErrorArgs;
use super::config::{FieldInjectorConfig, FieldsToInject};
use super::inject::{
    add_provide_attrs, check_existing_fields, has_oopsie_name_value, inject_fields,
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

    // 4. Check if user specified `code = "..."` in #[oopsie(...)] to suppress auto-code
    let has_user_code = has_oopsie_name_value(&input.attrs, "code");

    // 5. Add struct-level provide attrs (no variant name for structs)
    add_provide_attrs(
        &mut input.attrs,
        &config,
        args,
        &struct_name,
        None,
        to_inject.backtrace,
        to_inject.spantrace,
        has_user_code,
    );

    // 6. Add visibility, suffix, and path
    input
        .attrs
        .push(parse_quote! { #[oopsie(vis = pub(crate))] });
    input.attrs.push(parse_quote! { #[oopsie(suffix)] });
    if let Some(path) = &args.path {
        let path_str = quote::quote!(#path).to_string();
        input
            .attrs
            .push(parse_quote! { #[oopsie(path = #path_str)] });
    }

    // 7. Forward display format if specified in macro args
    if let Some(display) = &args.display {
        let display_lit = syn::LitStr::new(display, proc_macro2::Span::call_site());
        input.attrs.push(parse_quote! { #[oopsie(#display_lit)] });
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
