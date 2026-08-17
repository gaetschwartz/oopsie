#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![cfg_attr(feature = "unstable-try-trait-v2", feature(try_trait_v2))]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
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
//! impls for your error type. Pass `traced` to also capture a backtrace and, with the
//! `tracing` feature, a span-trace.
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
//! Pass `traced` to automatically capture a backtrace field and, with the
//! `tracing` feature, a span-trace:
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
//! Call `start_marker!` early in a thread to hide the setup frames below it
//! from rendered traces.
//!
//! See the [`#[oopsie]` documentation](oopsie) for the full parameter reference.
//!
//! # Reporting
//!
#![cfg_attr(
    feature = "fancy",
    doc = "
[`Report`] renders an error — message, source chain, span trace, backtrace —
as a rich, colorized report. It implements [`Termination`](std::process::Termination),
so it can be returned straight from `main`. [`Report::run`] additionally
installs the library's panic hook for the duration of the closure, so
panics are rendered in the same style:

```rust
use oopsie::Report;
use oopsie::prelude::*;

#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie(\"Key not found: {key}\")]
    MissingKey { key: String },
}

fn run() -> Result<(), AppError> {
    let config: Option<&str> = Some(\"42\");
    let _value = config.context(app_oopsies::MissingKey { key: \"answer\" })?;
    Ok(())
}

fn main() -> Report<AppError> {
    Report::run(run)
}
```

When `run` returns an error, the report is printed to stderr and the
process exits with a failure code. To render panics outside of
[`Report::run`], install the hook process-wide with [`install_panic_hook`]
once, early in `main`. Color output is auto-detected; override it with
[`set_color_mode`] or per report via [`Report::no_colors`] /
[`Report::force_colors`].

*(Requires the `fancy` feature.)*
"
)]
#![cfg_attr(
    not(feature = "fancy"),
    doc = "
