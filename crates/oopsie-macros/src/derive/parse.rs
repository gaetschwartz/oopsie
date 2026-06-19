//! Attribute parsing for `#[derive(Oopsie)]`.
//!
//! Parses `#[oopsie(...)]` attributes at three levels:
//! - Container (enum/struct): module, vis, suffix, path
//! - Variant/struct: display, transparent, help, code
//! - Field: from, capture, provide

#![allow(
    clippy::needless_continue,
    clippy::nonminimal_bool,
    clippy::if_not_else,
    clippy::allow_attributes,
    clippy::redundant_closure_call,
    reason = "derive macros (darling's FromAttributes, derive_syn_parse's #[call] closures) emit code tripping these lints, unreachable from our own logic"
)]

use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, LitStr, Path, Token, Type, Visibility};

// ─── Container-level attributes ──────────────────────────────────

/// A compile-time size constraint for the error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SizeConstraint {
    /// `size(N)` — exact size.
    Exact(usize),
    /// `size(..=N)` — at most N bytes.
    AtMost(usize),
    /// `size(N..)` — at least N bytes.
    AtLeast(usize),
    /// `size(N..=M)` — between N and M bytes inclusive.
    Range(usize, usize),
}

impl Parse for SizeConstraint {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Three of four shapes (`..=N`, `N..`, `N..=M`) parse as `Expr::Range`;
        // the bare integer `N` parses as `Expr::Lit`. Dispatch on whichever.
        let expr: syn::Expr = input.parse()?;
        match &expr {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(n),
                ..
            }) => Ok(Self::Exact(n.base10_parse()?)),
            syn::Expr::Range(r) => {
                let closed = matches!(r.limits, syn::RangeLimits::Closed(_));
                let start = r.start.as_deref().map(expr_to_usize).transpose()?;
                let end = r.end.as_deref().map(expr_to_usize).transpose()?;
                match (start, end, closed) {
                    (None, Some(e), true) => Ok(Self::AtMost(e)),
                    (Some(s), None, false) => Ok(Self::AtLeast(s)),
                    (Some(s), Some(e), true) => Ok(Self::Range(s, e)),
                    _ => Err(syn::Error::new_spanned(
                        &expr,
                        "unsupported range shape (use `..=N`, `N..`, or `N..=M`)",
                    )),
                }
            }
            _ => Err(syn::Error::new_spanned(
                &expr,
                "expected integer or range (e.g. `64`, `..=128`, `32..`, `32..=64`)",
            )),
        }
    }
}

fn expr_to_usize(expr: &syn::Expr) -> syn::Result<usize> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(n),
            ..
        }) => n.base10_parse(),
        _ => Err(syn::Error::new_spanned(expr, "expected integer literal")),
    }
}

impl darling::FromMeta for SizeConstraint {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        SizeAttr::from_meta(item).map(|attr| attr.constraint)
    }
}

/// A `size(...)` constraint plus the span of its arguments, so codegen can point
/// the generated assertion's diagnostic at the user's constraint.
#[derive(Debug, Clone)]
pub struct SizeAttr {
    pub constraint: SizeConstraint,
    pub span: proc_macro2::Span,
}

impl darling::FromMeta for SizeAttr {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::List(list) => {
                let constraint: SizeConstraint = syn::parse2(list.tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(&list.tokens))?;
                Ok(Self {
                    constraint,
                    span: size_arg_span(list),
                })
            }
            syn::Meta::Path(_) | syn::Meta::NameValue(_) => Err(darling::Error::custom(
                "expected `size(N)`, `size(..=N)`, `size(N..)`, or `size(N..=M)`",
            )
            .with_span(item)),
        }
    }
}

/// Span covering the `size(...)` arguments.
///
/// `Span::join` is a no-op off nightly, so a multi-token argument (any range)
/// can't be spanned precisely on stable; fall back to the delimiter group's own
/// span there, which the compiler supplies as a unit. A lone token (`size(N)`)
/// needs neither and is spanned directly.
fn size_arg_span(list: &syn::MetaList) -> proc_macro2::Span {
    let group = list.delimiter.span().join();
    let spans: Vec<proc_macro2::Span> = list.tokens.clone().into_iter().map(|t| t.span()).collect();
    match spans.as_slice() {
        [] => group,
        [single] => *single,
        [first, .., last] => first.join(*last).unwrap_or(group),
    }
}

/// A `exit_code = N` value: a process exit code in the range `1..=255`.
///
/// Stored as the validated byte plus the literal's span so codegen can point
/// any later diagnostic at the user's number.
#[derive(Debug, Clone, Copy)]
pub struct ExitCodeAttr {
    pub value: u8,
    pub span: proc_macro2::Span,
}

impl darling::FromMeta for ExitCodeAttr {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        let lit = match item {
            syn::Meta::NameValue(nv) => &nv.value,
            other => {
                return Err(darling::Error::custom("expected `exit_code = N`").with_span(other));
            }
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(int),
            ..
        }) = lit
        else {
            return Err(
                darling::Error::custom("expected an integer literal in `1..=255`").with_span(lit),
            );
        };
        // `ExitCode::from` accepts a `u8`; `0` denotes success, contradictory on
        // an error path, so the accepted range is `1..=255`.
        let value: u16 = int.base10_parse().map_err(darling::Error::custom)?;
        let value = u8::try_from(value)
            .ok()
            .filter(|&v| v != 0)
            .ok_or_else(|| {
                darling::Error::custom("exit code must be in `1..=255`").with_span(int)
            })?;
        Ok(Self {
            value,
            span: int.span(),
        })
    }
}

/// Container-level keys without `vis` (which is extracted by a pre-pass
/// because `pub(crate)` isn't a `syn::Expr`).
///
/// Used as a flattened component of `EnumContainerAttrs` and `StructAttrs`.
/// Each strict-darling struct exposes only the keys it owns; cross-scope
/// keys produce real darling errors.
#[derive(Debug, Default, darling::FromMeta)]
pub struct EnumContainerAttrsInner {
    #[darling(default)]
    pub module: Option<crate::utils::MaybeAloneOopsieValue<Ident>>,
    #[darling(default)]
    pub suffix: Option<crate::utils::MaybeAloneOopsieValue<String>>,
    #[darling(default)]
    pub size: Option<SizeAttr>,
    #[darling(default)]
    pub path: Option<Path>,
    #[darling(default)]
    pub exit_code: Option<ExitCodeAttr>,
}

#[derive(Debug, Clone)]
pub enum ModuleSetting {
    /// Module enabled with optional custom name.
    On(Option<Ident>),
    /// Module disabled.
    Off,
}

#[derive(Debug, Clone)]
pub enum SuffixSetting {
    /// No suffix (selector name = variant name).
    Off,
    /// Custom suffix.
    Custom(String),
}

impl EnumContainerAttrsInner {
    /// Resolve the suffix setting with defaults for the given item kind.
    /// - Enums: default → `Off` (selector name = variant name)
    /// - Structs: default → `Custom("Oopsie")` (e.g. `ConnOopsie`)
    pub fn effective_suffix(&self, is_enum: bool) -> SuffixSetting {
        use crate::utils::MaybeAloneOopsieValue as M;
        match &self.suffix {
            None => {
                if is_enum {
                    SuffixSetting::Off
                } else {
                    SuffixSetting::Custom("Oopsie".into())
                }
            }
            Some(M::Alone | M::Bool(true)) => SuffixSetting::Custom("Oopsie".into()),
            Some(M::Bool(false)) => SuffixSetting::Off,
            Some(M::Value(s)) => SuffixSetting::Custom(s.clone()),
        }
    }

