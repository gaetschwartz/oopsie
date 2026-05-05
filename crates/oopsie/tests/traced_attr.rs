#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all)]

use oopsie::{Oopsie, traced};

// ---- Test 1: traced macro on enum — basic usage ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum AppError {
    #[oopsie("conn failed: {addr}")]
    ConnFailed { addr: String },
}

#[test]
fn traced_enum_basic() {
    // The derive generates module `app_oopsies` with selectors inside.
    let err = app_oopsies::ConnFailed { addr: "127.0.0.1" }.build();
    assert!(matches!(err, AppError::ConnFailed { ref addr, .. } if addr == "127.0.0.1"));
    assert_eq!(err.to_string(), "conn failed: 127.0.0.1");
}

// ---- Test 2: traced macro on struct — basic usage ----

#[traced]
#[derive(Debug, Oopsie)]
pub struct ConnError {
    reason: String,
}

#[test]
fn traced_struct_basic() {
    // The derive defaults to suffix="Oopsie" for structs, so selector is ConnOopsie.
    let err = ConnOopsie { reason: "refused" }.build();
    assert_eq!(err.reason, "refused");
}

// ---- Test 3: traced macro injects backtrace (no panic) ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum InjectError {
    #[oopsie("injected")]
    Injected { info: String },
}

#[test]
fn traced_injects_backtrace() {
    // Should not panic — backtrace and spantrace are auto-injected by #[traced].
    let err = inject_oopsies::Injected { info: "test" }.build();
    assert!(matches!(err, InjectError::Injected { ref info, .. } if info == "test"));
}

// ---- Test 4: #[help] and #[code] on variant ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum HelpCodeError {
    #[oopsie(
        display("connection refused"),
        help = "Check network",
        code = "net::conn_refused"
    )]
    Refused { target: String },
}

#[test]
fn traced_with_help_and_code() {
    let err = help_code_oopsies::Refused { target: "db" }.build();
    assert!(matches!(err, HelpCodeError::Refused { ref target, .. } if target == "db"));

    #[cfg(feature = "unstable-error-generic-member-access")]
    {
        let help = core::error::request_value::<oopsie::HelpText>(&err);
        assert!(help.is_some());
        let code = core::error::request_value::<oopsie::ErrorCode>(&err);
        assert!(code.is_some());
    }
}

// ---- Test 5: enum module naming convention ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum FooBarError {
    #[oopsie("foo")]
    Foo,
}

#[traced]
#[derive(Debug, Oopsie)]
pub enum MyError {
    #[oopsie("my")]
    My,
}

#[test]
fn traced_enum_module_naming() {
    // FooBarError → strip "Error" → "FooBar" → snake_case → "foo_bar" → "foo_bar_oopsies"
    let _ = foo_bar_oopsies::Foo.build();
    // MyError → strip "Error" → "My" → snake_case → "my" → "my_oopsies"
    let _ = my_oopsies::My.build();
}

// ---- Test 6: does not duplicate pre-existing backtrace field ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum PreExistingBtError {
    #[oopsie("has backtrace")]
    #[oopsie(provide(ref, oopsie::BackTrace => bt.as_ref()))]
    HasBt {
        msg: String,
        #[oopsie(capture)]
        bt: Box<oopsie::BackTrace>,
    },
}

#[test]
fn traced_does_not_duplicate_backtrace() {
    let err = pre_existing_bt_oopsies::HasBt { msg: "test" }.build();
    assert_eq!(err.to_string(), "has backtrace");
}

// ---- Test 7: does not duplicate pre-existing spantrace field ----

#[traced]
#[derive(Debug, Oopsie)]
pub enum PreExistingStError {
    #[oopsie("has spantrace")]
    #[oopsie(provide(ref, oopsie::SpanTrace => st.as_ref()))]
    HasSt {
        msg: String,
        #[oopsie(capture)]
        st: Box<oopsie::SpanTrace>,
    },
}

#[test]
fn traced_does_not_duplicate_spantrace() {
    let err = pre_existing_st_oopsies::HasSt { msg: "test" }.build();
    assert_eq!(err.to_string(), "has spantrace");
}

// ---- Test 8: struct does not duplicate pre-existing backtrace ----

#[traced]
#[derive(Debug, Oopsie)]
pub struct PreExistingBtStructError {
    msg: String,
    #[oopsie(capture)]
    bt: Box<oopsie::BackTrace>,
}

#[test]
fn traced_struct_does_not_duplicate_backtrace() {
    let err = PreExistingBtStructOopsie { msg: "struct bt" }.build();
    assert_eq!(err.msg, "struct bt");
}

// ---- Test 9: explicit override — backtrace only ----

#[traced(backtrace)]
#[derive(Debug, Oopsie)]
pub enum BacktraceOnlyError {
    #[oopsie("bt only")]
    BtOnly { msg: String },
}

#[test]
fn traced_explicit_backtrace_only() {
    // Only backtrace should be injected, not spantrace
    let err = backtrace_only_oopsies::BtOnly { msg: "test" }.build();
    assert_eq!(err.to_string(), "bt only");
}

// ---- Test 10: explicit override — spantrace only ----

#[traced(spantrace)]
#[derive(Debug, Oopsie)]
pub enum SpantraceOnlyError {
    #[oopsie("st only")]
    StOnly { msg: String },
}

#[test]
fn traced_explicit_spantrace_only() {
    let err = spantrace_only_oopsies::StOnly { msg: "test" }.build();
    assert_eq!(err.to_string(), "st only");
}

// ---- Test 11: code = false disables auto error code ----

#[traced(code = false)]
#[derive(Debug, Oopsie)]
pub enum NoCodeError {
    #[oopsie("no code")]
    NoCode { msg: String },
}

#[test]
fn traced_code_disabled() {
    let err = no_code_oopsies::NoCode { msg: "test" }.build();
    assert_eq!(err.to_string(), "no code");

    #[cfg(feature = "unstable-error-generic-member-access")]
    {
        let code = core::error::request_value::<oopsie::ErrorCode>(&err);
        assert!(
            code.is_none(),
            "ErrorCode should not be provided when code=false"
        );
    }
}
