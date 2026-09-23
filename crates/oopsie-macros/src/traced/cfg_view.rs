//! Cfg-aware view of an item for injection decisions.
//!
//! The attribute macro runs before `#[cfg]`/`#[cfg_attr]` evaluation, but some
//! injection decisions read helper attributes that may sit inside a
//! `cfg_attr(pred, oopsie(...))`, or depend on trace fields that may be
//! `#[cfg]`-gated. Each such predicate becomes an atom; the decision is made
//! once per truth assignment of the atoms, and every injected piece is gated by
//! the predicate under which it was wanted.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Attribute, Fields, Meta, Token};

use super::field_detect::{
    is_backtrace_type, is_location_type, is_spantrace_type, is_timestamp_type, is_traces_type,
};
use crate::derive::parse::existence_pred;

/// Above this many independent predicates the assignment space gets too large
/// to enumerate at expansion time.
const MAX_ATOMS: usize = 8;

/// Whether an injected piece is present, absent, or present under a predicate.
#[derive(Clone, Debug)]
pub enum Gate {
    Off,
    On,
    Cfg(TokenStream2),
}

impl Gate {
    pub const fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    /// The `#[cfg(...)]` attribute that applies this gate to an item; empty
    /// when the item is unconditional.
    pub fn cfg_attr(&self) -> TokenStream2 {
        match self {
            Self::Off | Self::On => TokenStream2::new(),
            Self::Cfg(pred) => quote! { #[cfg(#pred)] },
        }
    }
}

impl From<bool> for Gate {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// The distinct predicates an item's injection decisions depend on.
#[derive(Default)]
pub struct CfgAtoms {
    atoms: Vec<TokenStream2>,
}

impl CfgAtoms {
    /// Collect the predicates of every `cfg_attr` that (possibly nested) gates
    /// an `oopsie` helper, and the existence predicate of every `#[cfg]`-gated
    /// field that could affect injection.
    pub fn collect<'a>(
        attr_lists: impl IntoIterator<Item = &'a [Attribute]>,
        fields: &Fields,
        span: proc_macro2::Span,
    ) -> syn::Result<Self> {
        let mut atoms = Self::default();
        for attrs in attr_lists {
            for attr in attrs {
                atoms.collect_meta(&attr.meta);
            }
        }
        for field in fields {
            for attr in &field.attrs {
                atoms.collect_meta(&attr.meta);
            }
            if is_injection_relevant(field)
                && let Some(pred) = existence_pred(&field.attrs)
            {
                atoms.intern(pred);
            }
        }
        if atoms.atoms.len() > MAX_ATOMS {
            return Err(syn::Error::new(
                span,
                format!(
                    "too many distinct `cfg`/`cfg_attr` predicates affect trace injection here \
                     (at most {MAX_ATOMS}); move some `oopsie(...)` helpers out of `cfg_attr` \
                     or use `#[derive(Oopsie)]` with explicit trace fields"
                ),
            ));
        }
        Ok(atoms)
    }

