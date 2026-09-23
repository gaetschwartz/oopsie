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

// ---- unit variant + explicit discriminant, nothing injected ----
//
// A fieldless variant carrying an explicit discriminant (no `#[repr(int)]`) is
// legal Rust. With every injectable trace turned off, the macro must leave it a
// unit variant: rewriting it to `A {} = 1` makes it a non-unit discriminant
// variant, which rustc rejects with E0732 — pointing into generated code.

#[oopsie(traced(backtrace(false), spantrace(false), location(false)))]
pub enum DiscriminantUnit {
    #[oopsie("a")]
    A = 1,
}

#[test]
fn unit_variant_with_discriminant_compiles() {
    let err = discriminant_unit_oopsies::A.build();
    assert!(matches!(err, DiscriminantUnit::A));
    assert_eq!(DiscriminantUnit::A as u8, 1);
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

// ---- Test 9: opting out of spantrace — backtrace only ----

#[oopsie(traced(spantrace(false)))]
pub enum BacktraceOnlyError {
    #[oopsie("bt only")]
    BtOnly { msg: String },
}

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

// ---- Test 12: per-variant traced toggle on enum variants ----

#[oopsie(traced)]
pub enum VariantToggleError {
    #[oopsie(traced = false)]
    #[oopsie("off: {msg}")]
    Off { msg: String },
    #[oopsie("on: {msg}")]
    On { msg: String },
}

#[test]
fn traced_variant_toggle_controls_injection_per_variant() {
    use oopsie::Diagnostic as _;

    let off = variant_toggle_oopsies::Off {
        msg: "x".to_owned(),
    }
    .build();
    assert_eq!(off.to_string(), "off: x");
    assert!(
        off.oopsie_backtrace().is_none(),
        "variant traced=false must skip backtrace injection"
    );
    assert!(
        off.oopsie_spantrace().is_none(),
        "variant traced=false must skip spantrace injection"
    );

    let on = variant_toggle_oopsies::On {
        msg: "x".to_owned(),
    }
    .build();
    assert_eq!(on.to_string(), "on: x");
    assert!(
        on.oopsie_backtrace().is_some(),
        "variant without override inherits enum traced behavior"
    );
    assert!(
        on.oopsie_spantrace().is_some(),
        "variant without override inherits enum traced behavior"
    );
}

// ---- Test 13: discriminant variant can opt out of traced injection ----

#[oopsie(traced)]
pub enum DiscriminantVariantOptOutError {
    #[oopsie(traced = false)]
    #[oopsie("a")]
    A = 1,
}

#[test]
fn traced_variant_opt_out_allows_explicit_discriminant() {
    let err = discriminant_variant_opt_out_oopsies::A.build();
    assert!(matches!(err, DiscriminantVariantOptOutError::A));
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
#[oopsie(traced(packed = false, spantrace(boxed = false)))]
pub enum MixedError {
    #[oopsie("boom: {info}")]
    Boom { info: String },
}

// Single trace (backtrace only) — packed is a no-op; lone boxed backtrace.
#[oopsie(traced(spantrace(false)))]
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

// Without the runtime tracing feature, capture degrades to a no-op stub: the
// spantrace accessor still returns a value, but it reports nothing captured.
// The backtrace continues to capture normally.
#[cfg(not(feature = "tracing"))]
#[test]
fn featureless_spantrace_degrades_to_uncaptured() {
    use oopsie::Diagnostic as _;
    oopsie::backtrace::set_override(oopsie::RustBacktrace::Enabled);
    let e = default_packed_oopsies::Boom { info: "x" }.build();
    assert!(
        !e.oopsie_spantrace()
            .expect("spantrace accessor present")
            .is_captured(),
        "featureless spantrace must report nothing captured"
    );
    assert!(
        e.oopsie_backtrace().is_some(),
        "backtrace accessor must capture in the featureless layout"
    );
    oopsie::backtrace::clear_override();
}

// Explicit `traced(spantrace)` is honored featureless — the shape the old
// pre-pass rejected — capturing the no-op stub.
#[cfg(not(feature = "tracing"))]
#[oopsie(traced(spantrace))]
pub struct FeaturelessSpantraceError {
    info: String,
}

#[cfg(not(feature = "tracing"))]
#[test]
fn featureless_explicit_spantrace_compiles_and_degrades() {
    use oopsie::Diagnostic as _;
    let e = FeaturelessSpantraceOopsie { info: "x" }.build();
    assert!(
        !e.oopsie_spantrace()
            .expect("spantrace accessor present")
            .is_captured()
    );
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
// `backtrace(r#type = Path)` injects `Path` (boxed by default); it must implement
// `Capturable` and `Borrow<oopsie::Backtrace>`.
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
    #[oopsie(module(false), suffix)]
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

// ---- Raw-ident variants must not leak `r#` into the auto error code ----

#[test]
#[expect(
    non_camel_case_types,
    reason = "the raw-identifier variant under test is intentionally keyword-shaped"
)]
fn raw_ident_variant_auto_code_drops_prefix() {
    use oopsie::Diagnostic as _;

    #[oopsie(traced)]
    #[oopsie(module(false))]
    enum RawCodeError {
        #[oopsie("raw")]
        r#type { info: String },
    }

    let err = r#type {
        info: "x".to_owned(),
    }
    .build();
    let code = err
        .oopsie_error_code()
        .expect("auto-code should be present");
    assert!(
        code.as_str().ends_with("::RawCodeError::type"),
        "auto-code leaked raw prefix: {}",
        code.as_str()
    );
    assert!(
        !code.as_str().contains("r#"),
        "auto-code leaked raw prefix: {}",
        code.as_str()
    );
}

