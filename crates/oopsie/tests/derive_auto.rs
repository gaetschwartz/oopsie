#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::{IntoError as _, Oopsie};
use std::io;

// ---- Test 1: auto field excluded from selector ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum AutoExcludedError {
    #[oopsie("missing item: {name}")]
    Missing {
        name: String,
        #[oopsie(auto)]
        bt: Box<oopsie::BackTrace>,
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
        #[oopsie(auto)]
        bt: Box<oopsie::BackTrace>,
    },
}

#[test]
fn auto_backtrace_generated() {
    // Should not panic — backtrace is auto-generated via GenerateImplicitData.
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
        #[oopsie(auto)]
        bt: Box<oopsie::BackTrace>,
    },
}

#[test]
fn auto_with_source() {
    let io_err = io::Error::new(io::ErrorKind::Other, "disk full");
    let err: AutoWithSourceError = IoProblem.into_error(io_err);
    assert!(matches!(err, AutoWithSourceError::IoProblem { .. }));
}

// ---- Test 4: multiple auto fields (backtrace + spantrace) ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum MultiAutoError {
    #[oopsie("multi auto")]
    Multi {
        label: String,
        #[oopsie(auto)]
        bt: Box<oopsie::BackTrace>,
        #[oopsie(auto)]
        st: Box<oopsie::SpanTrace>,
    },
}

#[test]
fn multiple_auto_fields() {
    let err = Multi { label: "test" }.build();
    assert!(matches!(err, MultiAutoError::Multi { ref label, .. } if label == "test"));
}
