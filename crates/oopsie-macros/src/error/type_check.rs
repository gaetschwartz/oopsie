//! Type checking helpers.

use darling::FromMeta as _;
use darling::util::PathList;

/// Verify that the item derives `Snafu`.
pub(super) fn check_derive_snafu(attrs: &[syn::Attribute]) -> syn::Result<()> {
    let has_snafu = attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && PathList::from_meta(&attr.meta).is_ok_and(|p| {
                p.iter()
                    .any(|p| p.segments.last().is_some_and(|s| s.ident == "Snafu"))
            })
    });
    if has_snafu {
        return Ok(());
    }
    let last_derive = attrs
        .iter()
        .rev()
        .find(|attr| attr.path().is_ident("derive"));
    Err(syn::Error::new_spanned(
        last_derive,
        "Types annotated with `#[oopsie]` must also derive `Snafu`",
    ))
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
