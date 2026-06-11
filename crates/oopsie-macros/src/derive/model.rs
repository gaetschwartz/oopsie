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
use syn::{DeriveInput, Ident};

use super::parse::{CategorizedFields, EnumContainerAttrs, StructAttrs, VariantAttrs};

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
                validate_transparent(&variant.ident, &fields)?;
                None
            } else {
                let ident = super::gen_selectors::selector_name(&variant.ident, &suffix)?;
                // cfg-gated variants may legitimately share a selector name
                // under mutually exclusive cfgs, so only unconditional variants
                // participate in the collision check.
                if cfg_attrs.is_empty()
                    && let Some(first) =
                        seen_selectors.insert(ident.to_string(), variant.ident.clone())
                {
                    return Err(selector_collision_error(&first, &variant.ident, &ident));
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
            validate_transparent(&input.ident, &fields)?;
        } else {
            // A transparent struct generates a `From` impl, not a named selector,
            // so the reserved/colliding-name check applies only to the non-
            // transparent path.
            let _ = selector_name_for_struct(input, attrs)?;
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
/// variant ident for an enum, the struct ident for a struct.
fn validate_transparent(
    no_source_span: &dyn quote::ToTokens,
    fields: &CategorizedFields,
) -> syn::Result<()> {
    if fields.source.is_none() {
        return Err(syn::Error::new_spanned(
            no_source_span,
            "`transparent` requires a source field (a field named `source` \
             or marked `#[oopsie(from)]`)",
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
