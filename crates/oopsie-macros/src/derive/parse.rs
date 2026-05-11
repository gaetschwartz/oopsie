//! Attribute parsing for `#[derive(Oopsie)]`.
//!
//! Parses `#[oopsie(...)]` attributes at three levels:
//! - Container (enum/struct): module, vis, suffix, path
//! - Variant/struct: display, transparent, help, code
//! - Field: from, capture, provide

use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, LitInt, LitStr, Path, Token, Type, Visibility};

// ─── Container-level attributes ──────────────────────────────────

/// Keys accepted inside `#[oopsie(...)]` at the *container* (enum/struct)
/// position. The first five are genuinely container-only; the rest are
/// variant/field-level keys that legitimately appear at struct-container
/// scope (a struct definition serves as both container and variant) and
/// are silently passed through to the variant/field passes.
const KNOWN_CONTAINER_KEYS: &[&str] = &[
    "module",
    "suffix",
    "size",
    "vis",
    "path",
    "display",
    "provide",
    "help",
    "code",
    "transparent",
];

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

#[derive(Debug, Default)]
pub struct ContainerAttrs {
    pub module: ModuleSetting,
    pub visibility: Option<Visibility>,
    pub suffix: SuffixSetting,
    pub path: Option<Path>,
    pub size: Option<SizeConstraint>,
}

#[derive(Debug, Default)]
pub enum ModuleSetting {
    /// Module enabled with optional custom name.
    On(Option<Ident>),
    /// Module disabled.
    Off,
    /// Not specified — use default (on for enums, off for structs).
    #[default]
    Default,
}

#[derive(Debug, Default)]
pub enum SuffixSetting {
    /// No suffix (selector name = variant name).
    Off,
    /// Default suffix "Oopsie".
    Default,
    /// Custom suffix.
    Custom(String),
    /// Not specified — use default (off for enums, "Oopsie" for structs).
    #[default]
    Unset,
}

