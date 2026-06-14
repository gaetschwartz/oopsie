//! Generic-parameter projection for context selectors.
//!
//! A selector struct is a separate type from the error it builds, so it can
//! only name a generic parameter of the error if it redeclares it. Carrying
//! *every* error parameter would force unrelated parameters onto a selector
//! whose fields never mention them (e.g. a leaf variant on `enum E<T, U>` whose
//! fields use neither). This module finds the minimal subset a selector's
//! captured fields actually reference and projects the error's generics down to
//! just that subset, preserving the original declaration order and bounds.

use std::collections::HashSet;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit::Visit;
use syn::{ConstParam, GenericParam, Generics, Lifetime, LifetimeParam, Type, TypeParam};

/// The set of generic parameters a selector carries, ready to render at the
/// declaration site (`<'a, T, const N: usize>`) and the use site (`<'a, T, N>`).
///
/// Built by [`project`] from the error's full [`Generics`] and the set of
/// parameter names a selector's fields reference. The two renderings stay in
/// lockstep: a parameter present in the declaration is present in the use list,
/// so a `Selector #decl` struct and a `Selector #use_` reference never diverge.
pub struct SelectorGenerics {
    decl_params: Vec<GenericParam>,
    use_args: Vec<TokenStream2>,
}

impl SelectorGenerics {
    /// The declaration-position generics (`<'a, T: Bound, const N: usize>`),
    /// bounds and all, or empty tokens when the selector carries no parameter.
    pub fn decl(&self) -> TokenStream2 {
        if self.decl_params.is_empty() {
            return quote! {};
        }
        let params = &self.decl_params;
        quote! { <#(#params),*> }
    }

    /// The use-position arguments (`<'a, T, N>`) naming the same parameters
    /// without their bounds, or empty tokens when the selector carries none.
    pub fn use_(&self) -> TokenStream2 {
        if self.use_args.is_empty() {
            return quote! {};
        }
        let args = &self.use_args;
        quote! { <#(#args),*> }
    }

    /// The projected parameter declarations, for callers that splice them with
    /// further parameters.
    pub fn params(&self) -> &[GenericParam] {
        &self.decl_params
    }
}

/// The bare name of a generic parameter — the lifetime ident without its tick,
/// or the type/const ident — for membership checks against a referenced set.
pub fn param_name(param: &GenericParam) -> String {
    match param {
        GenericParam::Lifetime(lt) => lt.lifetime.ident.to_string(),
        GenericParam::Type(tp) => tp.ident.to_string(),
        GenericParam::Const(cp) => cp.ident.to_string(),
    }
}

/// Whether a `where` predicate's subject (the bounded type or lifetime) names
/// any parameter in `names`. Used to route each error predicate to the impl or
/// method that has the parameter in scope.
pub fn predicate_mentions(
    pred: &syn::WherePredicate,
    names: &std::collections::HashSet<String>,
) -> bool {
    let probe_ty = |ty: &Type| {
        let mut found = false;
        for name in names {
            // A cheap textual check would misfire on substrings; reuse the type
            // walk by building a single-name declared set.
            let declared = DeclaredParams {
                types: std::iter::once(name.clone()).collect(),
                consts: std::iter::once(name.clone()).collect(),
                lifetimes: std::iter::once(name.clone()).collect(),
            };
            if declared.type_references_param(ty) {
                found = true;
                break;
            }
        }
        found
    };
    match pred {
        syn::WherePredicate::Type(ty_pred) => probe_ty(&ty_pred.bounded_ty),
        syn::WherePredicate::Lifetime(lt_pred) => {
            names.contains(&lt_pred.lifetime.ident.to_string())
        }
        _ => false,
    }
}

/// Project `generics` down to the parameters whose names appear in
/// `referenced`, keeping declaration order. A `where` predicate is **not**
/// pulled in here: selector structs carry no `where` clause (their `Into`
/// bounds suffice for construction), so only the parameter declarations and
/// their inline bounds travel onto the selector.
pub fn project(generics: &Generics, referenced: &ReferencedParams) -> SelectorGenerics {
    let mut decl_params = Vec::new();
    let mut use_args = Vec::new();
    for param in &generics.params {
        match param {
            GenericParam::Lifetime(lt) if referenced.lifetimes.contains(&lt.lifetime) => {
                decl_params.push(param.clone());
                let lifetime = &lt.lifetime;
                use_args.push(quote! { #lifetime });
            }
            GenericParam::Type(tp) if referenced.types.contains(&tp.ident.to_string()) => {
                decl_params.push(param.clone());
                let ident = &tp.ident;
                use_args.push(quote! { #ident });
            }
            GenericParam::Const(cp) if referenced.consts.contains(&cp.ident.to_string()) => {
                decl_params.push(param.clone());
                let ident = &cp.ident;
                use_args.push(quote! { #ident });
            }
            GenericParam::Lifetime(_) | GenericParam::Type(_) | GenericParam::Const(_) => {}
        }
    }
    SelectorGenerics {
        decl_params,
        use_args,
    }
}

/// The names of an error type's generic parameters, partitioned by kind, so a
/// type walk can tell whether a field type mentions one.
pub struct DeclaredParams {
    types: HashSet<String>,
    consts: HashSet<String>,
    lifetimes: HashSet<String>,
}

impl DeclaredParams {
    pub fn from_generics(generics: &Generics) -> Self {
        let mut types = HashSet::new();
        let mut consts = HashSet::new();
        let mut lifetimes = HashSet::new();
        for param in &generics.params {
            match param {
                GenericParam::Type(TypeParam { ident, .. }) => {
                    types.insert(ident.to_string());
                }
                GenericParam::Const(ConstParam { ident, .. }) => {
                    consts.insert(ident.to_string());
                }
                GenericParam::Lifetime(LifetimeParam { lifetime, .. }) => {
                    lifetimes.insert(lifetime.ident.to_string());
                }
            }
        }
        Self {
            types,
            consts,
            lifetimes,
        }
    }

    /// Whether `ty` references any declared parameter (type, const, or
    /// lifetime). Used per-field to decide whether the field keeps its concrete
    /// type on the selector or rides an `Into` parameter.
    pub fn type_references_param(&self, ty: &Type) -> bool {
        let mut probe = ReferencedParams::default();
        probe.add_type(ty, self);
        !probe.types.is_empty() || !probe.consts.is_empty() || !probe.lifetimes.is_empty()
    }
}

/// The subset of an error's declared parameters that a set of field types
/// references. Accumulated across a selector's fields, then handed to
/// [`project`] to pick the matching declarations.
#[derive(Default)]
pub struct ReferencedParams {
    types: HashSet<String>,
    consts: HashSet<String>,
    lifetimes: HashSet<Lifetime>,
}

impl ReferencedParams {
    /// Walk `ty` and record every declared parameter it mentions. A const
    /// parameter used in an array length (`[u8; N]`) surfaces as a bare path
    /// expression nested in the type, which the path visitor catches.
    pub fn add_type(&mut self, ty: &Type, declared: &DeclaredParams) {
        let mut visitor = RefVisitor {
            declared,
            found: self,
        };
        visitor.visit_type(ty);
    }

    /// The names of every referenced parameter (type, const, and lifetime),
    /// flattened across kinds for membership checks. Lifetimes contribute their
    /// bare ident (no tick), matching [`param_name`].
    pub fn names(&self) -> HashSet<String> {
        self.types
            .iter()
            .cloned()
            .chain(self.consts.iter().cloned())
            .chain(self.lifetimes.iter().map(|lt| lt.ident.to_string()))
            .collect()
    }
}

/// `syn` visitor recording which of `declared`'s parameters a type references.
/// An identifier counts as a reference only when its single-segment path name
/// is a declared type or const parameter, so a field `Vec<T>` records `T` but a
/// field `Vec<String>` records nothing.
struct RefVisitor<'a> {
    declared: &'a DeclaredParams,
    found: &'a mut ReferencedParams,
}

impl<'ast> Visit<'ast> for RefVisitor<'_> {
    fn visit_lifetime(&mut self, i: &'ast Lifetime) {
        if self.declared.lifetimes.contains(&i.ident.to_string()) {
            self.found.lifetimes.insert(i.clone());
        }
    }

    fn visit_path(&mut self, i: &'ast syn::Path) {
        if let Some(ident) = i.get_ident() {
            let name = ident.to_string();
            if self.declared.types.contains(&name) {
                self.found.types.insert(name.clone());
            }
            if self.declared.consts.contains(&name) {
                self.found.consts.insert(name);
            }
        }
        // Recurse so a parameter nested in generic arguments
        // (`Wrapper<T>`, `<T as Trait>::Assoc`) is still recorded.
        syn::visit::visit_path(self, i);
    }
}
