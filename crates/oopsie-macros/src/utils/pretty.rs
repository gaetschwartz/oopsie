use quote::ToTokens;
use syn::{Token, punctuated::Punctuated};

pub struct PrettyType<'a>(pub &'a syn::Type);

impl std::fmt::Display for PrettyType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_type(f, self.0)
    }
}

pub trait Pretty {
    type Display<'a>: std::fmt::Display + 'a
    where
        Self: 'a;

    fn pretty(&self) -> Self::Display<'_>;
}

impl Pretty for syn::Type {
    type Display<'a> = PrettyType<'a>;

    fn pretty(&self) -> Self::Display<'_> {
        PrettyType(self)
    }
}

fn write_path_segment(
    f: &mut impl std::fmt::Write,
    segment: &syn::PathSegment,
) -> std::fmt::Result {
    write!(f, "{}", segment.ident)?;
    match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) => {
            write_abga(f, args)?;
        }
        syn::PathArguments::Parenthesized(args) => {
            write!(f, "(")?;
            write_punct(f, &args.inputs, write_named_arg)?;
            write!(f, ")")?;
            if let syn::ReturnType::Type(_, output) = &args.output {
                write!(f, " -> ")?;
                write_type(f, output)?;
            }
        }
        syn::PathArguments::None => {}
    }
    Ok(())
}

fn write_abga(
    f: &mut impl std::fmt::Write,
    args: &syn::AngleBracketedGenericArguments,
) -> Result<(), std::fmt::Error> {
    if args.colon2_token.is_some() {
        write!(f, "::")?;
    }
    write!(f, "<")?;
    write_punct(f, &args.args, write_generic_argument)?;
    write!(f, ">")?;
    Ok(())
}

fn write_generic_argument(
    f: &mut impl std::fmt::Write,
    arg: &syn::GenericArgument,
) -> std::fmt::Result {
    match arg {
        syn::GenericArgument::Lifetime(lifetime) => write_token_stream(f, lifetime),
        syn::GenericArgument::Type(ty) => write_type(f, ty),
        syn::GenericArgument::Constraint(constraint) => {
            write!(f, "{}: ", constraint.ident)?;
            write_punct(f, &constraint.bounds, make_write_token_stream())?;
            Ok(())
        }
        syn::GenericArgument::AssocType(assoc_type) => {
            write!(f, "{}", assoc_type.ident)?;
            if let Some(bounds) = &assoc_type.generics {
                write_abga(f, bounds)?;
            }
            write!(f, " = ")?;
            write_type(f, &assoc_type.ty)
        }
        e => write_token_stream(f, e),
    }
}