// ---- `path = "..."` (renamed crate) must not lose the traced auto-code ----

#[test]
fn renamed_crate_path_preserves_auto_code() {
    use oopsie as renamed_oopsie;
    use oopsie::Diagnostic as _;

    #[oopsie(traced, path = "renamed_oopsie")]
    #[oopsie(module(false))]
    pub enum RenamedPathError {
        #[oopsie("boom: {msg}")]
        Boom { msg: String },
    }

    let err = Boom {
        msg: "x".to_owned(),
    }
    .build();
    let code = err
        .oopsie_error_code()
        .expect("auto-code should survive a renamed crate path");
    assert!(
        code.as_str().ends_with("::RenamedPathError::Boom"),
        "auto-code lost under a renamed crate path: {:?}",
        code.as_str()
    );
}

// ---- `cfg_attr`-gated helper attrs behave like the derive's ----

#[oopsie(traced)]
pub enum CfgAttrVariantError {
    #[cfg_attr(all(), oopsie(traced = false))]
    #[oopsie("untraced")]
    Untraced { info: String },
    #[cfg_attr(any(), oopsie(traced = false))]
    #[oopsie("traced")]
    Traced { info: String },
    #[cfg_attr(all(), oopsie("shown {info}"))]
    Shown { info: String },
    #[cfg_attr(any(), oopsie("never {info}"))]
    Hidden { info: String },
    #[cfg_attr(all(), oopsie(code = "custom::gated"))]
    #[oopsie("coded")]
    Coded { info: String },
}

#[test]
fn cfg_attr_traced_false_follows_predicate() {
    use oopsie::Diagnostic as _;
    let off = cfg_attr_variant_oopsies::Untraced { info: "x" }.build();
    assert!(off.oopsie_backtrace().is_none());
    assert!(off.oopsie_location().is_none());
    let on = cfg_attr_variant_oopsies::Traced { info: "x" }.build();
    assert!(on.oopsie_backtrace().is_some());
    assert!(on.oopsie_location().is_some());
}

#[test]
fn cfg_attr_display_follows_predicate() {
    let shown = cfg_attr_variant_oopsies::Shown { info: "x" }.build();
    assert_eq!(shown.to_string(), "shown x");
    let hidden = cfg_attr_variant_oopsies::Hidden { info: "x" }.build();
    assert_eq!(hidden.to_string(), "Hidden");
}

#[test]
fn cfg_attr_code_replaces_auto_code() {
    use oopsie::Diagnostic as _;
    let err = cfg_attr_variant_oopsies::Coded { info: "x" }.build();
    assert_eq!(
        err.oopsie_error_code().map(|c| c.as_str().to_owned()),
        Some("custom::gated".to_owned())
    );
}