`Report` (requires the `fancy` feature) renders an error — message, source chain,
span trace, backtrace — as a rich, colorized report. It implements
`Termination`, so it can be returned straight from `main`. `Report::run`
additionally installs the library's panic hook for the duration of the closure,
so panics are rendered in the same style. Color output is auto-detected; override
it with `set_color_mode` or per report via `Report::no_colors` /
`Report::force_colors`.
"
)]
//!
//! # Beyond typed errors
//!
//! - [`Welp`] is a string-shaped escape hatch for prototypes and one-off
//!   errors: `Welp::new("...")`, or `.welp_context("...")` on any `Result` via
//!   the prelude.
#![cfg_attr(
    feature = "serde",
    doc = "- The [`oopsie::erased`](erased) module converts any error into a serializable, \
type-erased representation — message, source chain, code/help, span trace, and \
backtrace — for transporting errors across process boundaries, e.g. API error \
responses. *(Requires the `serde` feature.)*"
)]
#![cfg_attr(
    not(feature = "serde"),
    doc = "- The `oopsie::erased` module (requires the `serde` feature) converts any error \
into a serializable, type-erased representation — message, source chain, code/help, \
span trace, and backtrace — for transporting errors across process boundaries, \
e.g. API error responses."
)]
//!
//! # What gets generated
//!
//! For each variant or struct, `#[oopsie]` generates a **context selector** — a struct
//! containing all the fields *except* the source error and any `#[oopsie(capture)]` fields.
//!
//! Selectors for **leaf** variants (no source) expose `.build()` and `.fail()`,
//! and work with `Option::context`. Selectors for variants **with a source**
//! expose `.build_error(source)` via the [`Contextual`] trait.
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
//! An error type declared inside a function body requires `#[oopsie(module(false))]`:
//! the generated module cannot reference items local to a function, so the wrapped
//! selectors would be unable to name the error type.
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
//! Each keyword has its own subsection below; the per-scope tables are a
//! scannable index into them.
//!
//! ## Container (`enum` / `struct`)
//!
//! | Keyword | Effect |
//! |---------|--------|
//! | `module` | Wrap selectors in a module |
//! | `suffix` | Suffix on selector names |
//! | `size` | Compile-time size assertion |
//! | `path` | Path to the `oopsie` crate |
//! | `vis` | Selector visibility |
//! | `exit_code` | Default process exit code |
//!
//! The short form `#[oopsie("msg")]` on a container sets the struct's display
//! message (on enums the message goes on each variant instead).
//!
#![doc = include_str!("__private/keyword_docs/container/module.md")]
#![doc = include_str!("__private/keyword_docs/container/suffix.md")]
#![doc = include_str!("__private/keyword_docs/container/size.md")]
#![doc = include_str!("__private/keyword_docs/container/path.md")]
#![doc = include_str!("__private/keyword_docs/container/vis.md")]
#![doc = include_str!("__private/keyword_docs/container/exit_code.md")]
//!
//! ## Variant / struct
//!
//! | Keyword | Effect |
//! |---------|--------|
//! | `display` | `Display` message |
//! | `transparent` | Delegate to the wrapped error |
//! | `help` | Static help text |
//! | `code` | Error code |
//! | `exit_code` | Process exit code |
//! | `provide` | Provide a typed value |
//! | `vis` | Selector visibility |
//!
//! The short form `#[oopsie("msg {field}")]` is shorthand for `display(...)`.
//! With the `unstable` feature, `help` and `code` are additionally surfaced
//! through the nightly `Provider` API.
//!
#![doc = include_str!("__private/keyword_docs/variant/display.md")]
#![doc = include_str!("__private/keyword_docs/variant/transparent.md")]
#![doc = include_str!("__private/keyword_docs/variant/help.md")]
#![doc = include_str!("__private/keyword_docs/variant/code.md")]
#![doc = include_str!("__private/keyword_docs/variant/exit_code.md")]
#![doc = include_str!("__private/keyword_docs/variant/provide.md")]
#![doc = include_str!("__private/keyword_docs/variant/vis.md")]
//!
//! ## Field
//!
//! | Keyword | Effect |
//! |---------|--------|
//! | `from` | Mark the chained source error |
//! | `capture` | Auto-fill via [`Capturable`] |
//! | `provide` | Provide a typed value |
//! | `backtrace` | Captured backtrace field |
//! | `spantrace` | Captured span-trace field |
//! | `traces` | Packed `(Backtrace, SpanTrace)` field |
//! | `location` | Captured caller location |
//! | `help` | Dynamic help from this field's `Display` |
//! | `forward` | Forward source's traces instead of capturing new ones |
//!
//! A field named `source` is auto-detected as the chained source error, and a
//! `Box<T>` source is auto-unboxed so the selector accepts `T` (trait objects
//! exempt).
//!
#![doc = include_str!("__private/keyword_docs/field/from.md")]
#![doc = include_str!("__private/keyword_docs/field/capture.md")]
#![doc = include_str!("__private/keyword_docs/field/provide.md")]
#![doc = include_str!("__private/keyword_docs/field/backtrace.md")]
#![doc = include_str!("__private/keyword_docs/field/spantrace.md")]
#![doc = include_str!("__private/keyword_docs/field/traces.md")]
#![doc = include_str!("__private/keyword_docs/field/location.md")]
#![doc = include_str!("__private/keyword_docs/field/help.md")]
#![doc = include_str!("__private/keyword_docs/field/forward.md")]
//!
//! ## Generic error types
//!
//! Type parameters, lifetimes, const parameters, bounds, and `where` clauses are
//! supported on both enums and structs. A context selector carries only the
//! parameters its captured fields reference: a field of type `Vec<T>` makes the
//! selector generic over `T`, while a leaf variant that mentions no parameter
//! stays non-generic and infers the destination's parameters from the call site.
//!
//! No bound is added beyond what you write. Interpolating a field in a `display`
//! (or `help`/`code`) format string requires that field's type to implement the
//! formatting trait the placeholder uses, exactly as in any `format!` — if a
//! type parameter is interpolated as `{field}` but is not `Display`, the error
//! is the ordinary missing-`Display` bound at your format string. Add the bound
//! yourself (`T: Display`) when the message needs it.
//!
//! ```
//! # use oopsie::Oopsie;
//! #[derive(Debug, Oopsie)]
//! #[oopsie(module(false))]
//! enum Lookup<K: std::fmt::Debug> {
//!     #[oopsie("no entry for {key:?}")]
//!     Missing { key: K },
//! }
//!
//! let err: Lookup<u32> = Missing { key: 7 }.build();
//! assert_eq!(err.to_string(), "no entry for 7");
//! ```
//!
//! # Feature flags
//!
//! | Feature | Default | Toolchain | Effect |
//! |---------|---------|-----------|--------|
//! | `std` | yes | stable | standard-library support (allocator, `std::error::Error`, …) |
//! | `fancy` | no | stable | `Report`, colorized output, and the panic hook |
//! | `tracing` | no | stable | span-trace capture and the `tracing-subscriber` error layer |
//! | `serde` | no | stable | the `erased` module: serialize any error as a type-erased value |
//! | `chrono` | no | stable | `chrono::DateTime<Local>` timestamps for `traced(timestamp(chrono = true))` |
//! | `jiff` | no | stable | `jiff::Timestamp` / `jiff::Zoned` timestamp capture |
//! | `extras` | no | stable | the `extras` module: environment-snapshot `Capturable` helpers |
//! | `experimental-settings` | no | stable | *(experimental)* read project-wide defaults from `[package.metadata.oopsie]` |
//! | `unstable` | no | nightly | umbrella for the two unstable features below |
//! | `unstable-error-generic-member-access` | no | nightly | trace and diagnostic surfacing through the Provider API |
//! | `unstable-try-trait-v2` | no | nightly | `?` converts a failed `Result` directly into a `Report` |
//!
//! # no_std
//!
//! `std` is on by default; build with `default-features = false` for `no_std` +
//! `alloc` targets (the crate always needs an allocator — bring your own with
//! `extern crate alloc` and a global allocator in the consumer). `fancy`
//! enables `std` (colorized `Report` rendering needs it), so it is unavailable
//! on `no_std` targets. Panic hooks, backtraces, `tracing`, clock-based
//! timestamps (`chrono` / `jiff`), and `extras` are also `std`-only;
//! everything else — the derive, `Welp`, error chains, and
//! `Diagnostic`/`Display` rendering — works unchanged.
//!
//! # Project-wide settings (experimental)
//!
//! `experimental-settings` is a debug tool, not a stable API: its shape and
//! behavior may change or be removed in a semver-compatible release.
//!
//! With the `experimental-settings` feature, a `[package.metadata.oopsie]`
//! table in a crate's `Cargo.toml` sets a default for every error derived in
//! that crate. The same keys may also be declared once at the workspace root
//! under `[workspace.metadata.oopsie]`, so a multi-crate workspace can share a
//! single set of defaults. Settings are read from each crate's own manifest,
//! never its dependencies'.
//!
//! | key | per-type equivalent | effect |
//! |---|---|---|
//! | `max-size = 64` | `size(..=64)` | cap on the error type's size, in bytes |
//! | `default-suffix = "Ctx"` | `suffix("Ctx")` | suffix on generated selector names |
//! | `default-suffix = false` | `suffix(false)` | no selector suffix |
//! | `default-vis = "pub(crate)"` | `vis(pub(crate))` | visibility of selectors and their module |
//! | `module = true` | `module` | wrap selectors in a module |
//! | `module = false` | `module(false)` | leave selectors at the crate top level |
//! | `module = { suffix = "errors" }` | *(none)* | name the module `<type>_errors` |
//! | `traced = true` | `traced` | trace by default |
//! | `traced = { location = false }` | `traced(location = false)` | tune a `traced(...)` sub-toggle |
//!
//! `module` and `traced` each take a bool or a table:
//!
//! ```toml
//! [package.metadata.oopsie.module]
//! enabled = true
//! suffix = "errors"
//!
//! [package.metadata.oopsie.traced]
//! enabled = true
//! location = true
//! timestamp = false
//! packed = true
//! boxed = true
//! code = true
//! ```
//!
//! ## Precedence
//!
//! Each knob resolves independently, in this order (first set value wins):
//!
//! 1. A per-type `#[oopsie(...)]` attribute.
//! 2. The crate's own `[package.metadata.oopsie]`.
//! 3. `[workspace.metadata.oopsie]` at the workspace root.
//! 4. The hardcoded default.
//!
//! Setting one key in `[package.metadata.oopsie]` does not discard the remaining
//! keys inherited from the workspace — each knob is resolved on its own.
//! A crate that the workspace `exclude`s does not inherit workspace settings.
//!
//! When `max-size` is exceeded, the error names its source — either
//! `[package.metadata.oopsie] max-size` or `[workspace.metadata.oopsie] max-size`
//! — so the right table is clear.
//!
//! ## Notes
//!
//! - `traced` only affects types using the `#[oopsie]` attribute; a bare
//!   `#[derive(Oopsie)]` is never traced. `enabled` is the trace-by-default
//!   switch; the other keys set defaults applied whenever a type is traced.
//! - `default-suffix` (the selector suffix) and `module.suffix` (the module name)
//!   are independent; `module.suffix` has no per-type form — a per-type
//!   `module(name)` sets the module's full name instead.
//! - `timestamp = true` records a `SystemTime`; the `chrono` / `provide` forms
//!   stay per-type (`traced(timestamp(chrono = true))`).
//! - `max-size` is silently skipped on generic error types: a per-type
//!   `size(...)` is rejected on them too, since their size depends on the
//!   type arguments and has no single value to assert.
//! - Setting the `CARGO_WORKSPACE_DIR` env var overrides workspace-root
//!   discovery, so `[workspace.metadata.oopsie]` is read from that directory
//!   instead of the ancestor Cargo detects; Cargo itself never sets it.
//!
//! `max-size` rejects `0` and `default-suffix = true` is rejected as ambiguous
//! (use a name or `false`); `default-suffix` must be a valid identifier
//! fragment, `module.suffix` a valid identifier (it can stand in as the whole
//! module name) and `default-vis` a valid visibility, else the build fails
//! with a clear message.
//!
//! ## Known limitations
//!
//! `experimental-settings` is a Cargo feature on the proc-macro crate, and
//! Cargo unifies features across a build: if *any* crate in the dependency
//! graph enables it, every crate compiled in that invocation gets the
//! settings machinery turned on. The macro still reads each expanding crate's
//! own manifest (never another crate's), but this means workspace defaults
//! under `[workspace.metadata.oopsie]` can end up applied to a crate that
//! never listed `experimental-settings` in its own `Cargo.toml` — whether a
//! crate is affected depends on which other crates share the build. Treat
//! this as a debug tool for a single crate or a fully-opted-in workspace, not
//! as a knob safe to enable from just one member of a larger one.

