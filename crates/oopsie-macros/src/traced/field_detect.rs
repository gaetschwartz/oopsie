//! Field detection helpers for backtrace/spantrace types.

/// If `ty` is `Box<T>`, return the inner type `T`.
pub fn extract_boxed_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident != "Box" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(inner_ty) => Some(inner_ty),
        _ => None,
    })
}

pub(super) fn is_backtrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "Backtrace") || is_boxed_ident_type(ty, "Backtrace")
}

pub(super) fn is_spantrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "SpanTrace") || is_boxed_ident_type(ty, "SpanTrace")
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

    // extract_boxed_inner tests

    #[test]
    fn extract_boxed_inner_simple() {
        let ty: syn::Type = parse_quote!(Box<io::Error>);
        let inner = extract_boxed_inner(&ty).unwrap();
        let expected: syn::Type = parse_quote!(io::Error);
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

    #[test]
    fn extract_boxed_inner_plain_type() {
        let ty: syn::Type = parse_quote!(io::Error);
        assert!(extract_boxed_inner(&ty).is_none());
    }

    #[test]
    fn extract_boxed_inner_not_box() {
        let ty: syn::Type = parse_quote!(Vec<io::Error>);
        assert!(extract_boxed_inner(&ty).is_none());
    }

    #[test]
    fn extract_boxed_inner_reference() {
        let ty: syn::Type = parse_quote!(&str);
        assert!(extract_boxed_inner(&ty).is_none());
    }

    #[test]
    fn extract_boxed_inner_simple_type() {
        let ty: syn::Type = parse_quote!(Box<String>);
        let inner = extract_boxed_inner(&ty).unwrap();
        let expected: syn::Type = parse_quote!(String);
        assert_eq!(
            quote::quote!(#inner).to_string(),
            quote::quote!(#expected).to_string()
        );
    }

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
        let ty: syn::Type = parse_quote!(SpanTrace);
        assert!(is_spantrace_type(&ty));
    }

    #[test]
    fn spantrace_type_boxed() {
        let ty: syn::Type = parse_quote!(Box<SpanTrace>);
        assert!(is_spantrace_type(&ty));
    }

    #[test]
    fn spantrace_type_qualified() {
        let ty: syn::Type = parse_quote!(tracing_error::SpanTrace);
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