#[oopsie(traced)]
#[cfg_attr(all(), oopsie("struct shown {info}"))]
pub struct CfgAttrStructError {
    info: String,
}

#[oopsie(traced)]
#[cfg_attr(all(), oopsie(transparent))]
pub struct CfgAttrStructTransparent {
    source: CfgAttrLeafError,
}

#[test]
fn cfg_attr_on_struct_container_is_honored() {
    use oopsie::Diagnostic as _;
    let err = CfgAttrStructOopsie { info: "x" }.build();
    assert_eq!(err.to_string(), "struct shown x");

    let leaf = cfg_attr_leaf_oopsies::Leaf { msg: "x" }.build();
    let leaf_code = leaf.oopsie_error_code().map(|c| c.as_str().to_owned());
    let err = CfgAttrStructTransparent::from(leaf);
    assert_eq!(err.to_string(), "leaf");
    assert_eq!(
        err.oopsie_error_code().map(|c| c.as_str().to_owned()),
        leaf_code
    );
}

#[oopsie(traced)]
pub enum CfgAttrLeafError {
    #[oopsie("leaf")]
    Leaf { msg: String },
}

#[oopsie(traced)]
pub struct CfgAttrForwardOn {
    #[cfg_attr(all(), oopsie(forward))]
    source: CfgAttrLeafError,
}

#[oopsie(traced)]
pub struct CfgAttrForwardOff {
    #[cfg_attr(any(), oopsie(forward))]
    source: CfgAttrLeafError,
}

#[oopsie(traced)]
pub struct CfgAttrForwardPlain {
    #[oopsie(forward)]
    source: CfgAttrLeafError,
}

#[oopsie(traced)]
pub struct CfgAttrCapturePlain {
    source: CfgAttrLeafError,
}

#[oopsie(traced)]
pub enum CfgAttrForwardEnum {
    #[oopsie("on")]
    On {
        #[cfg_attr(all(), oopsie(forward))]
        source: CfgAttrLeafError,
    },
    #[oopsie("off")]
    Off {
        #[cfg_attr(any(), oopsie(forward))]
        source: CfgAttrLeafError,
    },
}

#[test]
fn cfg_attr_forward_follows_predicate() {
    use oopsie::Contextual as _;
    use oopsie::Diagnostic as _;
    use std::mem::size_of;

    assert_eq!(
        size_of::<CfgAttrForwardOn>(),
        size_of::<CfgAttrForwardPlain>()
    );
    assert_eq!(
        size_of::<CfgAttrForwardOff>(),
        size_of::<CfgAttrCapturePlain>()
    );

    let leaf = || cfg_attr_leaf_oopsies::Leaf { msg: "x" }.build();

    let src = leaf();
    let src_bt: *const oopsie::Backtrace = src.oopsie_backtrace().unwrap();
    let on: CfgAttrForwardOn = CfgAttrForwardOnOopsie.build_error(src);
    assert!(std::ptr::eq(on.oopsie_backtrace().unwrap(), src_bt));

    let src = leaf();
    let src_bt: *const oopsie::Backtrace = src.oopsie_backtrace().unwrap();
    let on: CfgAttrForwardEnum = cfg_attr_forward_enum_oopsies::On.build_error(src);
    assert!(std::ptr::eq(on.oopsie_backtrace().unwrap(), src_bt));

    let src = leaf();
    let src_bt: *const oopsie::Backtrace = src.oopsie_backtrace().unwrap();
    let off: CfgAttrForwardEnum = cfg_attr_forward_enum_oopsies::Off.build_error(src);
    assert!(!std::ptr::eq(off.oopsie_backtrace().unwrap(), src_bt));
}

// ---- a `#[cfg]`-gated own trace field is mirrored when gated out ----

#[oopsie(traced)]
pub struct CfgGatedOutBacktrace {
    info: String,
    #[cfg(any())]
    backtrace: oopsie::Backtrace,
}

#[oopsie(traced)]
pub struct CfgGatedInBacktrace {
    info: String,
    #[cfg(all())]
    backtrace: oopsie::Backtrace,
}

#[oopsie(traced)]
pub enum CfgGatedTraceEnum {
    #[oopsie("out")]
    Out {
        #[cfg(any())]
        backtrace: oopsie::Backtrace,
    },
    #[oopsie("in")]
    In {
        #[cfg(all())]
        backtrace: oopsie::Backtrace,
    },
}