extern crate alloc;

// Re-export the proc-macro attribute and derive.
pub use oopsie_macros::Oopsie;
pub use oopsie_macros::oopsie;

// Explicit re-export of the public surface from `oopsie-core`. Avoid
// `pub use oopsie_core::*` so transitive deps (tracing-error,
// tracing-subscriber) don't accidentally become part of oopsie's SemVer
// contract via incidental glob re-export.
#[cfg(feature = "extras")]
pub use oopsie_core::extras;
pub use oopsie_core::{
    AsErrorSource, Backtrace, Capturable, Chain, Contextual, Diagnostic, ErrorChainExt, ErrorCode,
    HelpText, NoSource, OptionExt, OptionalSpanTrace, ResultExt, SpanTrace, SpanTraceStatus, Welp,
    WelpOptionExt, WelpResultExt,
};
#[cfg(feature = "std")]
pub use oopsie_core::{RustBacktrace, start_marker};

/// Thread-local control over backtrace capture.
///
/// [`current`](backtrace::current) reports the effective [`RustBacktrace`] setting
/// for the calling thread, derived from the environment unless overridden.
/// [`set_override`](backtrace::set_override), [`clear_override`](backtrace::clear_override),
/// and [`with_override`](backtrace::with_override) force that setting on the current
/// thread, taking precedence over the environment.
/// [`current_panic`](backtrace::current_panic) reports the setting that applies to
/// panic backtraces, which honor `RUST_BACKTRACE` only.
#[cfg(feature = "std")]
pub mod backtrace {
    pub use oopsie_core::{
        clear_rust_backtrace_override as clear_override, rust_backtrace as current,
        rust_panic_backtrace as current_panic, set_rust_backtrace_override as set_override,
        with_rust_backtrace_override as with_override,
    };
}

