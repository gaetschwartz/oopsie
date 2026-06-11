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

#[cfg(feature = "tracing")]
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

#[cfg(feature = "tracing")]
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

// ---- Test 9: opting out of spantrace — backtrace only ----

#[cfg(feature = "tracing")]
#[oopsie(traced(spantrace(false)))]
pub enum BacktraceOnlyError {
    #[oopsie("bt only")]
    BtOnly { msg: String },
}

#[cfg(feature = "tracing")]
#[test]
fn traced_explicit_backtrace_only() {
    use oopsie::Diagnostic as _;
    let err = backtrace_only_oopsies::BtOnly { msg: "test" }.build();
    assert_eq!(err.to_string(), "bt only");
    assert!(
        err.oopsie_backtrace().is_some(),
        "backtrace stays on when only spantrace is disabled"
    );
    assert!(
        err.oopsie_spantrace().is_none(),
        "`spantrace(false)` must NOT inject a spantrace"
    );
}

// ---- Test 10: opting out of backtrace — spantrace only ----

#[oopsie(traced(backtrace(false)))]
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

#[oopsie(traced(code = false))]
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

// mixed: backtrace stays on by default, spantrace tuned to inline.
#[cfg(feature = "tracing")]
#[oopsie(traced(packed = false, spantrace(boxed = false)))]
pub enum MixedError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// Single trace (backtrace only) — packed is a no-op; lone boxed backtrace.
#[cfg(feature = "tracing")]
#[oopsie(traced(spantrace(false)))]
pub enum SingleBacktraceError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

#[cfg(feature = "tracing")]
fn assert_both_traces<E: oopsie::Diagnostic>(e: &E) {
    assert!(e.oopsie_backtrace().is_some(), "backtrace accessor missing");
    assert!(e.oopsie_spantrace().is_some(), "spantrace accessor missing");
}