impl ContainerAttrs {
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("oopsie") {
                continue;
            }
            // Special-case `vis = <Visibility>` because `pub` / `pub(crate)`
            // are keywords and cannot be parsed as `syn::Expr` (which the
            // generic `Meta::NameValue` parser requires). When the attribute
            // body looks like `vis = ...` we parse the tail as a `Visibility`
            // directly and skip the generic Meta path.
            if let Ok((eaten, rest)) = attr.parse_args_with(parse_vis_prefix)
                && let Some(vis) = eaten
            {
                result.visibility = Some(vis);
                if rest.is_empty() {
                    continue;
                }
                // Fall through to parse the remaining comma-separated items.
                let nested = syn::parse::Parser::parse2(
                    Punctuated::<syn::Meta, Token![,]>::parse_terminated,
                    rest,
                )?;
                for meta in &nested {
                    result.parse_container_meta(meta)?;
                }
                continue;
            }
            // Try parsing as Meta items. If the attr starts with a string literal
            // (short display form), skip it — it's a variant/struct-level attr.
            let Ok(nested) =
                attr.parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
            else {
                continue;
            };
            for meta in &nested {
                result.parse_container_meta(meta)?;
            }
        }
        Ok(result)
    }

    fn parse_container_meta(&mut self, meta: &syn::Meta) -> syn::Result<()> {
        // Reject typos early. The match below only handles container-only
        // keys; legitimately variant/field-level keys on a struct container
        // (where the same `#[oopsie(...)]` provides both container and
        // variant data) are silently ignored here — `VariantAttrs` /
        // `FieldAttrs` pick them up on a later pass.
        let key = meta.path().get_ident().map(ToString::to_string);
        if !key
            .as_deref()
            .is_some_and(|k| KNOWN_CONTAINER_KEYS.contains(&k))
        {
            return Err(syn::Error::new_spanned(
                meta.path(),
                match key {
                    Some(name) => format!("unknown oopsie attribute: `{name}`"),
                    None => "unknown oopsie attribute (non-identifier path)".to_owned(),
                },
            ));
        }

        match meta {
            syn::Meta::Path(path) => {
                if path.is_ident("module") {
                    self.module = ModuleSetting::On(None);
                } else if path.is_ident("suffix") {
                    self.suffix = SuffixSetting::Default;
                }
                // Other known path-style keys (e.g. `transparent`) are
                // variant-level and handled by `VariantAttrs`.
            }
            syn::Meta::List(list) => {
                if list.path.is_ident("module") {
                    // module(name) or module(false)
                    let content: ModuleContent = syn::parse2(list.tokens.clone())?;
                    self.module = content.0;
                } else if list.path.is_ident("suffix") {
                    let content: SuffixContent = syn::parse2(list.tokens.clone())?;
                    self.suffix = content.0;
                } else if list.path.is_ident("size") {
                    let content: SizeContent = syn::parse2(list.tokens.clone())?;
                    self.size = Some(content.0);
                }
                // Other known list keys (display, provide, help) are
                // variant-level and handled by `VariantAttrs`.
            }
            syn::Meta::NameValue(nv) => {
                if nv.path.is_ident("vis") {
                    let vis: Visibility = syn::parse2(expr_to_tokens(&nv.value))?;
                    self.visibility = Some(vis);
                } else if nv.path.is_ident("suffix") {
                    if let Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        self.suffix = SuffixSetting::Custom(s.value());
                    } else {
                        return Err(syn::Error::new_spanned(
                            &nv.value,
                            "expected string literal",
                        ));
                    }
                } else if nv.path.is_ident("path") {
                    if let Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        self.path = Some(s.parse()?);
                    } else {
                        return Err(syn::Error::new_spanned(
                            &nv.value,
                            "expected string literal",
                        ));
                    }
                }
                // Other known name-value keys (help, code) are
                // variant-level and handled by `VariantAttrs`.
            }
        }
        Ok(())
    }

    /// Resolve the suffix setting with defaults for the given item kind.
    /// - Enums: default → `Off` (no suffix, selector name = variant name)
    /// - Structs: default → `Default` ("Oopsie" suffix, e.g. `ConnOopsie`)
    pub const fn effective_suffix(&self, is_enum: bool) -> &SuffixSetting {
        match &self.suffix {
            SuffixSetting::Unset => {
                if is_enum {
                    &SuffixSetting::Off
                } else {
                    &SuffixSetting::Default
                }
            }
            other => other,
        }
    }

    /// Resolve the module setting with defaults for the given item kind.
    pub fn effective_module(&self, is_enum: bool) -> ModuleSetting {
        match &self.module {
            ModuleSetting::Default => {
                if is_enum {
                    ModuleSetting::On(None)
                } else {
                    ModuleSetting::Off
                }
            }
            other => match other {
                ModuleSetting::On(name) => ModuleSetting::On(name.clone()),
                ModuleSetting::Off => ModuleSetting::Off,
                ModuleSetting::Default => unreachable!(),
            },
        }
    }
}

struct ModuleContent(ModuleSetting);

impl Parse for ModuleContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::LitBool) {
            let lit: syn::LitBool = input.parse()?;
            if lit.value {
                Ok(Self(ModuleSetting::On(None)))
            } else {
                Ok(Self(ModuleSetting::Off))
            }
        } else {
            let ident: Ident = input.parse()?;
            Ok(Self(ModuleSetting::On(Some(ident))))
        }
    }
}

struct SuffixContent(SuffixSetting);

impl Parse for SuffixContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::LitBool) {
            let lit: syn::LitBool = input.parse()?;
            if lit.value {
                Ok(Self(SuffixSetting::Default))
            } else {
                Ok(Self(SuffixSetting::Off))
            }
        } else if input.peek(LitStr) {
            let s: LitStr = input.parse()?;
            Ok(Self(SuffixSetting::Custom(s.value())))
        } else {
            Err(input.error("expected bool or string literal"))
        }
    }
}

struct SizeContent(SizeConstraint);