fn write_type(f: &mut impl std::fmt::Write, ty: &syn::Type) -> std::fmt::Result {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(qself) = &type_path.qself {
                write_type_path_with_qself(f, qself, type_path)
            } else {
                write_path(f, &type_path.path)
            }
        }
        syn::Type::Array(type_array) => {
            write!(f, "[")?;
            write_type(f, &type_array.elem)?;
            write!(f, "; {}]", type_array.len.to_token_stream())
        }
        syn::Type::Reference(type_reference) => {
            write!(f, "&")?;
            if let Some(lifetime) = &type_reference.lifetime {
                write_token_stream(f, lifetime)?;
                write!(f, " ")?;
            }
            if type_reference.mutability.is_some() {
                write!(f, "mut ")?;
            }
            write_type(f, &type_reference.elem)
        }
        syn::Type::Group(group) => write_type(f, &group.elem),
        syn::Type::Paren(type_paren) => {
            write!(f, "(")?;
            write_type(f, &type_paren.elem)?;
            write!(f, ")")
        }
        syn::Type::Ptr(type_ptr) => {
            write!(f, "*")?;
            match &type_ptr.mutability {
                syn::PointerMutability::Const(_) => write!(f, "const ")?,
                syn::PointerMutability::Mut(_) => write!(f, "mut ")?,
            }
            write_type(f, &type_ptr.elem)
        }
        syn::Type::Slice(type_slice) => {
            write!(f, "[")?;
            write_type(f, &type_slice.elem)?;
            write!(f, "]")
        }
        syn::Type::TraitObject(type_trait_object) => {
            write!(f, "dyn ")?;
            write_punct(f, &type_trait_object.bounds, make_write_token_stream())?;
            Ok(())
        }
        syn::Type::ImplTrait(impl_trait) => {
            write!(f, "impl ")?;
            write_punct(f, &impl_trait.bounds, make_write_token_stream())?;
            Ok(())
        }
        syn::Type::FnPtr(fn_ptr) => {
            if let Some(lifetimes) = &fn_ptr.lifetimes {
                write!(f, "for<")?;
                write_punct(f, &lifetimes.lifetimes, make_write_token_stream())?;
                write!(f, "> ")?;
            }
            if fn_ptr.unsafety.is_some() {
                write!(f, "unsafe ")?;
            }
            if let Some(abi) = &fn_ptr.abi {
                write_token_stream(f, abi)?;
                write!(f, " ")?;
            }
            write!(f, "fn(")?;
            write_punct(f, &fn_ptr.inputs, write_named_arg)?;
            if let Some(v) = &fn_ptr.variadic {
                if !fn_ptr.inputs.is_empty() {
                    write!(f, " ")?;
                }
                write_token_stream(f, v)?;
            }
            write!(f, ")")?;
            if let syn::ReturnType::Type(_, ty) = &fn_ptr.output {
                write!(f, " -> ")?;
                write_type(f, ty)?;
            }
            Ok(())
        }
        syn::Type::Tuple(type_tuple) => {
            write!(f, "(")?;
            write_punct(f, &type_tuple.elems, write_type)?;
            write!(f, ")")
        }
        e => write_token_stream(f, e),
    }
}

fn write_named_arg(f: &mut impl std::fmt::Write, input: &syn::NamedArg) -> std::fmt::Result {
    for attr in &input.attrs {
        write_token_stream(f, attr)?;
        write!(f, " ")?;
    }
    if let Some((name, _)) = &input.name {
        write!(f, "{name}: ")?;
    }
    write_type(f, &input.ty)?;
    Ok(())
}

fn write_type_path_with_qself(
    f: &mut impl std::fmt::Write,
    qself: &syn::QSelf,
    type_path: &syn::TypePath,
) -> Result<(), std::fmt::Error> {
    write!(f, "<")?;
    write_type(f, &qself.ty)?;
    if qself.as_token.is_some() {
        write!(f, " as ")?;
    }
    let pos = std::cmp::min(qself.position, type_path.path.segments.len());
    let mut segments = type_path.path.segments.pairs();
    if pos > 0 {
        if type_path.path.leading_colon.is_some() {
            write!(f, "::")?;
        }
        for (i, pair) in segments.by_ref().take(pos).enumerate() {
            write_path_segment(f, pair.value())?;
            if i == pos - 1 {
                write!(f, ">")?;
            }
            if pair.punct().is_some() {
                write!(f, "::")?;
            }
        }
    } else {
        write!(f, ">")?;
        if type_path.path.leading_colon.is_some() {
            write!(f, "::")?;
        }
    }

    for pair in segments {
        write_path_segment(f, pair.value())?;
        if pair.punct().is_some() {
            write!(f, "::")?;
        }
    }
    Ok(())
}

fn write_path(f: &mut impl std::fmt::Write, path: &syn::Path) -> std::fmt::Result {
    if path.leading_colon.is_some() {
        write!(f, "::")?;
    }
    write_punct(f, &path.segments, write_path_segment)?;
    Ok(())
}

fn make_write_token_stream<T: ToTokens, Fmt: std::fmt::Write>()
-> impl Fn(&mut Fmt, &T) -> std::fmt::Result {
    move |f, tt| write!(f, "{}", tt.to_token_stream())
}