    /// Resolve the module setting with defaults for the given item kind.
    /// - Enums: default → `On(None)` (auto-named module)
    /// - Structs: default → `Off`
    pub fn effective_module(&self, is_enum: bool) -> ModuleSetting {
        use crate::utils::MaybeAloneOopsieValue as M;
        match &self.module {
            None => {
                if is_enum {
                    ModuleSetting::On(None)
                } else {
                    ModuleSetting::Off
                }
            }
            Some(M::Alone | M::Bool(true)) => ModuleSetting::On(None),
            Some(M::Bool(false)) => ModuleSetting::Off,
            Some(M::Value(name)) => ModuleSetting::On(Some(name.clone())),
        }
    }

    /// Path override for the `::oopsie` crate (used by `gen_*` to qualify
    /// trait paths). Defaults to `::oopsie`.
    pub fn oopsie_path(&self) -> Path {
        self.path
            .clone()
            .unwrap_or_else(|| syn::parse_quote! { ::oopsie })
    }
}

/// Outer attribute container for enum types. Flattens
/// `EnumContainerAttrsInner` plus `vis`.
///
/// `vis` uses `SynParse<Visibility>` because `vis(pub(crate))` (the
/// user-facing form) routes through `Meta::List`, whose raw tokens
/// `SynParse` parses directly via `syn::Visibility::parse`. The legacy
/// quoted form `vis = "pub(crate)"` is also accepted as a fallback. The
/// bare `vis = pub(crate)` form is **not** accepted — `syn::Expr::parse`
/// rejects `pub` upstream of darling, and there's no FromMeta-side hook
/// to intercept it.
#[derive(Debug, Default, darling::FromAttributes)]
#[darling(attributes(oopsie))]
pub struct EnumContainerAttrs {
    #[darling(flatten)]
    pub inner: EnumContainerAttrsInner,
    #[darling(default)]
    pub vis: Option<crate::utils::SynParse<Visibility>>,
}

impl std::ops::Deref for EnumContainerAttrs {
    type Target = EnumContainerAttrsInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl EnumContainerAttrs {
    /// Parse `#[oopsie(...)]` on an enum definition. Strict: unknown keys
    /// (including variant-only keys like `display`) error.
    ///
    /// The short-display form is extracted first so a misplaced
    /// `#[oopsie("...")]` on the enum itself gets targeted guidance instead of
    /// darling's generic literal error.
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        use darling::FromAttributes as _;
        let (short_display, attrs) = extract_short_display(attrs)?;
        if let Some(short) = short_display {
            return Err(syn::Error::new_spanned(
                &short.format_str,
                "display strings don't go on the enum itself; put them in an \
                 `#[oopsie(\"...\")]` attribute on each enum variant",
            ));
        }
        Self::from_attributes(&attrs).map_err(syn::Error::from)
    }

    /// Unwrap the `SynParse` wrapper to expose the inner `Visibility`.
    #[inline]
    pub fn visibility(&self) -> Option<&Visibility> {
        self.vis.as_deref()
    }
}

// ─── Variant-level attributes ────────────────────────────────────

/// Variant-level keys without `vis` (which is extracted by a pre-pass).
///
/// `provide(...)` appears here even though it's logically field-level: the
/// trace-injection path auto-emits `#[oopsie(provide(...))]` at variant/struct
/// scope to surface injected backtrace/spantrace fields. Collected as a Vec
/// because multiple `provide` entries can appear per item.
#[derive(Debug, Default, darling::FromMeta)]
pub struct VariantAttrsInner {
    #[darling(default)]
    pub display: Option<DisplayAttr>,
    #[darling(default)]
    pub transparent: bool,
    #[darling(default)]
    pub help: Option<DisplayAttr>,
    #[darling(default)]
    pub code: Option<DisplayAttr>,
    #[darling(default)]
    pub exit_code: Option<ExitCodeAttr>,
    #[darling(default, multiple, rename = "provide")]
    pub provides: Vec<ProvideAttr>,
}

/// `display("fmt {}", expr)` / `help("fmt {}", expr)` body: a format string
/// followed by zero or more comma-separated expression arguments.
#[derive(Debug, Clone, derive_syn_parse::Parse)]
pub struct DisplayAttr {
    pub format_str: LitStr,
    #[prefix(Option<Token![,]> as c)]
    #[call(|s| if c.is_some() { Punctuated::parse_terminated(s) } else { Ok(Punctuated::new()) })]
    pub args: Punctuated<Expr, Token![,]>,
}

impl darling::FromMeta for DisplayAttr {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            // `display("fmt", args)` / `help("fmt", args)`
            syn::Meta::List(list) => syn::parse2(list.tokens.clone())
                .map_err(|e| darling::Error::custom(e).with_span(&list.tokens)),
            // `help = "plain"` — zero-arg display
            syn::Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Ok(Self {
                    format_str: s.clone(),
                    args: Punctuated::new(),
                }),
                other => Err(darling::Error::custom("expected string literal").with_span(other)),
            },
            syn::Meta::Path(p) => Err(darling::Error::custom("expected value").with_span(p)),
        }
    }
}

/// `#[oopsie(...)]` keywords accepted on a variant/struct, used to recognize one
/// misparsed as a trailing display format arg (`#[oopsie("fmt", transparent)]`).
const VARIANT_KEYWORDS: &[&str] = &[
    "display",
    "transparent",
    "help",
    "code",
    "exit_code",
    "provide",
];

/// Container-only keywords; valid on a struct's `#[oopsie(...)]` alongside the
/// variant set but never on an enum variant.
const CONTAINER_KEYWORDS: &[&str] = &["module", "suffix", "size", "path", "vis", "exit_code"];

/// Which `#[oopsie(...)]` keyword set a short display attaches to: an enum
/// variant accepts only variant keywords, a struct's single list mixes container
/// and variant keys.
#[derive(Clone, Copy)]
pub enum DisplayScope {
    Variant,
    Struct,
}

impl DisplayScope {
    fn is_keyword(self, ident: &str) -> bool {
        let in_set = |set: &[&str]| set.contains(&ident);
        match self {
            Self::Variant => in_set(VARIANT_KEYWORDS),
            Self::Struct => in_set(VARIANT_KEYWORDS) || in_set(CONTAINER_KEYWORDS),
        }
    }
}

/// The bare single-segment ident of `expr`, or `None` for any richer expression
/// (`self.0`, `foo()`, `a::b`, a generic path). A trailing display arg of this
/// shape is what an `#[oopsie(...)]` keyword degrades to once the parser swallows
/// it past the leading string.
fn bare_path_ident(expr: &Expr) -> Option<&Ident> {
    let Expr::Path(p) = expr else { return None };
    if p.qself.is_some() {
        return None;
    }
    let seg = match p.path.segments.len() {
        1 => &p.path.segments[0],
        _ => return None,
    };
    if p.path.leading_colon.is_some() || !matches!(seg.arguments, syn::PathArguments::None) {
        return None;
    }
    Some(&seg.ident)
}

impl DisplayAttr {
    /// Reject a trailing display arg that is actually a misparsed `#[oopsie(...)]`
    /// keyword. Greedy `Punctuated<Expr>` parsing of the args swallows a bare
    /// keyword after the string (`#[oopsie("wrapped: {source}", transparent)]`),
    /// which otherwise surfaces as an unused-argument warning plus a resolution
    /// error on a value that was never meant to be one.
    ///
    /// An arg is flagged only when it is a bare single ident matching a keyword
    /// for `scope` **and** is not a field of the item — a field of that name is a
    /// legitimate `{field}` interpolation argument, so it passes through.
    pub fn reject_keyword_args(
        &self,
        fields: &syn::Fields,
        scope: DisplayScope,
    ) -> syn::Result<()> {
        let field_named = |ident: &Ident| matches!(fields, syn::Fields::Named(f) if f.named.iter().any(|f| f.ident.as_ref() == Some(ident)));
        for arg in &self.args {
            let Some(ident) = bare_path_ident(arg) else {
                continue;
            };
            if field_named(ident) || !scope.is_keyword(&ident.to_string()) {
                continue;
            }
            let fmt = self.format_str.value();
            return Err(syn::Error::new_spanned(
                ident,
                format!(
                    "`{ident}` is an `#[oopsie(...)]` keyword, not a display format \
                     argument; give it its own attribute: \
                     `#[oopsie(\"{fmt}\")] #[oopsie({ident})]`"
                ),
            ));
        }
        Ok(())
    }