impl Parse for SizeContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Try `..=N` first (starts with `..=`)
        if input.peek(Token![..=]) {
            let _: Token![..=] = input.parse()?;
            let lit: LitInt = input.parse()?;
            let n: usize = lit.base10_parse()?;
            return Ok(Self(SizeConstraint::AtMost(n)));
        }

        // Otherwise must start with an integer
        let lit: LitInt = input.parse()?;
        let n: usize = lit.base10_parse()?;

        if input.peek(Token![..=]) {
            // N..=M
            let _: Token![..=] = input.parse()?;
            let lit2: LitInt = input.parse()?;
            let m: usize = lit2.base10_parse()?;
            Ok(Self(SizeConstraint::Range(n, m)))
        } else if input.peek(Token![..]) {
            // N..
            let _: Token![..] = input.parse()?;
            Ok(Self(SizeConstraint::AtLeast(n)))
        } else {
            // Exact(N)
            Ok(Self(SizeConstraint::Exact(n)))
        }
    }
}

// ─── Variant-level attributes ────────────────────────────────────

#[derive(Debug, Default)]
pub struct VariantAttrs {
    pub display: Option<DisplayAttr>,
    pub transparent: bool,
    pub help: Option<DisplayAttr>,
    pub code: Option<String>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone)]
pub struct DisplayAttr {
    pub format_str: LitStr,
    pub args: Vec<Expr>,
}

impl VariantAttrs {
    /// Parse variant-level `#[oopsie(...)]` attributes.
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("oopsie") {
                continue;
            }
            let nested =
                attr.parse_args_with(Punctuated::<OopsieVariantMeta, Token![,]>::parse_terminated)?;
            for item in nested {
                match item {
                    OopsieVariantMeta::ShortDisplay(d) => {
                        result.display = Some(d);
                    }
                    OopsieVariantMeta::Display(d) => {
                        result.display = Some(d);
                    }
                    OopsieVariantMeta::Transparent => {
                        result.transparent = true;
                    }
                    OopsieVariantMeta::Help(s) => {
                        result.help = Some(s);
                    }
                    OopsieVariantMeta::Code(s) => {
                        result.code = Some(s);
                    }
                    OopsieVariantMeta::Vis(v) => {
                        result.visibility = Some(v);
                    }
                    OopsieVariantMeta::Module(())
                    | OopsieVariantMeta::Suffix(())
                    | OopsieVariantMeta::Path(())
                    | OopsieVariantMeta::Size(())
                    | OopsieVariantMeta::Capture
                    | OopsieVariantMeta::From(())
                    | OopsieVariantMeta::Provide(()) => {
                        // Container or field-level attr; skip at variant level
                    }
                }
            }
        }
        Ok(result)
    }
}

/// A single item inside `#[oopsie(...)]`.
#[derive(Debug)]
enum OopsieVariantMeta {
    /// Short form: `"format string"` or `"format string", arg1, arg2`
    ShortDisplay(DisplayAttr),
    /// Long form: `display("format", args...)`
    Display(DisplayAttr),
    /// `transparent`
    Transparent,
    /// `help = "..."` or `help("format {}", args...)`
    Help(DisplayAttr),
    /// `code = "..."`
    Code(String),
    /// `vis = <visibility>`
    Vis(Visibility),
    /// `module` / `module(...)` — container level, skipped here
    Module(()),
    /// `suffix` / `suffix(...)` — container level, skipped here
    Suffix(()),
    /// `path = "..."` — container level, skipped here
    Path(()),
    /// `size(...)` — container level, skipped here
    Size(()),
    /// `capture` — field level, skipped here
    Capture,
    /// `from` / `from(...)` — field level, skipped here
    From(()),
    /// `provide(...)` — field level, skipped here
    Provide(()),
}

