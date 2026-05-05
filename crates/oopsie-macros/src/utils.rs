#![allow(dead_code)]
use std::ops::Deref;

use darling::FromMeta;
use proc_macro2::Span;
use syn::spanned::Spanned as _;
use syn::{MetaList, MetaNameValue};

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
pub struct Settings<T: FromMeta> {
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

#[derive(Debug)]
pub struct OopsieValue<T> {
    pub value: Option<T>,
    pub span: Span,
}

impl<T> OopsieValue<T> {
    #[inline]
    pub fn new_some(value: T) -> Self {
        Self {
            value: Some(value),
            span: Span::call_site(),
        }
    }

    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
}

impl<T> Deref for OopsieValue<T> {
    type Target = Option<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Default> Default for OopsieValue<T> {
    fn default() -> Self {
        Self {
            value: None,
            span: Span::call_site(),
        }
    }
}

impl<T: FromMeta> FromMeta for OopsieValue<T> {
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
            syn::Meta::NameValue(MetaNameValue { value, .. }) => {
                let span = value.span();
                let value = T::from_expr(value)?;
                Ok(OopsieValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::List(MetaList { tokens, .. }) => {
                let span = tokens.span();
                let meta_2 = syn::parse2::<syn::Meta>(tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(tokens))?;
                let value = T::from_meta(&meta_2)?;
                Ok(OopsieValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::Path(p) => Ok(OopsieValue {
                value: None,
                span: p.span(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct OopsieSynValue<T> {
    pub value: Option<T>,
    pub span: Span,
}

#[expect(unused_macros)]
macro_rules! oopsie_syn_value {
    ($($tt:tt)*) => {
        OopsieValue {
            value: Some(syn::parse_quote! { $($tt)* }),
            span: Span::call_site(),
        }
    };
}

#[expect(unused_imports)]
pub(crate) use oopsie_syn_value;

impl<T> OopsieSynValue<T> {
    #[inline]
    pub fn new_some(value: T) -> Self {
        Self {
            value: Some(value),
            span: Span::call_site(),
        }
    }

    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
}

impl<T> Deref for OopsieSynValue<T> {
    type Target = Option<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Default> Default for OopsieSynValue<T> {
    fn default() -> Self {
        Self {
            value: None,
            span: Span::call_site(),
        }
    }
}

impl<T: FromMeta + syn::parse::Parse> FromMeta for OopsieSynValue<T> {
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
            syn::Meta::NameValue(MetaNameValue { value, .. }) => {
                let span = value.span();
                let value = T::from_expr(value)?;
                Ok(OopsieSynValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::List(MetaList { tokens, .. }) => {
                let span = tokens.span();
                let value = syn::parse2::<T>(tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(tokens))?;
                Ok(OopsieSynValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::Path(p) => Ok(OopsieSynValue {
                value: None,
                span: p.span(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use darling::FromAttributes as _;
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

    // ── OopsieValue Deref test ───────────────────────────────────────

    #[test]
    fn oopsie_value_deref() {
        let val = OopsieValue::new_some(42);
        let inner: &Option<i32> = &val;
        assert_eq!(*inner, Some(42));

        let empty: OopsieValue<i32> = OopsieValue::default();
        let inner: &Option<i32> = &empty;
        assert_eq!(*inner, None);
    }

    // ── OopsieSynValue Deref test ────────────────────────────────────

    #[test]
    fn oopsie_syn_value_deref() {
        let val = OopsieSynValue::new_some(42i32);
        let inner: &Option<i32> = &val;
        assert_eq!(*inner, Some(42));

        let empty: OopsieSynValue<i32> = OopsieSynValue::default();
        let inner: &Option<i32> = &empty;
        assert_eq!(*inner, None);
    }

    // ── Existing tests ───────────────────────────────────────────────

    #[test]
    #[expect(clippy::items_after_statements, clippy::needless_continue)]
    fn oopsie_value_from_meta_name_value() {
        let meta: Vec<syn::Attribute> = parse_quote! {
            #[oopsie(visibility(pub(crate)))]
        };

        #[derive(Debug, darling::FromAttributes)]
        #[darling(attributes(oopsie))]
        struct OopsieValueTest {
            visibility: OopsieSynValue<syn::Visibility>,
        }

        let oopsie_value = OopsieValueTest::from_attributes(&meta).unwrap();
        assert_eq!(
            oopsie_value.visibility.value,
            Some(parse_quote! { pub(crate) })
        );
    }
}
