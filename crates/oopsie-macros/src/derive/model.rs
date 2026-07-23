//! Resolved per-item attribute model for `#[derive(Oopsie)]`.
//!
//! A [`ResolvedEnum`]/[`ResolvedStruct`] holds everything the generators need
//! about one error type: the container attributes, the per-variant (or struct)
//! parsed `#[oopsie(...)]` metas, the categorized fields, the `#[cfg(...)]`
//! gating, and the facts derived from them (selector name, whether any variant
//! is cfg-gated). It is built once from the `DeriveInput` and shared by
//! reference with every generator, so each attribute is parsed and each
//! cross-cutting rule is checked exactly once.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, Ident, Type, Visibility};

use crate::utils::pretty::Pretty as _;

use super::parse::{
    CategorizedFields, EnumContainerAttrs, ModuleSetting, SourceField, SourceKind, StructAttrs,
    VariantAttrs,
};

/// One enum variant with its attributes, fields, and derived facts resolved.
pub struct ResolvedVariant<'a> {
    /// The original variant node (ident, fields, discriminant, raw attrs).
    pub variant: &'a syn::Variant,
    /// Parsed `#[oopsie(...)]` metas on the variant.
    pub attrs: VariantAttrs,
    /// The variant's fields split into source/auto/user plus trace detection.
    pub fields: CategorizedFields,
    /// `#[cfg(...)]` attrs gating the whole variant, forwarded onto every
    /// generated mention so a stripped variant takes its impls with it.
    pub cfg_attrs: Vec<syn::Attribute>,
    /// The context selector struct name (variant name with `Error` stripped and
    /// the suffix applied); `None` for a `transparent` variant, which generates
    /// a `From` impl rather than a named selector.
    pub selector_ident: Option<Ident>,
}

impl ResolvedVariant<'_> {
    pub const fn ident(&self) -> &Ident {
        &self.variant.ident
    }
}

/// A fully resolved enum: container attributes plus every variant resolved.
pub struct ResolvedEnum<'a> {
    pub input: &'a DeriveInput,
    pub container: &'a EnumContainerAttrs,
    pub variants: Vec<ResolvedVariant<'a>>,
    /// Whether any variant carries a `#[cfg(...)]` gate, so generated matches
    /// over `self` need a wildcard fallback (the attribute-macro path expands
    /// before cfg-stripping).
    pub any_variant_cfg: bool,
}

/// A fully resolved struct: container + variant-role attributes plus fields.
pub struct ResolvedStruct<'a> {
    pub input: &'a DeriveInput,
    pub attrs: &'a StructAttrs,
    pub fields: CategorizedFields,
}

