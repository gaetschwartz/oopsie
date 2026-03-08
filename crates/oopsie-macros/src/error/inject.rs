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

/// Extract and remove a named attribute (e.g. `#[help("...")]`) from an attribute list.
///
/// Returns the string literal value if found, or an error if the attribute
/// is present but malformed.
pub(super) fn extract_and_strip_attr(
    attrs: &mut Vec<syn::Attribute>,
    name: &str,
) -> syn::Result<Option<syn::LitStr>> {
    let pos = attrs.iter().position(|a| a.path().is_ident(name));
    let Some(pos) = pos else {
        return Ok(None);
    };
    let attr = attrs.remove(pos);
    let lit: syn::LitStr = attr.parse_args().map_err(|e| {
        syn::Error::new(
            e.span(),
            format!("expected a string literal, e.g. #[{name}(\"...\")]"),
        )
    })?;
    Ok(Some(lit))
}

/// Add Oopsie provide attributes for backtrace, spantrace, error code, and help text.
#[expect(clippy::too_many_arguments)]
pub(super) fn add_provide_attrs(
    attrs: &mut Vec<syn::Attribute>,
    config: &FieldInjectorConfig,
    args: &ErrorArgs,
    type_name: &str,
    variant_name: Option<&str>,
    added_backtrace: bool,
    added_spantrace: bool,
    help_text: Option<&syn::LitStr>,
    code_override: Option<&syn::LitStr>,
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

    if args.code.is_enabled() {
        if let Some(code_lit) = code_override {
            let attr =
                parse_quote! { #[oopsie(provide(#code_type => #code_type::from(#code_lit)))] };
            attrs.push(attr);
        } else {
            let mut name = type_name.to_owned();
            if let Some(v) = variant_name {
                name.push_str("::");
                name.push_str(v);
            }
            let attr = parse_quote! { #[oopsie(provide(#code_type => #code_type::from(concat!(module_path!(), "::", #name))))] };
            attrs.push(attr);
        }
    }

    if let Some(help_lit) = help_text {
        attrs.push(
            parse_quote! { #[oopsie(provide(#magnetite_utils_path::HelpText => #magnetite_utils_path::HelpText(#help_lit)))] },
        );
    }
}
