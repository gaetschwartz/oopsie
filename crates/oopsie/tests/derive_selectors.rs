#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Contextual as _, NoSource, Oopsie};
use std::error::Error as _;
use std::io;

// ---- Enum definitions ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AppError {
    NotFound { path: String },

    IoFailed { source: io::Error, context: String },

    Timeout { source: io::Error },

    Config { host: String, port: u16 },
}

// ---- Struct definitions ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct ParseError {
    msg: String,
}

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
struct WrapError {
    source: io::Error,
    detail: String,
}

// ---- Tests ----

#[test]
fn leaf_enum_build() {
    let err = NotFound {
        path: "/tmp/missing",
    }
    .build();
    assert!(matches!(err, AppError::NotFound { path } if path == "/tmp/missing"));
}

#[test]
fn leaf_enum_fail() {
    let result: Result<(), AppError> = NotFound { path: "gone" }.fail();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, AppError::NotFound { path } if path == "gone"));
}

#[test]
fn source_enum_build_error() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let err: AppError = IoFailed {
        context: "reading config",
    }
    .build_error(io_err);
    match &err {
        AppError::IoFailed { source, context } => {
            assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(context, "reading config");
        }
        other => panic!("expected IoFailed, got {other:?}"),
    }
    // Error::source() should return the io::Error
    assert!(err.source().is_some());
}

#[test]
fn source_only_unit_selector() {
    // Timeout has only a source field, so its selector is a unit struct.
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
    let err: AppError = Timeout.build_error(io_err);
    assert!(matches!(err, AppError::Timeout { .. }));
    assert!(err.source().is_some());
}

#[test]
fn selector_into_bounds() {
    // NotFound selector field `path` has type String, but accepts &str via Into.
    let err = NotFound { path: "abc" }.build();
    assert!(matches!(err, AppError::NotFound { path } if path == "abc"));
}

#[test]
fn multi_field_selector() {
    let err = Config {
        host: "localhost",
        port: 8080u16,
    }
    .build();
    match &err {
        AppError::Config { host, port } => {
            assert_eq!(host, "localhost");
            assert_eq!(*port, 8080);
        }
        other => panic!("expected Config, got {other:?}"),
    }
}

#[test]
fn struct_leaf_build_fail() {
    // ParseError uses #[oopsie(suffix)], so selector is ParseOopsie ("Error" stripped).
    let err = ParseOopsie {
        msg: "unexpected token",
    }
    .build();
    assert_eq!(err.msg, "unexpected token");

    let result: Result<(), ParseError> = ParseOopsie { msg: "bad" }.fail();
    assert!(result.is_err());
}

#[test]
fn struct_source_build_error() {
    let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
    let err: WrapError = WrapOopsie {
        detail: "while reading",
    }
    .build_error(io_err);
    assert_eq!(err.detail, "while reading");
    assert!(err.source().is_some());
    assert_eq!(err.source().unwrap().to_string(), "file missing");
}

// ---- Error suffix stripping ----

/// Variant names ending in "Error" get the suffix stripped in selector names.
#[derive(Debug, Oopsie)]
#[oopsie(module(strip_oopsies))]
enum ServiceError {
    #[oopsie("connection error")]
    ConnectionError { addr: String },

    #[oopsie("timed out")]
    TimeoutError,

    #[oopsie("ok variant")]
    NotAnIssue { msg: String },
}

#[test]
fn error_suffix_stripped_from_selector() {
    // ConnectionError variant → selector is `Connection` (not `ConnectionError`)
    let err = strip_oopsies::Connection { addr: "localhost" }.build();
    assert!(matches!(err, ServiceError::ConnectionError { .. }));
    assert_eq!(err.to_string(), "connection error");

    // TimeoutError variant → selector is `Timeout` (not `TimeoutError`)
    let err = strip_oopsies::Timeout.build();
    assert!(matches!(err, ServiceError::TimeoutError));

    // NotAnIssue variant → selector stays `NotAnIssue` (no "Error" suffix to strip)
    let err = strip_oopsies::NotAnIssue { msg: "fine" }.build();
    assert!(matches!(err, ServiceError::NotAnIssue { .. }));
}