    /// Records `meta`'s predicate if it is a `cfg_attr` gating an `oopsie`
    /// helper; returns whether it gates one.
    fn collect_meta(&mut self, meta: &Meta) -> bool {
        if meta.path().is_ident("oopsie") {
            return true;
        }
        let Some((pred, gated)) = split_cfg_attr(meta) else {
            return false;
        };
        let mut gates_oopsie = false;
        for inner in &gated {
            gates_oopsie |= self.collect_meta(inner);
        }
        if gates_oopsie {
            self.intern(quote! { #pred });
        }
        gates_oopsie
    }

    fn intern(&mut self, pred: TokenStream2) {
        let key = pred.to_string();
        if !self.atoms.iter().any(|a| a.to_string() == key) {
            self.atoms.push(pred);
        }
    }

    fn index_of(&self, pred: &TokenStream2) -> Option<usize> {
        let key = pred.to_string();
        self.atoms.iter().position(|a| a.to_string() == key)
    }

    /// Every truth assignment of the atoms, in index order: assignment `i`
    /// sets atom `k` true iff bit `k` of `i` is set.
    pub fn assignments(&self) -> impl Iterator<Item = Assignment<'_>> {
        (0..1u32 << self.atoms.len()).map(move |bits| Assignment { atoms: self, bits })
    }

    /// The gate under which a piece wanted in exactly the assignments where
    /// `outcomes` holds is present. `outcomes` is indexed like
    /// [`assignments`](Self::assignments).
    pub fn gate(&self, outcomes: &[bool]) -> Gate {
        if outcomes.iter().all(|o| *o) {
            return Gate::On;
        }
        if !outcomes.iter().any(|o| *o) {
            return Gate::Off;
        }
        let relevant: Vec<usize> = (0..self.atoms.len())
            .filter(|&k| (0..outcomes.len()).any(|i| outcomes[i] != outcomes[i ^ (1 << k)]))
            .collect();
        let terms: Vec<TokenStream2> = (0..1u32 << relevant.len())
            .filter_map(|sub| {
                let bits = relevant
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| sub & (1 << j) != 0)
                    .fold(0usize, |acc, (_, &k)| acc | (1 << k));
                outcomes[bits].then(|| {
                    let lits: Vec<TokenStream2> = relevant
                        .iter()
                        .map(|&k| {
                            let atom = &self.atoms[k];
                            if bits & (1 << k) == 0 {
                                quote! { not(#atom) }
                            } else {
                                quote! { #atom }
                            }
                        })
                        .collect();
                    match lits.as_slice() {
                        [single] => single.clone(),
                        lits => quote! { all(#(#lits),*) },
                    }
                })
            })
            .collect();
        match terms.as_slice() {
            [single] => Gate::Cfg(single.clone()),
            terms => Gate::Cfg(quote! { any(#(#terms),*) }),
        }
    }
}

/// One truth assignment of a [`CfgAtoms`] set.
pub struct Assignment<'a> {
    atoms: &'a CfgAtoms,
    bits: u32,
}

impl Assignment<'_> {
    fn holds(&self, pred: &TokenStream2) -> Option<bool> {
        self.atoms.index_of(pred).map(|k| self.bits & (1 << k) != 0)
    }

    /// `attrs` as rustc would leave them under this assignment: a `cfg_attr`
    /// whose predicate is an atom is expanded or dropped; anything else is kept.
    pub fn resolve_attrs(&self, attrs: &[Attribute]) -> Vec<Attribute> {
        let mut out = Vec::with_capacity(attrs.len());
        for attr in attrs {
            self.resolve_into(attr, &attr.meta, &mut out);
        }
        out
    }

    fn resolve_into(&self, original: &Attribute, meta: &Meta, out: &mut Vec<Attribute>) {
        if let Some((pred, gated)) = split_cfg_attr(meta)
            && let Some(holds) = self.holds(&quote! { #pred })
        {
            if holds {
                for inner in &gated {
                    self.resolve_into(original, inner, out);
                }
            }
            return;
        }
        out.push(Attribute {
            meta: meta.clone(),
            ..original.clone()
        });
    }

    /// `fields` as rustc would leave them under this assignment: gated-out
    /// injection-relevant fields are removed, and every field's attributes are
    /// resolved.
    pub fn resolve_fields(&self, fields: &Fields) -> Fields {
        let mut fields = fields.clone();
        let keep = |field: &syn::Field| {
            existence_pred(&field.attrs)
                .and_then(|pred| self.holds(&pred))
                .unwrap_or(true)
        };
        match &mut fields {
            Fields::Named(named) => {
                named.named = std::mem::take(&mut named.named)
                    .into_iter()
                    .filter(keep)
                    .collect();
            }
            Fields::Unnamed(unnamed) => {
                unnamed.unnamed = std::mem::take(&mut unnamed.unnamed)
                    .into_iter()
                    .filter(keep)
                    .collect();
            }
            Fields::Unit => {}
        }
        for field in &mut fields {
            field.attrs = self.resolve_attrs(&field.attrs);
        }
        fields
    }
}

fn split_cfg_attr(meta: &Meta) -> Option<(Meta, Vec<Meta>)> {
    let list = meta.require_list().ok()?;
    if !list.path.is_ident("cfg_attr") {
        return None;
    }
    let metas = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    let mut metas = metas.into_iter();
    let pred = metas.next()?;
    Some((pred, metas.collect()))
}

/// Whether a field's presence can change what gets injected: it carries an
/// `oopsie` helper (possibly cfg_attr-gated) or has a detectable trace,
/// timestamp, or location type.
fn is_injection_relevant(field: &syn::Field) -> bool {
    fn mentions_oopsie(meta: &Meta) -> bool {
        meta.path().is_ident("oopsie")
            || split_cfg_attr(meta).is_some_and(|(_, gated)| gated.iter().any(mentions_oopsie))
    }
    field.attrs.iter().any(|a| mentions_oopsie(&a.meta))
        || is_backtrace_type(&field.ty)
        || is_spantrace_type(&field.ty)
        || is_traces_type(&field.ty)
        || is_timestamp_type(&field.ty)
        || is_location_type(&field.ty)
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    fn fields(item: syn::ItemStruct) -> Fields {
        item.fields
    }

    #[test]
    fn no_conditionals_yield_a_single_assignment() {
        let f = fields(parse_quote! { struct S { #[cfg(feature = "x")] info: String } });
        let atoms = CfgAtoms::collect([], &f, proc_macro2::Span::call_site()).unwrap();
        assert_eq!(atoms.assignments().count(), 1);
    }

    #[test]
    fn cfg_attr_oopsie_is_expanded_or_dropped() {
        let attrs: Vec<Attribute> =
            vec![parse_quote! { #[cfg_attr(feature = "x", oopsie(traced = false), doc = "d")] }];
        let atoms = CfgAtoms::collect(
            [attrs.as_slice()],
            &Fields::Unit,
            proc_macro2::Span::call_site(),
        )
        .unwrap();
        let resolved: Vec<Vec<Attribute>> = atoms
            .assignments()
            .map(|a| a.resolve_attrs(&attrs))
            .collect();
        assert!(resolved[0].is_empty());
        assert_eq!(resolved[1].len(), 2);
        assert!(resolved[1][0].path().is_ident("oopsie"));
    }

    #[test]
    fn cfg_gated_trace_field_is_an_atom() {
        let f = fields(parse_quote! { struct S { #[cfg(feature = "x")] bt: Backtrace } });
        let atoms = CfgAtoms::collect([], &f, proc_macro2::Span::call_site()).unwrap();
        let counts: Vec<usize> = atoms
            .assignments()
            .map(|a| a.resolve_fields(&f).len())
            .collect();
        assert_eq!(counts, [0, 1]);
    }

    #[test]
    fn gate_drops_atoms_the_outcome_ignores() {
        let mut atoms = CfgAtoms::default();
        atoms.intern(quote! { a });
        atoms.intern(quote! { b });
        let Gate::Cfg(pred) = atoms.gate(&[true, false, true, false]) else {
            panic!("expected a conditional gate");
        };
        assert_eq!(pred.to_string(), "not (a)");
    }

    #[test]
    fn gate_is_unconditional_when_outcomes_agree() {
        let mut atoms = CfgAtoms::default();
        atoms.intern(quote! { a });
        assert!(matches!(atoms.gate(&[true, true]), Gate::On));
        assert!(atoms.gate(&[false, false]).is_off());
    }
}
