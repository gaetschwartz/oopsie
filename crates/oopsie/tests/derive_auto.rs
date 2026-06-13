#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

use oopsie::{Contextual as _, Oopsie};
use std::io;

// ---- Test 1: auto field excluded from selector ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoExcludedError {
    #[oopsie("missing item: {name}")]
    Missing {
        name: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn auto_excluded_from_selector() {
    // The selector `Missing` should only have the `name` field, not `bt`.
    let err = Missing { name: "widget" }.build();
    assert!(matches!(err, AutoExcludedError::Missing { name, .. } if name == "widget"));
}

// ---- Test 2: auto backtrace is generated without panic ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoBtError {
    #[oopsie("something broke")]
    Broke {
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn auto_backtrace_generated() {
    // Should not panic — backtrace is auto-generated via Capturable.
    let err = Broke.build();
    assert!(matches!(err, AutoBtError::Broke { .. }));
}

// ---- Test 3: source variant + auto backtrace ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoWithSourceError {
    #[oopsie("io problem")]
    IoProblem {
        source: io::Error,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
    },
}

#[test]
fn auto_with_source() {
    let io_err = io::Error::new(io::ErrorKind::Other, "disk full");
    let err: AutoWithSourceError = IoProblem.build_error(io_err);
    assert!(matches!(err, AutoWithSourceError::IoProblem { .. }));
}

// ---- Test 4: multiple auto fields (backtrace + spantrace) ----

#[cfg(feature = "tracing")]
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MultiAutoError {
    #[oopsie("multi auto")]
    Multi {
        label: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
        #[oopsie(capture)]
        st: Box<oopsie::SpanTrace>,
    },
}

#[cfg(feature = "tracing")]
#[test]
fn multiple_auto_fields() {
    let err = Multi { label: "test" }.build();
    assert!(matches!(err, MultiAutoError::Multi { label, .. } if label == "test"));
}

// ---- Test 5: OptionalSpanTrace capture field with a Diagnostic source ----
//
// Regression: when the source implements `Diagnostic`, the generated capture
// path resolves through `Capturable`, which `OptionalSpanTrace` must implement.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum InnerDiagError {
    #[oopsie("inner")]
    Inner,
}

#[cfg(feature = "tracing")]
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum OptionalCaptureError {
    #[oopsie("wraps a diagnostic source")]
    Wrap {
        source: InnerDiagError,
        #[oopsie(capture)]
        st: oopsie::OptionalSpanTrace,
    },
}

#[cfg(feature = "tracing")]
#[test]
fn optional_span_trace_capture_with_diagnostic_source() {
    let err: OptionalCaptureError = Wrap.build_error(InnerDiagError::Inner);
    assert!(matches!(err, OptionalCaptureError::Wrap { .. }));
}

// ════════════════════════════════════════════════════════════════════════
// Multiple heterogeneous #[oopsie(capture)] fields
//
// `gen_auto_inits` iterates over ALL auto fields and calls `.capture()` on each
// with no type constraint, so any mix of `Capturable` types auto-initializes.
// These tests exercise (1) three heterogeneous capture fields in one variant,
// (2) a tuple `Capturable` field, and (3) a user-defined `Capturable` type.
// ════════════════════════════════════════════════════════════════════════

// (1) three capture fields of three different Capturable types in one variant.
#[cfg(feature = "tracing")]
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TripleCaptureError {
    #[oopsie("triple")]
    Triple {
        label: String,
        #[oopsie(capture)]
        bt: Box<oopsie::Backtrace>,
        #[oopsie(capture)]
        st: Box<oopsie::SpanTrace>,
        #[oopsie(capture)]
        ost: oopsie::OptionalSpanTrace,
    },
}

#[cfg(feature = "tracing")]
#[test]
fn three_heterogeneous_capture_fields_all_initialize() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    // Only `label` is on the selector; the three capture fields are auto-filled.
    let err = Triple { label: "x" }.build();
    let TripleCaptureError::Triple { bt, st, ost, .. } = &err;
    // Each captured field is concretely populated (content, not just presence):
    assert!(!bt.frames().is_empty(), "backtrace must capture frames");
    // SpanTrace and OptionalSpanTrace captured without a subscriber: they exist
    // and render (possibly empty) without panicking.
    let _ = st.status();
    let _ = ost.to_string();
}

// (2) a single capture field whose type is a `(A, B)` tuple Capturable.
#[cfg(feature = "tracing")]
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum TupleCaptureError {
    #[oopsie("tuple")]
    Tuple {
        #[oopsie(capture)]
        traces: (oopsie::Backtrace, oopsie::SpanTrace),
    },
}

#[cfg(feature = "tracing")]
#[test]
fn tuple_capture_field_initializes_both_elements() {
    oopsie::set_rust_backtrace_override(oopsie::RustBacktrace::Enabled);
    let err = Tuple.build();
    let TupleCaptureError::Tuple { traces } = &err;
    // The tuple `Capturable` impl captured both halves.
    assert!(
        !traces.0.frames().is_empty(),
        "tuple backtrace half must capture frames"
    );
    let _ = traces.1.status();
}

// ---- Test: empty (never) enum derives ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NeverError {}

#[test]
fn empty_enum_derives() {
    fn assert_error<E: std::error::Error>() {}
    assert_error::<NeverError>();
}

// (3) a user-defined Capturable type used as a capture field.
#[derive(Debug, Default)]
struct Marker {
    captured: bool,
}

impl oopsie::Capturable for Marker {
    fn capture() -> Self {
        Self { captured: true }
    }
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum CustomCaptureError {
    #[oopsie("custom capture")]
    Custom {
        info: String,
        #[oopsie(capture)]
        marker: Marker,
    },
}

#[test]
fn user_defined_capturable_field_is_captured() {
    let err = Custom { info: "x" }.build();
    let CustomCaptureError::Custom { marker, .. } = &err;
    // Proves `Marker::capture()` (not `Default::default()`) ran for the field.
    assert!(
        marker.captured,
        "custom Capturable::capture must populate the field"
    );
}

// A `cfg_attr`-gated `Debug` derive is recognized by `fix_derives`, so the attr
// macro does not also inject `Debug` (which would conflict when the cfg is on).
#[test]
fn cfg_attr_gated_debug_is_not_duplicated() {
    #[oopsie::oopsie]
    #[cfg_attr(test, derive(Debug))]
    #[oopsie(module(false))]
    enum CfgDebugError {
        #[oopsie("x")]
        X,
    }
    let _ = format!("{:?}", X.build());
}
