#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![cfg_attr(feature = "unstable-try-trait-v2", feature(try_trait_v2))]
// Doctests that use `#[oopsie::oopsie]` may generate `fn provide(...)` when the
// `unstable-error-generic-member-access` feature is active; inject the corresponding
// language feature flag so they compile under `--features unstable`.
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    doc(test(attr(feature(error_generic_member_access))))
)]

//! Ergonomic, structured error handling for Rust.
//!
//! `oopsie` centers on a single attribute macro:
//!
//! **[`#[oopsie]`](oopsie)** — generates context selectors, `Display`, `Debug`, and `Error`
//! impls for your error type. Pass `traced` to also capture a backtrace (and a span-trace
//! when the `tracing` feature is enabled).
//!
//! # Quick start
//!
//! **Define your error type:**
//!
//! ```
//! #[oopsie::oopsie]
//! pub enum AppError {
//!     #[oopsie("Connection to {host} failed")]
//!     Connect { host: String, source: std::io::Error },
//!
//!     #[oopsie("Key not found: {key}")]
//!     MissingKey { key: String },
//! }
//! # fn main() {}
//! ```
//!
//! **Use it** — bring the extension traits into scope with the prelude:
//!
//! ```
//! # #[oopsie::oopsie]
//! # pub enum AppError {
//! #     #[oopsie("Connection to {host} failed")]
//! #     Connect { host: String, source: std::io::Error },
//! # }
//! use oopsie::prelude::*;
//!
//! fn connect(host: &str) -> Result<(), AppError> {
//!     std::net::TcpStream::connect(host)
//!         .context(app_oopsies::Connect { host })?;
//!     Ok(())
//! }
//! # fn main() {}
//! ```
//!
//! # Diagnostics
//!
//! Pass `traced` to automatically capture a backtrace field (and a span-trace when
//! the `tracing` feature is enabled):
//!
//! ```
//! #[oopsie::oopsie(traced)]
//! pub enum AppError {
//!     #[oopsie("Connection to {host} failed")]
//!     Connect { host: String, source: std::io::Error },
//! }
//! # fn main() {}
//! ```
//!
//! `traced` also assigns each variant an automatic error code —
//! `module_path::Type::Variant` — shown as `Error[...]` in `Report` headers.
//! Override it per variant with `#[oopsie(code = "...")]`, or disable it with
//! `#[oopsie::oopsie(traced, code = false)]`.
//!
//! See the [`#[oopsie]` documentation](oopsie) for the full parameter reference.
//!
//! # Reporting
//!
//! [`Report`] renders an error — message, source chain, span trace, backtrace —
//! as a rich, colorized report. It implements [`Termination`](std::process::Termination),
//! so it can be returned straight from `main`. [`Report::run`] additionally
//! installs the library's panic hook for the duration of the closure, so
//! panics are rendered in the same style:
//!
//! ```
//! use oopsie::Report;
//! use oopsie::prelude::*;
//!
//! #[oopsie::oopsie(traced)]
//! pub enum AppError {
//!     #[oopsie("Key not found: {key}")]
//!     MissingKey { key: String },
//! }
//!
//! fn run() -> Result<(), AppError> {
//!     let config: Option<&str> = Some("42");
//!     let _value = config.context(app_oopsies::MissingKey { key: "answer" })?;
//!     Ok(())
//! }
//!
//! fn main() -> Report<AppError> {
//!     Report::run(run)
//! }
//! ```
//!
//! When `run` returns an error, the report is printed to stderr and the
//! process exits with a failure code. To render panics outside of
//! [`Report::run`], install the hook process-wide with [`install_panic_hook`]
//! once, early in `main`. Color output is auto-detected; override it with
//! [`set_color_mode`] or per report via [`Report::no_colors`] /
//! [`Report::force_colors`].
//!
//! # Beyond typed errors
//!
//! - [`Welp`] is a string-shaped escape hatch for prototypes and one-off
//!   errors: `Welp::new("...")`, or `.welp_context("...")` on any `Result` via
//!   the prelude.
//! - The [`oopsie::erased`](erased) module (available with the `serde` feature)
//!   converts any error into a serializable, type-erased representation —
//!   message, source chain, code/help, span trace, and backtrace — for
//!   transporting errors across process boundaries, e.g. API error responses.
//!
//! # What gets generated
//!
//! For each variant or struct, `#[oopsie]` generates a **context selector** — a struct
//! containing all the fields *except* the source error and any `#[oopsie(capture)]` fields.
//!
//! Selectors for **leaf** variants (no source) expose `.build()` and `.fail()`
//! (plus `Contextual<NoSource>` for `Option::context`). Selectors for variants
//! **with a source** expose `.build_error(source)` via the [`Contextual`] trait.
//! The methods are mutually exclusive — a source selector has no `.build()`.
//!
//! All selector fields accept `Into<T>`, so you can pass `"str"` for a `String` field.
//!
//! `.context(selector)` on `Result` / `Option` builds the error for you —
//! you only need to call these methods directly when constructing errors manually.
//!
//! # Selector naming
//!
//! The selector name is the **variant name** (for enums) or **struct name** (for structs),
//! with a trailing `"Error"` suffix stripped.
//!
//! Structs additionally get an `"Oopsie"` suffix by default — `struct QueryError`
//! → selector `QueryOopsie`. Disable with `#[oopsie(suffix(false))]` or set a
//! custom one with `#[oopsie(suffix = "X")]`.
//!
//! | Variant / struct | Selector name |
//! |------------------|---------------|
//! | `Connect` (variant) | `Connect` |
//! | `ConnectionError` (variant) | `Connection` |
//! | `QueryError` (struct) | `QueryOopsie` |
//!
//! ## Module wrapping
//!
//! Selectors can be placed in a generated module — the default for enums; structs
//! default to no module. The auto-generated module name is derived from the error
//! type name: strip trailing `"Error"`, convert to `snake_case`, append `_oopsies`:
//!
//! | Error type | Module |
//! |------------|--------|
//! | `AppError` | `app_oopsies` |
//! | `ConnError` | `conn_oopsies` |
//! | `MyError` | `my_oopsies` |
//!
//! Control this with `#[oopsie(module(false))]` (disable) or `#[oopsie(module(custom_name))]`.
//!
//! # Display messages
//!
//! Display strings follow `format!` semantics with named or positional interpolation:
//! - `#[oopsie("Failed to read {path}")]` — named field
//! - `#[oopsie("Got {} errors", count)]` — positional
//!
//! If no display attribute is given, the variant or struct name is used verbatim as the message.
//!
//! # Attribute reference
//!
//! ## Container (`enum` / `struct`)
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg")]` | Display message (short form; structs only — on enums it goes on each variant) |
//! | `#[oopsie(module)]` | Wrap selectors in auto-named module (enum default) |
//! | `#[oopsie(module(name))]` | Wrap selectors in module named `name` |
//! | `#[oopsie(module(false))]` | Disable module wrapping (struct default) |
//! | `#[oopsie(vis(pub))]` | Override selector visibility (default: the error type's own visibility) |
//! | `#[oopsie(suffix)]` | Append `"Oopsie"` suffix to selector names (struct default) |
//! | `#[oopsie(suffix = "X")]` | Append custom suffix to selector names |
//! | `#[oopsie(suffix(false))]` | No selector suffix (enum default) |
//! | `#[oopsie(size(N))]` | Assert error type is exactly `N` bytes at compile time |
//! | `#[oopsie(size(N..=M))]` | Assert error type size is within range at compile time |
//!
//! ## Variant / struct
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg {field}")]` | Short-form display message |
//! | `#[oopsie(display("msg"), ...)]` | Long-form display (combine with other attrs) |
//! | `#[oopsie(transparent)]` | Generate `From` impl instead of a selector struct |
//! | `#[oopsie(help = "...")]` | Help text, surfaced via [`Diagnostic::oopsie_help_text`] and consumed by `Report` |
//! | `#[oopsie(code = "...")]` / `#[oopsie(code("fmt {}", expr))]` | Error code with optional format-string interpolation, surfaced via [`Diagnostic::oopsie_error_code`] and consumed by `Report`; replaces the auto code from `traced` |
//!
//! With the `unstable` feature, help and code are additionally surfaced through the
//! nightly `Provider` API.
//!
//! ## Field
//! | Attribute | Effect |
//! |-----------|--------|
//! | *(named `source`)* | Auto-detected as the chained source error |
//! | `#[oopsie(from)]` | Mark as source (for non-`source`-named fields) |
//! | `#[oopsie(from(false))]` | Opt a field named `source` out of source detection |
//! | `#[oopsie(from(Type, transform))]` | Source with type transformation |
//! | *(type `Box<T>`, source)* | Auto-unboxed: the selector accepts `T` and boxes it (trait objects exempt) |
//! | `#[oopsie(capture)]` | Auto-filled via [`Capturable`]; excluded from selector. Trace-typed fields get this automatically; `capture(false)` opts out |
//! | `#[oopsie(help)]` | Dynamic help text from this field's `Display` |

