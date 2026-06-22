#![allow(
    dead_code,
    reason = "shared macro-helper utilities; not every helper is used by every derive"
)]
use std::ops::Deref;

use darling::FromMeta;
use syn::MetaNameValue;

#[derive(Clone, Debug)]
pub enum FieldSetting<const DEFAULT: bool, T: FromMeta> {
    Settings(Settings<T>),
    Flag(bool),
}

impl<const DEFAULT: bool, T: FromMeta> FieldSetting<DEFAULT, T> {
    pub const fn opt_settings(&self) -> Option<&T> {
        match self {
            Self::Settings(settings) => Some(&settings.settings),
            Self::Flag(_) => None,
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Settings(settings) => settings.enabled.unwrap_or(true),
            Self::Flag(value) => *value,
        }
    }
}
impl<const DEFAULT: bool, T: FromMeta + Default + Clone> FieldSetting<DEFAULT, T> {
    #[inline]
    pub fn settings(&self) -> std::borrow::Cow<'_, T> {
        match self {
            Self::Settings(settings) => std::borrow::Cow::Borrowed(&settings.settings),
            Self::Flag(_) => std::borrow::Cow::Owned(T::default()),
        }
    }
}

impl<const DEFAULT: bool, T: FromMeta> FromMeta for FieldSetting<DEFAULT, T> {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::Path(_) => Ok(Self::Flag(true)),
            // `field = true` / `field = false`
            syn::Meta::NameValue(syn::MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Bool(b),
                        ..
                    }),
                ..
            }) => Ok(Self::Flag(b.value)),
            // `field(true)` / `field(false)`
            syn::Meta::List(list) => {
                if let Ok(b) = syn::parse2::<syn::LitBool>(list.tokens.clone()) {
                    return Ok(Self::Flag(b.value));
                }
                Settings::<T>::from_meta(item).map(Self::Settings)
            }
            syn::Meta::NameValue(_) => Settings::<T>::from_meta(item).map(Self::Settings),
        }
    }

    fn from_bool(value: bool) -> darling::Result<Self> {
        Ok(Self::Flag(value))
    }

    fn from_none() -> Option<Self> {
        Some(Self::Flag(DEFAULT))
    }
}

impl<const DEFAULT: bool, T: FromMeta> Default for FieldSetting<DEFAULT, T> {
    #[inline]
    fn default() -> Self {
        Self::Flag(DEFAULT)
    }
}

#[derive(Clone, Debug, darling::FromMeta)]
pub struct Settings<T> {
    enabled: Option<bool>,
    #[darling(flatten)]
    settings: T,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BetterFlag<const DEFAULT: bool> {
    Enabled,
    Disabled,
    #[default]
    Default,
}

impl<const DEFAULT: bool> BetterFlag<DEFAULT> {
    pub const fn is_enabled(self) -> bool {
        match self {
            Self::Enabled => true,
            Self::Disabled => false,
            Self::Default => DEFAULT,
        }
    }

    pub const fn to_option(self) -> Option<bool> {
        match self {
            Self::Enabled => Some(true),
            Self::Disabled => Some(false),
            Self::Default => None,
        }
    }
}

impl<const DEFAULT: bool> FromMeta for BetterFlag<DEFAULT> {
    fn from_none() -> Option<Self> {
        Some(Self::Default)
    }

    fn from_value(value: &syn::Lit) -> darling::Result<Self> {
        match value {
            syn::Lit::Bool(b) => {
                if b.value {
                    Ok(Self::Enabled)
                } else {
                    Ok(Self::Disabled)
                }
            }
            _ => Err(darling::Error::unexpected_type("expected boolean").with_span(value)),
        }
    }

    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::Path(_) => Ok(Self::Enabled),
            syn::Meta::List(metalist) => {
                let lit = metalist.parse_args::<syn::Lit>()?;
                Self::from_value(&lit)
            }
            syn::Meta::NameValue(MetaNameValue { value, .. }) => match value {
                syn::Expr::Lit(expr_lit) => Self::from_value(&expr_lit.lit),
                _ => Err(darling::Error::unexpected_type("expected literal").with_span(value)),
            },
        }
    }
}