impl<'a> ResolvedEnum<'a> {
    /// Parse and validate every variant of `input` against `container`.
    ///
    /// Validation runs in phases, each sweeping every variant before the next
    /// begins: field categorization and the source-cfg rule, then attribute
    /// parsing with transparent/selector resolution, then display keyword-arg
    /// rejection, then the help-conflict rule. The phase order determines which
    /// error surfaces first when several rules fail; keep it stable, since the
    /// trybuild stderr fixtures pin the exact first diagnostic.
    pub fn resolve(input: &'a DeriveInput, container: &'a EnumContainerAttrs) -> syn::Result<Self> {
        let syn::Data::Enum(data) = &input.data else {
            unreachable!("ResolvedEnum::resolve called on a non-enum")
        };

        // Phase 1 — categorize every variant's fields, then the source-cfg rule.
        let mut categorized: Vec<CategorizedFields> = Vec::with_capacity(data.variants.len());
        for variant in &data.variants {
            categorized.push(CategorizedFields::from_fields(&variant.fields)?);
        }
        for fields in &categorized {
            reject_cfg_on_source(fields)?;
        }

        // Phase 2 — parse attributes, resolve selector names, validate
        // transparent shapes and selector collisions.
        let suffix = container.effective_suffix(true);
        let mut seen_selectors: std::collections::HashMap<String, Ident> =
            std::collections::HashMap::new();
        let mut seen_transparent_sources: std::collections::HashMap<String, Ident> =
            std::collections::HashMap::new();
        let mut variants = Vec::with_capacity(data.variants.len());
        for (variant, fields) in data.variants.iter().zip(categorized) {
            let attrs = VariantAttrs::from_attrs(&variant.attrs)?;
            let cfg_attrs: Vec<syn::Attribute> = variant
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("cfg"))
                .cloned()
                .collect();

            let selector_ident = if attrs.transparent {
                validate_transparent(&variant.ident, &input.ident, &fields, &attrs)?;
                // cfg-gated variants may legitimately share a source type under
                // mutually exclusive cfgs, so only unconditional variants
                // participate in the collision check.
                if cfg_attrs.is_empty()
                    && let Some(source) = &fields.source
                {
                    let ty = transparent_source_ty(source);
                    let key = ty.pretty().to_string();
                    if let Some(first) = seen_transparent_sources.insert(key, variant.ident.clone())
                    {
                        return Err(transparent_source_collision_error(
                            &first,
                            &variant.ident,
                            ty,
                        ));
                    }
                }
                None
            } else {
                let ident = super::gen_selectors::selector_name(&variant.ident, &suffix)?;
                // cfg-gated variants may legitimately share a selector name
                // under mutually exclusive cfgs, so only unconditional variants
                // participate in the collision check.
                if cfg_attrs.is_empty() {
                    if ident == input.ident {
                        return Err(selector_matches_enum_error(&variant.ident, &input.ident));
                    }
                    if let Some(first) =
                        seen_selectors.insert(ident.to_string(), variant.ident.clone())
                    {
                        return Err(selector_collision_error(&first, &variant.ident, &ident));
                    }
                }
                Some(ident)
            };

            variants.push(ResolvedVariant {
                variant,
                attrs,
                fields,
                cfg_attrs,
                selector_ident,
            });
        }

        // Phase 3 — display keyword-arg rejection.
        for v in &variants {
            reject_display_keywords(v.variant, &v.attrs)?;
        }

        // Phase 4 — help-conflict rule.
        for v in &variants {
            validate_help_conflict(&v.fields, &v.attrs)?;
        }

        Ok(Self {
            input,
            container,
            variants,
            any_variant_cfg: super::parse::any_variant_has_cfg(data),
        })
    }

    /// Reject a variant-level `traced` toggle when the enum isn't traced.
    ///
    /// The toggle is read only by `#[oopsie::oopsie(traced)]` trace injection;
    /// on a derive-only or otherwise non-traced enum it would silently do
    /// nothing. Call only after establishing that the enum isn't traced.
    pub fn reject_inert_variant_traced(&self) -> syn::Result<()> {
        const MSG: &str = "`traced` on an enum variant only applies when the enum is traced \
             with `#[oopsie::oopsie(traced)]`; add `traced` to the enum or remove this";
        for v in &self.variants {
            if v.attrs.traced.to_option().is_none() {
                continue;
            }
            return match v.variant.attrs.iter().find(|a| attr_mentions_traced(a)) {
                Some(attr) => Err(syn::Error::new_spanned(attr, MSG)),
                None => Err(syn::Error::new_spanned(v.ident(), MSG)),
            };
        }
        Ok(())
    }
}

/// Whether `attr` is an `#[oopsie(...)]` list that names the `traced` key, used
/// to point the inert-`traced` diagnostic at the offending attribute.
fn attr_mentions_traced(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("oopsie")
        && attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|metas| metas.iter().any(|m| m.path().is_ident("traced")))
}

