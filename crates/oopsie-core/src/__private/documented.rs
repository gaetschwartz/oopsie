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
    reason = "placeholder params exist only to shape the by-value signature shown on hover; &T/Copy would distort it"
)]

/// Placeholder types shown in the hover signatures of value-taking keywords.
/// Each stands for the shape of value the keyword accepts.
pub mod params {
    /// A bare identifier, e.g. `my_module`.
    pub struct Name;

    /// A string literal, e.g. `"..."`.
    pub struct Text;

    /// `true` or `false`.
    pub struct Bool;

    /// An exact size or a range, e.g. `64`, `..=64`, `16..`, `16..=64`.
    pub struct IntOrRange;

    /// A path to the `oopsie` crate, as a string: `"my_crate::oopsie"`.
    pub struct CratePath;

    /// A visibility, e.g. `pub`, `pub(crate)`.
    pub struct Vis;

    /// A `format!`-style string literal interpolating fields.
    pub struct FormatString;

    /// Optional trailing `format!` arguments.
    pub struct FmtArgs;

    /// `Type => expr`, optionally `ref, Type => expr`.
    pub struct TypeArrowExpr;

    /// `Type, transform` — or bare/`false` for plain marking/opt-out.
    pub struct TypeAndTransform;

    /// A type path, e.g. `my_crate::MyType`.
    pub struct TypePath;

    /// A nested settings list, e.g. `key(option = value, ...)`.
    pub struct Settings;
}

/// The `oopsie` helper-attribute name itself, read by `#[derive(Oopsie)]` and
/// the `#[oopsie::oopsie]` macro. The full overview lives on the [`oopsie`] fn,
/// which is what hover surfaces.
pub mod helper {
    /// The `oopsie` configuration attribute, read by `#[derive(Oopsie)]` and
    /// the `#[oopsie::oopsie]` macro. Which keywords it accepts depends on
    /// where it sits — on the error type, on a variant/struct, or on a field.
    /// Hover a keyword inside the parentheses for its specific meaning.
    pub const fn oopsie() {}
}

/// Keywords accepted in `#[oopsie(...)]` on the error type itself.
pub mod container {
    use super::params::{CratePath, IntOrRange, Name, Text, Vis};

    /// Wraps the generated context selectors in a module
    /// (enum default: on, auto-named; struct default: off).
    ///
    /// Forms: `module`, `module(name)`, `module(false)`.
    pub const fn module(name: Name) {
        _ = name;
    }

    /// Appends a suffix to generated selector names
    /// (enum default: none; struct default: `"Oopsie"`).
    ///
    /// Forms: `suffix`, `suffix = "X"`, `suffix(false)`.
    pub const fn suffix(text: Text) {
        _ = text;
    }

    /// Asserts the size of the error type at compile time.
    ///
    /// Forms: `size(N)`, `size(..=N)`, `size(N..)`, `size(N..=M)`.
    pub const fn size(size: IntOrRange) {
        _ = size;
    }

    /// Overrides the path to the `oopsie` crate used in generated code
    /// (default: `::oopsie`).
    ///
    /// Form: `path = "some::path"`.
    pub const fn path(path: CratePath) {
        _ = path;
    }

    /// Overrides the visibility of generated selectors
    /// (default: the error type's own visibility).
    ///
    /// Forms: `vis(pub)`, `vis(pub(crate))`, ...
    pub const fn vis(vis: Vis) {
        _ = vis;
    }
}

/// Keywords accepted in `#[oopsie(...)]` on an enum variant (or on a struct,
/// which plays both container and variant roles).
pub mod variant {
    use super::params::{FmtArgs, FormatString, TypeArrowExpr, Vis};

    /// Sets the `Display` message, with `format!` semantics (named or
    /// positional interpolation).
    ///
    /// Forms: `display("msg {field}")`, `display("msg {}", expr)`.
    pub const fn display(fmt: FormatString, args: FmtArgs) {
        _ = fmt;
        _ = args;
    }

    /// Delegates `Display` and `source` to the inner source field, and
    /// generates a `From` impl instead of a context selector.
    pub const fn transparent() {}

    /// Attaches help text, surfaced via `Diagnostic::oopsie_help_text` and
    /// shown by `Report`.
    ///
    /// Forms: `help = "..."`, `help("fmt {}", args)`.
    pub const fn help(text: FormatString, args: FmtArgs) {
        _ = text;
        _ = args;
    }

    /// Attaches an error code, surfaced via `Diagnostic::oopsie_error_code`.
    ///
    /// Forms: `code = "..."`, `code("fmt {}", args)`.
    pub const fn code(code: FormatString, args: FmtArgs) {
        _ = code;
        _ = args;
    }

    /// Provides a value or reference through the `std::error::Request`
    /// provider API (with the `unstable` feature).
    ///
    /// Forms: `provide(Type => expr)`, `provide(ref, Type => expr)`.
    pub const fn provide(spec: TypeArrowExpr) {
        _ = spec;
    }

    /// Overrides this variant's selector visibility.
    ///
    /// Forms: `vis(pub)`, `vis(pub(crate))`, ...
    pub const fn vis(vis: Vis) {
        _ = vis;
    }
}

/// Keywords accepted in `#[oopsie(...)]` on a field.
pub mod field {
    use super::params::{TypeAndTransform, TypeArrowExpr};

