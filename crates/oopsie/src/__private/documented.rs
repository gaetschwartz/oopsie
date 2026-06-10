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
    use super::params::{IdentiferOrFalse, IntOrRange, Path, StringOrFalse, Vis};

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
}

/// Keywords accepted in `#[oopsie(...)]` on an enum variant (or on a struct,
/// which plays both container and variant roles).
pub mod variant {
    use super::params::{FmtArg, FormatString, TypeArrowExpr, Vis};

    #[doc = include_str!("keyword_docs/variant/display.md")]
    pub fn display(fmt: FormatString, args: Vec<FmtArg>) {
        _ = fmt;
        _ = args;
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

    #[doc = include_str!("keyword_docs/field/help.md")]
    pub fn help() {}
}

/// Keywords accepted at the top level of the `#[oopsie::oopsie(...)]`
/// attribute macro's argument list.
pub mod attr {
    use super::params::{Path, Settings};

    /// Injects trace-capture fields (backtrace + spantrace, both on by
    /// default) into every variant / the struct.
    ///
    /// Forms: `traced`, `traced(false)`,
    /// `traced(backtrace(...), spantrace(...), timestamp(...), packed = ..., boxed = ...)`.
    pub fn traced(settings: Settings) {
        _ = settings;
    }

    /// With `traced`: attaches an auto-generated error code
    /// (`module_path::Type::Variant`) to every variant / the struct that has
    /// no explicit `code = "..."` (`transparent` items excluded).
    ///
    /// Forms: `code = false`, `code(r#type = Path)`
    /// (code type default: `ErrorCode`).
    pub fn code(settings: Settings) {
        _ = settings;
    }

    /// Path to the `oopsie` crate used in generated code
    /// (default: `::oopsie`); a container-level `#[oopsie(path = ...)]` on
    /// the type itself wins over this.
    ///
    /// Form: `path = "some::path"`.
    pub fn path(path: Path) {
        _ = path;
    }
}

/// Keywords accepted nested inside the `#[oopsie::oopsie(...)]` attribute
/// macro's argument list.
pub mod traced {
    use super::params::{Settings, TypePath};

    /// Enables and tunes capture of the backtrace (default: on). Mentioning
    /// one part never disables the others.
    ///
    /// Forms: `backtrace`, `backtrace(false)`,
    /// `backtrace(r#type = Path, boxed = ..., enabled = ...)`.
    pub fn backtrace(settings: Settings) {
        _ = settings;
    }

    /// Enables and tunes capture of the span trace (default: on). Mentioning
    /// one part never disables the others.
    ///
    /// Forms: `spantrace`, `spantrace(false)`,
    /// `spantrace(r#type = Path, boxed = ..., enabled = ...)`.
    pub fn spantrace(settings: Settings) {
        _ = settings;
    }

    /// Injects an auto-captured timestamp field (default: off).
    ///
    /// Forms: `timestamp`, `timestamp(chrono = ..., provide = ...)`.
    pub fn timestamp(settings: Settings) {
        _ = settings;
    }

    /// Stores backtrace + spantrace as one packed `(Backtrace, SpanTrace)`
    /// field (default: on); `packed = false` keeps separate fields. Packing
    /// requires backtrace and spantrace to share one boxing mode.
    ///
    /// Forms: `packed`, `packed = false`.
    pub fn packed(enabled: bool) {
        _ = enabled;
    }

    /// Boxes the injected trace field(s) (default: on). Applies to both
    /// traces at this level, or to one trace inside
    /// `backtrace(...)`/`spantrace(...)`.
    ///
    /// Forms: `boxed`, `boxed = false`.
    pub fn boxed(enabled: bool) {
        _ = enabled;
    }

    /// Inside `timestamp(...)`: uses `chrono::DateTime<Local>` instead of
    /// `SystemTime` as the timestamp type (needs the `chrono` feature;
    /// default: off).
    ///
    /// Form: `chrono = true`.
    pub fn chrono(enabled: bool) {
        _ = enabled;
    }

    /// Inside `timestamp(...)`: also exposes the timestamp through the
    /// `std::error::Request` provider API (default: off).
    ///
    /// Form: `provide = true`.
    pub fn provide(enabled: bool) {
        _ = enabled;
    }

    /// Overrides the injected type for this part: the trace field type inside
    /// `backtrace(...)`/`spantrace(...)`, or the error-code type inside
    /// `code(...)`.
    ///
    /// Form: `r#type = some::Path`.
    pub fn r#type(r#type: TypePath) {
        _ = r#type;
    }

    /// Explicit on/off switch accepted in any settings block, e.g.
    /// `backtrace(enabled = false, r#type = ...)`; a settings block without
    /// it counts as enabled.
    ///
    /// Forms: `enabled = true`, `enabled = false`.
    pub fn enabled(enabled: bool) {
        _ = enabled;
    }
}