impl<'a> ResolvedStruct<'a> {
    /// Parse and validate `input` against `attrs` in phases whose order fixes
    /// which error surfaces first: field categorization (and source-cfg) plus
    /// transparent/selector-name resolution, then display keyword-arg rejection,
    /// then the help-conflict rule. The trybuild stderr fixtures pin the exact
    /// first diagnostic, so keep the order stable.
    pub fn resolve(input: &'a DeriveInput, attrs: &'a StructAttrs) -> syn::Result<Self> {
        let syn::Data::Struct(data) = &input.data else {
            unreachable!("ResolvedStruct::resolve called on a non-struct")
        };
        let fields = CategorizedFields::from_fields(&data.fields)?;
        reject_cfg_on_source(&fields)?;

        if attrs.transparent {
            validate_transparent(&input.ident, &input.ident, &fields, attrs)?;
        } else {
            // A transparent struct generates a `From` impl, not a named selector,
            // so the reserved/colliding-name check applies only to the non-
            // transparent path.
            let selector_ident = selector_name_for_struct(input, attrs)?;
            // Without a module wrapping the selector, a selector sharing the
            // struct's own name collides at the same scope (E0428); the
            // module-wrapped path is safe because the selector lives in a
            // child module and is reached through `super::` (gen_selectors.rs).
            if selector_ident == input.ident
                && !matches!(
                    attrs.container.effective_module(false),
                    ModuleSetting::On(_)
                )
            {
                return Err(selector_matches_struct_error(&input.ident));
            }
        }

        reject_display_keywords_struct(input, attrs)?;
        validate_help_conflict(&fields, attrs)?;

        Ok(Self {
            input,
            attrs,
            fields,
        })
    }
}

/// `#[cfg(...)]` on a source field is unsupported: gating the source would strip
/// the `From`/`Contextual` impls that depend on it, with no sourceless fallback.
fn reject_cfg_on_source(fields: &CategorizedFields) -> syn::Result<()> {
    if let Some(source) = &fields.source
        && let Some(cfg) = source.cfg_attrs.first()
    {
        return Err(syn::Error::new_spanned(
            cfg,
            "`#[cfg(...)]` is not supported on a source field; gating the \
             source would strip the `From`/`Contextual` impls that depend on it",
        ));
    }
    Ok(())
}

/// A `transparent` item must have a source and no extra user fields.
/// `no_source_span` is the node the "requires a source" error points at — the
/// variant ident for an enum, the struct ident for a struct. `self_ident` is
/// the enclosing enum/struct's own name, checked against an auto-boxed
/// source's inner type.
fn validate_transparent(
    no_source_span: &dyn quote::ToTokens,
    self_ident: &Ident,
    fields: &CategorizedFields,
    attrs: &impl TransparentInertAttrs,
) -> syn::Result<()> {
    if let Some(vis) = attrs.vis() {
        return Err(syn::Error::new_spanned(
            vis,
            "`vis` has no effect on a `transparent` item: it generates a `From` impl, not a \
             named selector; remove `vis` or drop `transparent`",
        ));
    }
    if attrs.suffix_set() {
        return Err(syn::Error::new_spanned(
            self_ident,
            "`suffix` has no effect on a `transparent` struct: it generates a `From` impl, not \
             a named selector; remove `suffix` or drop `transparent`",
        ));
    }
    if attrs.module_set() {
        return Err(syn::Error::new_spanned(
            self_ident,
            "`module` has no effect on a `transparent` struct: it generates a `From` impl, not \
             a namespaced selector; remove `module` or drop `transparent`",
        ));
    }
    if fields.source.is_none() {
        return Err(syn::Error::new_spanned(
            no_source_span,
            "`transparent` requires a source field (a field named `source` \
             or marked `#[oopsie(from)]`)",
        ));
    }
    if let Some(src) = &fields.source
        && src.forward.any()
    {
        return Err(syn::Error::new_spanned(
            &src.ident,
            "`#[oopsie(forward(...))]` is redundant on a `transparent` error; transparent already forwards all traces",
        ));
    }
    if let Some(src) = &fields.source
        && let SourceKind::AutoBoxed { source_type } = &src.kind
        && type_is_self(source_type, self_ident)
    {
        return Err(syn::Error::new_spanned(
            &src.ident,
            format!(
                "`transparent` source `Box<{self_ident}>` auto-unboxes to \
                 `From<{self_ident}> for {self_ident}`, colliding with the standard \
                 library's blanket `impl<T> From<T> for T`; box a distinct type or \
                 drop `transparent`"
            ),
        ));
    }
    if let Some(extra) = fields.user_fields.first() {
        return Err(syn::Error::new_spanned(
            &extra.ident,
            "`transparent` allows no fields besides the source and \
             auto-captured trace fields; remove this field or drop `transparent`",
        ));
    }
    Ok(())
}