    /// Whether this renders to a `&'static str` literal rather than a `format!`.
    ///
    /// True iff there are no explicit args *and* the format string has no
    /// `{…}` placeholder: a placeholder-free string is byte-identical whether
    /// stored as a literal or run through `format!`, so the cheap const path is
    /// equivalent. An inline-capture placeholder like `{field}` makes this
    /// false even with zero trailing args, so it routes through `format!`.
    pub fn is_static(&self) -> bool {
        self.args.is_empty() && !format_str_has_placeholder(&self.format_str.value())
    }

    /// The literal to emit on the static path (`is_static()` true): the format
    /// string with `{{`/`}}` escapes collapsed, so a `from_static` render
    /// matches what `format!` would have produced.
    ///
    /// Errors on an unmatched `}`: with no placeholder to close, a lone `}` is
    /// malformed (rustc rejects it in real format strings), so the static path
    /// rejects it too rather than emitting it verbatim — keeping help at parity
    /// with `display`, which routes through `write!`.
    pub fn static_lit(&self) -> syn::Result<LitStr> {
        let mut value = self.format_str.value();
        if let Some(pos) = unmatched_close_brace(&value) {
            return Err(syn::Error::new_spanned(
                &self.format_str,
                format!(
                    "unmatched `}}` at byte {pos} in format string; \
                     write `}}}}` for a literal `}}`"
                ),
            ));
        }
        unescape_format_braces(&mut value);
        Ok(LitStr::new(&value, self.format_str.span()))
    }
}

/// Whether a format string contains a real `{…}` placeholder, treating `{{` as
/// an escape. Mirrors rustc's format parser (`rustc_parse_format`): a
/// placeholder is opened *only* by an unescaped `{`, so that is all we scan
/// for — `}`/`}}` never open one.
fn format_str_has_placeholder(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if b.get(i + 1) == Some(&b'{') {
                i += 2;
            } else {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Byte position of the first unmatched `}` (a `}` not part of a `}}` escape),
/// or `None` if every `}` is escaped. Used only on the static path, where there
/// is no placeholder for a `}` to close — so any lone `}` is malformed, exactly
/// as rustc's parser reports.
fn unmatched_close_brace(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'}' {
            if b.get(i + 1) == Some(&b'}') {
                i += 2;
            } else {
                return Some(i);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Collapse `{{`→`{` and `}}`→`}` in place, mirroring the `Piece::Lit` arms of
/// rustc's format parser. `String::retain` compacts the buffer it already owns
/// (from `LitStr::value()`) without a second allocation; the captured flags
/// carry the "just saw the first brace of a pair" state across chars.
fn unescape_format_braces(s: &mut String) {
    let (mut after_open, mut after_close) = (false, false);
    s.retain(|c| match c {
        '{' if after_open => {
            after_open = false;
            false
        }
        '{' => {
            after_open = true;
            after_close = false;
            true
        }
        '}' if after_close => {
            after_close = false;
            false
        }
        '}' => {
            after_close = true;
            after_open = false;
            true
        }
        _ => {
            after_open = false;
            after_close = false;
            true
        }
    });
}

/// Outer attribute container for enum variants. Flattens `VariantAttrsInner`
/// plus `vis`.
#[derive(Debug, Default, darling::FromAttributes)]
#[darling(attributes(oopsie))]
pub struct VariantAttrs {
    #[darling(flatten)]
    pub inner: VariantAttrsInner,
    #[darling(default)]
    pub vis: Option<crate::utils::SynParse<Visibility>>,
}

impl std::ops::Deref for VariantAttrs {
    type Target = VariantAttrsInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl VariantAttrs {
    /// Unwrap the `SynParse` wrapper to expose the inner `Visibility`.
    #[inline]
    pub fn visibility(&self) -> Option<&Visibility> {
        self.vis.as_deref()
    }

    /// Parse `#[oopsie(...)]` on an enum variant. Strict: unknown keys
    /// (including container-only keys like `module`) error.
    ///
    /// The short-display form `#[oopsie("fmt", args)]` is extracted by
    /// `extract_short_display` before darling runs (bare `LitStr` first item
    /// isn't a valid `NestedMeta`). The result is merged into `display`;
    /// duplicate display (short + long form) errors.
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        use darling::FromAttributes as _;
        let (short_display, attrs) = extract_short_display(attrs)?;
        let mut result = Self::from_attributes(&attrs).map_err(syn::Error::from)?;
        if let Some(short) = short_display {
            merge_short_display(&mut result.inner.display, short)?;
        }
        Ok(result)
    }
}

/// Outer attribute container for `#[derive(Oopsie)]` structs. Structs occupy
/// both container and variant roles on the same `#[oopsie(...)]` list, so
/// `container` flattens `EnumContainerAttrsInner` and variant-level fields
/// are inlined alongside it. Strict: keys outside the union of container +
/// variant keys produce darling errors.
///
/// Darling allows only one `#[darling(flatten)]` per struct (codegen
/// constraint), so variant fields cannot also be flattened from
/// `VariantAttrsInner` — they live inline here, duplicating those four
/// declarations.
#[derive(Debug, Default, darling::FromAttributes)]
#[darling(attributes(oopsie))]
pub struct StructAttrs {
    #[darling(flatten)]
    pub container: EnumContainerAttrsInner,
    #[darling(default)]
    pub vis: Option<crate::utils::SynParse<Visibility>>,
    #[darling(default)]
    pub display: Option<DisplayAttr>,
    #[darling(default)]
    pub transparent: bool,
    #[darling(default)]
    pub help: Option<DisplayAttr>,
    #[darling(default)]
    pub code: Option<DisplayAttr>,
    /// See `VariantAttrsInner::provides` for the rationale (struct-level
    /// `#[oopsie(provide(...))]` emitted by trace injection).
    #[darling(default, multiple, rename = "provide")]
    pub provides: Vec<ProvideAttr>,
}

impl StructAttrs {
    /// Unwrap the `SynParse` wrapper to expose the inner `Visibility`.
    #[inline]
    pub fn visibility(&self) -> Option<&Visibility> {
        self.vis.as_deref()
    }

    /// Parse `#[oopsie(...)]` on a struct definition. Strict on unknown keys.
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        use darling::FromAttributes as _;
        let (short_display, attrs) = extract_short_display(attrs)?;
        let mut result = Self::from_attributes(&attrs).map_err(syn::Error::from)?;
        if let Some(short) = short_display {
            merge_short_display(&mut result.display, short)?;
        }
        Ok(result)
    }
}

/// Merge a pre-pass-extracted short-display into the target slot. Errors if
/// the slot is already populated (long-form `display(...)` from darling).
fn merge_short_display(target: &mut Option<DisplayAttr>, short: DisplayAttr) -> syn::Result<()> {
    if target.is_some() {
        return Err(syn::Error::new_spanned(
            &short.format_str,
            "duplicate display: short form and `display(...)` cannot both be specified",
        ));
    }
    *target = Some(short);
    Ok(())
}

// ─── Forward-trace attribute ─────────────────────────────────────

/// Sub-keys of `#[oopsie(forward(...))]`: which diagnostic traces this source
/// supplies. `backtrace` and `spantrace` default on; `location` defaults off.
#[derive(Clone, Debug, Default, darling::FromMeta)]
pub struct ForwardArgs {
    #[darling(default)]
    pub backtrace: crate::utils::BetterFlag<true>,
    #[darling(default)]
    pub spantrace: crate::utils::BetterFlag<true>,
    #[darling(default)]
    pub location: crate::utils::BetterFlag<false>,
}

/// Flattened forward decision for one source field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolvedForward {
    pub backtrace: bool,
    pub spantrace: bool,
    pub location: bool,
}

impl ResolvedForward {
    pub fn resolve(setting: &crate::utils::FieldSetting<false, ForwardArgs>) -> Self {
        if !setting.is_enabled() {
            return Self::default();
        }
        let s = setting.opt_settings();
        Self {
            backtrace: s.is_none_or(|a| a.backtrace.is_enabled()),
            spantrace: s.is_none_or(|a| a.spantrace.is_enabled()),
            location: s.is_some_and(|a| a.location.is_enabled()),
        }
    }

    pub const fn any(self) -> bool {
        self.backtrace || self.spantrace || self.location
    }
}

/// Resolves a field's `forward(...)` for the inject stage, which operates on raw
/// `syn::Field`s before the derive-layer attribute parsing runs.
pub fn field_forward(field: &syn::Field) -> syn::Result<ResolvedForward> {
    use darling::FromAttributes as _;
    let attrs = FieldAttrs::from_attributes(&field.attrs).map_err(syn::Error::from)?;
    Ok(ResolvedForward::resolve(&attrs.forward))
}

// ─── Field-level attributes ──────────────────────────────────────

#[expect(
    clippy::struct_excessive_bools,
    reason = "each boolean attribute keyword maps to its own bool field"
)]
#[derive(Debug, Default, darling::FromAttributes)]
#[darling(attributes(oopsie))]
pub struct FieldAttrs {
    #[darling(default)]
    pub from: SourceKind,
    #[darling(default)]
    pub capture: crate::utils::BetterFlag<false>,
    #[darling(default, multiple)]
    pub provide: Vec<ProvideAttr>,
    #[darling(default)]
    pub backtrace: bool,
    #[darling(default)]
    pub spantrace: bool,
    #[darling(default)]
    pub traces: bool,
    #[darling(default)]
    pub location: bool,
    #[darling(default)]
    pub help: bool,
    #[darling(default)]
    pub forward: crate::utils::FieldSetting<false, ForwardArgs>,
}

#[derive(Debug, Default)]
pub enum SourceKind {
    /// Not a source field (nothing specified).
    #[default]
    No,
    /// Explicit opt-out (`#[oopsie(from(false))]`): never a source, even when
    /// the field is named `source`.
    Disabled,
    /// Marked as source (auto-detected or `#[oopsie(from)]`).
    Yes,
    /// Source with type transformation: `#[oopsie(from(Type, transform))]`.
    Transformed {
        source_type: Box<Type>,
        transform: Expr,
    },
}

/// Inner shape for `from(Type, transform)`. Used only as a parsing helper.
/// The `#[prefix]` consumes the separating comma before `transform` without
/// binding it.
#[derive(derive_syn_parse::Parse)]
struct SourceKindTransform {
    ty: Type,
    #[prefix(Token![,])]
    transform: Expr,
}

impl darling::FromMeta for SourceKind {
    fn from_word() -> darling::Result<Self> {
        Ok(Self::Yes)
    }

    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::Path(_) => Ok(Self::Yes),
            syn::Meta::List(list) => {
                if let Ok(b) = syn::parse2::<syn::LitBool>(list.tokens.clone()) {
                    return Ok(if b.value { Self::Yes } else { Self::Disabled });
                }
                let parsed: SourceKindTransform = syn::parse2(list.tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(&list.tokens))?;
                Ok(Self::Transformed {
                    source_type: Box::new(parsed.ty),
                    transform: parsed.transform,
                })
            }
            syn::Meta::NameValue(nv) => Err(darling::Error::custom(
                "`from` does not accept a `= value` form; use `from(false)` to opt out or `from(Type, transform)` for a custom source type",
            )
            .with_span(&nv.value)),
        }
    }

    fn from_none() -> Option<Self> {
        Some(Self::No)
    }
}