/// Bridge `syn::Parse → darling::FromMeta`.
///
/// Accepts `key(tokens)` (parses tokens via `syn::parse2::<T>`) and
/// `key = "tokens"` (parses the string literal's contents as `T`). Bare-flag
/// form (`key` alone) is an error; use a flag/tristate type when you need it.
pub struct SynParse<T: syn::parse::Parse>(pub T);

impl<T: syn::parse::Parse + std::fmt::Debug> std::fmt::Debug for SynParse<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SynParse").field(&self.0).finish()
    }
}

impl<T: syn::parse::Parse> Deref for SynParse<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: syn::parse::Parse> FromMeta for SynParse<T> {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::List(list) => syn::parse2(list.tokens.clone())
                .map(Self)
                .map_err(|e| darling::Error::custom(e).with_span(&list.tokens)),
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => s
                    .parse()
                    .map(Self)
                    .map_err(|e| darling::Error::custom(e).with_span(s)),
                other => Err(
                    darling::Error::custom("expected `key(...)` or `key = \"...\"`")
                        .with_span(other),
                ),
            },
            syn::Meta::Path(p) => Err(darling::Error::custom(
                "expected a value, e.g. `key(...)` or `key = \"...\"`",
            )
            .with_span(p)),
        }
    }
}

/// Tristate for keys that accept three syntactic shapes:
/// `key` (alone), `key(true|false)` (bool toggle), or `key(value)` / `key = value`.
///
/// Parser stays dumb: `Bool(true)` and `Alone` are distinct variants — the
/// resolver folds them (see `effective_module` / `effective_suffix`).
/// Use `Option<MaybeAloneOopsieValue<T>>` in attribute structs to represent
/// absence; this enum has no "Unset" variant.
#[derive(Debug, Clone)]
pub enum MaybeAloneOopsieValue<T> {
    Alone,
    Bool(bool),
    Value(T),
}

impl<T: FromMeta> FromMeta for MaybeAloneOopsieValue<T> {
    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        match item {
            syn::Meta::Path(_) => Ok(Self::Alone),
            syn::Meta::List(list) => {
                if let Ok(b) = syn::parse2::<syn::LitBool>(list.tokens.clone()) {
                    return Ok(Self::Bool(b.value));
                }
                // Re-route `key(value)` through `T::from_expr` — most `FromMeta`
                // impls (e.g. `syn::Ident`, `String`) override `from_expr` but
                // not `from_list`, so the default dispatch on `Meta::List`
                // would otherwise hit `from_list` and fail.
                let expr: syn::Expr = syn::parse2(list.tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(&list.tokens))?;
                T::from_expr(&expr).map(Self::Value)
            }
            syn::Meta::NameValue(_) => T::from_meta(item).map(Self::Value),
        }
    }
}

/// A token tying generated code to the consumer's `Cargo.toml` so editing
/// `[package.metadata.oopsie]` re-runs the macro; nothing when `settings` is off.
pub fn manifest_dep_token() -> proc_macro2::TokenStream {
    #[cfg(feature = "settings")]
    {
        settings::manifest_dep_token()
    }
    #[cfg(not(feature = "settings"))]
    {
        quote::quote! {}
    }
}

/// A project-wide selector-suffix default, resolved from the manifest.
pub enum SuffixDefault {
    /// No suffix — selector name equals the variant/type name.
    Off,
    /// A custom suffix appended to the stripped name.
    Name(String),
}

/// Resolved naming/visibility defaults from `[package.metadata.oopsie]`. Each is
/// `None` when unset (the per-kind hardcoded default applies); a per-type
/// `#[oopsie(...)]` attribute always overrides these.
#[derive(Default)]
pub struct NamingDefaults {
    pub module: Option<bool>,
    pub module_suffix: Option<String>,
    pub suffix: Option<SuffixDefault>,
    pub vis: Option<syn::Visibility>,
}