#[derive(Debug, Oopsie)]
#[oopsie(module(suffix_strip_oopsies), suffix)]
enum SuffixStripError {
    #[oopsie("conn failed")]
    ConnectionError { addr: String },
}

#[test]
fn error_suffix_stripped_with_oopsie_suffix() {
    // ConnectionError + suffix → `ConnectionOopsie` (not `ConnectionErrorOopsie`)
    let err = suffix_strip_oopsies::ConnectionOopsie { addr: "db" }.build();
    assert!(matches!(err, SuffixStripError::ConnectionError { .. }));
}

#[test]
fn leaf_selector_build_error_no_source() {
    // Leaf selectors implement Contextual<NoSource> for OptionExt support.
    let err: AppError = NotFound { path: "x" }.build_error(NoSource);
    assert!(matches!(err, AppError::NotFound { path } if path == "x"));
}

// ---- selector fields accept any `Into<field_ty>`, not just &str -> String ----

/// Wrapper carrying a custom `From<Meters> for u16` so we can confirm the
/// selector's `__T: Into<field_ty>` bound accepts user-defined conversions.
struct Meters(u16);

impl From<Meters> for u16 {
    fn from(m: Meters) -> Self {
        m.0
    }
}

/// Wrapper with a custom `From<HostName> for String` (distinct from the stdlib
/// `&str -> String` path already exercised by `selector_into_bounds`).
struct HostName(&'static str);

impl From<HostName> for String {
    fn from(h: HostName) -> Self {
        format!("host::{}", h.0)
    }
}

#[test]
fn selector_accepts_custom_into_impls() {
    // `Config { host: String, port: u16 }` — pass custom wrapper types whose
    // `From` impls feed the generated `Into<String>` / `Into<u16>` bounds.
    let err = Config {
        host: HostName("db"),
        port: Meters(443),
    }
    .build();
    match &err {
        AppError::Config { host, port } => {
            assert_eq!(host, "host::db");
            assert_eq!(*port, 443);
        }
        other => panic!("expected Config, got {other:?}"),
    }
}

// ---- generated selector struct derives Debug, Copy, Clone ----
// Every selector (both unit and field-bearing) derives `Debug, Copy, Clone`, so a
// move-after-use compiles.

#[test]
fn selector_derives_debug_clone_copy() {
    // Unit selector (Timeout: source-only variant -> unit struct).
    let unit = Timeout;
    assert_eq!(format!("{unit:?}"), "Timeout");
    let unit_clone = unit.clone();
    // Copy: using `unit` after the `let unit2 = unit` move below must still compile.
    let unit2 = unit;
    let _ = unit;
    assert_eq!(format!("{unit2:?}"), "Timeout");
    assert_eq!(format!("{unit_clone:?}"), "Timeout");

    // Field-bearing selector. Debug output includes the field name and value.
    let sel = NotFound { path: "/tmp/x" };
    let dbg = format!("{sel:?}");
    assert!(dbg.contains("NotFound"), "debug was {dbg:?}");
    assert!(dbg.contains("/tmp/x"), "debug was {dbg:?}");

    let sel_clone = sel.clone();
    // Copy: build() consumes by value yet `sel` remains usable afterwards.
    let e1 = sel.build();
    let e2 = sel.build();
    assert!(matches!(e1, AppError::NotFound { .. }));
    assert!(matches!(e2, AppError::NotFound { .. }));

    let e3 = sel_clone.build();
    assert!(matches!(e3, AppError::NotFound { .. }));
}

// ---- selector struct fields are `pub` ----
// Selector fields are unconditionally `pub`. Visibility is observable by
// reading/destructuring the field through a struct-literal-built selector
// instance, and across a module boundary.

mod external {
    use super::{AppError, Config};
    use oopsie::Contextual as _;

    /// If `host`/`port` were private, this function (in a child module) could
    /// neither construct nor read those fields.
    pub fn build_via_pub_fields() -> AppError {
        let sel = Config {
            host: "remote",
            port: 9000u16,
        };
        // Read pub fields back out before consuming the selector.
        assert_eq!(sel.host, "remote");
        assert_eq!(sel.port, 9000u16);
        sel.build()
    }
}

#[test]
fn selector_fields_are_pub() {
    let sel = NotFound { path: "visible" };
    // Direct field read — only compiles if `path` is `pub`.
    let p: &str = sel.path;
    assert_eq!(p, "visible");

    // Destructuring a pub field.
    let NotFound { path } = NotFound {
        path: "destructured",
    };
    assert_eq!(path, "destructured");

    // Field access works from a separate module too.
    let err = external::build_via_pub_fields();
    assert!(matches!(err, AppError::Config { .. }));
}

// A raw-identifier variant (`r#try`) strips to the keyword `try`, which a
// non-rawness-aware `Ident::new` would panic on; the selector must round-trip
// as a raw ident instead.
#[test]
#[expect(
    non_camel_case_types,
    reason = "the raw-identifier variant under test is intentionally keyword-shaped"
)]
fn raw_identifier_variant_generates_raw_selector() {
    #[oopsie::oopsie]
    #[oopsie(module(false))]
    enum KwError {
        #[oopsie("looped")]
        r#try,
    }
    let e = r#try.build();
    assert_eq!(e.to_string(), "looped");
}

