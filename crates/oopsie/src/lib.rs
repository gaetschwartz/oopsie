#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![cfg_attr(feature = "unstable-try-trait-v2", feature(try_trait_v2))]

//! Ergonomic, structured error handling for Rust.
//!
//! `oopsie` is built around two macros:
//!
//! - **[`#[derive(Oopsie)]`](Oopsie)** — generates context selectors, `Display`, `Error`, and
//!   optional `Provider` implementations for your error type.
//! - **[`#[traced]`](traced)** — batteries-included layer: injects backtrace and span-trace
//!   fields automatically, then delegates to `#[derive(Oopsie)]`.
//!
//! # Quick start
//!
//! **Define your error type:**
//!
//! ```
//! #[derive(Debug, oopsie::Oopsie)]
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
//! # #[derive(Debug, oopsie::Oopsie)]
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
//! For production error types, add [`#[traced]`](traced) as the **outermost** attribute
//! (above `#[derive]`) to automatically capture backtrace and span-trace fields:
//!
//! ```
//! #[oopsie::traced]              // ← must be above #[derive]
//! #[derive(Debug, oopsie::Oopsie)]
//! pub enum AppError {
//!     #[oopsie("Connection to {host} failed")]
//!     Connect { host: String, source: std::io::Error },
//! }
//! # fn main() {}
//! ```
//!
//! See the [`#[traced]` documentation](traced) for the full parameter reference.
//!
//! # What the derive generates
//!
//! For each variant or struct, `#[derive(Oopsie)]` generates a **context selector** — a struct
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
//! | `#[oopsie(vis = pub)]` | Override default selector visibility |
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

// Re-export the #[traced] proc-macro attribute and #[derive(Oopsie)].
pub use oopsie_macros::Oopsie;
pub use oopsie_macros::traced;

// Re-export all core types.
pub use oopsie_core::*;

/// Extension traits for error handling at the call site.
///
/// Import this at the top of any file that calls `.context(...)` or
/// `.with_context(...)` on `Result` / `Option` values, or inspects oopsie errors.
/// You do **not** need this to *define* error types — `#[oopsie::traced]` and
/// `#[derive(oopsie::Oopsie)]` work as fully-qualified attributes with no `use`.
///
/// ```
/// use oopsie::prelude::*;
///
/// #[derive(Debug, oopsie::Oopsie)]
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
    pub use crate::{ErrorExt, OptionExt, ResultExt};
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