/// `provide(ref, Type => expr)` or `provide(Type => expr)`.
///
/// `ref_kw` is `Some` when the optional `ref` prefix is present. Access via
/// the `is_ref()` helper for clarity at call sites.
#[derive(Debug, Clone, derive_syn_parse::Parse)]
pub struct ProvideAttr {
    pub ref_kw: Option<Token![ref]>,
    #[parse_if(ref_kw.is_some())]
    #[expect(
        dead_code,
        reason = "syntactic punctuation captured by the parser but never read"
    )]
    pub ref_comma: Option<Token![,]>,
    pub provided_type: Type,
    #[prefix(Token![=>])]
    pub expr: Expr,
}

impl ProvideAttr {
    #[inline]
    pub const fn is_ref(&self) -> bool {
        self.ref_kw.is_some()
    }
}

impl darling::FromMeta for ProvideAttr {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::List(list) => syn::parse2(list.tokens.clone())
                .map_err(|e| darling::Error::custom(e).with_span(&list.tokens)),
            other => Err(darling::Error::custom(
                "expected `provide(ref, Type => expr)` or `provide(Type => expr)`",
            )
            .with_span(other)),
        }
    }
}

impl FieldAttrs {
    /// Parse `#[oopsie(...)]` on a field, then apply name/type-based
    /// auto-detection on top:
    /// - field named `source` → `from = Yes` (unless attrs already set `from`)
    /// - field whose type is a backtrace/spantrace/packed-traces type
    ///   → `capture = true`
    /// - `backtrace` / `spantrace` flags imply `capture = true`
    /// - source field whose type is `Box<T>` (non-trait-object) auto-upgrades
    ///   to `Transformed { source_type: T, transform: Box::new }`
    pub fn from_field(field: &syn::Field) -> syn::Result<Self> {
        use darling::FromAttributes as _;

        let mut result = Self::from_attributes(&field.attrs).map_err(syn::Error::from)?;

        // `backtrace` / `spantrace` / `traces` / `location` flags imply
        // `capture`; an explicit opt-out alongside them is contradictory.
        if result.backtrace || result.spantrace || result.traces || result.location {
            if matches!(result.capture, crate::utils::BetterFlag::Disabled) {
                return Err(syn::Error::new_spanned(
                    field,
                    "`capture(false)` cannot be combined with `backtrace`/`spantrace`/`traces`/`location`",
                ));
            }
            result.capture = crate::utils::BetterFlag::Enabled;
        }

        // Auto-source by field name (only if no explicit `from`).
        if matches!(result.from, SourceKind::No)
            && let Some(ident) = &field.ident
            && ident == "source"
        {
            result.from = SourceKind::Yes;
        }

        // Auto-capture for trace fields, detected by type. A field merely
        // *named* `backtrace`/`spantrace` of an unrelated type is an ordinary
        // field, not auto-captured. An explicit `capture(false)` opts out of
        // this detection: the field stays on the selector and is caller-supplied.
        if matches!(result.capture, crate::utils::BetterFlag::Default)
            && (crate::traced::field_detect::is_backtrace_type(&field.ty)
                || crate::traced::field_detect::is_spantrace_type(&field.ty)
                || crate::traced::field_detect::is_traces_type(&field.ty)
                || crate::traced::field_detect::is_location_type(&field.ty))
        {
            result.capture = crate::utils::BetterFlag::Enabled;
        }

        // Auto-boxing: source field with `Box<T>` type (and T is not a trait
        // object — unwrapping `Box<dyn Trait>` would force `?Sized` on the
        // selector's `Source` and break every use site). Explicit
        // `from(T, transform)` already sets `Transformed` and takes precedence.
        if matches!(result.from, SourceKind::Yes)
            && let Some(inner) = crate::traced::field_detect::extract_boxed_inner(&field.ty)
            && !matches!(
                crate::traced::field_detect::peel_groups(inner),
                syn::Type::TraitObject(_)
            )
        {
            result.from = SourceKind::Transformed {
                source_type: Box::new(inner.clone()),
                transform: syn::parse_quote! { ::std::boxed::Box::new },
            };
        }

        Ok(result)
    }