// Re-export the proc-macro attribute and derive.
pub use oopsie_macros::Oopsie;
pub use oopsie_macros::oopsie;

// Explicit re-export of the public surface from `oopsie-core`. Avoid
// `pub use oopsie_core::*` so transitive deps (tracing-error,
// tracing-subscriber) don't accidentally become part of oopsie's SemVer
// contract via incidental glob re-export.
pub use oopsie_core::{
    AsErrorSource, Backtrace, Capturable, CaptureExt, Contextual, Diagnostic, ErrorCode, HelpText,
    NoSource, OptionExt, ResultExt, RustBacktrace, Welp, WelpOptionExt, WelpResultExt,
    clear_rust_backtrace_override, rust_backtrace, rust_panic_backtrace,
    set_rust_backtrace_override, with_rust_backtrace_override,
};
#[cfg(feature = "tracing")]
pub use oopsie_core::{OptionalSpanTrace, SpanTrace};

// Hidden re-export so macro-generated code can reach the autoref-probe
// machinery via `::oopsie::__private::CaptureProbe`. Not part of the public
// API; do not depend on its contents.
#[doc(hidden)]
pub use oopsie_core::__private;

#[cfg(feature = "serde")]
pub mod erased {
    pub use oopsie_core::erased::{
        Diagnostics, ErasedBacktrace, ErasedError, ErasedFrame, ErasedMetadata, ErasedSpan,
        ErasedSpanTrace, TracingLevel,
    };
}