impl Parse for OopsieVariantMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Short form: starts with a string literal, followed by optional format args.
        // `#[oopsie("format {}", expr)]` is equivalent to `#[oopsie(display("format {}", expr))]`.
        //
        // The short form consumes ALL remaining tokens — no other meta items (help, code, etc.)
        // are allowed alongside it. This prevents confusing ambiguities like:
        //   `#[oopsie("i need {help}", help = "you")]`
        // where `help` could be a format arg or a meta keyword.
        // Use `display(...)` explicitly when combining with other attributes:
        //   `#[oopsie(display("i need {help}"), help = "you")]`
        if input.peek(LitStr) {
            let format_str: LitStr = input.parse()?;
            let mut args = Vec::new();
            while input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
                if input.is_empty() {
                    break;
                }
                // Check for meta keywords that indicate the user is mixing
                // short-form display with other oopsie attributes.
                if input.peek(Ident) {
                    let ahead = input.fork();
                    let ident = ahead.parse::<Ident>()?;
                    let kw = ident.to_string();
                    let is_keyword = match kw.as_str() {
                        "transparent" | "capture" | "backtrace" | "spantrace" => true,
                        "module" | "suffix" | "from" => true,
                        "display" | "provide" | "size" => ahead.peek(syn::token::Paren),
                        "help" | "code" | "vis" | "path" => {
                            ahead.peek(Token![=]) && !ahead.peek(Token![==])
                        }
                        _ => false,
                    };
                    if is_keyword {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "`{kw}` cannot be combined with the short display form; \
                                 use `#[oopsie(display(...), {kw}...)]` instead"
                            ),
                        ));
                    }
                }
                let arg: Expr = input.parse()?;
                args.push(arg);
            }
            return Ok(Self::ShortDisplay(DisplayAttr { format_str, args }));
        }

        // Keyword-based forms
        let ident: Ident = input.parse()?;
        let ident_str = ident.to_string();

        match ident_str.as_str() {
            "display" => {
                let content;
                syn::parenthesized!(content in input);
                let format_str: LitStr = content.parse()?;
                let mut args = Vec::new();
                while content.peek(Token![,]) {
                    let _: Token![,] = content.parse()?;
                    if content.is_empty() {
                        break;
                    }
                    let arg: Expr = content.parse()?;
                    args.push(arg);
                }
                Ok(Self::Display(DisplayAttr { format_str, args }))
            }
            "transparent" => Ok(Self::Transparent),
            "help" => {
                if input.peek(syn::token::Paren) {
                    // help("format {}", arg1, arg2)
                    let content;
                    syn::parenthesized!(content in input);
                    let format_str: LitStr = content.parse()?;
                    let mut args = Vec::new();
                    while content.peek(Token![,]) {
                        let _: Token![,] = content.parse()?;
                        if content.is_empty() {
                            break;
                        }
                        let arg: Expr = content.parse()?;
                        args.push(arg);
                    }
                    Ok(Self::Help(DisplayAttr { format_str, args }))
                } else {
                    // help = "plain string"
                    let _: Token![=] = input.parse()?;
                    let lit: LitStr = input.parse()?;
                    Ok(Self::Help(DisplayAttr {
                        format_str: lit,
                        args: vec![],
                    }))
                }
            }
            "code" => {
                let _: Token![=] = input.parse()?;
                let lit: LitStr = input.parse()?;
                Ok(Self::Code(lit.value()))
            }
            "vis" => {
                let _: Token![=] = input.parse()?;
                let vis: Visibility = input.parse()?;
                Ok(Self::Vis(vis))
            }
            "module" => {
                // Skip content if present
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let _ = content.parse::<proc_macro2::TokenStream>()?;
                }
                Ok(Self::Module(()))
            }
            "suffix" => {
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let _ = content.parse::<proc_macro2::TokenStream>()?;
                } else if input.peek(Token![=]) {
                    let _: Token![=] = input.parse()?;
                    let _: LitStr = input.parse()?;
                }
                Ok(Self::Suffix(()))
            }
            "path" => {
                let _: Token![=] = input.parse()?;
                let _: LitStr = input.parse()?;
                Ok(Self::Path(()))
            }
            "size" => {
                let content;
                syn::parenthesized!(content in input);
                let _ = content.parse::<proc_macro2::TokenStream>()?;
                Ok(Self::Size(()))
            }
            "capture" => Ok(Self::Capture),
            "from" => {
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let _ = content.parse::<proc_macro2::TokenStream>()?;
                }
                Ok(Self::From(()))
            }
            "provide" => {
                let content;
                syn::parenthesized!(content in input);
                let _ = content.parse::<proc_macro2::TokenStream>()?;
                Ok(Self::Provide(()))
            }
            _ => Err(syn::Error::new(
                ident.span(),
                format!("unknown oopsie attribute: {ident_str}"),
            )),
        }
    }
}

// ─── Field-level attributes ──────────────────────────────────────

#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
pub struct FieldAttrs {
    pub from: SourceKind,
    pub capture: bool,
    pub provide: Vec<ProvideAttr>,
    pub backtrace: bool,
    pub spantrace: bool,
    pub help: bool,
}

