//! Field injection helpers.

use syn::punctuated::Punctuated;
use syn::{Fields, FieldsNamed, parse_quote, token};

use super::args::ErrorArgs;
use super::config::{FieldExistence, FieldInjectorConfig, FieldsToInject};
use super::type_check::{is_backtrace_type, is_spantrace_type};

/// Check which fields already exist in a `Fields` collection.
pub(super) fn check_existing_fields(fields: &Fields, timestamp_type: &syn::Type) -> FieldExistence {
    let mut existence = FieldExistence::default();

    let iter: Box<dyn Iterator<Item = &syn::Field>> = match fields {
        Fields::Named(f) => Box::new(f.named.iter()),
        Fields::Unnamed(f) => Box::new(f.unnamed.iter()),
        Fields::Unit => return existence,
    };

    for field in iter {
        if is_backtrace_type(&field.ty) {
            existence.has_backtrace = true;
        }
        if is_spantrace_type(&field.ty) {
            existence.has_spantrace = true;
        }
        if field.ty == *timestamp_type {
            existence.has_timestamp = true;
        }
    }
    existence
}

/// Inject backtrace/spantrace/timestamp fields into a `Fields` collection.
pub(super) fn inject_fields(
    fields: &mut Fields,
    config: &FieldInjectorConfig,
    to_inject: &FieldsToInject,
) -> syn::Result<()> {
    match fields {
        Fields::Named(named) => {
            inject_into_named(named, config, to_inject);
            Ok(())
        }
        Fields::Unit => {
            let mut named = FieldsNamed {
                brace_token: token::Brace::default(),
                named: Punctuated::default(),
            };
            inject_into_named(&mut named, config, to_inject);
            *fields = Fields::Named(named);
            Ok(())
        }
        Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
            unnamed,
            "`#[oopsie]` does not support tuple variants/structs; use named fields instead",
        )),
    }
}

fn inject_into_named(
    fields: &mut FieldsNamed,
    config: &FieldInjectorConfig,
    to_inject: &FieldsToInject,
) {
    let FieldInjectorConfig {
        backtrace_ident,
        backtrace_type,
        backtrace_attrs,
        spantrace_ident,
        spantrace_type,
        spantrace_attrs,
        timestamp_ident,
        timestamp_type,
        timestamp_provide_attr,
        ..
    } = config;

    if to_inject.backtrace {
        fields
            .named
            .push(parse_quote! { #backtrace_attrs #backtrace_ident: #backtrace_type });
    }
    if to_inject.spantrace {
        fields
            .named
            .push(parse_quote! { #spantrace_attrs #spantrace_ident: #spantrace_type });
    }
    if to_inject.timestamp {
        fields
            .named
            .push(parse_quote! { #timestamp_provide_attr #timestamp_ident: #timestamp_type });
    }
}

/// Check whether any `#[oopsie(...)]` attribute on this item contains a
/// name-value entry like `code = "..."` or `help = "..."`.
pub(super) fn has_oopsie_name_value(attrs: &[syn::Attribute], key: &str) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("oopsie") {
            continue;
        }
        let Ok(tokens) = attr.parse_args::<proc_macro2::TokenStream>() else {
            continue;
        };
        let mut iter = tokens.into_iter().peekable();
        while let Some(tok) = iter.next() {
            if let proc_macro2::TokenTree::Ident(ident) = &tok
                && ident == key
                && let Some(proc_macro2::TokenTree::Punct(p)) = iter.peek()
                && p.as_char() == '='
            {
                return true;
            }
        }
    }
    false
}

/// Add Oopsie provide attributes for backtrace, spantrace, and auto-generated error code.
///
/// Help text and user-specified error codes are handled by the derive macro
/// via `#[oopsie(help = "...", code = "...")]` on variants/structs.
#[expect(clippy::too_many_arguments)]
pub(super) fn add_provide_attrs(
    attrs: &mut Vec<syn::Attribute>,
    config: &FieldInjectorConfig,
    args: &ErrorArgs,
    type_name: &str,
    variant_name: Option<&str>,
    added_backtrace: bool,
    added_spantrace: bool,
    has_user_code: bool,
) {
    let FieldInjectorConfig {
        backtrace_ident,
        spantrace_ident,
        code_type,
        magnetite_utils_path,
        ..
    } = config;

    if added_backtrace {
        attrs.push(
            parse_quote! { #[oopsie(provide(ref, #magnetite_utils_path::Backtrace => #backtrace_ident.as_ref()))] },
        );
    }

    if added_spantrace {
        attrs.push(
            parse_quote! { #[oopsie(provide(ref, #magnetite_utils_path::Spantrace => #spantrace_ident.as_ref()))] },
        );
    }

    // Only generate auto-code from module_path!() when the code feature is enabled
    // AND the user did not specify their own `code = "..."` on the variant/struct.
    if args.code.is_enabled() && !has_user_code {
        let mut name = type_name.to_owned();
        if let Some(v) = variant_name {
            name.push_str("::");
            name.push_str(v);
        }
        let attr = parse_quote! { #[oopsie(provide(#code_type => #code_type::from(concat!(module_path!(), "::", #name))))] };
        attrs.push(attr);
    }
}
