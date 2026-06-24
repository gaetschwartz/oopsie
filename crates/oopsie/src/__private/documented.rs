//! Hover-documentation targets for `oopsie` attribute keywords.
//!
//! Both the derive expansion (for `#[oopsie(...)]` helper attributes) and the
//! `#[oopsie]` attribute macro (for its own argument list) emit a hidden
//! `use ...::documented::<scope>::<keyword> as _;` per keyword they parse,
//! interpolating the ident from the user's attribute with its original span.
//! rust-analyzer resolves a hover on the keyword through that `use` path to
//! the function here and shows its docs.
//!
//! Keywords are scoped because the same name can mean different things at
//! different levels (e.g. `help` on a variant vs. on a field).
//!
//! Value-taking keywords carry a placeholder parameter whose type hints the
//! dominant accepted shape; the `Forms:` doc line stays authoritative for the
//! alternatives. Pure marker keywords keep zero-arg signatures.

#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_const_for_fn,
    reason = "placeholder fns exist only to shape the signature shown on hover; by-value params and non-const are intentional"
)]

/// Placeholder types shown in the hover signatures of value-taking keywords.
/// Each stands for the shape of value the keyword accepts.
pub mod params {
    /// A bare identifier, e.g. `my_module`.
    pub struct IdentiferOrFalse;

    /// A string literal, e.g. `"..."`.
    pub struct StringOrFalse;

    /// An exact size or a range, e.g. `64`, `..=64`, `16..`, `16..=64`.
    pub struct IntOrRange;

    /// A path to the `oopsie` crate, as a string: `"my_crate::oopsie"`.
    pub struct Path;

    /// A visibility, e.g. `pub`, `pub(crate)`.
    pub struct Vis;

    /// A `format!`-style string literal interpolating fields.
    pub struct FormatString;

    /// Optional trailing `format!` arguments.
    pub struct FmtArg;

    /// `Type => expr`, optionally `ref, Type => expr`.
    pub struct TypeArrowExpr;

    /// `Type, transform` — or bare/`false` for plain marking/opt-out.
    pub struct TypeAndTransform;

    /// A type path, e.g. `my_crate::MyType`.
    pub struct TypePath;

    /// A nested settings list, e.g. `key(option = value, ...)`.
    pub struct Settings;

    /// Argument for `#[oopsie(...)]`.
    pub struct OopsieArg;

    /// A process exit code, an integer in `1..=255`.
    pub struct ExitCode;
}

/// The `oopsie` helper-attribute name itself, read by `#[derive(Oopsie)]` and
/// the `#[oopsie::oopsie]` macro. The full overview lives on the `oopsie` fn,
/// which is what hover surfaces.
pub mod helper {
    use super::params::OopsieArg;

    /// The `oopsie` configuration attribute, read by `#[derive(Oopsie)]` and the
    /// `#[oopsie::oopsie]` macro. Which keywords it accepts depends on where it
    /// sits — on the error type, on a variant/struct, or on a field. Hover a
    /// keyword inside the parentheses for its specific meaning.
    pub fn oopsie(args: Vec<OopsieArg>) {
        _ = args;
    }
}

/// Keywords accepted in `#[oopsie(...)]` on the error type itself.
pub mod container {
    use super::params::{ExitCode, IdentiferOrFalse, IntOrRange, Path, StringOrFalse, Vis};

    #[doc = include_str!("keyword_docs/container/module.md")]
    pub fn module(name: IdentiferOrFalse) {
        _ = name;
    }

    #[doc = include_str!("keyword_docs/container/suffix.md")]
    pub fn suffix(text: StringOrFalse) {
        _ = text;
    }

    #[doc = include_str!("keyword_docs/container/size.md")]
    pub fn size(size: IntOrRange) {
        _ = size;
    }

    #[doc = include_str!("keyword_docs/container/path.md")]
    pub fn path(path: Path) {
        _ = path;
    }

    #[doc = include_str!("keyword_docs/container/vis.md")]
    pub fn vis(vis: Vis) {
        _ = vis;
    }

    #[doc = include_str!("keyword_docs/container/exit_code.md")]
    pub fn exit_code(code: ExitCode) {
        _ = code;
    }
}

/// Keywords accepted in `#[oopsie(...)]` on an enum variant (or on a struct,
/// which plays both container and variant roles).
pub mod variant {
    use super::params::{ExitCode, FmtArg, FormatString, TypeArrowExpr, Vis};

