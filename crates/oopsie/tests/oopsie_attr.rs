#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{Oopsie, oopsie};

// ---- Test 1: attr macro on enum — basic usage ----

#[oopsie]
#[derive(Debug)]
pub enum AppError {
    #[oopsie("conn failed: {addr}")]
    ConnFailed { addr: String },
}

#[test]
fn attr_enum_basic() {
    // The attr macro generates module `app_oopsies` with selectors inside.
    let err = app_oopsies::ConnFailed { addr: "127.0.0.1" }.build();
    assert!(matches!(err, AppError::ConnFailed { ref addr, .. } if addr == "127.0.0.1"));
    assert_eq!(err.to_string(), "conn failed: 127.0.0.1");
}

// ---- Test 2: attr macro on struct — basic usage ----

#[oopsie]
#[derive(Debug)]
pub struct ConnError {
    reason: String,
}

#[test]
fn attr_struct_basic() {
    // The attr macro adds #[oopsie(suffix)] for structs, so selector is ConnErrorOopsie.
    let err = ConnErrorOopsie { reason: "refused" }.build();
    assert_eq!(err.reason, "refused");
}

// ---- Test 3: attr macro injects backtrace (no panic) ----

#[oopsie]
#[derive(Debug)]
pub enum InjectError {
    #[oopsie("injected")]
    Injected { info: String },
}

#[test]
fn attr_injects_backtrace() {
    // Should not panic — backtrace and spantrace are auto-injected by the attr macro.
    let err = inject_oopsies::Injected { info: "test" }.build();
    assert!(matches!(err, InjectError::Injected { ref info, .. } if info == "test"));
}

// ---- Test 4: #[help] and #[code] on variant ----

#[oopsie]
#[derive(Debug)]
pub enum HelpCodeError {
    #[help("Check network")]
    #[code("net::conn_refused")]
    #[oopsie("connection refused")]
    Refused { target: String },
}

#[test]
fn attr_with_help_and_code() {
    let err = help_code_oopsies::Refused { target: "db" }.build();
    assert!(matches!(err, HelpCodeError::Refused { ref target, .. } if target == "db"));

    #[cfg(feature = "unstable")]
    {
        let help = core::error::request_value::<oopsie::HelpText>(&err);
        assert!(help.is_some());
        let code = core::error::request_value::<oopsie::ErrorCode>(&err);
        assert!(code.is_some());
    }
}

// ---- Test 5: does not duplicate #[derive(Oopsie)] ----
// When the user writes `#[derive(Debug, Oopsie)]` and `#[oopsie]`, the attr macro
// detects the existing Oopsie derive and doesn't add it again.

#[::oopsie::oopsie]
#[derive(Debug, Oopsie)]
pub struct DupDeriveError {
    detail: String,
}

#[test]
fn attr_does_not_duplicate_derive() {
    // If derive were duplicated, this would fail to compile.
    let err = DupDeriveErrorOopsie { detail: "dup" }.build();
    assert_eq!(err.detail, "dup");
}

// ---- Test 6: enum module naming convention ----

#[oopsie]
#[derive(Debug)]
pub enum FooBarError {
    #[oopsie("foo")]
    Foo,
}

#[oopsie]
#[derive(Debug)]
pub enum MyError {
    #[oopsie("my")]
    My,
}

#[test]
fn attr_enum_module_naming() {
    // FooBarError → strip "Error" → "FooBar" → snake_case → "foo_bar" → "foo_bar_oopsies"
    let _ = foo_bar_oopsies::Foo.build();
    // MyError → strip "Error" → "My" → snake_case → "my" → "my_oopsies"
    let _ = my_oopsies::My.build();
}
