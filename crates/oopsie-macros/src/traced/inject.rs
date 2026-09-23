//! Field injection helpers.

use syn::punctuated::Punctuated;
use syn::spanned::Spanned as _;
use syn::{Fields, FieldsNamed, parse_quote, token};

use super::args::ResolvedTraceArgs;
use super::cfg_view::{CfgAtoms, Gate};
use super::config::{FieldExistence, FieldInjectorConfig, FieldsToInject, InjectPlan};
use super::field_detect::TraceRole;
use crate::derive::parse::{FieldAttrs, ResolvedForward, field_forward};

/// Check which fields already exist in a `Fields` collection, by explicit
/// `#[oopsie(backtrace|spantrace|traces|location)]` role or by type. `attrs`
/// holds each field's parsed helper attributes, in field order.
pub(super) fn check_existing_fields(
    fields: &Fields,
    attrs: &[FieldAttrs],
    timestamp_type: &syn::Type,
) -> syn::Result<FieldExistence> {
    let mut existence = FieldExistence::default();

    for (field, attrs) in fields.iter().zip(attrs) {
        // The mangled name guards re-expansion of already-injected fields; a
        // user field merely *named* `timestamp` of an unrelated type is an
        // ordinary field and does not suppress injection (same rule as
        // backtrace/spantrace detection).
        let is_injected_timestamp = field
            .ident
            .as_ref()
            .is_some_and(|id| id == "__oopsie_timestamp");

        let role = TraceRole::of_field(field, attrs, timestamp_type);
        if role.traces {
            // One packed field supplies both traces; mark all three so neither
            // separate field is also injected.
            existence.has_traces = true;
            existence.has_backtrace = true;
            existence.has_spantrace = true;
        }
        if role.backtrace {
            existence.has_backtrace = true;
        }
        if role.spantrace {
            existence.has_spantrace = true;
        }
        if is_injected_timestamp || role.timestamp {
            existence.has_timestamp = true;
        }
        if role.timestamp && !is_injected_timestamp {
            existence
                .timestamp_conflict
                .get_or_insert_with(|| field.span());
        }
        if role.location {
            existence.has_location = true;
        }

        // A field merely *named* like one of the mangled idents `traced`
        // injects, but of an unrelated type, would otherwise pass through
        // unnoticed and collide with the field `inject_fields` pushes later —
        // rustc then reports the resulting duplicate-field shape, not this
        // cause. Reject the collision here, at the actual field.
        reject_mangled_collision(field, "__oopsie_traces", role.traces)?;
        reject_mangled_collision(field, "__oopsie_backtrace", role.backtrace)?;
        reject_mangled_collision(field, "__oopsie_spantrace", role.spantrace)?;
        reject_mangled_collision(field, "__oopsie_location", role.location)?;
    }
    Ok(existence)
}

/// What an item's own `#[oopsie(...)]` attributes say about injection.
pub(super) struct ItemFacts {
    pub traced: bool,
    pub has_user_code: bool,
    pub transparent: bool,
}

/// The injection outcome for one cfg assignment that doesn't reject the item.
enum Decision {
    Inject {
        fields: FieldsToInject,
        auto_code: bool,
    },
    /// The attributes rustc would leave don't parse: nothing is injected, and
    /// the derive reports the error if this assignment is the real one.
    Deferred(syn::Error),
}