    pub const fn is_source(&self) -> bool {
        match self.from {
            SourceKind::No | SourceKind::Disabled => false,
            SourceKind::Yes | SourceKind::Transformed { .. } => true,
        }
    }
}

// ─── Pre-pass helpers ────────────────────────────────────────────

/// Extracts the short-display form (`#[oopsie("fmt {}", arg)]`) from a list
/// of attributes. Returns the synthesized `DisplayAttr` (if any) along with
/// the remaining attributes (with the short-display attrs removed) so the
/// caller can hand those to darling.
///
/// At most one short display per attribute list — a second one is an error.
/// Short display cannot be combined with meta keywords in the same
/// `#[oopsie(...)]` (preserves the existing diagnostic).
pub fn extract_short_display(
    attrs: &[syn::Attribute],
) -> syn::Result<(Option<DisplayAttr>, Vec<syn::Attribute>)> {
    let mut display: Option<DisplayAttr> = None;
    let mut kept: Vec<syn::Attribute> = Vec::with_capacity(attrs.len());
    for attr in attrs {
        if !attr.path().is_ident("oopsie") {
            kept.push(attr.clone());
            continue;
        }
        // Parse the body: if it starts with a string literal, consume it as a
        // short-display form; otherwise leave the attribute for darling.
        let parsed: Option<DisplayAttr> =
            attr.parse_args_with(|input: ParseStream| -> syn::Result<Option<DisplayAttr>> {
                if input.peek(LitStr) {
                    Ok(Some(input.parse()?))
                } else {
                    // Consume the rest so parse_args_with succeeds; the
                    // attribute is preserved for darling to parse later.
                    let _: proc_macro2::TokenStream = input.parse()?;
                    Ok(None)
                }
            })?;
        match parsed {
            Some(d) => {
                if display.is_some() {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "at most one short-display form per item; use `display(...)` if combining",
                    ));
                }
                display = Some(d);
            }
            None => kept.push(attr.clone()),
        }
    }
    Ok((display, kept))
}

#[cfg(test)]
mod tests {
    use darling::FromMeta as _;
    use syn::parse_quote;

    use super::*;

    // ── SizeConstraint ──────────────────────────────────────────────

    #[test]
    fn size_constraint_exact() {
        let meta: syn::Meta = parse_quote!(size(64));
        assert_eq!(
            SizeConstraint::from_meta(&meta).unwrap(),
            SizeConstraint::Exact(64)
        );
    }

    #[test]
    fn size_constraint_at_most() {
        let meta: syn::Meta = parse_quote!(size(..=128));
        assert_eq!(
            SizeConstraint::from_meta(&meta).unwrap(),
            SizeConstraint::AtMost(128)
        );
    }

    #[test]
    fn size_constraint_at_least() {
        let meta: syn::Meta = parse_quote!(size(32..));
        assert_eq!(
            SizeConstraint::from_meta(&meta).unwrap(),
            SizeConstraint::AtLeast(32)
        );
    }

    #[test]
    fn size_constraint_range() {
        let meta: syn::Meta = parse_quote!(size(32..=64));
        assert_eq!(
            SizeConstraint::from_meta(&meta).unwrap(),
            SizeConstraint::Range(32, 64)
        );
    }

    #[test]
    fn size_constraint_name_value_rejected() {
        let meta: syn::Meta = parse_quote!(size = 64);
        SizeConstraint::from_meta(&meta).unwrap_err();
    }

    // ── ExitCodeAttr ────────────────────────────────────────────────

    #[test]
    fn exit_code_accepts_in_range() {
        let meta: syn::Meta = parse_quote!(exit_code = 78);
        assert_eq!(ExitCodeAttr::from_meta(&meta).unwrap().value, 78);
        let meta: syn::Meta = parse_quote!(exit_code = 1);
        assert_eq!(ExitCodeAttr::from_meta(&meta).unwrap().value, 1);
        let meta: syn::Meta = parse_quote!(exit_code = 255);
        assert_eq!(ExitCodeAttr::from_meta(&meta).unwrap().value, 255);
    }

