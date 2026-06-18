#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::Oopsie;

// Test 1: Default module name = strip "Error" + snake_case + "_oopsies".
#[derive(Debug, Oopsie)]
#[oopsie(module)]
enum MyAppError {
    #[oopsie("not found: {name}")]
    NotFound { name: String },

    #[oopsie("timed out")]
    TimedOut,
}

#[test]
fn default_module_name() {
    // MyAppError → strip "Error" → "MyApp" → snake_case → "my_app" → "my_app_oopsies"
    let err = my_app_oopsies::NotFound { name: "widget" }.build();
    assert!(matches!(err, MyAppError::NotFound { .. }));
    assert_eq!(err.to_string(), "not found: widget");

    let err = my_app_oopsies::TimedOut.build();
    assert!(matches!(err, MyAppError::TimedOut));
    assert_eq!(err.to_string(), "timed out");
}

// Test 2: Custom module name.
#[derive(Debug, Oopsie)]
#[oopsie(module(custom_mod))]
enum ServiceError {
    #[oopsie("unavailable: {reason}")]
    Unavailable { reason: String },

    #[oopsie("rate limited")]
    RateLimited,
}

#[test]
fn custom_module_name() {
    // Selectors live in `custom_mod::` module.
    let err = custom_mod::Unavailable {
        reason: "maintenance",
    }
    .build();
    assert!(matches!(err, ServiceError::Unavailable { .. }));
    assert_eq!(err.to_string(), "unavailable: maintenance");

    let err = custom_mod::RateLimited.build();
    assert!(matches!(err, ServiceError::RateLimited));
    assert_eq!(err.to_string(), "rate limited");
}

// Test 3: Module disabled — selectors at same scope level.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum FlatError {
    #[oopsie("bad input: {detail}")]
    BadInput { detail: String },

    #[oopsie("internal")]
    Internal,
}

#[test]
fn module_false_no_wrapping() {
    // Selectors are directly accessible, no module prefix.
    let err = BadInput {
        detail: "negative value",
    }
    .build();
    assert!(matches!(err, FlatError::BadInput { .. }));
    assert_eq!(err.to_string(), "bad input: negative value");

    let err = Internal.build();
    assert!(matches!(err, FlatError::Internal));
    assert_eq!(err.to_string(), "internal");
}

// Test 4: Bare enum (no container `#[oopsie(...)]`) — enums default to an
// auto-named module. `AppError` → strip "Error" → "App" → snake_case → "app"
// → module `app_oopsies`.
#[derive(Debug, Oopsie)]
enum AppError {
    #[oopsie("connection failed")]
    Connect,

    #[oopsie("disk full: {bytes}")]
    DiskFull { bytes: u64 },
}

#[test]
fn bare_enum_default_auto_module() {
    let err = app_oopsies::Connect.build();
    assert!(matches!(err, AppError::Connect));
    assert_eq!(err.to_string(), "connection failed");

    let err = app_oopsies::DiskFull { bytes: 4096u64 }.build();
    assert!(matches!(err, AppError::DiskFull { bytes: 4096 }));
    assert_eq!(err.to_string(), "disk full: 4096");
}

// Test 5: Enum literally named `Error` with module wrapping. The auto-naming
// in gen_module.rs strips the "Error" suffix unconditionally (no non-empty
// filter, unlike selector naming), leaving "". snake_case("") is "", which is
// empty so no underscore is appended, and "oopsies" is pushed — the module is
// named bare `oopsies` (not `_oopsies`).
#[derive(Debug, Oopsie)]
#[oopsie(module)]
enum Error {
    #[oopsie("generic error")]
    Generic,

    #[oopsie("specific: {what}")]
    Specific { what: String },
}

#[test]
fn enum_named_error_module_is_oopsies() {
    let err = oopsies::Generic.build();
    assert!(matches!(err, Error::Generic));
    assert_eq!(err.to_string(), "generic error");

    let err = oopsies::Specific { what: "boom" }.build();
    assert!(matches!(err, Error::Specific { .. }));
    assert_eq!(err.to_string(), "specific: boom");
}