/// Decide what to inject into one struct or variant, and under which cfg
/// predicates. `facts` parses the item-level attributes; it and the field
/// attribute parsing run once per cfg assignment on the attributes rustc would
/// leave under it.
pub(super) fn plan_injection(
    resolved: &ResolvedTraceArgs<'_>,
    config: &FieldInjectorConfig,
    attrs: &[syn::Attribute],
    fields: &Fields,
    span: proc_macro2::Span,
    facts: impl Fn(&[syn::Attribute]) -> syn::Result<ItemFacts>,
) -> syn::Result<(InjectPlan, Gate)> {
    let atoms = CfgAtoms::collect([attrs], fields, &config.timestamp_type, span)?;
    let decisions: Vec<syn::Result<Decision>> = atoms
        .assignments()
        .map(|assignment| {
            let attrs = assignment.resolve_attrs(attrs);
            let fields = assignment.resolve_fields(fields);
            decide(resolved, config, &attrs, &fields, &facts)
        })
        .collect();
    reject_if_any(&atoms, &decisions)?;
    if let Some(Ok(Decision::Deferred(first))) = decisions.first()
        && decisions
            .iter()
            .all(|d| matches!(d, Ok(Decision::Deferred(_))))
    {
        return Err(first.clone());
    }
    let injected: Vec<Option<&FieldsToInject>> = decisions
        .iter()
        .map(|d| match d {
            Ok(Decision::Inject { fields, .. }) => Some(fields),
            Ok(Decision::Deferred(_)) | Err(_) => None,
        })
        .collect();
    let auto_code: Vec<bool> = decisions
        .iter()
        .map(|d| match d {
            Ok(Decision::Inject { auto_code, .. }) => *auto_code,
            Ok(Decision::Deferred(_)) | Err(_) => false,
        })
        .collect();
    Ok((InjectPlan::merge(&atoms, &injected), atoms.gate(&auto_code)))
}

/// Every base-predicate assignment is a configuration that can be built, so an
/// assignment that rejects the item is reported even when it isn't the current
/// one, naming the configurations that hit it.
fn reject_if_any(atoms: &CfgAtoms, decisions: &[syn::Result<Decision>]) -> syn::Result<()> {
    let Some(first) = decisions.iter().find_map(|d| d.as_ref().err()) else {
        return Ok(());
    };
    let rejected: Vec<bool> = decisions.iter().map(Result::is_err).collect();
    match atoms.gate(&rejected) {
        Gate::Off | Gate::On => Err(first.clone()),
        Gate::Cfg(pred) => Err(syn::Error::new(
            first.span(),
            format!("{first} (in configurations where `cfg({pred})` holds)"),
        )),
    }
}

/// The injection decision for one cfg assignment; an error rejects the item
/// under that assignment.
fn decide(
    resolved: &ResolvedTraceArgs<'_>,
    config: &FieldInjectorConfig,
    attrs: &[syn::Attribute],
    fields: &Fields,
    facts: impl Fn(&[syn::Attribute]) -> syn::Result<ItemFacts>,
) -> syn::Result<Decision> {
    let facts = match facts(attrs) {
        Ok(facts) => facts,
        Err(err) => return Ok(Decision::Deferred(err)),
    };
    let field_attrs = match fields
        .iter()
        .map(FieldAttrs::from_field)
        .collect::<syn::Result<Vec<_>>>()
    {
        Ok(field_attrs) => field_attrs,
        Err(err) => return Ok(Decision::Deferred(err)),
    };
    let existence = check_existing_fields(fields, &field_attrs, &config.timestamp_type)?;
    if facts.traced
        && resolved.timestamp_explicit
        && let Some(span) = existence.timestamp_conflict
    {
        return Err(timestamp_conflict_error(span));
    }

    let forward = if facts.traced {
        match fields
            .iter()
            .map(field_forward)
            .collect::<syn::Result<Vec<_>>>()
        {
            Ok(forwards) => forwards.into_iter().find(|f| f.any()).unwrap_or_default(),
            Err(err) => return Ok(Decision::Deferred(err)),
        }
    } else {
        ResolvedForward::default()
    };

    let inject_backtrace = facts.traced && resolved.backtrace && !forward.backtrace;
    let inject_spantrace = facts.traced && resolved.spantrace && !forward.spantrace;
    let inject_location = facts.traced && resolved.location && !forward.location;

    // Packed only applies when both traces are enabled and no trace field
    // already exists; otherwise fall back to per-field (unpacked) injection,
    // which also covers the single-trace case.
    let packed = resolved.packed
        && inject_backtrace
        && inject_spantrace
        && !existence.has_backtrace
        && !existence.has_spantrace
        && !existence.has_traces;

    let to_inject = FieldsToInject {
        backtrace: !packed && inject_backtrace && !existence.has_backtrace,
        spantrace: !packed && inject_spantrace && !existence.has_spantrace,
        timestamp: facts.traced && resolved.timestamp && !existence.has_timestamp,
        traces: packed,
        location: inject_location && !existence.has_location,
    };
    let auto_code = wants_auto_code(
        facts.traced && resolved.code,
        facts.has_user_code,
        facts.transparent,
    );
    Ok(Decision::Inject {
        fields: to_inject,
        auto_code,
    })
}