/// Whether `ty`'s last path segment names `self_ident` (an explicit reference
/// to the enclosing enum/struct) or the literal `Self` keyword.
fn type_is_self(ty: &Type, self_ident: &Ident) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "Self" || seg.ident == *self_ident)
}

/// The type a transparent variant's/struct's `From` impl is generated for —
/// the pre-transform type for `from(T, transform)` and auto-boxed sources, the
/// field's own type otherwise. Mirrors the `param_ty` resolution in
/// `gen_selectors.rs`.
fn transparent_source_ty(source: &SourceField) -> &Type {
    match &source.kind {
        SourceKind::Transformed { source_type, .. } | SourceKind::AutoBoxed { source_type } => {
            source_type
        }
        SourceKind::Yes => &source.ty,
        SourceKind::No | SourceKind::Disabled => {
            unreachable!(
                "a transparent variant's source is validated to be Yes/Transformed/AutoBoxed"
            )
        }
    }
}

fn transparent_source_collision_error(first: &Ident, second: &Ident, ty: &Type) -> syn::Error {
    let ty = ty.pretty();
    let mut err = syn::Error::new_spanned(
        second,
        format!(
            "variants `{first}` and `{second}` are both `transparent` over source type \
             `{ty}`, generating two `From<{ty}>` impls for the same type (E0119); merge \
             the variants or give one a distinct source type"
        ),
    );
    err.combine(syn::Error::new_spanned(
        first,
        format!("`{first}` also generates `From<{ty}>`"),
    ));
    err
}

/// A dynamic `#[oopsie(help)]` field and a static `help = ...` attribute cannot
/// both resolve a variant's help text.
fn validate_help_conflict(fields: &CategorizedFields, attrs: &impl HasHelp) -> syn::Result<()> {
    if let (Some(help_field), Some(_)) = (&fields.help_field, attrs.help_attr()) {
        return Err(syn::Error::new_spanned(
            help_field,
            "ambiguous help: this `#[oopsie(help)]` field conflicts with the \
             `help = ...` attribute; remove one",
        ));
    }
    Ok(())
}

/// Reject a trailing display arg on an enum variant that is actually a misparsed
/// `#[oopsie(...)]` keyword.
fn reject_display_keywords(variant: &syn::Variant, attrs: &VariantAttrs) -> syn::Result<()> {
    if let Some(display) = &attrs.display {
        display.reject_keyword_args(&variant.fields, super::parse::DisplayScope::Variant)?;
    }
    Ok(())
}

/// Reject a trailing display arg on a struct that is actually a misparsed
/// `#[oopsie(...)]` keyword.
fn reject_display_keywords_struct(input: &DeriveInput, attrs: &StructAttrs) -> syn::Result<()> {
    let syn::Data::Struct(data) = &input.data else {
        unreachable!()
    };
    if let Some(display) = &attrs.display {
        display.reject_keyword_args(&data.fields, super::parse::DisplayScope::Struct)?;
    }
    Ok(())
}

