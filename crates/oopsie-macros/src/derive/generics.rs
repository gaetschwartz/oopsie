//! Generic-parameter projection for context selectors.
//!
//! A selector struct is a separate type from the error it builds, so it can
//! only name a generic parameter of the error if it redeclares it. Carrying
//! *every* error parameter would force unrelated parameters onto a selector
//! whose fields never mention them (e.g. a leaf variant on `enum E<T, U>` whose
//! fields use neither). This module finds the minimal subset a selector's
//! captured fields actually reference and projects the error's generics down to
//! just that subset, preserving the original declaration order. Inline bounds
//! are lowered to `where` predicates on the impls (a struct needs none, and a
//! bound may name a parameter the selector does not carry).

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
    /// The declaration-position generics (`<'a, T, const N: usize>`), bounds
    /// lowered out, or empty tokens when the selector carries no parameter.
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

/// A copy of `param` with any default (`= u8`, `= 4`) removed, for the
/// declaration positions that forbid one: an `impl<...>` header or a method's
/// generic list. Bounds are legal there and left intact; lifetimes have no
/// default. A default is only ever valid on the original type/struct/enum
/// declaration, which the derive never re-emits.
pub fn strip_default(param: &GenericParam) -> GenericParam {
    let mut param = param.clone();
    match &mut param {
        GenericParam::Type(tp) => {
            tp.default = None;
        }
        GenericParam::Const(cp) => {
            cp.default = None;
        }
        GenericParam::Lifetime(_) => {}
    }
    param
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

/// Project `generics` down to the parameters whose names appear in
/// `referenced`, keeping declaration order. Inline bounds are **dropped** from
/// the projected declarations: a struct (`struct EV<U> { val: U }`) needs no
/// bound to be well-formed, and a bound may name a parameter the selector does
/// not carry (`U: From<T>` with `T` unprojected), which would dangle on the
/// struct. The bounds re-surface as `where` predicates on the impls that name
/// the destination error — see [`projected_bound_predicates`], routed by the
/// generators to whichever impl or method has every named parameter in scope.
pub fn project(generics: &Generics, referenced: &ReferencedParams) -> SelectorGenerics {
    let mut decl_params = Vec::new();
    let mut use_args = Vec::new();
    for param in &generics.params {
        match param {
            GenericParam::Lifetime(lt) if referenced.lifetimes.contains(&lt.lifetime) => {
                let mut bare = lt.clone();
                bare.bounds.clear();
                bare.colon_token = None;
                decl_params.push(GenericParam::Lifetime(bare));
                let lifetime = &lt.lifetime;
                use_args.push(quote! { #lifetime });
            }
            GenericParam::Type(tp) if referenced.types.contains(&tp.ident.to_string()) => {
                let mut bare = tp.clone();
                bare.bounds.clear();
                bare.colon_token = None;
                bare.default = None;
                decl_params.push(GenericParam::Type(bare));
                let ident = &tp.ident;
                use_args.push(quote! { #ident });
            }
            GenericParam::Const(cp) if referenced.consts.contains(&cp.ident.to_string()) => {
                decl_params.push(strip_default(param));
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

/// The `where` predicates equivalent to the inline bounds of the parameters
/// named in `projected` (`<U: From<T>>` → `U: From<T>`). [`project`] strips
/// these bounds off the selector's struct declarations, so they must travel
/// onto the impls that name the destination error; a caller routes each one to
/// the impl or method that has every parameter it names in scope.
pub fn projected_bound_predicates(
    generics: &Generics,
    projected: &std::collections::HashSet<String>,
) -> Vec<syn::WherePredicate> {
    let mut predicates = Vec::new();
    for param in &generics.params {
        match param {
            GenericParam::Type(tp) if projected.contains(&tp.ident.to_string()) => {
                if tp.bounds.is_empty() {
                    continue;
                }
                let ident = &tp.ident;
                let bounds = &tp.bounds;
                predicates.push(syn::parse_quote! { #ident: #bounds });
            }
            GenericParam::Lifetime(lt) if projected.contains(&lt.lifetime.ident.to_string()) => {
                if lt.bounds.is_empty() {
                    continue;
                }
                let lifetime = &lt.lifetime;
                let bounds = &lt.bounds;
                predicates.push(syn::parse_quote! { #lifetime: #bounds });
            }
            GenericParam::Type(_) | GenericParam::Lifetime(_) | GenericParam::Const(_) => {}
        }
    }
    predicates
}

/// Every generic parameter a `where` predicate names — in its subject and in
/// its bounds — restricted to those declared in `declared`. Routing a predicate
/// requires the *whole* set in scope, so a bound that names an extra parameter
/// (`U: From<T>`) is not silently dropped the way a subject-only check would.
pub fn predicate_named_params(
    pred: &syn::WherePredicate,
    declared: &DeclaredParams,
) -> std::collections::HashSet<String> {
    let mut found = ReferencedParams::default();
    match pred {
        syn::WherePredicate::Type(ty_pred) => {
            found.add_type(&ty_pred.bounded_ty, declared);
            for bound in &ty_pred.bounds {
                match bound {
                    syn::TypeParamBound::Trait(tb) => {
                        let mut visitor = RefVisitor {
                            declared,
                            found: &mut found,
                            record_projection_base: false,
                        };
                        visitor.visit_path(&tb.path);
                    }
                    syn::TypeParamBound::Lifetime(lt)
                        if declared.lifetimes.contains(&lt.ident.to_string()) =>
                    {
                        found.lifetimes.insert(lt.clone());
                    }
                    _ => {}
                }
            }
        }
        syn::WherePredicate::Lifetime(lt_pred) => {
            if declared
                .lifetimes
                .contains(&lt_pred.lifetime.ident.to_string())
            {
                found.lifetimes.insert(lt_pred.lifetime.clone());
            }
            for bound in &lt_pred.bounds {
                if declared.lifetimes.contains(&bound.ident.to_string()) {
                    found.lifetimes.insert(bound.clone());
                }
            }
        }
        _ => {}
    }
    found.names()
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

    /// Whether `name` is already taken by a declared type or const parameter.
    /// Used to probe for a synthetic parameter name that cannot collide with
    /// the error's own declaration (E0403).
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains(name) || self.consts.contains(name)
    }

    /// Whether `ty` references any declared parameter (type, const, or
    /// lifetime). Used per-field to decide whether the field keeps its concrete
    /// type on the selector or rides an `Into` parameter.
    pub fn type_references_param(&self, ty: &Type) -> bool {
        let mut probe = ReferencedParams::default();
        probe.add_type(ty, self);
        !probe.types.is_empty() || !probe.consts.is_empty() || !probe.lifetimes.is_empty()
    }

    /// The declared parameters an `Into<ty>` bound depends on, counting the base
    /// of a shorthand associated-type projection (`T::Item` → `T`) that
    /// [`type_references_param`] deliberately ignores so the field rides `Into`.
    /// The bound must land on an impl or method with every one of these in scope.
    pub fn params_for_into_bound(&self, ty: &Type) -> HashSet<String> {
        let mut probe = ReferencedParams::default();
        let mut visitor = RefVisitor {
            declared: self,
            found: &mut probe,
            record_projection_base: true,
        };
        visitor.visit_type(ty);
        probe.names()
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
            record_projection_base: false,
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
    /// Also record the leading segment of a shorthand associated-type projection
    /// (`T::Item` → `T`). A single-segment path already lands via `get_ident`;
    /// segment recursion never re-enters `visit_path` for the bare leading
    /// segment of a multi-segment path, so it needs recording here.
    record_projection_base: bool,
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
        } else if self.record_projection_base
            && let Some(first) = i.segments.first()
        {
            let name = first.ident.to_string();
            if self.declared.types.contains(&name) {
                self.found.types.insert(name);
            }
        }
        // Recurse so a parameter nested in generic arguments
        // (`Wrapper<T>`, `<T as Trait>::Assoc`) is still recorded.
        syn::visit::visit_path(self, i);
    }
}