/// Resolved `traced` defaults from `[package.metadata.oopsie]`. `None` keeps the
/// hardcoded default; a per-attribute `traced(...)` setting overrides these.
#[derive(Default)]
pub struct TracedDefaults {
    pub traced: Option<bool>,
    pub location: Option<bool>,
    pub timestamp: Option<bool>,
    pub packed: Option<bool>,
    pub boxed: Option<bool>,
    pub code: Option<bool>,
}

/// Naming/visibility defaults plus a `compile_error!` for an invalid manifest
/// (empty when the `settings` feature is off or nothing is configured).
pub fn manifest_naming() -> (NamingDefaults, proc_macro2::TokenStream) {
    #[cfg(feature = "settings")]
    {
        settings::naming_defaults()
    }
    #[cfg(not(feature = "settings"))]
    {
        (NamingDefaults::default(), quote::quote! {})
    }
}

/// `traced` defaults plus a `compile_error!` for an invalid manifest (empty when
/// the `settings` feature is off or nothing is configured).
pub fn manifest_traced() -> (TracedDefaults, proc_macro2::TokenStream) {
    #[cfg(feature = "settings")]
    {
        settings::traced_defaults()
    }
    #[cfg(not(feature = "settings"))]
    {
        (TracedDefaults::default(), quote::quote! {})
    }
}

// Per-knob naming accessors for the deep codegen sites. They drop the validation
// `compile_error!` — that is surfaced once via `manifest_naming().1` at the
// expansion entry, which aborts the build, so the defaults returned here on a
// bad manifest never reach generated code.
fn naming() -> NamingDefaults {
    manifest_naming().0
}

pub fn manifest_module_default() -> Option<bool> {
    naming().module
}

pub fn manifest_module_suffix() -> Option<String> {
    naming().module_suffix
}

pub fn manifest_suffix_default() -> Option<SuffixDefault> {
    naming().suffix
}

pub fn manifest_vis_default() -> Option<syn::Visibility> {
    naming().vis
}

#[cfg(feature = "settings")]
pub mod settings {
    use std::str::FromStr as _;
    use std::sync::OnceLock;

    use __serde::Deserialize as _;
    use __serde::de::IntoDeserializer as _;
    use quote::quote;

    #[derive(Debug, Clone, Default, __serde::Deserialize)]
    #[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
    pub struct Settings {
        max_size: Option<usize>,
        default_suffix: Option<RawSuffix>,
        default_vis: Option<String>,
        module: Option<ModuleSetting>,
        traced: Option<TracedSetting>,
    }

    /// `default-suffix` accepts a name string or `false` (disable). `true` is
    /// rejected as ambiguous (it would force the struct-style suffix onto enums).
    #[derive(Debug, Clone, __serde::Deserialize)]
    #[serde(crate = "__serde", untagged)]
    enum RawSuffix {
        Toggle(bool),
        Name(String),
    }

    /// `module` is either a bool (wrap on/off) or a table of options.
    #[derive(Debug, Clone, __serde::Deserialize)]
    #[serde(crate = "__serde", untagged)]
    enum ModuleSetting {
        Toggle(bool),
        Table(ModuleTable),
    }

    #[derive(Debug, Clone, __serde::Deserialize)]
    #[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
    struct ModuleTable {
        enabled: Option<bool>,
        suffix: Option<String>,
    }

    /// `traced` is either a bool (trace-by-default on/off) or a table whose keys
    /// set the defaults for the matching `traced(...)` sub-toggles. `enabled`
    /// controls trace-by-default; the sub-toggle defaults apply whenever tracing
    /// is active (globally or per type).
    #[derive(Debug, Clone, __serde::Deserialize)]
    #[serde(crate = "__serde", untagged)]
    enum TracedSetting {
        Toggle(bool),
        Table(TracedTable),
    }

    #[derive(Debug, Clone, __serde::Deserialize)]
    #[serde(crate = "__serde", rename_all = "kebab-case", deny_unknown_fields)]
    struct TracedTable {
        enabled: Option<bool>,
        location: Option<bool>,
        timestamp: Option<bool>,
        packed: Option<bool>,
        boxed: Option<bool>,
        code: Option<bool>,
    }

