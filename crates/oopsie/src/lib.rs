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
//! impls for your error type. Pass `traced` to also capture backtrace and span-trace.
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
//! Pass `traced` to automatically capture backtrace and span-trace fields:
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
//! See the [`#[oopsie]` documentation](oopsie) for the full parameter reference.
//!
//! # What gets generated
//!
//! For each variant or struct, `#[oopsie]` generates a **context selector** — a struct
//! containing all the fields *except* the source error and any `#[oopsie(capture)]` fields.
//! Every selector exposes three methods:
//!
//! | Method | Use when |
//! |--------|----------|
//! | `.build()` | Leaf error — no source to wrap |
//! | `.build_error(source)` | Wrapping error — takes the source value |
//! | `.fail()` | Shorthand for `Err(self.build())` |
//!
//! All selector fields accept `Into<T>`, so you can pass `"str"` for a `String` field.
//!
//! `.context(selector)` on `Result` / `Option` calls `build_error` / `build` for you —
//! you only need to call these methods directly when constructing errors manually.
//!
//! # Selector naming
//!
//! The selector name is the **variant name** (for enums) or **struct name** (for structs),
//! with a trailing `"Error"` suffix stripped:
//!
//! | Variant / struct | Selector name |
//! |------------------|---------------|
//! | `Connect` | `Connect` |
//! | `ConnectionError` | `Connection` |
//! | `NotFound` | `NotFound` |
//!
//! ## Module wrapping
//!
//! For enums, selectors are placed in a generated module. The module name is derived from
//! the enum type name: strip trailing `"Error"`, convert to `snake_case`, append `_oopsies`:
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
//! - `#[oopsie("Got {} errors", count)]` — positional (long form only)
//!
//! If no display attribute is given, the variant or struct name is used verbatim as the message.
//!
//! # Attribute reference
//!
//! ## Container (`enum` / `struct`)
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg")]` | Display message (short form) |
//! | `#[oopsie(module)]` | Wrap selectors in auto-named module |
//! | `#[oopsie(module(name))]` | Wrap selectors in module named `name` |
//! | `#[oopsie(module(false))]` | Disable module wrapping |
//! | `#[oopsie(vis(pub))]` | Override default selector visibility |
//! | `#[oopsie(suffix)]` | Append `"Oopsie"` suffix to selector names |
//! | `#[oopsie(suffix = "X")]` | Append custom suffix to selector names |
//! | `#[oopsie(size(N))]` | Assert error type is exactly `N` bytes at compile time |
//! | `#[oopsie(size(N..=M))]` | Assert error type size is within range at compile time |
//!
//! ## Variant / struct
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg {field}")]` | Short-form display message |
//! | `#[oopsie(display("msg"), ...)]` | Long-form display (combine with other attrs) |
//! | `#[oopsie(transparent)]` | Generate `From` impl instead of a selector struct |
//! | `#[oopsie(help = "...")]` | Help text surfaced via the `Provider` API |
//! | `#[oopsie(code = "...")]` | Error code surfaced via the `Provider` API |
//!
//! ## Field
//! | Attribute | Effect |
//! |-----------|--------|
//! | *(named `source`)* | Auto-detected as the chained source error |
//! | `#[oopsie(from)]` | Mark as source (for non-`source`-named fields) |
//! | `#[oopsie(from(Type, transform))]` | Source with type transformation |
//! | `#[oopsie(capture)]` | Auto-filled via [`Capturable`]; excluded from selector |

// Re-export the proc-macro attribute and derive.
pub use oopsie_macros::Oopsie;
pub use oopsie_macros::oopsie;

// Explicit re-export of the public surface from `oopsie-core`. Avoid
// `pub use oopsie_core::*` so transitive deps (tracing-error,
// tracing-subscriber) don't accidentally become part of oopsie's SemVer
// contract via incidental glob re-export.
pub use oopsie_core::{
    AsErrorSource, BackTrace, Capturable, CaptureExt, Contextual, Diagnostic, ErrorCode, HelpText,
    NoSource, OptionExt, OptionalSpanTrace, ResultExt, RustBacktrace, SpanTrace, Welp,
    WelpOptionExt, WelpResultExt, clear_rust_backtrace_override, install, install_panic_hook,
    rust_backtrace, set_rust_backtrace_override,
};

// Hidden re-export so macro-generated code can reach the autoref-probe
// machinery via `::oopsie::__private::CaptureProbe`. Not part of the public
// API; do not depend on its contents.
#[doc(hidden)]
pub use oopsie_core::__private;

/// `tracing-subscriber` integration helpers.
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
pub mod trace_printer;