#[test]
fn cfg_gated_out_trace_field_still_gets_a_backtrace() {
    use oopsie::Diagnostic as _;
    let err = CfgGatedOutBacktraceOopsie { info: "x" }.build();
    assert!(err.oopsie_backtrace().is_some());
    assert!(err.oopsie_spantrace().is_some());
    let err = cfg_gated_trace_enum_oopsies::Out.build();
    assert!(err.oopsie_backtrace().is_some());
}

#[test]
fn cfg_gated_in_trace_field_is_the_backtrace() {
    use oopsie::Diagnostic as _;
    let err = CfgGatedInBacktraceOopsie { info: "x" }.build();
    assert!(std::ptr::eq(
        err.oopsie_backtrace().unwrap(),
        &raw const err.backtrace
    ));
    assert!(err.oopsie_spantrace().is_some());
    let err = cfg_gated_trace_enum_oopsies::In.build();
    let CfgGatedTraceEnum::In { backtrace, .. } = &err else {
        unreachable!()
    };
    assert!(std::ptr::eq(err.oopsie_backtrace().unwrap(), backtrace));
}

// ---- explicit trace roles accept aliased and custom types ----

type AliasedBt = oopsie::Backtrace;

type AliasedTraces = Box<(oopsie::Backtrace, oopsie::SpanTrace)>;

#[oopsie(traced)]
pub struct AliasedTracesError {
    info: String,
    #[oopsie(traces)]
    traces: AliasedTraces,
}

#[oopsie(traced)]
pub struct AliasedBacktraceError {
    info: String,
    #[oopsie(backtrace)]
    bt: AliasedBt,
}

pub mod renamed_trace {
    #[derive(Debug)]
    pub struct MyBt(pub oopsie::Backtrace);

    impl oopsie::Capturable for MyBt {
        #[track_caller]
        fn capture() -> Self {
            Self(oopsie::Capturable::capture())
        }
    }

    impl core::borrow::Borrow<oopsie::Backtrace> for MyBt {
        fn borrow(&self) -> &oopsie::Backtrace {
            &self.0
        }
    }

    #[derive(Debug)]
    pub struct MySt(pub oopsie::SpanTrace);

    impl oopsie::Capturable for MySt {
        #[track_caller]
        fn capture() -> Self {
            Self(oopsie::Capturable::capture())
        }
    }

    impl core::borrow::Borrow<oopsie::SpanTrace> for MySt {
        fn borrow(&self) -> &oopsie::SpanTrace {
            &self.0
        }
    }
}

#[oopsie(traced(packed = false, backtrace(r#type = renamed_trace::MyBt)))]
pub enum RenamedBacktraceError {
    #[oopsie("custom")]
    Custom { info: String },
}

#[oopsie(traced(packed = false, spantrace(r#type = renamed_trace::MySt, boxed = false)))]
pub struct RenamedSpantraceError {
    info: String,
}

#[oopsie(traced(packed = false, backtrace(r#type = renamed_trace::MyBt)))]
pub struct RenamedBoxedBacktraceError {
    info: String,
}

#[test]
fn explicit_trace_roles_accept_aliased_and_custom_types() {
    use oopsie::Diagnostic as _;
    let err = AliasedBacktraceOopsie { info: "x" }.build();
    assert!(std::ptr::eq(
        err.oopsie_backtrace().unwrap(),
        &raw const err.bt
    ));

    let err = renamed_backtrace_oopsies::Custom { info: "x" }.build();
    assert!(err.oopsie_backtrace().is_some());

    let err = RenamedSpantraceOopsie { info: "x" }.build();
    assert!(err.oopsie_spantrace().is_some());

    let err = RenamedBoxedBacktraceOopsie { info: "x" }.build();
    assert!(err.oopsie_backtrace().is_some());

    let err = AliasedTracesOopsie { info: "x" }.build();
    assert!(std::ptr::eq(
        err.oopsie_backtrace().unwrap(),
        &raw const err.traces.0
    ));
}

// ---- correlated cfg predicates never combine into impossible configurations ----

