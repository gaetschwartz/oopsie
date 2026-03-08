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

    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Settings(settings) => settings.enabled.unwrap_or(true),
            Self::Flag(value) => *value,
        }
    }
}
impl<const DEFAULT: bool, T: FromMeta + Default + Clone> FieldSetting<DEFAULT, T> {
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
pub struct SnafuValue<T> {
    pub value: Option<T>,
    pub span: Span,
}

impl<T> SnafuValue<T> {
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

impl<T> Deref for SnafuValue<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Default> Default for SnafuValue<T> {
    fn default() -> Self {
        Self {
            value: None,
            span: Span::call_site(),
        }
    }
}

impl<T: FromMeta> FromMeta for SnafuValue<T> {
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
            syn::Meta::NameValue(MetaNameValue { value, .. }) => {
                let span = value.span();
                let value = T::from_expr(value)?;
                Ok(SnafuValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::List(MetaList { tokens, .. }) => {
                let span = tokens.span();
                let meta_2 = syn::parse2::<syn::Meta>(tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(tokens))?;
                let value = T::from_meta(&meta_2)?;
                Ok(SnafuValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::Path(p) => Ok(SnafuValue {
                value: None,
                span: p.span(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct SnafuSynValue<T> {
    pub value: Option<T>,
    pub span: Span,
}

#[expect(unused_macros)]
macro_rules! snafu_syn_value {
    ($($tt:tt)*) => {
        SnafuValue {
            value: Some(syn::parse_quote! { $($tt)* }),
            span: Span::call_site(),
        }
    };
}

#[expect(unused_imports)]
pub(crate) use snafu_syn_value;

impl<T> SnafuSynValue<T> {
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

impl<T> Deref for SnafuSynValue<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Default> Default for SnafuSynValue<T> {
    fn default() -> Self {
        Self {
            value: None,
            span: Span::call_site(),
        }
    }
}

impl<T: FromMeta + syn::parse::Parse> FromMeta for SnafuSynValue<T> {
    fn from_meta(meta: &syn::Meta) -> darling::Result<Self> {
        match meta {
            syn::Meta::NameValue(MetaNameValue { value, .. }) => {
                let span = value.span();
                let value = T::from_expr(value)?;
                Ok(SnafuSynValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::List(MetaList { tokens, .. }) => {
                let span = tokens.span();
                let value = syn::parse2::<T>(tokens.clone())
                    .map_err(|e| darling::Error::custom(e).with_span(tokens))?;
                Ok(SnafuSynValue {
                    value: Some(value),
                    span,
                })
            }
            syn::Meta::Path(p) => Ok(SnafuSynValue {
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

    #[test]
    fn snafu_value_from_meta_name_value() {
        let meta: Vec<syn::Attribute> = parse_quote! {
            #[snafu(visibility(pub(crate)))]
        };

        #[derive(Debug, darling::FromAttributes)]
        #[darling(attributes(snafu))]
        struct SnafuValueTest {
            visibility: SnafuSynValue<syn::Visibility>,
        }

        let snafu_value = SnafuValueTest::from_attributes(&meta).unwrap();
        assert_eq!(
            snafu_value.visibility.value,
            Some(parse_quote! { pub(crate) })
        );
    }
}