    #[test]
    fn exit_code_rejects_zero() {
        let meta: syn::Meta = parse_quote!(exit_code = 0);
        ExitCodeAttr::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn exit_code_rejects_out_of_range() {
        let meta: syn::Meta = parse_quote!(exit_code = 300);
        ExitCodeAttr::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn exit_code_rejects_non_integer() {
        let meta: syn::Meta = parse_quote!(exit_code = "x");
        ExitCodeAttr::from_meta(&meta).unwrap_err();
    }

    // ── DisplayAttr ─────────────────────────────────────────────────

    #[test]
    fn display_attr_format_only() {
        let meta: syn::Meta = parse_quote!(display("hello"));
        let d = DisplayAttr::from_meta(&meta).unwrap();
        assert_eq!(d.format_str.value(), "hello");
        assert!(d.args.is_empty());
    }

    #[test]
    fn display_attr_format_with_one_arg() {
        let meta: syn::Meta = parse_quote!(display("hello {}", name));
        let d = DisplayAttr::from_meta(&meta).unwrap();
        assert_eq!(d.format_str.value(), "hello {}");
        assert_eq!(d.args.len(), 1);
    }

    #[test]
    fn display_attr_format_with_multiple_args() {
        let meta: syn::Meta = parse_quote!(display("{} and {}", a, b));
        let d = DisplayAttr::from_meta(&meta).unwrap();
        assert_eq!(d.args.len(), 2);
    }

    #[test]
    fn display_attr_with_non_trivial_expr_args() {
        let meta: syn::Meta = parse_quote!(display("{} and {}", self.0, foo.bar()));
        let d = DisplayAttr::from_meta(&meta).unwrap();
        assert_eq!(d.args.len(), 2);
    }

    #[test]
    fn display_attr_name_value_string_form() {
        // `help = "plain"` builds a zero-arg DisplayAttr.
        let meta: syn::Meta = parse_quote!(help = "plain");
        let d = DisplayAttr::from_meta(&meta).unwrap();
        assert_eq!(d.format_str.value(), "plain");
        assert!(d.args.is_empty());
    }

    // ── format-string helpers ───────────────────────────────────────

    #[test]
    fn placeholder_detection_matches_rustc_escapes() {
        assert!(!format_str_has_placeholder("plain text"));
        assert!(!format_str_has_placeholder("escaped {{ and }}"));
        assert!(!format_str_has_placeholder("a }} lone } close"));
        assert!(format_str_has_placeholder("{field}"));
        assert!(format_str_has_placeholder("positional {}"));
        assert!(format_str_has_placeholder("a {{b}} then {c}"));
        // A lone trailing `{` opens an (ill-formed) placeholder — same as rustc.
        assert!(format_str_has_placeholder("trailing {"));
    }

    #[test]
    fn unescape_collapses_double_braces_in_place() {
        let mut s = "wrap {{names}} and {{{{nested}}}}".to_owned();
        unescape_format_braces(&mut s);
        assert_eq!(s, "wrap {names} and {{nested}}");

        let mut plain = "no braces here".to_owned();
        unescape_format_braces(&mut plain);
        assert_eq!(plain, "no braces here");
    }

    #[test]
    fn is_static_distinguishes_literal_from_interpolated() {
        let plain: DisplayAttr = DisplayAttr::from_meta(&parse_quote!(help = "plain")).unwrap();
        assert!(plain.is_static());

        let escaped: DisplayAttr =
            DisplayAttr::from_meta(&parse_quote!(help = "use {{x}}")).unwrap();
        assert!(escaped.is_static());
        assert_eq!(escaped.static_lit().unwrap().value(), "use {x}");

        let inline: DisplayAttr =
            DisplayAttr::from_meta(&parse_quote!(help = "fix {path}")).unwrap();
        assert!(!inline.is_static());

        let positional: DisplayAttr =
            DisplayAttr::from_meta(&parse_quote!(help("hi {}", name))).unwrap();
        assert!(!positional.is_static());
    }

    #[test]
    fn static_lit_rejects_unmatched_close_brace() {
        // A lone `}` is malformed in a format string (rustc errors on it); the
        // static path must reject it rather than render it literally.
        let stray: DisplayAttr = DisplayAttr::from_meta(&parse_quote!(help = "oops }")).unwrap();
        stray.static_lit().unwrap_err();

        // `}}` is a valid escape and unescapes to a single `}`.
        let escaped: DisplayAttr = DisplayAttr::from_meta(&parse_quote!(help = "ok }}")).unwrap();
        assert_eq!(escaped.static_lit().unwrap().value(), "ok }");
    }

    // ── SourceKind ──────────────────────────────────────────────────

    #[test]
    fn source_kind_word_is_yes() {
        let meta: syn::Meta = parse_quote!(from);
        assert!(matches!(
            SourceKind::from_meta(&meta).unwrap(),
            SourceKind::Yes
        ));
    }

    #[test]
    fn source_kind_with_transform() {
        let meta: syn::Meta = parse_quote!(from(MyType, MyType::from));
        match SourceKind::from_meta(&meta).unwrap() {
            SourceKind::Transformed {
                source_type,
                transform: _,
            } => {
                let s = quote::quote! { #source_type }.to_string();
                assert!(s.contains("MyType"));
            }
            other => panic!("expected Transformed, got {other:?}"),
        }
    }

    #[test]
    fn source_kind_missing_transform_rejected() {
        let meta: syn::Meta = parse_quote!(from(MyType));
        SourceKind::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn source_kind_name_value_rejected() {
        let meta: syn::Meta = parse_quote!(from = "X");
        SourceKind::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn source_kind_from_none_is_no() {
        assert!(matches!(SourceKind::from_none().unwrap(), SourceKind::No));
    }

    #[test]
    fn source_kind_from_false_is_disabled() {
        let meta: syn::Meta = parse_quote!(from(false));
        assert!(matches!(
            SourceKind::from_meta(&meta).unwrap(),
            SourceKind::Disabled
        ));
    }

    #[test]
    fn source_kind_from_true_is_yes() {
        let meta: syn::Meta = parse_quote!(from(true));
        assert!(matches!(
            SourceKind::from_meta(&meta).unwrap(),
            SourceKind::Yes
        ));
    }

    // ── ProvideAttr ─────────────────────────────────────────────────

    #[test]
    fn provide_attr_value_form() {
        let meta: syn::Meta = parse_quote!(provide(MyType => self.value));
        let p = ProvideAttr::from_meta(&meta).unwrap();
        assert!(!p.is_ref());
    }

    #[test]
    fn provide_attr_ref_form() {
        let meta: syn::Meta = parse_quote!(provide(ref, MyType => self.value.as_ref()));
        let p = ProvideAttr::from_meta(&meta).unwrap();
        assert!(p.is_ref());
    }

    #[test]
    fn provide_attr_missing_arrow_rejected() {
        let meta: syn::Meta = parse_quote!(provide(MyType));
        ProvideAttr::from_meta(&meta).unwrap_err();
    }

    // ── extract_short_display ───────────────────────────────────────

    #[test]
    fn short_display_extracts_simple_string() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie("simple message")]
        };
        let (display, kept) = extract_short_display(&attrs).unwrap();
        let d = display.expect("expected a display attr");
        assert_eq!(d.format_str.value(), "simple message");
        assert!(d.args.is_empty());
        assert!(kept.is_empty());
    }

    #[test]
    fn short_display_extracts_with_args() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie("fmt {}", arg)]
        };
        let (display, _kept) = extract_short_display(&attrs).unwrap();
        let d = display.expect("expected a display attr");
        assert_eq!(d.format_str.value(), "fmt {}");
        assert_eq!(d.args.len(), 1);
    }

    #[test]
    fn short_display_allows_non_keyword_call_arg() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie("fmt {}", helper(x))]
        };
        let (display, _kept) = extract_short_display(&attrs).unwrap();
        let d = display.expect("expected a display attr");
        assert_eq!(d.args.len(), 1);
    }

    #[test]
    fn short_display_keeps_long_form_attrs() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(display("fmt"), help = "X")]
        };
        let (display, kept) = extract_short_display(&attrs).unwrap();
        assert!(display.is_none());
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn short_display_rejects_two() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[oopsie("first")]
            #[oopsie("second")]
        };
        extract_short_display(&attrs).unwrap_err();
    }

    #[test]
    fn short_display_passes_through_non_oopsie_attrs() {
        let attrs: Vec<syn::Attribute> = parse_quote! {
            #[other(stuff)]
            #[oopsie("display me")]
            #[cfg(feature = "x")]
        };
        let (display, kept) = extract_short_display(&attrs).unwrap();
        assert!(display.is_some());
        assert_eq!(kept.len(), 2);
    }

    // Typo rejection is handled by darling's strict mode on the per-scope
    // attribute structs; coverage lives in the `from_attrs` integration paths.

    #[test]
    fn short_display_allows_bare_arg_named_like_keyword() {
        let attrs: Vec<syn::Attribute> = parse_quote! { #[oopsie("trace: {}", backtrace)] };
        let (display, _) = extract_short_display(&attrs).unwrap();
        assert_eq!(display.unwrap().args.len(), 1);
    }

    fn fields_of(item: syn::ItemStruct) -> syn::Fields {
        item.fields
    }

    #[test]
    fn reject_keyword_args_flags_non_field_keyword() {
        let d: DisplayAttr =
            DisplayAttr::from_meta(&parse_quote!(display("wrapped", transparent))).unwrap();
        let fields = fields_of(parse_quote! { struct S { source: std::io::Error } });
        d.reject_keyword_args(&fields, DisplayScope::Variant)
            .unwrap_err();
    }

    #[test]
    fn reject_keyword_args_allows_keyword_named_field() {
        let d: DisplayAttr = DisplayAttr::from_meta(&parse_quote!(display("{}", code))).unwrap();
        let fields = fields_of(parse_quote! { struct S { code: u16 } });
        d.reject_keyword_args(&fields, DisplayScope::Variant)
            .unwrap();
    }

    #[test]
    fn reject_keyword_args_allows_non_keyword_and_rich_exprs() {
        let d: DisplayAttr =
            DisplayAttr::from_meta(&parse_quote!(display("{} {}", extra, self.0))).unwrap();
        let fields = fields_of(parse_quote! { struct S { whatever: u8 } });
        d.reject_keyword_args(&fields, DisplayScope::Variant)
            .unwrap();
    }

    #[test]
    fn reject_keyword_args_struct_scope_flags_container_keyword() {
        let d: DisplayAttr = DisplayAttr::from_meta(&parse_quote!(display("x", size))).unwrap();
        let fields = fields_of(parse_quote! { struct S { msg: String } });
        // `size` is container-only: legal on a struct's list, never on a variant.
        d.reject_keyword_args(&fields, DisplayScope::Variant)
            .unwrap();
        d.reject_keyword_args(&fields, DisplayScope::Struct)
            .unwrap_err();
    }

    #[test]
    fn capture_false_keeps_trace_typed_field_on_selector() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { #[oopsie(capture = false)] bt: Backtrace, msg: String }
        };
        let categorized = CategorizedFields::from_fields(&item.fields).unwrap();
        assert!(categorized.user_fields.iter().any(|f| f.ident == "bt"));
        assert!(!categorized.auto_fields.iter().any(|f| f.ident == "bt"));
        // Still surfaced through Diagnostic accessors.
        assert_eq!(
            categorized
                .backtrace_field
                .as_ref()
                .map(ToString::to_string),
            Some("bt".into())
        );
    }

    #[test]
    fn capture_false_with_backtrace_flag_errors() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { #[oopsie(backtrace, capture(false))] bt: Backtrace }
        };
        CategorizedFields::from_fields(&item.fields).unwrap_err();
    }

    // ── CategorizedFields ──────────────────────────────────────────────

    #[test]
    fn categorize_detects_packed_traces_field_by_attr() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { #[oopsie(traces)] t: Box<(Backtrace, SpanTrace)>, msg: String }
        };
        let categorized = CategorizedFields::from_fields(&item.fields).unwrap();
        assert_eq!(
            categorized.traces_field.as_ref().map(ToString::to_string),
            Some("t".to_owned())
        );
        assert!(categorized.auto_fields.iter().any(|f| f.ident == "t"));
        assert!(categorized.backtrace_field.is_none());
        assert!(categorized.spantrace_field.is_none());
    }

    #[test]
    fn categorize_detects_packed_traces_field_by_type() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { t: (Backtrace, SpanTrace) }
        };
        let categorized = CategorizedFields::from_fields(&item.fields).unwrap();
        assert!(categorized.traces_field.is_some());
        assert!(categorized.auto_fields.iter().any(|f| f.ident == "t"));
    }

    #[test]
    fn categorize_ignores_wrongly_typed_trace_named_fields() {
        // A field merely *named* `backtrace`/`spantrace`/`traces` of an
        // unrelated type is an ordinary field, never a trace field.
        let item: syn::ItemStruct = parse_quote! {
            struct S { backtrace: String, spantrace: u32, traces: Vec<String> }
        };
        let categorized = CategorizedFields::from_fields(&item.fields).unwrap();
        assert!(categorized.backtrace_field.is_none());
        assert!(categorized.spantrace_field.is_none());
        assert!(categorized.traces_field.is_none());
        // Left untouched as ordinary (non-captured) user fields.
        for name in ["backtrace", "spantrace", "traces"] {
            assert!(categorized.user_fields.iter().any(|f| f.ident == name));
            assert!(!categorized.auto_fields.iter().any(|f| f.ident == name));
        }
    }

    #[test]
    fn categorize_classifies_trace_fields_by_type_regardless_of_name() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { bt: Backtrace, st: SpanTrace }
        };
        let categorized = CategorizedFields::from_fields(&item.fields).unwrap();
        assert_eq!(
            categorized
                .backtrace_field
                .as_ref()
                .map(ToString::to_string),
            Some("bt".to_owned())
        );
        assert_eq!(
            categorized
                .spantrace_field
                .as_ref()
                .map(ToString::to_string),
            Some("st".to_owned())
        );
    }

    #[test]
    fn categorize_rejects_explicit_backtrace_attr_on_wrong_type() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { #[oopsie(backtrace)] b: String }
        };
        CategorizedFields::from_fields(&item.fields).unwrap_err();
    }

    #[test]
    fn categorize_rejects_packed_traces_alongside_standalone_backtrace() {
        let item: syn::ItemStruct = parse_quote! {
            struct S { traces: (Backtrace, SpanTrace), bt: Backtrace }
        };
        CategorizedFields::from_fields(&item.fields).unwrap_err();
    }
}