    const MANIFEST_ENV_VAR: &str = "CARGO_MANIFEST_DIR";

    fn compile_error(msg: &str) -> proc_macro2::TokenStream {
        quote! { ::core::compile_error!(#msg); }
    }

    /// A non-empty run of identifier characters, so appending it to `name_`
    /// yields a valid identifier.
    fn is_ident_fragment(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Parse `[package.metadata.oopsie]` out of a Cargo manifest. A manifest with
    /// no such section yields default (empty) settings; only a malformed section
    /// or an unknown / invalid key is an error.
    fn parse_settings(manifest_toml: &str) -> Result<Settings, String> {
        let mut manifest =
            toml_edit::DocumentMut::from_str(manifest_toml).map_err(|e| e.to_string())?;
        let Some(metadata) = manifest
            .as_table_mut()
            .get_mut("package")
            .and_then(|item| item.as_table_mut())
            .and_then(|table| table.get_mut("metadata"))
            .and_then(|item| item.as_table_mut())
            .and_then(|table| table.get_mut("oopsie"))
            .and_then(|item| item.as_table_mut())
        else {
            return Ok(Settings::default());
        };
        Settings::deserialize(
            toml_edit::Value::from(std::mem::take(metadata).into_inline_table())
                .into_deserializer(),
        )
        .map_err(|e| format!("invalid [package.metadata.oopsie] section: {e}"))
    }

    fn read_settings() -> Result<Settings, String> {
        let manifest_dir = std::env::var(MANIFEST_ENV_VAR).map_err(|e| e.to_string())?;
        let cargo_toml_path = std::path::Path::new(&manifest_dir).join("Cargo.toml");
        let content = std::fs::read_to_string(&cargo_toml_path)
            .map_err(|e| format!("failed to read {}: {e}", cargo_toml_path.display()))?;
        parse_settings(&content)
    }

    fn cached_settings() -> &'static Result<Settings, String> {
        static CACHE: OnceLock<Result<Settings, String>> = OnceLock::new();
        CACHE.get_or_init(read_settings)
    }

    /// Reject `Some(0)` — a zero cap can never be satisfied, so it's a config
    /// mistake rather than a meaningful limit.
    fn validate_cap(max_size: Option<usize>) -> Result<Option<usize>, String> {
        match max_size {
            Some(0) => Err(
                "[package.metadata.oopsie] max-size is 0, which is not a valid size \
                            limit; remove the key to disable the cap instead"
                    .to_owned(),
            ),
            Some(n) => Ok(Some(n)),
            None => Ok(None),
        }
    }

    /// The validated project-wide size cap declared in the consumer's manifest.
    pub fn cap() -> Result<Option<usize>, String> {
        match cached_settings() {
            Ok(settings) => validate_cap(settings.max_size),
            Err(e) => Err(e.clone()),
        }
    }

    /// Resolve and validate the naming/visibility defaults. A manifest *parse*
    /// error is reported once via [`cap`]; defer to it here (returning defaults +
    /// no token) so a malformed manifest doesn't emit the same error per accessor.
    pub fn naming_defaults() -> (super::NamingDefaults, proc_macro2::TokenStream) {
        let Ok(settings) = cached_settings() else {
            return (super::NamingDefaults::default(), quote! {});
        };
        match resolve_naming(settings) {
            Ok(naming) => (naming, quote! {}),
            Err(e) => (super::NamingDefaults::default(), compile_error(&e)),
        }
    }

    fn resolve_naming(settings: &Settings) -> Result<super::NamingDefaults, String> {
        let (module, module_suffix) = match &settings.module {
            None => (None, None),
            Some(ModuleSetting::Toggle(b)) => (Some(*b), None),
            Some(ModuleSetting::Table(t)) => {
                let suffix = match &t.suffix {
                    Some(s) if !is_ident_fragment(s) => {
                        return Err(format!(
                            "[package.metadata.oopsie] module.suffix {s:?} is not a valid identifier fragment"
                        ));
                    }
                    other => other.clone(),
                };
                (t.enabled, suffix)
            }
        };
        let suffix = match &settings.default_suffix {
            None => None,
            Some(RawSuffix::Toggle(false)) => Some(super::SuffixDefault::Off),
            Some(RawSuffix::Toggle(true)) => {
                return Err(
                    "[package.metadata.oopsie] default-suffix = true is ambiguous; \
                            use a suffix name or `false` to disable"
                        .to_owned(),
                );
            }
            Some(RawSuffix::Name(s)) if !is_ident_fragment(s) => {
                return Err(format!(
                    "[package.metadata.oopsie] default-suffix {s:?} is not a valid identifier fragment"
                ));
            }
            Some(RawSuffix::Name(s)) => Some(super::SuffixDefault::Name(s.clone())),
        };
        let vis = match &settings.default_vis {
            None => None,
            Some(v) => Some(syn::parse_str::<syn::Visibility>(v).map_err(|e| {
                format!(
                    "[package.metadata.oopsie] default-vis {v:?} is not a valid visibility: {e}"
                )
            })?),
        };
        Ok(super::NamingDefaults {
            module,
            module_suffix,
            suffix,
            vis,
        })
    }

    /// Resolve the `traced` defaults (no validation needed — all booleans). A
    /// manifest parse error is reported once via [`cap`]; defer to it here.
    pub fn traced_defaults() -> (super::TracedDefaults, proc_macro2::TokenStream) {
        let Ok(settings) = cached_settings() else {
            return (super::TracedDefaults::default(), quote! {});
        };
        let resolved = match &settings.traced {
            None => super::TracedDefaults::default(),
            Some(TracedSetting::Toggle(b)) => super::TracedDefaults {
                traced: Some(*b),
                ..super::TracedDefaults::default()
            },
            Some(TracedSetting::Table(t)) => super::TracedDefaults {
                traced: t.enabled,
                location: t.location,
                timestamp: t.timestamp,
                packed: t.packed,
                boxed: t.boxed,
                code: t.code,
            },
        };
        (resolved, quote! {})
    }

    /// A token tying generated code to the consumer's `Cargo.toml`, so editing
    /// `[package.metadata.oopsie]` re-runs the macro.
    pub fn manifest_dep_token() -> proc_macro2::TokenStream {
        quote! {
            const _: &[u8] = include_bytes!(concat!(env!(#MANIFEST_ENV_VAR), "/Cargo.toml"));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn missing_section_is_default() {
            assert_eq!(
                parse_settings("[package]\nname = \"x\"\n")
                    .unwrap()
                    .max_size,
                None
            );
        }

        #[test]
        fn reads_max_size() {
            let settings = parse_settings("[package.metadata.oopsie]\nmax-size = 64\n").unwrap();
            assert_eq!(settings.max_size, Some(64));
        }

        #[test]
        fn rejects_unknown_key() {
            assert!(parse_settings("[package.metadata.oopsie]\nfoo = 1\n").is_err());
        }

        #[test]
        fn rejects_non_integer() {
            assert!(parse_settings("[package.metadata.oopsie]\nmax-size = \"big\"\n").is_err());
        }

        #[test]
        fn validate_cap_rejects_zero() {
            assert!(validate_cap(Some(0)).is_err());
            assert_eq!(validate_cap(Some(8)).unwrap(), Some(8));
            assert_eq!(validate_cap(None).unwrap(), None);
        }

        fn naming(toml: &str) -> Result<super::super::NamingDefaults, String> {
            resolve_naming(&parse_settings(toml).expect("valid toml"))
        }

        #[test]
        fn module_table_suffix() {
            let n =
                naming("[package.metadata.oopsie]\nmodule = { suffix = \"errors\" }\n").unwrap();
            assert_eq!(n.module_suffix.as_deref(), Some("errors"));
            // A table without `enabled` leaves the per-kind default in place.
            assert_eq!(n.module, None);
        }

        #[test]
        fn module_bool_and_enabled_forms() {
            assert_eq!(
                naming("[package.metadata.oopsie]\nmodule = false\n")
                    .unwrap()
                    .module,
                Some(false)
            );
            let t =
                naming("[package.metadata.oopsie]\nmodule = { enabled = true, suffix = \"e\" }\n")
                    .unwrap();
            assert_eq!(t.module, Some(true));
            assert_eq!(t.module_suffix.as_deref(), Some("e"));
        }

        #[test]
        fn rejects_bad_module_suffix() {
            assert!(
                naming("[package.metadata.oopsie]\nmodule = { suffix = \"has space\" }\n").is_err()
            );
        }

        #[test]
        fn rejects_unknown_module_key() {
            assert!(parse_settings("[package.metadata.oopsie]\nmodule = { bogus = 1 }\n").is_err());
        }

        #[test]
        fn default_suffix_forms() {
            use super::super::SuffixDefault;
            let off = naming("[package.metadata.oopsie]\ndefault-suffix = false\n").unwrap();
            assert!(matches!(off.suffix, Some(SuffixDefault::Off)));
            let named = naming("[package.metadata.oopsie]\ndefault-suffix = \"Ctx\"\n").unwrap();
            assert!(matches!(named.suffix, Some(SuffixDefault::Name(s)) if s == "Ctx"));
            // `true` is ambiguous and rejected.
            assert!(naming("[package.metadata.oopsie]\ndefault-suffix = true\n").is_err());
        }

        #[test]
        fn parses_default_vis() {
            let ok = naming("[package.metadata.oopsie]\ndefault-vis = \"pub(crate)\"\n").unwrap();
            assert!(ok.vis.is_some());
            assert!(naming("[package.metadata.oopsie]\ndefault-vis = \"bogus\"\n").is_err());
        }

        #[test]
        fn traced_bool_and_table_forms() {
            assert!(matches!(
                parse_settings("[package.metadata.oopsie]\ntraced = true\n")
                    .unwrap()
                    .traced,
                Some(TracedSetting::Toggle(true))
            ));
            let table = parse_settings(
                "[package.metadata.oopsie]\n[package.metadata.oopsie.traced]\nlocation = false\n",
            )
            .unwrap()
            .traced;
            assert!(
                matches!(table, Some(TracedSetting::Table(t)) if t.location == Some(false) && t.enabled.is_none())
            );
        }

        #[test]
        fn rejects_unknown_traced_key() {
            assert!(parse_settings(
                "[package.metadata.oopsie]\n[package.metadata.oopsie.traced]\nlocaiton = false\n"
            )
            .is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    // ── BetterFlag tests ──────────────────────────────────────────────

    #[test]
    fn better_flag_is_enabled() {
        assert!(BetterFlag::<true>::Enabled.is_enabled());
        assert!(!BetterFlag::<true>::Disabled.is_enabled());
        assert!(BetterFlag::<true>::Default.is_enabled());

        assert!(BetterFlag::<false>::Enabled.is_enabled());
        assert!(!BetterFlag::<false>::Disabled.is_enabled());
        assert!(!BetterFlag::<false>::Default.is_enabled());
    }

    #[test]
    fn better_flag_to_option() {
        assert_eq!(BetterFlag::<true>::Enabled.to_option(), Some(true));
        assert_eq!(BetterFlag::<true>::Disabled.to_option(), Some(false));
        assert_eq!(BetterFlag::<true>::Default.to_option(), None);

        assert_eq!(BetterFlag::<false>::Enabled.to_option(), Some(true));
        assert_eq!(BetterFlag::<false>::Disabled.to_option(), Some(false));
        assert_eq!(BetterFlag::<false>::Default.to_option(), None);
    }

    #[test]
    fn better_flag_from_none() {
        let flag = BetterFlag::<true>::from_none();
        assert!(matches!(flag, Some(BetterFlag::Default)));

        let flag = BetterFlag::<false>::from_none();
        assert!(matches!(flag, Some(BetterFlag::Default)));
    }

    #[test]
    fn better_flag_from_value_bool() {
        let true_lit: syn::Lit = parse_quote!(true);
        let flag = BetterFlag::<true>::from_value(&true_lit).unwrap();
        assert!(matches!(flag, BetterFlag::Enabled));

        let false_lit: syn::Lit = parse_quote!(false);
        let flag = BetterFlag::<true>::from_value(&false_lit).unwrap();
        assert!(matches!(flag, BetterFlag::Disabled));

        // Non-bool literal should error
        let str_lit: syn::Lit = parse_quote!("hello");
        BetterFlag::<true>::from_value(&str_lit).unwrap_err();
    }

    #[test]
    fn better_flag_from_meta_path() {
        let meta: syn::Meta = parse_quote!(my_flag);
        let flag = BetterFlag::<true>::from_meta(&meta).unwrap();
        assert!(matches!(flag, BetterFlag::Enabled));
    }

    #[test]
    fn better_flag_from_meta_name_value() {
        let meta: syn::Meta = parse_quote!(my_flag = true);
        let flag = BetterFlag::<true>::from_meta(&meta).unwrap();
        assert!(matches!(flag, BetterFlag::Enabled));

        let meta: syn::Meta = parse_quote!(my_flag = false);
        let flag = BetterFlag::<true>::from_meta(&meta).unwrap();
        assert!(matches!(flag, BetterFlag::Disabled));
    }

    #[test]
    fn better_flag_from_meta_list() {
        let meta: syn::Meta = parse_quote!(my_flag(true));
        let flag = BetterFlag::<true>::from_meta(&meta).unwrap();
        assert!(matches!(flag, BetterFlag::Enabled));

        let meta: syn::Meta = parse_quote!(my_flag(false));
        let flag = BetterFlag::<true>::from_meta(&meta).unwrap();
        assert!(matches!(flag, BetterFlag::Disabled));
    }

    // ── FieldSetting tests ───────────────────────────────────────────

    #[derive(Debug, Clone, Default, darling::FromMeta)]
    struct TestSettings {
        #[darling(default)]
        value: Option<String>,
    }

    #[test]
    fn field_setting_opt_settings_returns_some_for_settings_variant() {
        let setting = FieldSetting::<true, TestSettings>::Settings(Settings {
            enabled: Some(true),
            settings: TestSettings {
                value: Some("hello".into()),
            },
        });
        let s = setting.opt_settings();
        assert!(s.is_some());
        assert_eq!(s.unwrap().value.as_deref(), Some("hello"));
    }

    #[test]
    fn field_setting_opt_settings_returns_none_for_flag_variant() {
        let flag = FieldSetting::<true, TestSettings>::Flag(true);
        assert!(flag.opt_settings().is_none());
    }

    #[test]
    fn field_setting_is_enabled() {
        // Settings with enabled=Some(true)
        let setting = FieldSetting::<true, TestSettings>::Settings(Settings {
            enabled: Some(true),
            settings: TestSettings::default(),
        });
        assert!(setting.is_enabled());

        // Settings with enabled=Some(false)
        let setting = FieldSetting::<true, TestSettings>::Settings(Settings {
            enabled: Some(false),
            settings: TestSettings::default(),
        });
        assert!(!setting.is_enabled());

        // Settings with enabled=None defaults to true
        let setting = FieldSetting::<true, TestSettings>::Settings(Settings {
            enabled: None,
            settings: TestSettings::default(),
        });
        assert!(setting.is_enabled());

        // Flag(true)
        let flag = FieldSetting::<true, TestSettings>::Flag(true);
        assert!(flag.is_enabled());

        // Flag(false)
        let flag = FieldSetting::<true, TestSettings>::Flag(false);
        assert!(!flag.is_enabled());
    }

    // ── SynParse tests ───────────────────────────────────────────────

    #[test]
    fn syn_parse_list_form_parses_inner_tokens() {
        let meta: syn::Meta = parse_quote!(key(..=128));
        let parsed = SynParse::<syn::ExprRange>::from_meta(&meta).unwrap();
        assert!(parsed.start.is_none());
        assert!(parsed.end.is_some());
    }

    #[test]
    fn syn_parse_name_value_string_form_parses_string_contents() {
        let meta: syn::Meta = parse_quote!(key = "..=128");
        let parsed = SynParse::<syn::ExprRange>::from_meta(&meta).unwrap();
        assert!(parsed.start.is_none());
        assert!(parsed.end.is_some());
    }

    #[test]
    fn syn_parse_path_form_is_rejected() {
        let meta: syn::Meta = parse_quote!(key);
        SynParse::<syn::ExprRange>::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn syn_parse_name_value_non_string_is_rejected() {
        let meta: syn::Meta = parse_quote!(key = 42);
        SynParse::<syn::ExprRange>::from_meta(&meta).unwrap_err();
    }

    #[test]
    fn syn_parse_with_syn_path() {
        let meta: syn::Meta = parse_quote!(key(crate::a::b));
        let parsed = SynParse::<syn::Path>::from_meta(&meta).unwrap();
        assert_eq!(parsed.segments.len(), 3);

        let meta: syn::Meta = parse_quote!(key = "crate::a::b");
        let parsed = SynParse::<syn::Path>::from_meta(&meta).unwrap();
        assert_eq!(parsed.segments.len(), 3);
    }

    // ── MaybeAloneOopsieValue tests ──────────────────────────────────

    #[test]
    fn maybe_alone_path_form_is_alone() {
        let meta: syn::Meta = parse_quote!(key);
        let parsed = MaybeAloneOopsieValue::<syn::Ident>::from_meta(&meta).unwrap();
        assert!(matches!(parsed, MaybeAloneOopsieValue::Alone));
    }

    #[test]
    fn maybe_alone_bool_forms() {
        let meta: syn::Meta = parse_quote!(key(true));
        let parsed = MaybeAloneOopsieValue::<syn::Ident>::from_meta(&meta).unwrap();
        assert!(matches!(parsed, MaybeAloneOopsieValue::Bool(true)));

        let meta: syn::Meta = parse_quote!(key(false));
        let parsed = MaybeAloneOopsieValue::<syn::Ident>::from_meta(&meta).unwrap();
        assert!(matches!(parsed, MaybeAloneOopsieValue::Bool(false)));
    }

    #[test]
    fn maybe_alone_value_via_list_form() {
        let meta: syn::Meta = parse_quote!(key(my_name));
        let parsed = MaybeAloneOopsieValue::<syn::Ident>::from_meta(&meta).unwrap();
        match parsed {
            MaybeAloneOopsieValue::Value(ident) => assert_eq!(ident, "my_name"),
            other => panic!("expected Value, got {other:?}"),
        }
    }

    #[test]
    fn maybe_alone_value_via_name_value_form_widening() {
        // `key = name` name-value form resolves to Value.
        let meta: syn::Meta = parse_quote!(key = "my_name");
        let parsed = MaybeAloneOopsieValue::<syn::Ident>::from_meta(&meta).unwrap();
        match parsed {
            MaybeAloneOopsieValue::Value(ident) => assert_eq!(ident, "my_name"),
            other => panic!("expected Value, got {other:?}"),
        }
    }

    #[test]
    fn maybe_alone_value_string_typed() {
        let meta: syn::Meta = parse_quote!(key("CustomSuffix"));
        let parsed = MaybeAloneOopsieValue::<String>::from_meta(&meta).unwrap();
        match parsed {
            MaybeAloneOopsieValue::Value(s) => assert_eq!(s, "CustomSuffix"),
            other => panic!("expected Value, got {other:?}"),
        }

        let meta: syn::Meta = parse_quote!(key = "CustomSuffix");
        let parsed = MaybeAloneOopsieValue::<String>::from_meta(&meta).unwrap();
        match parsed {
            MaybeAloneOopsieValue::Value(s) => assert_eq!(s, "CustomSuffix"),
            other => panic!("expected Value, got {other:?}"),
        }
    }
}