fn write_token_stream(f: &mut impl std::fmt::Write, tt: &impl ToTokens) -> std::fmt::Result {
    write!(f, "{}", tt.to_token_stream())
}

fn write_punct<T, P, F: Fn(&mut Fmt, &T) -> std::fmt::Result, Fmt: std::fmt::Write>(
    f: &mut Fmt,
    punctuated: &Punctuated<T, P>,
    write_token: F,
) -> std::fmt::Result
where
    P: Punct,
{
    let punct = P::default().to_token_stream().to_string();
    for (i, pair) in punctuated.pairs().enumerate() {
        if i > 0 {
            P::write_suffix(f)?;
        }
        write_token(f, pair.value())?;
        if pair.punct().is_some() {
            P::write_prefix(f)?;
            write!(f, "{punct}")?;
        }
    }
    Ok(())
}

trait Punct: Default + ToTokens {
    fn write_prefix(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        _ = f;
        Ok(())
    }
    fn write_suffix(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        _ = f;
        Ok(())
    }
}

impl Punct for Token![,] {
    fn write_suffix(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, " ")
    }
}
impl Punct for Token![+] {
    fn write_prefix(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, " ")
    }
    fn write_suffix(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, " ")
    }
}
impl Punct for Token![::] {}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // Round-trip identity: a canonical-form type string parses to an AST that prints
    // back to the same string. `syn::parse` discards source whitespace, so what's under
    // test is whether the printer re-emits canonical, idiomatic Rust spacing.
    #[rstest]
    // paths & generics
    #[case::plain_ident("T")]
    #[case::vec("Vec<T>")]
    #[case::nested_angle("Vec<Vec<T>>")]
    #[case::qualified_path("std::collections::HashMap<K, V>")]
    #[case::leading_colon("::std::vec::Vec<T>")]
    #[case::const_generic_arg("Foo<3>")]
    #[case::assoc_type_binding("Foo<Bar = u8>")]
    #[case::lifetime_arg("Cow<'a, str>")]
    // qself
    #[case::qself_no_trait("<T>::Output")]
    #[case::qself_one_trait("<T as Trait>::Output")]
    #[case::qself_absolute_trait("<T as ::std::ops::Add>::Output")]
    #[case::qself_nested(
        "<<std::collections::HashMap<K, V> as Trait>::AssociatedType as AnotherTrait>::NestedAssociatedType"
    )]
    // references & pointers
    #[case::ref_plain("&T")]
    #[case::ref_mut("&mut T")]
    #[case::ref_lifetime_mut("&'a mut T")]
    #[case::ptr_const("*const T")]
    #[case::ptr_mut("*mut T")]
    // arrays, slices, tuples
    #[case::slice("[u8]")]
    #[case::array("[T; 3]")]
    #[case::array_zero("[u8; 0]")]
    #[case::unit("()")]
    #[case::tuple_one("(T,)")]
    #[case::tuple_many("(A, B, C)")]
    // trait objects & impl trait (multi-segment bounds aren't recursively pretty-printed)
    #[case::dyn_single("dyn Trait")]
    #[case::dyn_bounds("dyn Trait + Send + 'a")]
    #[case::impl_trait("impl Iterator")]
    #[case::boxed_dyn("Box<dyn Error + Send + Sync + 'static>")]
    // bare fns
    #[case::fn_unit("fn()")]
    #[case::fn_simple("fn(u8) -> u8")]
    #[case::fn_extern("extern \"C\" fn()")]
    #[case::fn_hrtb("for<'a, 'b> unsafe extern \"C\" fn(&'a T, &'b U, ...) -> &'a V")]
    // never & infer (catch-all → token stream)
    #[case::never("!")]
    #[case::infer("_")]
    fn test_pretty_type(#[case] input: &str) {
        let ty: syn::Type = syn::parse_str(input).unwrap();
        let our_pretty = ty.pretty().to_string();
        pretty_assertions::assert_str_eq!(our_pretty, input);
    }
}
