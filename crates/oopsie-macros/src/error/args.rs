//! Argument types for the `#[oopsie]` macro.

use crate::utils::{BetterFlag, FieldSetting};

#[derive(Debug, darling::FromMeta)]
pub(super) struct ErrorArgs {
    pub spantrace: FieldSetting<true, TraceSettings>,
    pub backtrace: FieldSetting<true, TraceSettings>,
    pub timestamp: FieldSetting<false, TimestampSettings>,
    pub code: FieldSetting<true, CodeSettings>,
    pub path: Option<syn::Path>,
    pub module: BetterFlag<true>,
    pub no_suffix: BetterFlag<false>,
    /// Display format string, forwarded to the derive as `#[oopsie("...")]`.
    pub display: Option<String>,
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct TraceSettings {
    pub r#type: Option<syn::Path>,
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct TimestampSettings {
    pub chrono: BetterFlag<true>,
    pub provide: BetterFlag<true>,
}

#[derive(Debug, darling::FromMeta)]
pub(super) struct CodeSettings {
    pub r#type: Option<syn::Path>,
}

impl<const DEFAULT: bool> FieldSetting<DEFAULT, TraceSettings> {
    pub fn r#type(&self) -> Option<&syn::Path> {
        let s = self.opt_settings()?;
        s.r#type.as_ref()
    }
}
