#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::oopsie;

// ---- Test 1: #[oopsie(traced)] on enum — basic usage ----

#[oopsie(traced)]
pub enum AppError {
    #[oopsie("conn failed: {addr}")]
    ConnFailed { addr: String },
}

#[test]
fn traced_enum_basic() {
    // The derive generates module `app_oopsies` with selectors inside.
    let err = app_oopsies::ConnFailed { addr: "127.0.0.1" }.build();
    assert!(matches!(&err, AppError::ConnFailed { addr, .. } if addr == "127.0.0.1"));
    assert_eq!(err.to_string(), "conn failed: 127.0.0.1");
}

// ---- Test 2: #[oopsie(traced)] on struct — basic usage ----

#[oopsie(traced)]
pub struct ConnError {
    reason: String,
}

#[test]
fn traced_struct_basic() {
    // The derive defaults to suffix="Oopsie" for structs, so selector is ConnOopsie.
    let err = ConnOopsie { reason: "refused" }.build();
    assert_eq!(err.reason, "refused");
}

// ---- Test 3: #[oopsie(traced)] injects backtrace (no panic) ----

#[oopsie(traced)]
pub enum InjectError {
    #[oopsie("injected")]
    Injected { info: String },
}

#[test]
fn traced_injects_backtrace() {
    // Should not panic — backtrace and spantrace are auto-injected by #[oopsie(traced)].
    let err = inject_oopsies::Injected { info: "test" }.build();
    assert!(matches!(&err, InjectError::Injected { info, .. } if info == "test"));
}

// ---- Test 4: #[help] and #[code] on variant ----

#[oopsie(traced)]
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
    assert!(matches!(&err, HelpCodeError::Refused { target, .. } if target == "db"));

    #[cfg(feature = "unstable-error-generic-member-access")]
    {
        let help = core::error::request_value::<oopsie::HelpText>(&err);
        assert!(help.is_some());
        let code = core::error::request_value::<oopsie::ErrorCode>(&err);
        assert!(code.is_some());
    }
}

// ---- Test 5: enum module naming convention ----

#[oopsie(traced)]
pub enum FooBarError {
    #[oopsie("foo")]
    Foo,
}

#[oopsie(traced)]
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

#[oopsie(traced)]
pub enum PreExistingBtError {
    #[oopsie("has backtrace")]
    #[oopsie(provide(ref, oopsie::Backtrace => bt.as_ref()))]
    HasBt {
        msg: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn traced_does_not_duplicate_backtrace() {
    let err = pre_existing_bt_oopsies::HasBt { msg: "test" }.build();
    assert_eq!(err.to_string(), "has backtrace");
}

// ---- Test 7: does not duplicate pre-existing spantrace field ----

#[oopsie(traced)]
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

#[oopsie(traced)]
pub struct PreExistingBtStructError {
    msg: String,
    #[oopsie(capture)]
    bt: Box<oopsie::Backtrace>,
}

#[test]
fn traced_struct_does_not_duplicate_backtrace() {
    let err = PreExistingBtStructOopsie { msg: "struct bt" }.build();
    assert_eq!(err.msg, "struct bt");
}

// ---- Test 9: explicit override — backtrace only ----

#[oopsie(backtrace)]
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

#[oopsie(spantrace)]
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

#[oopsie(traced, code = false)]
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

// ---- Trace storage layouts (packed/boxed matrix) ----

use oopsie::Diagnostic as _;

// Default: packed + boxed => one Box<(Backtrace, SpanTrace)> field.
#[oopsie(traced)]
pub enum DefaultPackedError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// packed + inline => one (Backtrace, SpanTrace) field.
#[oopsie(traced(boxed = false))]
pub enum PackedInlineError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// unpacked + boxed (prior default) => Box<Backtrace>, Box<SpanTrace>.
#[oopsie(traced(packed = false))]
pub enum SeparateBoxedError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// unpacked + inline => Backtrace, SpanTrace.
#[oopsie(traced(packed = false, boxed = false))]
pub enum SeparateInlineError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// mixed: both traces listed (explicit mode keeps both), spantrace inline.
#[oopsie(traced(packed = false, backtrace, spantrace(boxed = false)))]
pub enum MixedError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// Single trace (backtrace only) — packed is a no-op; lone boxed backtrace.
#[oopsie(traced(backtrace))]
pub enum SingleBacktraceError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

fn assert_both_traces<E: oopsie::Diagnostic>(e: &E) {
    assert!(e.oopsie_backtrace().is_some(), "backtrace accessor missing");
    assert!(e.oopsie_spantrace().is_some(), "spantrace accessor missing");
}

#[test]
fn layout_default_packed_exposes_both_traces() {
    let e = default_packed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_packed_inline_exposes_both_traces() {
    let e = packed_inline_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_separate_boxed_exposes_both_traces() {
    let e = separate_boxed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_separate_inline_exposes_both_traces() {
    let e = separate_inline_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_mixed_exposes_both_traces() {
    let e = mixed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_single_trace_fallback_backtrace_only() {
    let e = single_backtrace_oopsies::Boom { info: "x" }.build();
    assert!(e.oopsie_backtrace().is_some());
    assert!(e.oopsie_spantrace().is_none());
}

// Struct path (symmetric to the enum cases above): default packed + boxed,
// and the unpacked inline layout that exercises the `Borrow` accessor.
#[oopsie(traced)]
pub struct PackedStructError {
    info: String,
}

#[oopsie(traced(packed = false, boxed = false))]
pub struct InlineStructError {
    info: String,
}

#[test]
fn layout_struct_default_packed_exposes_both_traces() {
    let e = PackedStructOopsie { info: "x" }.build();
    assert_both_traces(&e);
}

#[test]
fn layout_struct_separate_inline_exposes_both_traces() {
    let e = InlineStructOopsie { info: "x" }.build();
    assert_both_traces(&e);
}

// A field merely *named* `backtrace` of an unrelated type is an ordinary field,
// not the error's backtrace. The real backtrace is still injected and surfaced.
#[oopsie(traced)]
pub struct WrongTypedBacktraceError {
    backtrace: String,
    info: String,
}

#[test]
fn wrong_typed_backtrace_field_is_ordinary_and_real_backtrace_injected() {
    let e = WrongTypedBacktraceOopsie {
        backtrace: "external textual backtrace",
        info: "x",
    }
    .build();
    // The injected packed trace is still surfaced via the stable accessor.
    assert_both_traces(&e);
    // The user's same-named field is untouched (not treated as a backtrace).
    assert_eq!(e.backtrace, "external textual backtrace");
}