#[oopsie(traced)]
pub enum CorrelatedCfgError {
    #[cfg_attr(debug_assertions, oopsie(traced = false))]
    #[oopsie("gated")]
    Gated {
        info: String,
        #[cfg(not(debug_assertions))]
        backtrace: oopsie::Backtrace,
    },
}

#[oopsie(traced(timestamp))]
pub enum CorrelatedTimestampError {
    #[cfg_attr(not(debug_assertions), oopsie(traced = false))]
    #[oopsie("gated")]
    Gated {
        #[cfg(not(any(debug_assertions)))]
        at: std::time::SystemTime,
    },
}

#[test]
fn correlated_cfg_attr_and_cfg_field_agree() {
    use oopsie::Diagnostic as _;
    let err = correlated_cfg_oopsies::Gated { info: "x" }.build();
    assert_eq!(err.oopsie_backtrace().is_some(), !cfg!(debug_assertions));
}

#[test]
fn correlated_timestamp_field_never_conflicts() {
    #[cfg(debug_assertions)]
    {
        let before = std::time::SystemTime::now();
        let CorrelatedTimestampError::Gated {
            __oopsie_timestamp, ..
        } = correlated_timestamp_oopsies::Gated.build();
        assert!(__oopsie_timestamp >= before);
    }
    #[cfg(not(debug_assertions))]
    {
        let at = std::time::SystemTime::now();
        let CorrelatedTimestampError::Gated { at: stored } =
            correlated_timestamp_oopsies::Gated { at }.build();
        assert_eq!(stored, at);
    }
}

// ---- `traced(timestamp)` only conflicts with a timestamp field that can exist ----

#[oopsie(traced(timestamp))]
pub struct AbsentTimestampFieldError {
    info: String,
    #[cfg(any())]
    at: std::time::SystemTime,
}

#[oopsie(traced(timestamp))]
pub struct ContradictoryTimestampFieldError {
    info: String,
    #[cfg(all(debug_assertions, not(debug_assertions)))]
    at: std::time::SystemTime,
}

#[test]
fn impossible_timestamp_field_keeps_the_injected_timestamp() {
    let before = std::time::SystemTime::now();
    let err = AbsentTimestampFieldOopsie { info: "x" }.build();
    assert!(err.__oopsie_timestamp >= before);
    let err = ContradictoryTimestampFieldOopsie { info: "x" }.build();
    assert!(err.__oopsie_timestamp >= before);
}

// ---- injected auto-code must not resolve through a user `concat!`/`module_path!` shadow ----

#[test]
fn auto_code_ignores_shadowed_core_macros() {
    use oopsie::Diagnostic as _;

    macro_rules! concat {
        ($($t:tt)*) => {
            compile_error!("generated code must not call the shadowed local `concat!`")
        };
    }
    macro_rules! module_path {
        () => {
            compile_error!("generated code must not call the shadowed local `module_path!`")
        };
    }

    #[oopsie(traced)]
    #[oopsie(module(false))]
    enum ShadowedMacroError {
        #[oopsie("shadowed")]
        Boom { info: String },
    }

    let err = Boom {
        info: "x".to_owned(),
    }
    .build();
    let code = err
        .oopsie_error_code()
        .expect("auto-code should be present despite locally shadowed core macros");
    assert!(
        code.as_str().ends_with("::ShadowedMacroError::Boom"),
        "auto-code broken by shadowed macros: {}",
        code.as_str()
    );
}

/// A traced enum denying `missing_docs`, whose variant fields are public, so
/// an undocumented injected field would trip it.
pub mod missing_docs_pub_traced {
    #![deny(missing_docs)]

    use oopsie::oopsie;

    /// A publicly documented traced enum.
    #[oopsie(traced)]
    pub enum DocumentedEnumError {
        /// The connection failed.
        #[oopsie("conn failed: {addr}")]
        ConnFailed {
            /// The address that failed to connect.
            addr: String,
        },
    }
}

#[test]
fn injected_trace_fields_are_doc_hidden() {
    use missing_docs_pub_traced::{DocumentedEnumError, documented_enum_oopsies};

    let err = documented_enum_oopsies::ConnFailed { addr: "127.0.0.1" }.build();
    assert!(matches!(&err, DocumentedEnumError::ConnFailed { addr, .. } if addr == "127.0.0.1"));
}