    /// Marks this field as the chained source error, accepted by the
    /// selector at construction.
    ///
    /// Forms: `from`, `from(false)` (opt a `source`-named field out),
    /// `from(Type, transform)` (accept `Type`, store `transform(value)`).
    pub const fn from(spec: TypeAndTransform) {
        _ = spec;
    }

    /// Auto-fills this field via `Capturable` when the error is constructed;
    /// the field is excluded from the context selector.
    pub const fn capture() {}

    /// Provides a value or reference through the `std::error::Request`
    /// provider API (with the `unstable` feature).
    ///
    /// Forms: `provide(Type => expr)`, `provide(ref, Type => expr)`.
    pub const fn provide(spec: TypeArrowExpr) {
        _ = spec;
    }

    /// Marks this field as the captured backtrace. The field type's last
    /// path segment must be `Backtrace`.
    pub const fn backtrace() {}

    /// Marks this field as the captured span trace. The field type's last
    /// path segment must be `SpanTrace`.
    pub const fn spantrace() {}

    /// Marks this field as the packed `(Backtrace, SpanTrace)` pair.
    pub const fn traces() {}

    /// Uses this field's `Display` output as the error's dynamic help text.
    pub const fn help() {}
}

/// Keywords accepted at the top level of the `#[oopsie::oopsie(...)]`
/// attribute macro's argument list.
pub mod attr {
    use super::params::{CratePath, Settings};

    /// Injects trace-capture fields (backtrace + spantrace, both on by
    /// default) into every variant / the struct.
    ///
    /// Forms: `traced`, `traced(false)`,
    /// `traced(backtrace(...), spantrace(...), timestamp(...), packed = ..., boxed = ...)`.
    pub const fn traced(settings: Settings) {
        _ = settings;
    }

    /// With `traced`: attaches an auto-generated error code
    /// (`module_path::Type::Variant`) to every variant / the struct that has
    /// no explicit `code = "..."` (`transparent` items excluded).
    ///
    /// Forms: `code = false`, `code(r#type = Path)`
    /// (code type default: `ErrorCode`).
    pub const fn code(settings: Settings) {
        _ = settings;
    }

    /// Path to the `oopsie` crate used in generated code
    /// (default: `::oopsie`); a container-level `#[oopsie(path = ...)]` on
    /// the type itself wins over this.
    ///
    /// Form: `path = "some::path"`.
    pub const fn path(path: CratePath) {
        _ = path;
    }
}

/// Keywords accepted nested inside the `#[oopsie::oopsie(...)]` attribute
/// macro's argument list.
pub mod traced {
    use super::params::{Bool, Settings, TypePath};

    /// Enables and tunes capture of the backtrace (default: on). Mentioning
    /// one part never disables the others.
    ///
    /// Forms: `backtrace`, `backtrace(false)`,
    /// `backtrace(r#type = Path, boxed = ..., enabled = ...)`.
    pub const fn backtrace(settings: Settings) {
        _ = settings;
    }

    /// Enables and tunes capture of the span trace (default: on). Mentioning
    /// one part never disables the others.
    ///
    /// Forms: `spantrace`, `spantrace(false)`,
    /// `spantrace(r#type = Path, boxed = ..., enabled = ...)`.
    pub const fn spantrace(settings: Settings) {
        _ = settings;
    }

    /// Injects an auto-captured timestamp field (default: off).
    ///
    /// Forms: `timestamp`, `timestamp(chrono = ..., provide = ...)`.
    pub const fn timestamp(settings: Settings) {
        _ = settings;
    }

    /// Stores backtrace + spantrace as one packed `(Backtrace, SpanTrace)`
    /// field (default: on); `packed = false` keeps separate fields. Packing
    /// requires backtrace and spantrace to share one boxing mode.
    ///
    /// Forms: `packed`, `packed = false`.
    pub const fn packed(enabled: Bool) {
        _ = enabled;
    }

    /// Boxes the injected trace field(s) (default: on). Applies to both
    /// traces at this level, or to one trace inside
    /// `backtrace(...)`/`spantrace(...)`.
    ///
    /// Forms: `boxed`, `boxed = false`.
    pub const fn boxed(enabled: Bool) {
        _ = enabled;
    }

    /// Inside `timestamp(...)`: uses `chrono::DateTime<Local>` instead of
    /// `SystemTime` as the timestamp type (needs the `chrono` feature;
    /// default: off).
    ///
    /// Form: `chrono = true`.
    pub const fn chrono(enabled: Bool) {
        _ = enabled;
    }

    /// Inside `timestamp(...)`: also exposes the timestamp through the
    /// `std::error::Request` provider API (default: off).
    ///
    /// Form: `provide = true`.
    pub const fn provide(enabled: Bool) {
        _ = enabled;
    }

    /// Overrides the injected type for this part: the trace field type inside
    /// `backtrace(...)`/`spantrace(...)`, or the error-code type inside
    /// `code(...)`.
    ///
    /// Form: `r#type = some::Path`.
    pub const fn r#type(r#type: TypePath) {
        _ = r#type;
    }

    /// Explicit on/off switch accepted in any settings block, e.g.
    /// `backtrace(enabled = false, r#type = ...)`; a settings block without
    /// it counts as enabled.
    ///
    /// Forms: `enabled = true`, `enabled = false`.
    pub const fn enabled(enabled: Bool) {
        _ = enabled;
    }
}
