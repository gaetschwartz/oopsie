#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![cfg_attr(feature = "unstable-try-trait-v2", feature(try_trait_v2))]

//! Ergonomic, structured error handling for Rust.
//!
//! `oopsie` is built around two macros:
//!
//! - **[`#[traced]`](traced)** — batteries-included attribute: injects backtrace and span-trace
//!   fields automatically, then delegates to `#[derive(Oopsie)]`.
//! - **[`#[derive(Oopsie)]`](Oopsie)** — generates context selectors, `Display`, `Error`, and
//!   optional `Provider` implementations for your error type.
//!
//! # Quick start
//!
//! **Define your error type** — no `use` needed, macros work fully qualified:
//!
//! ```
//! #[oopsie::traced]
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
//! # #[oopsie::traced]
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
//! ## Attribute summary
//!
//! ### Container (`enum` / `struct`)
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg")]` | Display message (short form) |
//! | `#[oopsie(module)]` / `#[oopsie(module(name))]` | Wrap selectors in a module |
//! | `#[oopsie(module(false))]` | Disable module wrapping |
//! | `#[oopsie(vis = pub)]` | Default selector visibility |
//! | `#[oopsie(suffix)]` / `#[oopsie(suffix = "X")]` | Selector name suffix |
//!
//! ### Variant / struct
//! | Attribute | Effect |
//! |-----------|--------|
//! | `#[oopsie("msg {field}")]` | Short-form display |
//! | `#[oopsie(display("msg"), ...)]` | Long-form display (combine with other attrs) |
//! | `#[oopsie(transparent)]` | Generate `From` impl instead of a selector |
//! | `#[oopsie(help = "...")]` | Help text (via Provider API) |
//! | `#[oopsie(code = "...")]` | Error code (via Provider API) |
//!
//! ### Field
//! | Attribute | Effect |
//! |-----------|--------|
//! | *(named `source`)* | Auto-detected as the chained source error |
//! | `#[oopsie(from)]` | Mark as source (non-`source`-named field) |
//! | `#[oopsie(from(Type, transform))]` | Source with type transformation |
//! | `#[oopsie(capture)]` | Auto-filled via [`Capturable`]; excluded from selector |
//!
//! ## `#[traced]` parameters
//! | Parameter | Effect |
//! |-----------|--------|
//! | *(bare)* | Inject backtrace + spantrace |
//! | `backtrace` | Inject backtrace only |
//! | `spantrace` | Inject spantrace only |
//! | `timestamp` | Inject timestamp |
//! | `code = false` | Disable error-code injection |

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
/// #[oopsie::traced]
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