/// Error for `traced(timestamp)` requested on a struct/variant whose
/// pre-existing SystemTime/DateTime-typed field suppresses injection: unlike
/// backtrace/spantrace fields, such a field is an ordinary selector, not
/// auto-captured or wired to `provide`, so the request would otherwise be a
/// silent no-op.
pub(super) fn timestamp_conflict_error(span: proc_macro2::Span) -> syn::Error {
    syn::Error::new(
        span,
        "this field's type suppresses `traced(timestamp)` injection, but the field is not \
         auto-captured or wired to `provide` like a backtrace/spantrace field would be; \
         `timestamp` has no effect here — rename or retype this field, or drop `timestamp` \
         from `traced`",
    )
}

fn reject_mangled_collision(
    field: &syn::Field,
    mangled_name: &str,
    is_trace_typed: bool,
) -> syn::Result<()> {
    if is_trace_typed {
        return Ok(());
    }
    let Some(ident) = field.ident.as_ref() else {
        return Ok(());
    };
    if ident != mangled_name {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        ident,
        format!("`{mangled_name}` conflicts with a field injected by `traced`; rename it"),
    ))
}

/// Inject backtrace/spantrace/timestamp fields into a `Fields` collection.
pub(super) fn inject_fields(
    fields: &mut Fields,
    config: &FieldInjectorConfig,
    to_inject: &InjectPlan,
) -> syn::Result<()> {
    match fields {
        Fields::Named(named) => {
            inject_into_named(named, config, to_inject);
            Ok(())
        }
        Fields::Unit => {
            // Leave a unit variant/struct untouched when nothing is injected:
            // rewriting it to empty braces changes its shape, and for a variant
            // carrying an explicit discriminant that turns it into a non-unit
            // variant, which rustc rejects without a `#[repr(int)]`.
            if !to_inject.any() {
                return Ok(());
            }
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
            "trace injection does not support tuple variants/structs; use named fields instead",
        )),
    }
}

