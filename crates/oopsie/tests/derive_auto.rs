#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all)]

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
    assert!(matches!(err, AutoExcludedError::Missing { ref name, .. } if name == "widget"));
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

#[test]
fn multiple_auto_fields() {
    let err = Multi { label: "test" }.build();
    assert!(matches!(err, MultiAutoError::Multi { ref label, .. } if label == "test"));
}

// ---- Test 5: OptionalSpanTrace capture field with a Diagnostic source ----
//
// Regression: when the source implements `Diagnostic`, the generated capture
// path resolves through `CaptureExt`, which `OptionalSpanTrace` must implement.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum InnerDiagError {
    #[oopsie("inner")]
    Inner,
}

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

#[test]
fn optional_span_trace_capture_with_diagnostic_source() {
    let err: OptionalCaptureError = Wrap.build_error(InnerDiagError::Inner);
    assert!(matches!(err, OptionalCaptureError::Wrap { .. }));
}
