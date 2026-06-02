#![allow(dead_code)]
use std::ops::Deref;

use darling::FromMeta;
use syn::MetaNameValue;

#[derive(Debug)]
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
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        if let syn::Meta::Path(_) = meta {
            Ok(Self::Flag(true))
        } else if let syn::Meta::NameValue(nv) = meta {
            // Handle `field = true` / `field = false`
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Bool(b),
                ..
            }) = &nv.value
            {
                Ok(Self::Flag(b.value))
            } else {
                let settings = Settings::<T>::from_meta(meta)?;
                Ok(Self::Settings(settings))
            }
        } else {
            let settings = Settings::<T>::from_meta(meta)?;
            Ok(Self::Settings(settings))
        }
    }

    fn from_bool(value: bool) -> darling::Result<Self> {
        Ok(Self::Flag(value))
    }

    fn from_none() -> Option<Self> {
        Some(Self::Flag(DEFAULT))
    }
}

#[derive(Debug, darling::FromMeta)]
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
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
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
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
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
            syn::Meta::NameValue(_) => T::from_meta(meta).map(Self::Value),
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
        // Widened form: `key = name` was previously rejected; now accepted.
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
