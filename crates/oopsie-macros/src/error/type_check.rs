//! Type checking helpers.

use darling::FromMeta as _;
use darling::util::PathList;

/// Ensure that the item has `#[derive(Oopsie)]` in its derives, adding it if not present.
pub(super) fn ensure_derive_oopsie(attrs: &mut Vec<syn::Attribute>) {
    let has_oopsie = attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && PathList::from_meta(&attr.meta).is_ok_and(|p| {
                p.iter()
                    .any(|p| p.segments.last().is_some_and(|s| s.ident == "Oopsie"))
            })
    });
    if has_oopsie {
        return;
    }

    // Find an existing derive attribute to extend, or add a new one
    if let Some(derive_attr) = attrs.iter_mut().find(|attr| attr.path().is_ident("derive")) {
        // Extend existing derive with Oopsie
        if let syn::Meta::List(list) = &mut derive_attr.meta {
            let existing: proc_macro2::TokenStream = list.tokens.clone();
            list.tokens = quote::quote! { #existing, Oopsie };
        }
    } else {
        // No derive attribute at all, add one
        attrs.push(syn::parse_quote! { #[derive(Oopsie)] });
    }
}

pub(super) fn is_backtrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "Backtrace") || is_boxed_ident_type(ty, "Backtrace")
}

pub(super) fn is_spantrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "Spantrace") || is_boxed_ident_type(ty, "Spantrace")
}

fn is_ident_type(ty: &syn::Type, ident: &str) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == ident)
}

fn is_boxed_ident_type(ty: &syn::Type, ident: &str) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    let Some(last) = type_path.path.segments.last() else {
        return false;
    };
    if last.ident != "Box" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    args.args.iter().any(|arg| match arg {
        syn::GenericArgument::Type(inner_ty) => is_ident_type(inner_ty, ident),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    // is_backtrace_type tests

    #[test]
    fn backtrace_type_direct() {
        let ty: syn::Type = parse_quote!(Backtrace);
        assert!(is_backtrace_type(&ty));
    }

    #[test]
    fn backtrace_type_boxed() {
        let ty: syn::Type = parse_quote!(Box<Backtrace>);
        assert!(is_backtrace_type(&ty));
    }

    #[test]
    fn backtrace_type_qualified() {
        let ty: syn::Type = parse_quote!(oopsie_core::Backtrace);
        assert!(is_backtrace_type(&ty));
    }

    #[test]
    fn backtrace_type_negative() {
        let ty: syn::Type = parse_quote!(String);
        assert!(!is_backtrace_type(&ty));
    }

    // is_spantrace_type tests

    #[test]
    fn spantrace_type_direct() {
        let ty: syn::Type = parse_quote!(Spantrace);
        assert!(is_spantrace_type(&ty));
    }

    #[test]
    fn spantrace_type_boxed() {
        let ty: syn::Type = parse_quote!(Box<Spantrace>);
        assert!(is_spantrace_type(&ty));
    }

    #[test]
    fn spantrace_type_qualified() {
        let ty: syn::Type = parse_quote!(tracing_error::Spantrace);
        assert!(is_spantrace_type(&ty));
    }

    #[test]
    fn spantrace_type_negative() {
        let ty: syn::Type = parse_quote!(String);
        assert!(!is_spantrace_type(&ty));
    }

    // is_ident_type tests

    #[test]
    fn ident_type_matching() {
        let ty: syn::Type = parse_quote!(Backtrace);
        assert!(is_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn ident_type_not_matching() {
        let ty: syn::Type = parse_quote!(String);
        assert!(!is_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn ident_type_non_path() {
        let ty: syn::Type = parse_quote!(&str);
        assert!(!is_ident_type(&ty, "str"));
    }

    #[test]
    fn ident_type_tuple() {
        let ty: syn::Type = parse_quote!(());
        assert!(!is_ident_type(&ty, "anything"));
    }

    // is_boxed_ident_type tests

    #[test]
    fn boxed_ident_type_matching() {
        let ty: syn::Type = parse_quote!(Box<Backtrace>);
        assert!(is_boxed_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn boxed_ident_type_wrong_inner() {
        let ty: syn::Type = parse_quote!(Box<String>);
        assert!(!is_boxed_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn boxed_ident_type_not_box() {
        let ty: syn::Type = parse_quote!(Vec<Backtrace>);
        assert!(!is_boxed_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn boxed_ident_type_plain_type() {
        let ty: syn::Type = parse_quote!(Backtrace);
        assert!(!is_boxed_ident_type(&ty, "Backtrace"));
    }

    #[test]
    fn boxed_ident_type_non_path() {
        let ty: syn::Type = parse_quote!(&str);
        assert!(!is_boxed_ident_type(&ty, "str"));
    }

    #[test]
    fn boxed_ident_type_tuple() {
        let ty: syn::Type = parse_quote!(());
        assert!(!is_boxed_ident_type(&ty, "anything"));
    }
}
