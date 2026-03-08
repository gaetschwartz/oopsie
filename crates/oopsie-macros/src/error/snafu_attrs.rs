//! SNAFU attribute parsing.
#![expect(clippy::needless_continue)]

use darling::FromAttributes as _;
use darling::util::{Ignored, SpannedValue};

use crate::utils::SnafuSynValue;

#[derive(Debug, darling::FromAttributes)]
#[darling(attributes(snafu), allow_unknown_fields)]
pub(super) struct SnafuAttrs {
    pub module: Option<SnafuSynValue<syn::Ident>>,
    pub visibility: Option<SnafuSynValue<syn::Visibility>>,
    pub context: Option<SpannedValue<Ignored>>,
}

pub(super) fn extract_snafu_attr(attrs: &[syn::Attribute]) -> darling::Result<SnafuAttrs> {
    SnafuAttrs::from_attributes(attrs)
}