/// Categorized fields for a variant/struct.
#[derive(Debug)]
pub struct CategorizedFields {
    /// The source field (if any).
    pub source: Option<SourceField>,
    /// Fields marked with `#[oopsie(capture)]` — excluded from selector.
    pub auto_fields: Vec<AutoField>,
    /// User fields — included in selector.
    pub user_fields: Vec<UserField>,
    /// Provider attributes from fields.
    pub provides: Vec<(Ident, ProvideAttr)>,
    /// Field identified as backtrace (via `#[oopsie(backtrace)]` or `Backtrace` type).
    pub backtrace_field: Option<Ident>,
    /// Field identified as spantrace (via `#[oopsie(spantrace)]` or `SpanTrace` type).
    pub spantrace_field: Option<Ident>,
    /// Field holding the packed `(Backtrace, SpanTrace)` pair (via
    /// `#[oopsie(traces)]` or tuple-type detection).
    pub traces_field: Option<Ident>,
    /// Field holding the captured caller location (via `#[oopsie(location)]` or
    /// `&'static Location<'static>` type detection).
    pub location_field: Option<Ident>,
    /// Field identified as help (via `#[oopsie(help)]`).
    pub help_field: Option<Ident>,
}

#[derive(Debug)]
pub struct SourceField {
    pub ident: Ident,
    pub ty: Type,
    pub kind: SourceKind,
    pub forward: ResolvedForward,
    /// `#[cfg(...)]`/`#[cfg_attr(...)]` attrs on the field, forwarded onto every
    /// generated mention so stripped fields take their references with them.
    pub cfg_attrs: Vec<syn::Attribute>,
}

#[derive(Debug)]
pub struct AutoField {
    pub ident: Ident,
    pub ty: Type,
    /// See [`SourceField::cfg_attrs`].
    pub cfg_attrs: Vec<syn::Attribute>,
}

#[derive(Debug)]
pub struct UserField {
    pub ident: Ident,
    pub ty: Type,
    /// See [`SourceField::cfg_attrs`].
    pub cfg_attrs: Vec<syn::Attribute>,
}

/// Whether any variant carries a `#[cfg(...)]` gate, meaning the set of variants
/// rustc keeps is not knowable at macro-expansion time. Generated matches over
/// `self` then need a wildcard fallback: the attribute-macro path expands before
/// cfg-stripping, so an all-stripped enum would otherwise leave an empty `match`
/// on a still-inhabited reference.
pub fn any_variant_has_cfg(data: &syn::DataEnum) -> bool {
    data.variants
        .iter()
        .any(|v| v.attrs.iter().any(|a| a.path().is_ident("cfg")))
}

/// Field attributes a stripped field would take with it: `#[cfg(...)]` gates and
/// `#[cfg_attr(...)]` conditionals. Forwarded verbatim onto every generated
/// reference (selector field, struct-expression field, match-arm binding) so
/// the reference vanishes together with the field rustc strips.
fn field_cfg_attrs(field: &syn::Field) -> Vec<syn::Attribute> {
    field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
        .cloned()
        .collect()
}