/// `tracing-subscriber` integration helpers.
#[cfg(feature = "tracing")]
pub mod tracing {
    pub use oopsie_core::json_error_layer;
}

/// Extension traits for error handling at the call site.
///
/// Import this at the top of any file that calls `.context(...)` or
/// `.with_context(...)` on `Result` / `Option` values, or inspects oopsie errors.
/// You do **not** need this to *define* error types — `#[oopsie::oopsie]` works
/// as a fully-qualified attribute with no `use`.
///
/// ```
/// use oopsie::prelude::*;
///
/// #[oopsie::oopsie]
/// enum MyError {
///     #[oopsie("Not found")]
///     NotFound,
/// }
///
/// # fn main() {
/// let opt: Option<i32> = None;
/// let err = opt.context(my_oopsies::NotFound).unwrap_err();
/// assert_eq!(err.to_string(), "Not found");
/// # }
/// ```
pub mod prelude {
    pub use crate::{Contextual, Diagnostic, OptionExt, ResultExt, WelpOptionExt, WelpResultExt};
}

#[cfg(feature = "fancy")]
mod color;
#[cfg(feature = "fancy")]
pub use color::{ColorConfig, get_color_mode, set_color_mode};

#[cfg(feature = "fancy")]
mod report;
#[cfg(feature = "fancy")]
pub use report::Report;

#[cfg(feature = "fancy")]
mod panic_hook;
#[cfg(feature = "fancy")]
pub use panic_hook::install_panic_hook;

#[cfg(feature = "fancy")]
pub mod trace_printer;
