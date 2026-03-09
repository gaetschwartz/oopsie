#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(unused, clippy::all)]

use oopsie::Oopsie;

// Test 1: Default module name = snake_case of enum name.
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
    // Selectors live in `my_app_error::` module.
    let err = my_app_error::NotFound { name: "widget" }.build();
    assert!(matches!(err, MyAppError::NotFound { .. }));
    assert_eq!(err.to_string(), "not found: widget");

    let err = my_app_error::TimedOut.build();
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