#[cfg(feature = "tracing")]
#[test]
fn layout_default_packed_exposes_both_traces() {
    let e = default_packed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
#[test]
fn layout_packed_inline_exposes_both_traces() {
    let e = packed_inline_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
#[test]
fn layout_separate_boxed_exposes_both_traces() {
    let e = separate_boxed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
#[test]
fn layout_separate_inline_exposes_both_traces() {
    let e = separate_inline_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
#[test]
fn layout_mixed_exposes_both_traces() {
    let e = mixed_oopsies::Boom { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
#[test]
fn layout_single_trace_fallback_backtrace_only() {
    let e = single_backtrace_oopsies::Boom { info: "x" }.build();
    assert!(e.oopsie_backtrace().is_some());
    assert!(e.oopsie_spantrace().is_none());
}

// Without the tracing feature the layout types collapse to backtrace-only.
// Verify the accessor still works — the test is intentionally ungated on tracing
// so it exercises the no-tracing codepath in every bare combo.
#[cfg(not(feature = "tracing"))]
#[test]
fn layout_default_packed_backtrace_only_without_tracing() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    let e = default_packed_oopsies::Boom { info: "x" }.build();
    assert!(
        e.oopsie_backtrace().is_some(),
        "backtrace accessor must work in no-tracing layout"
    );
    oopsie::clear_rust_backtrace_override();
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

#[cfg(feature = "tracing")]
#[test]
fn layout_struct_default_packed_exposes_both_traces() {
    let e = PackedStructOopsie { info: "x" }.build();
    assert_both_traces(&e);
}

#[cfg(feature = "tracing")]
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

#[cfg(feature = "tracing")]
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

// ════════════════════════════════════════════════════════════════════════
// Timestamp injection
//
// Findings characterized by these tests:
// * `#[oopsie(traced(timestamp))]` injects a `__oopsie_timestamp` field
//   alongside the default traces. Like backtrace/spantrace it is auto-captured
//   at build time — it never appears on the context selector. Default type is
//   `std::time::SystemTime`.
// * Timestamp-only errors disable the traces explicitly:
//   `traced(backtrace(false), spantrace(false), timestamp)` — the diagnostic
//   accessors return `None`.
// * `timestamp(chrono = true)` swaps the field type to
//   `::chrono::DateTime<::chrono::Local>` — auto-capture then requires the
//   `chrono` feature of `oopsie`, so that test is feature-gated. The leading
//   `::` means the field references the crate-root `chrono`.
// * `chrono` and `provide` are opt-in: an omitted flag inside a `timestamp(...)`
//   block stays disabled. `timestamp(provide = true)` surfaces the timestamp
//   through the `Provider` API (queryable via `request_value::<SystemTime>()`).
// ════════════════════════════════════════════════════════════════════════

// nested `traced(timestamp)` — default SystemTime field.
#[oopsie(traced(timestamp))]
pub enum TimestampError {
    #[oopsie("ts: {info}")]
    Boom { info: String },
}

#[test]
fn timestamp_nested_injects_systemtime_field() {
    let before = std::time::SystemTime::now();
    // Selector takes only the user field — no `__oopsie_timestamp`.
    let e = timestamp_oopsies::Boom {
        info: "x".to_owned(),
    }
    .build();
    assert_eq!(e.to_string(), "ts: x");
    let TimestampError::Boom {
        __oopsie_timestamp, ..
    } = &e;
    // Field is concretely `SystemTime` and auto-captured at build time.
    let stored: &std::time::SystemTime = __oopsie_timestamp;
    assert!(*stored >= before);
}

// timestamp ONLY: both traces disabled explicitly inside `traced(...)`.
#[cfg(feature = "tracing")]
#[oopsie(traced(backtrace(false), spantrace(false), timestamp))]
pub enum BareTimestampError {
    #[oopsie("bare ts")]
    Boom { info: String },
}

#[cfg(feature = "tracing")]
#[test]
fn timestamp_is_auto_captured_and_absent_from_selector() {
    use oopsie::Diagnostic as _;
    let before = std::time::SystemTime::now();
    let e = bare_timestamp_oopsies::Boom {
        info: "x".to_owned(),
    }
    .build();
    let BareTimestampError::Boom {
        __oopsie_timestamp, ..
    } = &e;
    assert!(*__oopsie_timestamp >= before);
    assert!(
        e.oopsie_backtrace().is_none(),
        "backtrace(false) must not inject a backtrace"
    );
    assert!(
        e.oopsie_spantrace().is_none(),
        "spantrace(false) must not inject a spantrace"
    );
}

// Auto-capture must also satisfy `transparent` variants: the generated `From`
// impl fills the timestamp itself (no `Default` bound on the field type).
#[oopsie]
pub enum TransparentInnerError {
    #[oopsie("inner")]
    Inner,
}

#[oopsie(traced(timestamp))]
pub enum TransparentTimestampError {
    #[oopsie(transparent)]
    Wrapped { source: TransparentInnerError },
}

#[test]
fn timestamp_composes_with_transparent() {
    let before = std::time::SystemTime::now();
    let outer: TransparentTimestampError = transparent_inner_oopsies::Inner.build().into();
    let TransparentTimestampError::Wrapped {
        __oopsie_timestamp, ..
    } = &outer;
    let _: &std::time::SystemTime = __oopsie_timestamp;
    assert!(*__oopsie_timestamp >= before);
}

// `timestamp(chrono = true)` swaps the field type to chrono's
// DateTime<Local>. Auto-capture needs the `Capturable` impl behind the
// `chrono` feature of `oopsie`, hence the gate.
#[cfg(feature = "chrono")]
mod chrono_timestamp {
    use oopsie::oopsie;

    #[oopsie(traced(timestamp(chrono = true)))]
    pub enum ChronoTimestampError {
        #[oopsie("chrono ts")]
        Boom { info: String },
    }

    #[test]
    fn timestamp_chrono_flag_swaps_field_type_to_datetime_local() {
        let before = chrono::Local::now();
        let e = chrono_timestamp_oopsies::Boom {
            info: "x".to_owned(),
        }
        .build();
        assert_eq!(e.to_string(), "chrono ts");
        let ChronoTimestampError::Boom {
            __oopsie_timestamp, ..
        } = &e;
        // The field type is concretely `chrono::DateTime<chrono::Local>`;
        // assigning it to that binding fails to compile if the macro chose any
        // other type.
        let stored: &chrono::DateTime<chrono::Local> = __oopsie_timestamp;
        assert!(*stored >= before);
    }
}

// `timestamp(provide = true)` surfaces the injected timestamp through
// the Provider API by emitting a real `provide(SystemTime => *field)`.
#[oopsie(traced(timestamp(provide = true)))]
pub enum ProvidedTimestampError {
    #[oopsie("provided ts")]
    Boom { info: String },
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn timestamp_provide_surfaces_via_provider_api() {
    let before = std::time::SystemTime::now();
    let e = provided_timestamp_oopsies::Boom {
        info: "x".to_owned(),
    }
    .build();
    let got = core::error::request_value::<std::time::SystemTime>(&e);
    let ts = got.expect("timestamp must be queryable via the provider API");
    assert!(ts >= before);
}

// ════════════════════════════════════════════════════════════════════════
// Custom backtrace/spantrace type override
//
// Findings:
// * `backtrace(r#type = Path)` injects `Path` as the field type. Constraints
//   discovered: (a) the override's last path segment must be literally
//   `Backtrace` (the injector tags the field `#[oopsie(backtrace)]`, whose
//   field-type validation requires that segment name); (b) the type must
//   implement `Capturable` AND `Borrow<oopsie::Backtrace>` (the generated
//   Diagnostic accessor borrows it as `&oopsie::Backtrace`); (c) it must be
//   inline (`boxed = false` on the trace) — `Box<Custom>` only borrows to
//   `Custom`, not to `oopsie::Backtrace`, so a boxed custom type won't compile.
// ════════════════════════════════════════════════════════════════════════

#[cfg(feature = "tracing")]
mod custom_trace {
    use std::borrow::Borrow;

    #[derive(Debug)]
    #[expect(
        unnameable_types,
        reason = "pub fixture type in a private module: referenced by the generated pub Diagnostic accessor but not nameable at pub"
    )]
    pub struct Backtrace {
        pub inner: oopsie::Backtrace,
        pub tag: u32,
    }

    impl oopsie::Capturable for Backtrace {
        fn capture() -> Self {
            Self {
                inner: oopsie::Capturable::capture(),
                tag: 0xC0FFEE,
            }
        }
    }

    impl Borrow<oopsie::Backtrace> for Backtrace {
        fn borrow(&self) -> &oopsie::Backtrace {
            &self.inner
        }
    }
}

#[cfg(feature = "tracing")]
#[oopsie(traced(
    packed = false,
    backtrace(r#type = custom_trace::Backtrace, boxed = false),
    spantrace(boxed = false)
))]
pub enum CustomBacktraceError {
    #[oopsie("custom bt: {info}")]
    Boom { info: String },
}

#[cfg(feature = "tracing")]
#[test]
fn custom_backtrace_type_is_injected_and_captured() {
    use oopsie::Diagnostic as _;
    let e = custom_backtrace_oopsies::Boom {
        info: "x".to_owned(),
    }
    .build();

    let CustomBacktraceError::Boom {
        __oopsie_backtrace, ..
    } = &e;
    // The injected field is the custom newtype, not `oopsie::Backtrace`. Binding
    // to `&custom_trace::Backtrace` only typechecks if the override took effect,
    // and the tag proves our `Capturable::capture` ran.
    let bt: &custom_trace::Backtrace = __oopsie_backtrace;
    assert_eq!(bt.tag, 0xC0FFEE);

    // The custom type still satisfies the Diagnostic accessor via Borrow, and
    // the separately-listed spantrace is injected normally.
    assert!(e.oopsie_backtrace().is_some());
    assert!(e.oopsie_spantrace().is_some());
}

// The injected chrono timestamp type routes through `oopsie::__private::chrono`,
// so the caller needs no direct `chrono` dependency and no `::chrono` in scope.
#[cfg(feature = "chrono")]
#[test]
fn chrono_timestamp_compiles_without_direct_dep_path() {
    #[oopsie(traced(timestamp(chrono = true)))]
    #[oopsie(module(false))]
    pub enum ChronoStamp {
        #[oopsie("boom")]
        Boom,
    }
    let e = Boom.build();
    assert!(format!("{e:?}").contains("__oopsie_timestamp"));
}

// A user field merely *named* `timestamp` of an unrelated type does not
// suppress timestamp injection; the injected field uses the mangled name.
#[test]
fn wrongly_typed_timestamp_named_field_does_not_suppress_injection() {
    #[oopsie(traced(timestamp))]
    pub struct TsNamed {
        timestamp: u64,
    }
    let e = TsNamedOopsie { timestamp: 5u64 }.build();
    assert!(format!("{e:?}").contains("__oopsie_timestamp"));
}

// ---- List-form `code(...)` suppresses the traced auto-code ----
//
// `code("fmt", args)` is the only way to interpolate an error code. The traced
// auto-code injection must treat it as a user code and step aside, on both the
// stable accessor and (nightly) the provider path, which std resolves first-wins.

#[oopsie(traced)]
#[oopsie(module(false))]
pub enum ListCodeError {
    #[oopsie("conn failed")]
    #[oopsie(code("app::{}", kind))]
    Conn { kind: String },
}

#[test]
fn list_form_code_suppresses_auto_code() {
    use oopsie::Diagnostic as _;
    let err = Conn {
        kind: "db".to_owned(),
    }
    .build();
    let code = err
        .oopsie_error_code()
        .expect("list-form code should be present");
    assert_eq!(code.as_str(), "app::db");
    // The auto-code would qualify the variant path; its absence proves it was
    // suppressed rather than overriding the user's formatted code.
    assert!(
        !code.as_str().contains("ListCodeError"),
        "auto-code leaked: {}",
        code.as_str()
    );
}

#[cfg(feature = "unstable-error-generic-member-access")]
#[test]
fn list_form_code_accessor_and_provider_agree() {
    use oopsie::Diagnostic as _;
    let err = Conn {
        kind: "net".to_owned(),
    }
    .build();
    let via_accessor = err.oopsie_error_code().expect("accessor yields code");
    let via_provider =
        core::error::request_value::<oopsie::ErrorCode>(&err).expect("provider yields code");
    assert_eq!(via_accessor.as_str(), "app::net");
    assert_eq!(via_accessor.as_str(), via_provider.as_str());
}