impl CategorizedFields {
    /// Categorize fields of a variant/struct into source, auto, and user fields.
    pub fn from_fields(fields: &syn::Fields) -> syn::Result<Self> {
        use crate::traced::field_detect::{
            is_backtrace_type, is_location_type, is_spantrace_type, is_traces_type,
        };

        let mut source = None;
        let mut auto_fields = Vec::new();
        let mut user_fields = Vec::new();
        let mut provides = Vec::new();
        let mut backtrace_field = None;
        let mut spantrace_field = None;
        let mut traces_field = None;
        let mut location_field = None;
        let mut help_field = None;

        let named = match fields {
            syn::Fields::Named(f) => &f.named,
            syn::Fields::Unit => {
                return Ok(Self {
                    source,
                    auto_fields,
                    user_fields,
                    provides,
                    backtrace_field,
                    spantrace_field,
                    traces_field,
                    location_field,
                    help_field,
                });
            }
            syn::Fields::Unnamed(unnamed) => {
                return Err(syn::Error::new_spanned(
                    unnamed,
                    "#[derive(Oopsie)] does not support tuple variants/structs",
                ));
            }
        };

        for field in named {
            let Some(ident) = field.ident.clone() else {
                continue;
            };
            let attrs = FieldAttrs::from_field(field)?;
            let cfg_attrs = field_cfg_attrs(field);

            // Collect provides
            for p in &attrs.provide {
                provides.push((ident.clone(), p.clone()));
            }

            // Detect backtrace/spantrace/traces/help fields. A trace field is
            // one carrying an explicit `#[oopsie(...)]` attribute or whose type
            // matches (by last path segment). A field merely *named* `backtrace`
            // of the wrong type is an ordinary field — the real trace is injected
            // separately under a mangled name.
            if attrs.backtrace && !is_backtrace_type(&field.ty) {
                return Err(syn::Error::new_spanned(
                    field,
                    "`#[oopsie(backtrace)]` requires a field whose type's last path segment is `Backtrace`",
                ));
            }
            if attrs.spantrace && !is_spantrace_type(&field.ty) {
                return Err(syn::Error::new_spanned(
                    field,
                    "`#[oopsie(spantrace)]` requires a field whose type's last path segment is `SpanTrace`",
                ));
            }
            if attrs.traces && !is_traces_type(&field.ty) {
                return Err(syn::Error::new_spanned(
                    field,
                    "`#[oopsie(traces)]` requires a field of type `(Backtrace, SpanTrace)`",
                ));
            }
            if attrs.location && !is_location_type(&field.ty) {
                return Err(syn::Error::new_spanned(
                    field,
                    "`#[oopsie(location)]` requires a field of type `&'static Location<'static>`",
                ));
            }
            let is_traces = attrs.traces || is_traces_type(&field.ty);
            if attrs.backtrace || is_backtrace_type(&field.ty) {
                if backtrace_field.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one backtrace field per variant/struct",
                    ));
                }
                backtrace_field = Some(ident.clone());
            }
            if attrs.spantrace || is_spantrace_type(&field.ty) {
                if spantrace_field.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one spantrace field per variant/struct",
                    ));
                }
                spantrace_field = Some(ident.clone());
            }
            if is_traces {
                if traces_field.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one packed `traces` field per variant/struct",
                    ));
                }
                traces_field = Some(ident.clone());
            }
            if attrs.location || is_location_type(&field.ty) {
                if location_field.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one location field per variant/struct",
                    ));
                }
                location_field = Some(ident.clone());
            }
            if attrs.help {
                if help_field.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one `#[oopsie(help)]` field per variant/struct",
                    ));
                }
                help_field = Some(ident.clone());
            }

            let forward = ResolvedForward::resolve(&attrs.forward);
            if forward.any() && !attrs.is_source() {
                return Err(syn::Error::new_spanned(
                    field,
                    "`#[oopsie(forward)]` must be on a source field (named `source` or marked `#[oopsie(from)]`)",
                ));
            }

            if attrs.is_source() {
                if source.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "at most one source field per variant/struct",
                    ));
                }
                source = Some(SourceField {
                    ident: ident.clone(),
                    ty: field.ty.clone(),
                    kind: attrs.from,
                    forward,
                    cfg_attrs,
                });
            } else if attrs.capture.is_enabled() {
                auto_fields.push(AutoField {
                    ident,
                    ty: field.ty.clone(),
                    cfg_attrs,
                });
            } else {
                user_fields.push(UserField {
                    ident,
                    ty: field.ty.clone(),
                    cfg_attrs,
                });
            }
        }

        // A packed `traces` field already supplies both traces; a coexisting
        // standalone backtrace/spantrace field would be silently dropped by the
        // `traces`-first emit chains in gen_error, so reject the ambiguity.
        if traces_field.is_some() {
            if let Some(bt) = &backtrace_field {
                return Err(syn::Error::new_spanned(
                    bt,
                    "a packed `traces` field cannot coexist with a separate `backtrace` field",
                ));
            }
            if let Some(st) = &spantrace_field {
                return Err(syn::Error::new_spanned(
                    st,
                    "a packed `traces` field cannot coexist with a separate `spantrace` field",
                ));
            }
        }

        // A forwarded trace and an own field for that same trace would both claim
        // the accessor; reject the ambiguity. Checked post-loop against the
        // accumulated own-trace fields, since the conflicting field is a sibling
        // of the source. (Auto-injected trace fields for forwarded traces are
        // suppressed before this runs, so only user-declared fields remain.)
        if let Some(src) = &source {
            if src.forward.backtrace && (backtrace_field.is_some() || traces_field.is_some()) {
                return Err(syn::Error::new_spanned(
                    &src.ident,
                    "`forward(backtrace)` cannot coexist with an own backtrace/traces field",
                ));
            }
            if src.forward.spantrace && (spantrace_field.is_some() || traces_field.is_some()) {
                return Err(syn::Error::new_spanned(
                    &src.ident,
                    "`forward(spantrace)` cannot coexist with an own spantrace/traces field",
                ));
            }
            if src.forward.location && location_field.is_some() {
                return Err(syn::Error::new_spanned(
                    &src.ident,
                    "`forward(location)` cannot coexist with an own location field",
                ));
            }
        }

        Ok(Self {
            source,
            auto_fields,
            user_fields,
            provides,
            backtrace_field,
            spantrace_field,
            traces_field,
            location_field,
            help_field,
        })
    }
}

#[cfg(test)]
mod forward_tests {
    use darling::FromAttributes as _;

    use super::*;

    fn attrs(a: syn::Attribute) -> FieldAttrs {
        FieldAttrs::from_attributes(&[a]).expect("parse field attrs")
    }

    #[test]
    fn bare_forward_forwards_backtrace_and_spantrace_not_location() {
        let rf = ResolvedForward::resolve(&attrs(syn::parse_quote!(#[oopsie(forward)])).forward);
        assert_eq!(
            rf,
            ResolvedForward {
                backtrace: true,
                spantrace: true,
                location: false
            }
        );
    }

    #[test]
    fn forward_disables_backtrace_and_enables_location() {
        let rf = ResolvedForward::resolve(
            &attrs(syn::parse_quote!(#[oopsie(forward(backtrace = false, location = true))]))
                .forward,
        );
        assert_eq!(
            rf,
            ResolvedForward {
                backtrace: false,
                spantrace: true,
                location: true
            }
        );
    }

    #[test]
    fn forward_accepts_paren_flag_spelling() {
        let rf = ResolvedForward::resolve(
            &attrs(syn::parse_quote!(#[oopsie(forward(location(true)))])).forward,
        );
        assert!(rf.location);
    }

    #[test]
    fn absent_forward_resolves_to_nothing() {
        let rf = ResolvedForward::resolve(&attrs(syn::parse_quote!(#[oopsie(from)])).forward);
        assert_eq!(rf, ResolvedForward::default());
    }

    #[test]
    fn explicit_forward_false_resolves_to_nothing() {
        let rf =
            ResolvedForward::resolve(&attrs(syn::parse_quote!(#[oopsie(forward = false)])).forward);
        assert_eq!(rf, ResolvedForward::default());
    }
}