// Test 6: struct with explicit custom module name.
#[derive(Debug, Oopsie)]
#[oopsie(module(query_oopsies))]
#[oopsie("query failed: {what}")]
pub struct QueryError {
    what: String,
}

#[test]
fn struct_module_wraps_selector() {
    let err = query_oopsies::QueryOopsie { what: "join" }.build();
    assert_eq!(err.to_string(), "query failed: join");
}

// Test 7: struct with auto-named module (module keyword alone).
// ParseError → strip "Error" → "Parse" → snake_case → "parse" → "parse_oopsies"
#[derive(Debug, Oopsie)]
#[oopsie(module)]
#[oopsie("parse failed")]
pub struct ParseError;

#[test]
fn struct_auto_module_name() {
    let err = parse_oopsies::ParseOopsie.build();
    assert_eq!(err.to_string(), "parse failed");
}

// Test 8: struct default (no module attr) — structs default to module form
// just like enums. FlatStructError → strip "Error" → "FlatStruct" → snake_case
// → "flat_struct" → module `flat_struct_oopsies`, selector `FlatStruct`.
#[derive(Debug, Oopsie)]
#[oopsie("flat struct error")]
pub struct FlatStructError;

#[test]
fn struct_default_no_module() {
    let err = FlatStructOopsie.build();
    assert_eq!(err.to_string(), "flat struct error");
}

// Test 9: struct with pub(crate) visibility + module(...) exercises the
// lift_into_child_module path for structs. pub(crate) is crate-absolute, so
// the selector inside the generated child module stays reachable crate-wide.
mod crate_vis {
    #[expect(
        clippy::redundant_pub_crate,
        reason = "pub(crate) is the point: it exercises the restricted-visibility lift path"
    )]
    #[derive(Debug, oopsie::Oopsie)]
    #[oopsie(module(scoped_oopsies))]
    #[oopsie("scoped: {what}")]
    pub(crate) struct ScopedError {
        pub(crate) what: String,
    }
}

#[test]
fn restricted_vis_struct_module_selector_reachable_crate_wide() {
    let err = crate_vis::scoped_oopsies::ScopedOopsie { what: "x" }.build();
    assert_eq!(err.to_string(), "scoped: x");
}

// Test 10: Module wrapping combined with a variant-level `vis(...)` override.
// Both selectors default to `pub` (mirroring the `pub enum`). `PublicVariant`
// additionally carries an explicit `vis(pub)` override. Both live inside the
// `test_oopsies` module; the re-export below verifies `pub use` works for the
// explicitly-annotated selector.
#[derive(Debug, Oopsie)]
#[oopsie(module(test_oopsies))]
pub enum MixedModuleVisError {
    #[oopsie("private variant")]
    PrivateVariant { detail: String },

    #[oopsie("public variant")]
    #[oopsie(vis(pub))]
    PublicVariant { code: i32 },
}

pub use test_oopsies::PublicVariant;

#[test]
fn module_wrapping_with_variant_vis_override() {
    // Module wrapping applies to both selectors regardless of vis.
    let err = test_oopsies::PrivateVariant { detail: "secret" }.build();
    assert!(matches!(err, MixedModuleVisError::PrivateVariant { .. }));
    assert_eq!(err.to_string(), "private variant");

    // The `pub`-overridden selector is reachable both via the module path and
    // via the crate-level `pub use` re-export above.
    let err = test_oopsies::PublicVariant { code: 7i32 }.build();
    assert!(matches!(
        err,
        MixedModuleVisError::PublicVariant { code: 7 }
    ));
    assert_eq!(err.to_string(), "public variant");

    let err = PublicVariant { code: 42i32 }.build();
    assert!(matches!(
        err,
        MixedModuleVisError::PublicVariant { code: 42 }
    ));
}
