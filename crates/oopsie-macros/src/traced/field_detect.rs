//! Field detection helpers for backtrace/spantrace types.

/// Peel `Type::Paren` / `Type::Group` wrappers to reach the underlying type.
/// syn parses a parenthesized trait object (`(dyn Error + Send)`) as
/// `Type::Paren(Type::TraitObject)`, which would otherwise slip past a bare
/// `matches!(_, Type::TraitObject(_))` guard.
pub fn peel_groups(ty: &syn::Type) -> &syn::Type {
    let mut ty = ty;
    loop {
        match ty {
            syn::Type::Paren(p) => ty = &p.elem,
            syn::Type::Group(g) => ty = &g.elem,
            _ => return ty,
        }
    }
}

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

// Trace fields are classified (in `parse.rs`) and detected as already-present
// (in `check_existing_fields`) by these predicates alone: a field is a trace
// field iff it carries an explicit `#[oopsie(backtrace|spantrace|traces)]`
// attribute or its type matches here. Matching is on the type's last path
// segment only — a proc-macro has no type information, so re-exports
// (`oopsie::Backtrace`, `oopsie_core::Backtrace`) all match, an opaque alias
// (`type Bt = Backtrace; field: Bt`) does not, and an unrelated user type whose
// final segment is `Backtrace` is a false positive. The explicit attribute is
// the escape hatch for the alias case.
pub fn is_backtrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "Backtrace") || is_boxed_ident_type(ty, "Backtrace")
}

pub fn is_spantrace_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "SpanTrace") || is_boxed_ident_type(ty, "SpanTrace")
}

/// `true` for a 2-tuple `(Backtrace, SpanTrace)` or `Box<(Backtrace,
/// SpanTrace)>` — the packed trace field shape. Element order is fixed:
/// backtrace first, spantrace second. Like `is_backtrace_type`, this is a
/// best-effort last-segment match and cannot resolve paths.
pub fn is_traces_type(ty: &syn::Type) -> bool {
    // Unwrap one optional `Box<...>` layer, then require a 2-tuple whose
    // elements are `Backtrace` and `SpanTrace` by last path segment.
    let inner = extract_boxed_inner(ty).unwrap_or(ty);
    let syn::Type::Tuple(tuple) = inner else {
        return false;
    };
    let mut elems = tuple.elems.iter();
    let (Some(a), Some(b), None) = (elems.next(), elems.next(), elems.next()) else {
        return false;
    };
    is_ident_type(a, "Backtrace") && is_ident_type(b, "SpanTrace")
}

/// `true` for a type whose last path segment is `SystemTime` or `DateTime` —
/// the injectable timestamp shapes. Same last-segment heuristic (and the same
/// alias/false-positive trade-offs) as `is_backtrace_type`.
pub fn is_timestamp_type(ty: &syn::Type) -> bool {
    is_ident_type(ty, "SystemTime") || is_ident_type(ty, "DateTime")
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

    // is_traces_type tests

    #[test]
    fn traces_type_inline_tuple() {
        let ty: syn::Type = parse_quote!((Backtrace, SpanTrace));
        assert!(is_traces_type(&ty));
    }

    #[test]
    fn traces_type_boxed_tuple() {
        let ty: syn::Type = parse_quote!(Box<(Backtrace, SpanTrace)>);
        assert!(is_traces_type(&ty));
    }

    #[test]
    fn traces_type_qualified_elements() {
        let ty: syn::Type = parse_quote!((oopsie::Backtrace, tracing_error::SpanTrace));
        assert!(is_traces_type(&ty));
    }

    #[test]
    fn traces_type_wrong_arity() {
        let ty: syn::Type = parse_quote!((Backtrace, SpanTrace, u32));
        assert!(!is_traces_type(&ty));
    }

    #[test]
    fn traces_type_wrong_elements() {
        let ty: syn::Type = parse_quote!((String, SpanTrace));
        assert!(!is_traces_type(&ty));
    }

    #[test]
    fn traces_type_not_tuple() {
        let ty: syn::Type = parse_quote!(Backtrace);
        assert!(!is_traces_type(&ty));
    }

    // peel_groups tests

    #[test]
    fn peel_groups_unwraps_parenthesized_trait_object() {
        let ty: syn::Type = parse_quote!((dyn Error + Send));
        assert!(matches!(peel_groups(&ty), syn::Type::TraitObject(_)));
    }

    #[test]
    fn peel_groups_passes_through_plain_type() {
        let ty: syn::Type = parse_quote!(io::Error);
        assert!(matches!(peel_groups(&ty), syn::Type::Path(_)));
    }
}
