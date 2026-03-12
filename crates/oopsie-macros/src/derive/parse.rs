//! Attribute parsing for `#[derive(Oopsie)]`.
//!
//! Parses `#[oopsie(...)]` attributes at three levels:
//! - Container (enum/struct): module, vis, suffix, path
//! - Variant/struct: display, transparent, help, code
//! - Field: from, auto, provide

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, LitStr, Path, Token, Type, Visibility};

// ─── Container-level attributes ──────────────────────────────────

#[derive(Debug, Default)]
pub struct ContainerAttrs {
    pub module: ModuleSetting,
    pub visibility: Option<Visibility>,
    pub suffix: SuffixSetting,
    pub path: Option<Path>,
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
    #[default]
    Off,
    /// Default suffix "Oopsie".
    Default,
    /// Custom suffix.
    Custom(String),
}

impl ContainerAttrs {
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("oopsie") {
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
        match meta {
            syn::Meta::Path(path) => {
                if path.is_ident("module") {
                    self.module = ModuleSetting::On(None);
                } else if path.is_ident("suffix") {
                    self.suffix = SuffixSetting::Default;
                } else {
                    return Err(syn::Error::new_spanned(path, "unknown oopsie attribute"));
                }
            }
            syn::Meta::List(list) => {
                if list.path.is_ident("module") {
                    // module(name) or module(false)
                    let content: ModuleContent = syn::parse2(list.tokens.clone())?;
                    self.module = content.0;
                } else if list.path.is_ident("suffix") {
                    let content: SuffixContent = syn::parse2(list.tokens.clone())?;
                    self.suffix = content.0;
                } else {
                    // Don't error on unknown list attrs at container level -
                    // they might be variant-level attrs on a struct
                }
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
                } else {
                    // Ignore unknown name-value attrs - they may be variant-level on structs
                }
            }
        }
        Ok(())
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
                    | OopsieVariantMeta::Auto
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
    /// `auto` — field level, skipped here
    Auto,
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
                        "transparent" | "auto" => true,
                        "module" | "suffix" | "from" => true,
                        "display" | "provide" => ahead.peek(syn::token::Paren),
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
            "auto" => Ok(Self::Auto),
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

#[derive(Debug, Default)]
pub struct FieldAttrs {
    pub from: SourceKind,
    pub auto: bool,
    pub provide: Vec<ProvideAttr>,
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
                    FieldMeta::Auto => {
                        result.auto = true;
                    }
                    FieldMeta::Provide(p) => {
                        result.provide.push(*p);
                    }
                }
            }
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
    Auto,
    Provide(Box<ProvideAttr>),
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
            "auto" => Ok(Self::Auto),
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

/// Categorized fields for a variant/struct.
#[derive(Debug)]
pub struct CategorizedFields {
    /// The source field (if any).
    pub source: Option<SourceField>,
    /// Fields marked with `#[oopsie(auto)]` — excluded from selector.
    pub auto_fields: Vec<AutoField>,
    /// User fields — included in selector.
    pub user_fields: Vec<UserField>,
    /// Provider attributes from fields.
    pub provides: Vec<(Ident, ProvideAttr)>,
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

        let named = match fields {
            syn::Fields::Named(f) => &f.named,
            syn::Fields::Unit => {
                return Ok(Self {
                    source,
                    auto_fields,
                    user_fields,
                    provides,
                });
            }
            syn::Fields::Unnamed(_) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "#[derive(Oopsie)] does not support tuple variants/structs",
                ));
            }
        };

        for field in named {
            let ident = field.ident.clone().expect("named field has ident");
            let attrs = FieldAttrs::from_field(field)?;

            // Collect provides
            for p in &attrs.provide {
                provides.push((ident.clone(), p.clone()));
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
            } else if attrs.auto {
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
        })
    }
}