// `capture = false` opts a trace-typed field out of type-based auto-capture:
// it stays a selector field the caller supplies, instead of being captured.
#[test]
fn capture_false_keeps_backtrace_field_on_selector() {
    #[oopsie::oopsie]
    #[oopsie(module(false))]
    enum E {
        #[oopsie("snap")]
        Snap {
            #[oopsie(capture = false)]
            bt: oopsie::Backtrace,
            msg: String,
        },
    }
    let e = Snap {
        bt: <oopsie::Backtrace as oopsie::Capturable>::capture(),
        msg: "m",
    }
    .build();
    assert_eq!(e.to_string(), "snap");
}

// ---- Struct module-form selectors (default) ----

// A plain struct with no container attributes: selector lives in the
// auto-named module and carries the `Error`-stripped name, matching enums.
#[derive(Debug, Oopsie)]
#[oopsie("load failed: {what}")]
struct LoadError {
    what: String,
}

#[test]
fn struct_default_module_form() {
    let err = LoadOopsie { what: "config" }.build();
    assert_eq!(err.to_string(), "load failed: config");
    assert_eq!(err.what, "config");
}

// A traced struct also lands in module form; the injected backtrace stays
// auto-captured and absent from the selector's fields.
#[oopsie::oopsie(traced)]
#[oopsie("decode failed: {stage}")]
struct DecodeError {
    stage: String,
}

#[test]
fn struct_traced_default_module_form() {
    let err = DecodeOopsie { stage: "header" }.fail::<()>();
    let err = err.unwrap_err();
    assert_eq!(err.to_string(), "decode failed: header");
    assert!(oopsie::Diagnostic::oopsie_backtrace(&err).is_some());
}

// `module(false)` keeps the selector bare at item scope; the `Error`-stripped
// name doesn't collide with the type.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("flat: {detail}")]
struct FlatError {
    detail: String,
}

#[test]
fn struct_module_false_bare_selector() {
    let err = FlatOopsie { detail: "x" }.build();
    assert_eq!(err.to_string(), "flat: x");
}

// `suffix` distinguishes the selector from the type name, which is what lets a
// non-`Error`-suffixed struct opt back into a bare `module(false)` selector
// without colliding with itself.
#[derive(Debug, Oopsie)]
#[oopsie(module(false), suffix)]
#[oopsie("widget broke: {part}")]
struct Widget {
    part: String,
}

#[test]
fn struct_suffix_disambiguates_bare_selector() {
    let err = WidgetOopsie { part: "gear" }.build();
    assert_eq!(err.to_string(), "widget broke: gear");
}

// A `pub(crate)` struct's module-wrapped selector must reach back to crate
// scope through the same visibility lift enums use.
mod restricted {
    #[expect(
        clippy::redundant_pub_crate,
        reason = "pub(crate) is the point: it exercises the restricted-visibility lift path"
    )]
    #[derive(Debug, oopsie::Oopsie)]
    #[oopsie("scoped: {what}")]
    pub(crate) struct ScopedError {
        pub(crate) what: String,
    }
}

#[test]
fn struct_pub_crate_vis_lifted_into_module() {
    let err = restricted::ScopedOopsie { what: "y" }.build();
    assert_eq!(err.to_string(), "scoped: y");
}