    #[doc = include_str!("keyword_docs/variant/display.md")]
    pub fn display(fmt: FormatString, args: Vec<FmtArg>) {
        _ = fmt;
        _ = args;
    }

    #[doc = include_str!("keyword_docs/variant/traced.md")]
    pub fn traced(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/variant/transparent.md")]
    pub fn transparent() {}

    #[doc = include_str!("keyword_docs/variant/help.md")]
    pub fn help(text: FormatString, args: Vec<FmtArg>) {
        _ = text;
        _ = args;
    }

    #[doc = include_str!("keyword_docs/variant/code.md")]
    pub fn code(code: FormatString, args: Vec<FmtArg>) {
        _ = code;
        _ = args;
    }

    #[doc = include_str!("keyword_docs/variant/exit_code.md")]
    pub fn exit_code(code: ExitCode) {
        _ = code;
    }

    #[doc = include_str!("keyword_docs/variant/provide.md")]
    pub fn provide(spec: TypeArrowExpr) {
        _ = spec;
    }

    #[doc = include_str!("keyword_docs/variant/vis.md")]
    pub fn vis(vis: Vis) {
        _ = vis;
    }
}

/// Keywords accepted in `#[oopsie(...)]` on a field.
pub mod field {
    use super::params::{TypeAndTransform, TypeArrowExpr};

    #[doc = include_str!("keyword_docs/field/from.md")]
    pub fn from(spec: TypeAndTransform) {
        _ = spec;
    }

    #[doc = include_str!("keyword_docs/field/capture.md")]
    pub fn capture() {}

    #[doc = include_str!("keyword_docs/field/provide.md")]
    pub fn provide(spec: TypeArrowExpr) {
        _ = spec;
    }

    #[doc = include_str!("keyword_docs/field/backtrace.md")]
    pub fn backtrace() {}

    #[doc = include_str!("keyword_docs/field/spantrace.md")]
    pub fn spantrace() {}

    #[doc = include_str!("keyword_docs/field/traces.md")]
    pub fn traces() {}

    #[doc = include_str!("keyword_docs/field/location.md")]
    pub fn location() {}

    #[doc = include_str!("keyword_docs/field/help.md")]
    pub fn help() {}

    #[doc = include_str!("keyword_docs/field/forward.md")]
    pub fn forward() {}
}

/// Keywords accepted at the top level of the `#[oopsie::oopsie(...)]`
/// attribute macro's argument list.
pub mod attr {
    use super::params::{Path, Settings};

    #[doc = include_str!("keyword_docs/attr/traced.md")]
    pub fn traced(settings: Settings) {
        _ = settings;
    }

    #[doc = include_str!("keyword_docs/attr/path.md")]
    pub fn path(path: Path) {
        _ = path;
    }

    #[doc = include_str!("keyword_docs/attr/debug.md")]
    pub fn debug(enabled: bool) {
        _ = enabled;
    }
}

/// Keywords accepted nested inside the `#[oopsie::oopsie(...)]` attribute
/// macro's argument list.
pub mod traced {
    use super::params::{Settings, TypePath};

    #[doc = include_str!("keyword_docs/traced/backtrace.md")]
    pub fn backtrace(settings: Settings) {
        _ = settings;
    }

    #[doc = include_str!("keyword_docs/traced/spantrace.md")]
    pub fn spantrace(settings: Settings) {
        _ = settings;
    }

    #[doc = include_str!("keyword_docs/traced/timestamp.md")]
    pub fn timestamp(settings: Settings) {
        _ = settings;
    }

    #[doc = include_str!("keyword_docs/traced/packed.md")]
    pub fn packed(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/traced/code.md")]
    pub fn code(settings: Settings) {
        _ = settings;
    }

    #[doc = include_str!("keyword_docs/traced/boxed.md")]
    pub fn boxed(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/traced/chrono.md")]
    pub fn chrono(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/traced/location.md")]
    pub fn location(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/traced/provide.md")]
    pub fn provide(enabled: bool) {
        _ = enabled;
    }

    #[doc = include_str!("keyword_docs/traced/type.md")]
    pub fn r#type(r#type: TypePath) {
        _ = r#type;
    }

    #[doc = include_str!("keyword_docs/traced/enabled.md")]
    pub fn enabled(enabled: bool) {
        _ = enabled;
    }
}