fn selector_name_for_struct(input: &DeriveInput, attrs: &StructAttrs) -> syn::Result<Ident> {
    super::gen_selectors::selector_name(&input.ident, &attrs.container.effective_suffix(false))
}

fn selector_matches_enum_error(variant: &Ident, enum_ident: &Ident) -> syn::Error {
    syn::Error::new_spanned(
        variant,
        format!(
            "variant `{variant}` generates a selector named `{enum_ident}`, colliding with the \
             enum's own name; rename the variant or set `#[oopsie(suffix = \"...\")]`"
        ),
    )
}

fn selector_matches_struct_error(ident: &Ident) -> syn::Error {
    syn::Error::new_spanned(
        ident,
        format!(
            "the selector for `{ident}` would also be named `{ident}` (no `Error` suffix to \
             strip, and `suffix(false)`); set `#[oopsie(suffix = \"...\")]` or enable `module`"
        ),
    )
}

fn selector_collision_error(first: &Ident, second: &Ident, selector: &Ident) -> syn::Error {
    let mut err = syn::Error::new_spanned(
        second,
        format!(
            "variants `{first}` and `{second}` both generate a selector named \
             `{selector}` (a trailing `Error` is stripped from variant names); \
             rename one of the variants"
        ),
    );
    err.combine(syn::Error::new_spanned(
        first,
        format!("`{first}` also generates selector `{selector}`"),
    ));
    err
}

/// Selector-naming/visibility knobs that go inert once an item is
/// `transparent` — it generates a `From` impl rather than a named, visible
/// selector, so nothing consumes them. `suffix`/`module` are container-scoped
/// concepts that only apply to structs (an enum's apply to the whole enum, not
/// to one transparent variant), hence their default `false`.
trait TransparentInertAttrs {
    fn vis(&self) -> Option<&Visibility>;
    fn suffix_set(&self) -> bool {
        false
    }
    fn module_set(&self) -> bool {
        false
    }
}

impl TransparentInertAttrs for VariantAttrs {
    fn vis(&self) -> Option<&Visibility> {
        self.visibility()
    }
}

impl TransparentInertAttrs for StructAttrs {
    fn vis(&self) -> Option<&Visibility> {
        self.visibility()
    }
    fn suffix_set(&self) -> bool {
        self.container.suffix.is_some()
    }
    fn module_set(&self) -> bool {
        // `module(false)` matches a struct's own default (`Off`) regardless of
        // `transparent`, so it's inert either way and not worth flagging —
        // only an explicit request that would actually wrap a selector (one
        // that doesn't exist here) is a dropped request.
        self.container.module.is_some()
            && matches!(self.container.effective_module(false), ModuleSetting::On(_))
    }
}

/// The `help = ...` attribute slot, abstracted over the variant and struct
/// attribute containers so the help-conflict rule reads from one place.
pub trait HasHelp {
    fn help_attr(&self) -> Option<&super::parse::DisplayAttr>;
}

impl HasHelp for VariantAttrs {
    fn help_attr(&self) -> Option<&super::parse::DisplayAttr> {
        self.help.as_ref()
    }
}

impl HasHelp for StructAttrs {
    fn help_attr(&self) -> Option<&super::parse::DisplayAttr> {
        self.help.as_ref()
    }
}

/// Field-binding patterns (`name,`) for a destructure, each carrying its
/// `#[cfg(...)]`/`#[cfg_attr(...)]` attrs so a binding for a stripped field is
/// stripped too — a trailing `..` in the pattern absorbs the gap. Pattern fields
/// accept attributes; the destructures relying on this do so.
pub fn field_binding_pats(fields: &syn::Fields) -> Vec<TokenStream2> {
    let syn::Fields::Named(named) = fields else {
        return Vec::new();
    };
    named
        .named
        .iter()
        .filter_map(|f| {
            let ident = f.ident.as_ref()?;
            let cfg = f
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"));
            Some(quote! { #(#cfg)* #ident, })
        })
        .collect()
}