// Hidden surface macro-generated code reaches via `::oopsie::__private::…`:
// the autoref-probe machinery re-exported from `oopsie-core`, plus the
// hover-documentation targets, which live here so their `include_str!`
// examples compile against this crate's `#[oopsie]` macro. Not part of the
// public API; do not depend on its contents.
#[doc(hidden)]
mod generated;

#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "fancy")]
    pub use crate::generated::{GENERATED_SITES, GeneratedSite};
    #[cfg(feature = "fancy")]
    pub use linkme;
    pub use oopsie_core::__private::*;

    pub mod documented;
}

/// Serializable, type-erased representations of errors.
///
/// [`ErasedError`](erased::ErasedError) captures an error's message, source chain, code, help, span
/// trace, and backtrace into owned data that implements `Serialize` /
/// `Deserialize`, for transporting errors across process boundaries such as API
/// error responses.
#[cfg(feature = "serde")]
pub mod erased {
    pub use oopsie_core::erased::{
        Diagnostics, ErasedBacktrace, ErasedError, ErasedFrame, ErasedLocation, ErasedMetadata,
        ErasedSpan, ErasedSpanTrace, TracingLevel,
    };
}

/// `tracing` integration helpers.
#[cfg(feature = "tracing")]
pub use oopsie_core::tracing;

/// Common imports for error handling at the call site.
///
/// Brings in the extension traits behind `.context(...)` / `.with_context(...)`
/// on `Result` / `Option`, the [`Diagnostic`] accessors, and the ready-made
/// error type [`Welp`] (plus `Report` with the `fancy` feature).
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
    #[cfg(feature = "fancy")]
    pub use crate::Report;
    pub use crate::{
        Contextual, Diagnostic, ErrorChainExt, OptionExt, ResultExt, Welp, WelpOptionExt,
        WelpResultExt,
    };
}

#[cfg(feature = "fancy")]
mod color;
#[cfg(feature = "fancy")]
pub use color::{ColorMode, get_color_mode, set_color_mode};

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

#[cfg(feature = "fancy")]
mod style;
#[cfg(feature = "fancy")]
pub use style::Style;

#[cfg(feature = "fancy")]
mod theme;
#[cfg(feature = "fancy")]
pub use theme::{Theme, get_theme, set_theme};