#[derive(Debug, Default)]
pub enum SourceKind {
    /// Not a source field.
    #[default]
    No,
    /// Marked as source (auto-detected or `#[oopsie(from)]`).
    Yes,
    /// Source with type transformation: `#[oopsie(from(Type, transform))]`.
    Transformed {
        source_type: Box<Type>,
        transform: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct ProvideAttr {
    pub is_ref: bool,
    pub provided_type: Type,
    pub expr: Expr,
}

impl FieldAttrs {
    pub fn from_field(field: &syn::Field) -> syn::Result<Self> {
        let mut result = Self::default();

        // Auto-detect source field by name
        if let Some(ident) = &field.ident
            && ident == "source"
        {
            result.from = SourceKind::Yes;
        }

        // Auto-detect backtrace/spantrace fields by name
        if let Some(ident) = &field.ident
            && (ident == "backtrace"
                || ident == "back_trace"
                || ident == "spantrace"
                || ident == "span_trace")
        {
            result.capture = true;
        }

        for attr in &field.attrs {
            if !attr.path().is_ident("oopsie") {
                continue;
            }
            let nested =
                attr.parse_args_with(Punctuated::<FieldMeta, Token![,]>::parse_terminated)?;
            for item in nested {
                match item {
                    FieldMeta::From(kind) => {
                        result.from = kind;
                    }
                    FieldMeta::Capture => {
                        result.capture = true;
                    }
                    FieldMeta::Provide(p) => {
                        result.provide.push(*p);
                    }
                    FieldMeta::Backtrace => {
                        result.backtrace = true;
                        result.capture = true;
                    }
                    FieldMeta::Spantrace => {
                        result.spantrace = true;
                        result.capture = true;
                    }
                    FieldMeta::Help => {
                        result.help = true;
                    }
                }
            }
        }

        // Auto-boxing: if field type is Box<T> and source was auto-detected
        // (SourceKind::Yes), upgrade to Transformed with Box::new.
        // Explicit `from(T, transform)` already sets Transformed, so it takes precedence.
        if matches!(result.from, SourceKind::Yes)
            && let Some(inner) = crate::traced::field_detect::extract_boxed_inner(&field.ty)
        {
            result.from = SourceKind::Transformed {
                source_type: Box::new(inner.clone()),
                transform: syn::parse_quote! { ::std::boxed::Box::new },
            };
        }

        Ok(result)
    }

    pub const fn is_source(&self) -> bool {
        !matches!(self.from, SourceKind::No)
    }
}

#[derive(Debug)]
enum FieldMeta {
    From(SourceKind),
    Capture,
    Provide(Box<ProvideAttr>),
    Backtrace,
    Spantrace,
    Help,
}

impl Parse for FieldMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "from" => {
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let source_type: Type = content.parse()?;
                    let _: Token![,] = content.parse()?;
                    let transform: Expr = content.parse()?;
                    Ok(Self::From(SourceKind::Transformed {
                        source_type: Box::new(source_type),
                        transform,
                    }))
                } else {
                    Ok(Self::From(SourceKind::Yes))
                }
            }
            "capture" => Ok(Self::Capture),
            "backtrace" => Ok(Self::Backtrace),
            "spantrace" => Ok(Self::Spantrace),
            "help" => Ok(Self::Help),
            "provide" => {
                let content;
                syn::parenthesized!(content in input);
                let is_ref = if content.peek(Token![ref]) {
                    let _: Token![ref] = content.parse()?;
                    let _: Token![,] = content.parse()?;
                    true
                } else {
                    false
                };
                let provided_type: Type = content.parse()?;
                let _: Token![=>] = content.parse()?;
                let expr: Expr = content.parse()?;
                Ok(Self::Provide(Box::new(ProvideAttr {
                    is_ref,
                    provided_type,
                    expr,
                })))
            }
            other => Err(syn::Error::new(
                ident.span(),
                format!("unknown oopsie field attribute: {other}"),
            )),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────

fn expr_to_tokens(expr: &Expr) -> proc_macro2::TokenStream {
    use quote::ToTokens as _;
    expr.to_token_stream()
}

/// Special-case parser for `vis = <Visibility>` at the front of an attribute
/// list (e.g. `#[oopsie(vis = pub, module(foo))]`).
///
/// Returns `(Some(vis), rest)` if the input starts with `vis = ...`, where
/// `rest` is the remaining tokens (everything after the visibility). Returns
/// `(None, rest)` if the input does not begin with `vis =`.
fn parse_vis_prefix(
    input: ParseStream,
) -> syn::Result<(Option<syn::Visibility>, proc_macro2::TokenStream)> {
    let fork = input.fork();
    if fork.peek(syn::Ident) {
        let ident: syn::Ident = fork.parse()?;
        if ident == "vis" && fork.peek(Token![=]) && !fork.peek(Token![==]) {
            // Commit to the fork by re-parsing `vis = <Visibility>` on the
            // real input.
            let _: syn::Ident = input.parse()?;
            let _: Token![=] = input.parse()?;
            let vis: syn::Visibility = input.parse()?;
            // Optional trailing comma
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
            let rest = input.parse::<proc_macro2::TokenStream>()?;
            return Ok((Some(vis), rest));
        }
    }
    let rest = input.parse::<proc_macro2::TokenStream>()?;
    Ok((None, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_size(tokens: proc_macro2::TokenStream) -> syn::Result<SizeConstraint> {
        let content: SizeContent = syn::parse2(tokens)?;
        Ok(content.0)
    }

    #[test]
    fn test_size_exact() {
        let result = parse_size(quote::quote! { 64 }).unwrap();
        assert_eq!(result, SizeConstraint::Exact(64));
    }

    #[test]
    fn test_size_at_most() {
        let result = parse_size(quote::quote! { ..=128 }).unwrap();
        assert_eq!(result, SizeConstraint::AtMost(128));
    }

    #[test]
    fn test_size_at_least() {
        let result = parse_size(quote::quote! { 32.. }).unwrap();
        assert_eq!(result, SizeConstraint::AtLeast(32));
    }

    #[test]
    fn test_size_range() {
        let result = parse_size(quote::quote! { 32..=64 }).unwrap();
        assert_eq!(result, SizeConstraint::Range(32, 64));
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
    /// Field identified as backtrace (via `#[oopsie(backtrace)]` or name detection).
    pub backtrace_field: Option<Ident>,
    /// Field identified as spantrace (via `#[oopsie(spantrace)]` or name detection).
    pub spantrace_field: Option<Ident>,
    /// Field identified as help (via `#[oopsie(help)]`).
    pub help_field: Option<Ident>,
}

#[derive(Debug)]
pub struct SourceField {
    pub ident: Ident,
    pub ty: Type,
    pub kind: SourceKind,
}

#[derive(Debug)]
pub struct AutoField {
    pub ident: Ident,
    pub ty: Type,
}

#[derive(Debug)]
pub struct UserField {
    pub ident: Ident,
    pub ty: Type,
}

impl CategorizedFields {
    /// Categorize fields of a variant/struct into source, auto, and user fields.
    pub fn from_fields(fields: &syn::Fields) -> syn::Result<Self> {
        let mut source = None;
        let mut auto_fields = Vec::new();
        let mut user_fields = Vec::new();
        let mut provides = Vec::new();
        let mut backtrace_field = None;
        let mut spantrace_field = None;
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

            // Collect provides
            for p in &attrs.provide {
                provides.push((ident.clone(), p.clone()));
            }

            // Detect backtrace/spantrace/help fields
            let ident_str = ident.to_string();
            if attrs.backtrace || ident_str == "backtrace" || ident_str == "back_trace" {
                backtrace_field = Some(ident.clone());
            }
            if attrs.spantrace || ident_str == "spantrace" || ident_str == "span_trace" {
                spantrace_field = Some(ident.clone());
            }
            if attrs.help {
                help_field = Some(ident.clone());
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
                });
            } else if attrs.capture {
                auto_fields.push(AutoField {
                    ident,
                    ty: field.ty.clone(),
                });
            } else {
                user_fields.push(UserField {
                    ident,
                    ty: field.ty.clone(),
                });
            }
        }

        Ok(Self {
            source,
            auto_fields,
            user_fields,
            provides,
            backtrace_field,
            spantrace_field,
            help_field,
        })
    }
}
