//! Cfg-aware view of an item for injection decisions.
//!
//! The attribute macro runs before `#[cfg]`/`#[cfg_attr]` evaluation, but some
//! injection decisions read helper attributes that may sit inside a
//! `cfg_attr(pred, oopsie(...))`, or depend on trace fields that may be
//! `#[cfg]`-gated. Each such predicate is an atom, lowered to an expression
//! over base predicates (`feature = "x"`, `unix`, ...) so that correlated atoms
//! such as `a` and `not(a)` never take contradictory values. The decision is
//! made once per truth assignment of the base predicates, and every injected
//! piece is gated by the (minimised) predicate under which it was wanted.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Attribute, Fields, Meta, Token};

use super::field_detect::TraceRole;

/// Above this many base predicates the assignment space gets too large to
/// enumerate at expansion time.
const MAX_BASES: usize = 8;

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
    pub fn cfg_attribute(&self) -> TokenStream2 {
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

/// A cfg predicate lowered over the base predicates of a [`CfgAtoms`] set.
#[derive(Debug)]
enum Expr {
    Const(bool),
    Base(usize),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
}

impl Expr {
    fn eval(&self, bits: u32) -> bool {
        match self {
            Self::Const(value) => *value,
            Self::Base(k) => bits & (1 << k) != 0,
            Self::All(exprs) => exprs.iter().all(|e| e.eval(bits)),
            Self::Any(exprs) => exprs.iter().any(|e| e.eval(bits)),
            Self::Not(expr) => !expr.eval(bits),
        }
    }
}

/// The predicates an item's injection decisions depend on, and the base
/// predicates they are built from.
#[derive(Default)]
pub struct CfgAtoms {
    bases: Vec<TokenStream2>,
    atoms: Vec<(String, Expr)>,
}

impl CfgAtoms {
    /// Collect the predicates of every `cfg_attr` that (possibly nested) gates
    /// an `oopsie` helper, and the existence predicate of every `#[cfg]`-gated
    /// field that could affect injection.
    pub fn collect<'a>(
        attr_lists: impl IntoIterator<Item = &'a [Attribute]>,
        fields: &Fields,
        timestamp_type: &syn::Type,
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
            if is_injection_relevant(field, timestamp_type)
                && let Some(pred) = existence_pred(&field.attrs)
            {
                atoms.intern(&pred);
            }
        }
        if atoms.bases.len() > MAX_BASES {
            let bases: Vec<String> = atoms.bases.iter().map(|b| format!("`{b}`")).collect();
            return Err(syn::Error::new(
                span,
                format!(
                    "too many distinct `cfg` predicates affect trace injection here \
                     (at most {MAX_BASES}, found {}: {}); move some `oopsie(...)` helpers out \
                     of `cfg_attr` or use `#[derive(Oopsie)]` with explicit trace fields",
                    bases.len(),
                    bases.join(", "),
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
            self.intern(&quote! { #pred });
        }
        gates_oopsie
    }

    fn intern(&mut self, pred: &TokenStream2) {
        let key = pred.to_string();
        if self.atoms.iter().any(|(k, _)| *k == key) {
            return;
        }
        let expr = match syn::parse2::<Meta>(pred.clone()) {
            Ok(meta) => self.lower(&meta),
            Err(_) => self.base(pred.clone()),
        };
        self.atoms.push((key, expr));
    }

    fn lower(&mut self, meta: &Meta) -> Expr {
        match meta {
            Meta::Path(path) if path.is_ident("true") => Expr::Const(true),
            Meta::Path(path) if path.is_ident("false") => Expr::Const(false),
            Meta::List(list)
                if ["all", "any", "not"]
                    .iter()
                    .any(|op| list.path.is_ident(op)) =>
            {
                let Ok(args) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return self.base(quote! { #meta });
                };
                let mut args: Vec<Expr> = args.iter().map(|arg| self.lower(arg)).collect();
                if list.path.is_ident("all") {
                    Expr::All(args)
                } else if list.path.is_ident("any") {
                    Expr::Any(args)
                } else if args.len() == 1
                    && let Some(arg) = args.pop()
                {
                    Expr::Not(Box::new(arg))
                } else {
                    self.base(quote! { #meta })
                }
            }
            Meta::Path(_) | Meta::List(_) | Meta::NameValue(_) => self.base(quote! { #meta }),
        }
    }

    fn base(&mut self, pred: TokenStream2) -> Expr {
        let key = pred.to_string();
        let k = self
            .bases
            .iter()
            .position(|b| b.to_string() == key)
            .unwrap_or_else(|| {
                self.bases.push(pred);
                self.bases.len() - 1
            });
        Expr::Base(k)
    }

    /// Every truth assignment of the base predicates, in index order:
    /// assignment `i` sets base `k` true iff bit `k` of `i` is set.
    pub fn assignments(&self) -> impl Iterator<Item = Assignment<'_>> {
        (0..1u32 << self.bases.len()).map(move |bits| Assignment { atoms: self, bits })
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
        let terms: Vec<TokenStream2> = minimal_cover(self.bases.len(), outcomes)
            .into_iter()
            .map(|cube| {
                let lits: Vec<TokenStream2> = (0..self.bases.len())
                    .filter(|&k| cube.care & (1 << k) != 0)
                    .map(|k| {
                        let base = &self.bases[k];
                        if cube.value & (1 << k) == 0 {
                            quote! { not(#base) }
                        } else {
                            quote! { #base }
                        }
                    })
                    .collect();
                match lits.as_slice() {
                    [single] => single.clone(),
                    lits => quote! { all(#(#lits),*) },
                }
            })
            .collect();
        match terms.as_slice() {
            [single] => Gate::Cfg(single.clone()),
            terms => Gate::Cfg(quote! { any(#(#terms),*) }),
        }
    }
}

/// A product term: the assignments whose `care` bits equal `value`.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Cube {
    care: u32,
    value: u32,
}

impl Cube {
    const fn covers(self, bits: u32) -> bool {
        bits & self.care == self.value
    }
}

/// A small sum of products covering exactly the assignments where `outcomes`
/// holds: the prime implicants (Quine–McCluskey), then a greedy cover.
fn minimal_cover(n: usize, outcomes: &[bool]) -> Vec<Cube> {
    let minterms: Vec<u32> = (0u32..)
        .zip(outcomes)
        .filter_map(|(bits, on)| on.then_some(bits))
        .collect();
    let full = (1u32 << n) - 1;
    let mut current: Vec<Cube> = minterms
        .iter()
        .map(|&value| Cube { care: full, value })
        .collect();
    let mut primes = Vec::new();
    while !current.is_empty() {
        let mut merged = vec![false; current.len()];
        let mut next: Vec<Cube> = Vec::new();
        for i in 0..current.len() {
            for j in i + 1..current.len() {
                let (a, b) = (current[i], current[j]);
                let diff = a.value ^ b.value;
                if a.care == b.care && diff.is_power_of_two() {
                    merged[i] = true;
                    merged[j] = true;
                    let cube = Cube {
                        care: a.care & !diff,
                        value: a.value & !diff,
                    };
                    if !next.contains(&cube) {
                        next.push(cube);
                    }
                }
            }
        }
        primes.extend(
            current
                .iter()
                .zip(&merged)
                .filter_map(|(cube, merged)| (!merged).then_some(*cube)),
        );
        current = next;
    }

    let mut uncovered = minterms;
    let mut cover = Vec::new();
    while let Some(best) = primes
        .iter()
        .copied()
        .max_by_key(|p| uncovered.iter().filter(|&&m| p.covers(m)).count())
        .filter(|p| uncovered.iter().any(|&m| p.covers(m)))
    {
        uncovered.retain(|&m| !best.covers(m));
        cover.push(best);
    }
    cover
}

/// One truth assignment of a [`CfgAtoms`] set's base predicates.
pub struct Assignment<'a> {
    atoms: &'a CfgAtoms,
    bits: u32,
}

impl Assignment<'_> {
    fn holds(&self, pred: &TokenStream2) -> Option<bool> {
        let key = pred.to_string();
        self.atoms
            .atoms
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, expr)| expr.eval(self.bits))
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

/// The predicate under which an item carrying `attrs` survives cfg-stripping,
/// or `None` when nothing gates its existence. `#[cfg(P)]` contributes `P`;
/// `#[cfg_attr(Q, cfg(P))]` contributes `any(not(Q), P)`, since the nested `cfg`
/// only reaches the item when `Q` holds. A `cfg_attr` gating no `cfg` (however
/// deeply nested) conditions other attributes without removing the item, so it
/// contributes nothing.
fn existence_pred(attrs: &[Attribute]) -> Option<TokenStream2> {
    conjunction(attrs.iter().filter_map(|a| meta_existence_pred(&a.meta)))
}

fn meta_existence_pred(meta: &Meta) -> Option<TokenStream2> {
    let list = meta.require_list().ok()?;
    if list.path.is_ident("cfg") {
        let pred = &list.tokens;
        return Some(quote! { #pred });
    }
    if !list.path.is_ident("cfg_attr") {
        return None;
    }
    let metas = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    let mut metas = metas.iter();
    let pred = metas.next()?;
    let inner = conjunction(metas.filter_map(meta_existence_pred))?;
    Some(quote! { any(not(#pred), #inner) })
}

fn conjunction(preds: impl IntoIterator<Item = TokenStream2>) -> Option<TokenStream2> {
    let preds: Vec<TokenStream2> = preds.into_iter().collect();
    match preds.as_slice() {
        [] => None,
        [pred] => Some(quote! { #pred }),
        preds => Some(quote! { all(#(#preds),*) }),
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
/// `oopsie` helper (possibly cfg_attr-gated, so its roles are unknown until
/// resolved) or its type fills an injectable role.
fn is_injection_relevant(field: &syn::Field, timestamp_type: &syn::Type) -> bool {
    fn mentions_oopsie(meta: &Meta) -> bool {
        meta.path().is_ident("oopsie")
            || split_cfg_attr(meta).is_some_and(|(_, gated)| gated.iter().any(mentions_oopsie))
    }
    field.attrs.iter().any(|a| mentions_oopsie(&a.meta))
        || TraceRole::of_type(&field.ty, timestamp_type).any()
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    fn fields(item: syn::ItemStruct) -> Fields {
        item.fields
    }

    fn collect(attrs: &[Attribute], fields: &Fields) -> syn::Result<CfgAtoms> {
        CfgAtoms::collect(
            [attrs],
            fields,
            &parse_quote!(::std::time::SystemTime),
            proc_macro2::Span::call_site(),
        )
    }

    fn atoms(preds: &[TokenStream2]) -> CfgAtoms {
        let mut atoms = CfgAtoms::default();
        for pred in preds {
            atoms.intern(pred);
        }
        atoms
    }

    fn cfg(gate: &Gate) -> String {
        let Gate::Cfg(pred) = gate else {
            panic!("expected a conditional gate, got {gate:?}");
        };
        pred.to_string()
    }

    #[test]
    fn no_conditionals_yield_a_single_assignment() {
        let f = fields(parse_quote! { struct S { #[cfg(feature = "x")] info: String } });
        let atoms = collect(&[], &f).unwrap();
        assert_eq!(atoms.assignments().count(), 1);
    }

    #[test]
    fn cfg_attr_oopsie_is_expanded_or_dropped() {
        let attrs: Vec<Attribute> =
            vec![parse_quote! { #[cfg_attr(feature = "x", oopsie(traced = false), doc = "d")] }];
        let atoms = collect(&attrs, &Fields::Unit).unwrap();
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
        let atoms = collect(&[], &f).unwrap();
        let counts: Vec<usize> = atoms
            .assignments()
            .map(|a| a.resolve_fields(&f).len())
            .collect();
        assert_eq!(counts, [0, 1]);
    }

    #[test]
    fn cfg_gated_configured_timestamp_type_is_an_atom() {
        let f = fields(parse_quote! { struct S { #[cfg(feature = "x")] at: Stamp } });
        let atoms = CfgAtoms::collect([], &f, &parse_quote!(Stamp), proc_macro2::Span::call_site())
            .unwrap();
        assert_eq!(atoms.assignments().count(), 2);
    }

    #[test]
    fn correlated_predicates_share_their_base() {
        let attrs: Vec<Attribute> =
            vec![parse_quote! { #[cfg_attr(feature = "a", oopsie(traced = false))] }];
        let f = fields(parse_quote! {
            struct S { #[cfg(all(unix, not(feature = "a")))] bt: Backtrace }
        });
        let atoms = collect(&attrs, &f).unwrap();
        let seen: Vec<(bool, bool)> = atoms
            .assignments()
            .map(|a| {
                (
                    !a.resolve_attrs(&attrs).is_empty(),
                    !a.resolve_fields(&f).is_empty(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [(false, false), (true, false), (false, true), (true, false)]
        );
    }

    #[test]
    fn constant_predicates_add_no_assignments() {
        let attrs: Vec<Attribute> = vec![
            parse_quote! { #[cfg_attr(all(), oopsie(traced = false))] },
            parse_quote! { #[cfg_attr(any(), oopsie(code = "x"))] },
        ];
        let f = fields(parse_quote! {
            struct S { #[cfg(any())] bt: Backtrace, #[cfg(not(any()))] st: SpanTrace }
        });
        let atoms = collect(&attrs, &f).unwrap();
        let resolved: Vec<(usize, usize)> = atoms
            .assignments()
            .map(|a| (a.resolve_attrs(&attrs).len(), a.resolve_fields(&f).len()))
            .collect();
        assert_eq!(resolved, [(1, 1)]);
    }

    #[test]
    fn too_many_bases_are_listed() {
        let f = fields(parse_quote! {
            struct S {
                #[cfg(all(a, b, c))] bt: Backtrace,
                #[cfg(any(d, e, f))] st: SpanTrace,
                #[cfg(all(g, h, not(i)))] at: SystemTime,
            }
        });
        let err = collect(&[], &f).err().unwrap();
        assert_eq!(
            err.to_string(),
            "too many distinct `cfg` predicates affect trace injection here (at most 8, found \
             9: `a`, `b`, `c`, `d`, `e`, `f`, `g`, `h`, `i`); move some `oopsie(...)` helpers \
             out of `cfg_attr` or use `#[derive(Oopsie)]` with explicit trace fields"
        );
    }

    #[test]
    fn gate_drops_atoms_the_outcome_ignores() {
        let atoms = atoms(&[quote! { a }, quote! { b }]);
        assert_eq!(cfg(&atoms.gate(&[true, false, true, false])), "not (a)");
    }

    #[test]
    fn gate_is_unconditional_when_outcomes_agree() {
        let atoms = atoms(&[quote! { a }]);
        assert!(matches!(atoms.gate(&[true, true]), Gate::On));
        assert!(atoms.gate(&[false, false]).is_off());
    }

    #[test]
    fn gate_merges_adjacent_terms() {
        let atoms = atoms(&[quote! { a }, quote! { b }, quote! { c }]);
        let outcomes: Vec<bool> = (0u32..8)
            .map(|bits| bits & 1 != 0 || bits & 0b110 == 0b110)
            .collect();
        assert_eq!(cfg(&atoms.gate(&outcomes)), "any (a , all (b , c))");
    }

    #[test]
    fn gate_names_the_bases_of_a_compound_atom() {
        let atoms = atoms(&[quote! { all(feature = "x", not(unix)) }]);
        let outcomes: Vec<bool> = (0u32..4).map(|bits| bits == 0b01).collect();
        assert_eq!(
            cfg(&atoms.gate(&outcomes)),
            "all (feature = \"x\" , not (unix))"
        );
    }
}