fn inject_into_named(
    fields: &mut FieldsNamed,
    config: &FieldInjectorConfig,
    to_inject: &InjectPlan,
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
        timestamp_attrs,
        traces_ident,
        traces_type,
        traces_attrs,
        location_ident,
        location_type,
        location_attrs,
        ..
    } = config;

    let mut push = |gate: &Gate, field: syn::Field| {
        if gate.is_off() {
            return;
        }
        let cfg = gate.cfg_attribute();
        // Injected fields are mangled internals: keep them out of docs and `missing_docs`.
        fields
            .named
            .push(parse_quote! { #cfg #[doc(hidden)] #field });
    };
    push(
        &to_inject.traces,
        parse_quote! { #traces_attrs #traces_ident: #traces_type },
    );
    push(
        &to_inject.backtrace,
        parse_quote! { #backtrace_attrs #backtrace_ident: #backtrace_type },
    );
    push(
        &to_inject.spantrace,
        parse_quote! { #spantrace_attrs #spantrace_ident: #spantrace_type },
    );
    push(
        &to_inject.timestamp,
        parse_quote! { #timestamp_attrs #timestamp_ident: #timestamp_type },
    );
    push(
        &to_inject.location,
        parse_quote! { #location_attrs #location_ident: #location_type },
    );
}

/// Whether an item gets the auto-generated error code; `transparent` items
/// forward their source's instead.
pub(super) const fn wants_auto_code(
    code_enabled: bool,
    has_user_code: bool,
    is_transparent: bool,
) -> bool {
    code_enabled && !has_user_code && !is_transparent
}

/// Add the provide attribute carrying the auto-generated error code, under
/// `gate`.
///
/// Backtrace and spantrace are handled by Diagnostic via field detection, so
/// only ErrorCode needs a provide attr for nightly Error::provide support.
pub(super) fn add_provide_attrs(
    attrs: &mut Vec<syn::Attribute>,
    config: &FieldInjectorConfig,
    type_name: &str,
    variant_name: Option<&str>,
    gate: &Gate,
) {
    let FieldInjectorConfig { code_type, .. } = config;
    let mut name = type_name.to_owned();
    if let Some(v) = variant_name {
        name.push_str("::");
        name.push_str(v);
    }
    let provide = quote::quote! {
        oopsie(provide(#code_type => #code_type::from(::core::concat!(::core::module_path!(), "::", #name))))
    };
    match gate {
        Gate::Off => {}
        Gate::On => attrs.push(parse_quote! { #[#provide] }),
        Gate::Cfg(pred) => attrs.push(parse_quote! { #[cfg_attr(#pred, #provide)] }),
    }
}

#[cfg(test)]
mod tests {
    use quote::{format_ident, quote};
    use syn::parse_quote;

    use super::*;

    fn parse_fields(tokens: proc_macro2::TokenStream) -> syn::Fields {
        let item: syn::ItemStruct = syn::parse2(tokens).expect("failed to parse struct");
        item.fields
    }

    fn existence_of(fields: &Fields, ts: &syn::Type) -> syn::Result<FieldExistence> {
        let attrs = fields
            .iter()
            .map(FieldAttrs::from_field)
            .collect::<syn::Result<Vec<_>>>()?;
        check_existing_fields(fields, &attrs, ts)
    }

    fn test_config() -> FieldInjectorConfig {
        FieldInjectorConfig {
            backtrace_ident: format_ident!("__oopsie_backtrace"),
            backtrace_type: quote! { ::std::boxed::Box<Backtrace> },
            backtrace_attrs: quote! { #[oopsie(backtrace)] },
            spantrace_ident: format_ident!("__oopsie_spantrace"),
            spantrace_type: quote! { ::std::boxed::Box<SpanTrace> },
            spantrace_attrs: quote! { #[oopsie(spantrace)] },
            timestamp_ident: format_ident!("__oopsie_timestamp"),
            timestamp_type: parse_quote! { std::time::SystemTime },
            timestamp_attrs: quote! { #[oopsie(capture)] },
            traces_ident: format_ident!("__oopsie_traces"),
            traces_type: quote! { ::std::boxed::Box<(Backtrace, SpanTrace)> },
            traces_attrs: quote! { #[oopsie(traces)] },
            location_ident: format_ident!("__oopsie_location"),
            location_type: quote! { &'static ::core::panic::Location<'static> },
            location_attrs: quote! { #[oopsie(location)] },
            code_type: quote! { ErrorCode },
        }
    }

    // ── check_existing_fields ────────────────────────────────────────

    #[test]
    fn check_existing_fields_detects_backtrace() {
        let fields = parse_fields(quote! { struct S { backtrace: Backtrace, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_detects_spantrace() {
        let fields = parse_fields(quote! { struct S { trace: SpanTrace, message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(!existence.has_backtrace);
        assert!(existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn wrongly_typed_timestamp_name_does_not_suppress_injection() {
        let fields = parse_fields(quote! { struct S { timestamp: u64, msg: String } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        assert!(!existence_of(&fields, &ts).unwrap().has_timestamp);
    }

    #[test]
    fn timestamp_typed_field_suppresses_regardless_of_name() {
        let fields = parse_fields(quote! { struct S { when: SystemTime } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        assert!(existence_of(&fields, &ts).unwrap().has_timestamp);
    }

    #[test]
    fn timestamp_typed_field_records_conflict_span() {
        let fields = parse_fields(quote! { struct S { when: SystemTime } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        assert!(
            existence_of(&fields, &ts)
                .unwrap()
                .timestamp_conflict
                .is_some()
        );
    }

    #[test]
    fn reexpanded_injected_timestamp_field_does_not_record_conflict() {
        let fields = parse_fields(quote! { struct S { __oopsie_timestamp: SystemTime } });
        let ts: syn::Type = parse_quote!(::std::time::SystemTime);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(existence.has_timestamp);
        assert!(existence.timestamp_conflict.is_none());
    }

    #[test]
    fn check_existing_fields_detects_none() {
        let fields = parse_fields(quote! { struct S { message: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(!existence.has_backtrace);
        assert!(!existence.has_spantrace);
        assert!(!existence.has_timestamp);
    }

    #[test]
    fn check_existing_fields_unit_returns_default() {
        let fields: syn::Fields = syn::Fields::Unit;
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(!existence.has_backtrace);
    }

    #[test]
    fn inject_fields_converts_unit_to_named() {
        let mut fields = syn::Fields::Unit;
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
            traces: false,
            location: false,
        };
        inject_fields(&mut fields, &config, &to_inject.into()).unwrap();
        assert!(matches!(fields, syn::Fields::Named(_)));
    }

    #[test]
    fn inject_fields_leaves_unit_when_nothing_injected() {
        let mut fields = syn::Fields::Unit;
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: false,
            spantrace: false,
            timestamp: false,
            traces: false,
            location: false,
        };
        inject_fields(&mut fields, &config, &to_inject.into()).unwrap();
        assert!(
            matches!(fields, syn::Fields::Unit),
            "a unit variant must stay unit when no fields are injected"
        );
    }

    #[test]
    fn inject_fields_rejects_unnamed() {
        let mut fields = parse_fields(quote! { struct S(String, u32); });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: true,
            spantrace: false,
            timestamp: false,
            traces: false,
            location: false,
        };
        assert!(inject_fields(&mut fields, &config, &to_inject.into()).is_err());
    }

    // ── add_provide_attrs ────────────────────────────────────────────

    #[test]
    fn add_provide_attrs_no_code_when_disabled() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            None,
            &wants_auto_code(false, false, false).into(),
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_adds_code_when_enabled() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            None,
            &wants_auto_code(true, false, false).into(),
        );
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_user_code_present() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            None,
            &wants_auto_code(true, true, false).into(),
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_skips_code_when_transparent() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            Some("Variant"),
            &wants_auto_code(true, false, true).into(),
        );
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn add_provide_attrs_with_variant_name() {
        let mut attrs: Vec<syn::Attribute> = vec![];
        let config = test_config();
        add_provide_attrs(
            &mut attrs,
            &config,
            "MyError",
            Some("Variant"),
            &wants_auto_code(true, false, false).into(),
        );
        assert_eq!(attrs.len(), 1);
        let attr_str = quote! { #(#attrs)* }.to_string();
        insta::assert_snapshot!(attr_str);
    }

    #[test]
    fn inject_fields_adds_packed_traces_field() {
        let mut fields = parse_fields(quote! { struct S { msg: String } });
        let config = test_config();
        let to_inject = FieldsToInject {
            backtrace: false,
            spantrace: false,
            timestamp: false,
            traces: true,
            location: false,
        };
        inject_fields(&mut fields, &config, &to_inject.into()).unwrap();
        let rendered = quote! { #fields }.to_string();
        assert!(rendered.contains("__oopsie_traces"), "{rendered}");
    }

    #[test]
    fn check_existing_detects_packed_traces_by_type() {
        let fields =
            parse_fields(quote! { struct S { t: Box<(Backtrace, SpanTrace)>, msg: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(existence.has_traces);
        // A packed field stands in for both, suppressing separate injection.
        assert!(existence.has_backtrace && existence.has_spantrace);
    }

    #[test]
    fn check_existing_fields_rejects_mangled_name_collision() {
        let fields = parse_fields(quote! { struct S { __oopsie_traces: u8, msg: String } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let err = existence_of(&fields, &ts).err().unwrap();
        assert_eq!(
            err.to_string(),
            "`__oopsie_traces` conflicts with a field injected by `traced`; rename it"
        );
    }

    #[test]
    fn check_existing_fields_allows_correctly_typed_mangled_field() {
        let fields = parse_fields(quote! { struct S { __oopsie_backtrace: Box<Backtrace> } });
        let ts: syn::Type = parse_quote!(std::time::Instant);
        let existence = existence_of(&fields, &ts).unwrap();
        assert!(existence.has_backtrace);
    }
}
