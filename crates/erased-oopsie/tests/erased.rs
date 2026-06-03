#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use erased_oopsie::{
    ErasedBacktrace, ErasedError, ErasedMetadata, ErasedSpan, ErasedSpanTrace, TracingLevel,
};
use oopsie::oopsie;

#[oopsie(traced)]
#[oopsie("Something went wrong: {message}")]
#[oopsie(help = "Try restarting the service")]
pub struct ErrorWithHelp {
    message: String,
}

#[oopsie(traced)]
#[oopsie("Code-only error: {message}")]
pub struct ErrorWithCodeOnly {
    message: String,
}

#[test]
fn test_erased_error_display() {
    let error = ErasedError::from_error(common::make_error());
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("erased_error_display"), error);
    });
}

#[test]
fn test_erased_error_json() {
    let error = ErasedError::from_error(common::make_error());
    redact!(json, {
        insta::assert_json_snapshot!(snap_name!("erased_error_json"), error);
    });
}

#[test]
fn test_help_extraction() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let erased = ErasedError::from_error(error);

    assert_eq!(
        erased.diagnostics.help(),
        Some("Try restarting the service")
    );
    assert!(erased.diagnostics.code().is_some());
}

#[test]
fn test_code_only_extraction() {
    let error = ErrorWithCodeOnlyOopsie { message: "timeout" }.build();
    let erased = ErasedError::from_error(error);

    assert!(erased.diagnostics.code().is_some());
    assert_eq!(erased.diagnostics.help(), None);
}

#[test]
fn test_format_short_includes_help() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let short = erased.format_short();

    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("format_short_with_help"), short);
    });
}

#[test]
fn test_extract_backtrace_returns_some_when_provided() {
    let error = common::make_error();
    let bt = oopsie::Diagnostic::oopsie_backtrace(&error);
    assert!(
        bt.is_some(),
        "extract_backtrace should return Some for oopsie errors"
    );
}

#[test]
fn test_extract_error_code_returns_some_for_oopsie_errors() {
    let error = ErrorWithHelpOopsie { message: "test" }.build();
    let erased = ErasedError::from_error(error);
    assert!(
        erased.diagnostics.code().is_some(),
        "oopsie errors with help should have an error code"
    );
}

// --- Diagnostics::is_none() ---

#[test]
fn test_diagnostics_is_none_for_default() {
    let diag = erased_oopsie::Diagnostics::default();
    assert!(
        diag.is_none(),
        "default Diagnostics should have no code or help"
    );
}

#[test]
fn test_diagnostics_is_not_none_when_code_present() {
    let error = ErrorWithCodeOnlyOopsie {
        message: "has code",
    }
    .build();
    let erased = ErasedError::from_error(error);
    assert!(
        !erased.diagnostics.is_none(),
        "Diagnostics with a code should not be is_none()"
    );
}

// --- write_text ---

#[test]
fn test_write_text_contains_message_and_help() {
    let error = ErrorWithHelpOopsie {
        message: "text output test",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let mut buf = Vec::new();
    erased.write_text(&mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(
        text.contains("text output test"),
        "write_text should include the error message"
    );
    assert!(
        text.contains("Try restarting the service"),
        "write_text should include help text"
    );
}

// --- write_json ---

#[test]
fn test_write_json_output_is_valid_json_with_expected_fields() {
    let error = ErrorWithHelpOopsie {
        message: "json output test",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let mut buf = Vec::new();
    erased.write_json(&mut buf).unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&buf).expect("write_json should produce valid JSON");
    assert_eq!(
        json["message"], "Something went wrong: json output test",
        "JSON message field should match Display"
    );
    assert_eq!(
        json["diagnostics"]["help"], "Try restarting the service",
        "JSON diagnostics.help should match oopsie help attribute"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// GAP 46: ErasedFrame Display — name=Some, filename=None
//
// `ErasedBacktrace::frames` is private with no public frame-taking constructor,
// but the type derives `Deserialize`, so an integration test reconstructs it
// from JSON to exercise the `name = Some, filename = None` Display branch
// (a name line, with no trailing `at` location line).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_erased_frame_display_name_without_location() {
    let json = serde_json::json!({
        "frames": [
            { "name": "my::func", "filename": null, "line": null, "column": null }
        ]
    });
    let bt: ErasedBacktrace = serde_json::from_value(json).unwrap();
    assert_eq!(bt.to_string(), "  1: my::func\n");
}

// ─────────────────────────────────────────────────────────────────────────────
// GAP 47: Clone — explicit clone() assertions on the erased types.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_erased_error_clone() {
    let original = ErasedError::from_error(common::make_error());
    let cloned = original.clone();
    assert_eq!(original.message, cloned.message);
    assert_eq!(original.source_chain, cloned.source_chain);
    assert_eq!(
        original.diagnostics.code(),
        cloned.diagnostics.code(),
        "cloned diagnostics code must match"
    );
    assert_eq!(
        original.diagnostics.help(),
        cloned.diagnostics.help(),
        "cloned diagnostics help must match"
    );
}

#[test]
fn test_erased_backtrace_clone() {
    let json = serde_json::json!({
        "frames": [
            { "name": "frame::one", "filename": "src/lib.rs", "line": 10, "column": 2 }
        ]
    });
    let original: ErasedBacktrace = serde_json::from_value(json).unwrap();
    let cloned = original.clone();
    assert_eq!(original.to_string(), cloned.to_string());
    assert_eq!(original.frames().len(), cloned.frames().len());
}

#[test]
fn test_erased_spantrace_and_span_and_metadata_clone() {
    let json = serde_json::json!({
        "spans": [{
            "metadata": {
                "name": "span", "target": "tgt", "level": "INFO",
                "file": "src/lib.rs", "line": 7
            },
            "fields": "k=v"
        }]
    });
    let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
    let cloned_spantrace = spantrace.clone();
    assert_eq!(spantrace, cloned_spantrace);

    let span: ErasedSpan = serde_json::from_value(serde_json::json!({
        "metadata": { "name": "s", "target": "t", "level": "WARN" },
        "fields": ""
    }))
    .unwrap();
    let cloned_span = span.clone();
    assert_eq!(span, cloned_span);

    let metadata = cloned_span.metadata;
    assert_eq!(span.metadata, metadata);
}

// ─────────────────────────────────────────────────────────────────────────────
// GAP 48: PartialEq/Eq — direct equality assertions on the erased span types.
// (Only the spantrace family derives PartialEq/Eq; ErasedError/ErasedBacktrace
// do not, so they are excluded.)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_erased_spantrace_partial_eq() {
    let json = serde_json::json!({
        "spans": [{
            "metadata": { "name": "n", "target": "t", "level": "INFO" },
            "fields": "a=1"
        }]
    });
    let left: ErasedSpanTrace = serde_json::from_value(json.clone()).unwrap();
    let right: ErasedSpanTrace = serde_json::from_value(json).unwrap();
    assert_eq!(left, right, "identical spantraces must be equal");

    let other: ErasedSpanTrace = serde_json::from_value(serde_json::json!({
        "spans": [{
            "metadata": { "name": "different", "target": "t", "level": "INFO" },
            "fields": "a=1"
        }]
    }))
    .unwrap();
    assert_ne!(
        left, other,
        "spantraces differing in span name must be unequal"
    );
}

#[test]
fn test_erased_span_and_metadata_partial_eq() {
    let make = |fields: &str| -> ErasedSpan {
        serde_json::from_value(serde_json::json!({
            "metadata": { "name": "n", "target": "t", "level": "DEBUG" },
            "fields": fields
        }))
        .unwrap()
    };
    let a = make("x=1");
    let b = make("x=1");
    let c = make("x=2");
    assert_eq!(a, b, "identical spans must be equal");
    assert_ne!(a, c, "spans differing in fields must be unequal");

    let meta_a: ErasedMetadata = serde_json::from_value(serde_json::json!({
        "name": "m", "target": "t", "level": "ERROR"
    }))
    .unwrap();
    let meta_b: ErasedMetadata = serde_json::from_value(serde_json::json!({
        "name": "m", "target": "t", "level": "ERROR"
    }))
    .unwrap();
    let meta_c: ErasedMetadata = serde_json::from_value(serde_json::json!({
        "name": "m", "target": "t", "level": "WARN"
    }))
    .unwrap();
    assert_eq!(meta_a, meta_b, "identical metadata must be equal");
    assert_ne!(
        meta_a, meta_c,
        "metadata differing in level must be unequal"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// GAP 55: ErasedSpanTrace Display — empty spans vector renders the empty string.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_erased_spantrace_display_empty_spans() {
    let spantrace: ErasedSpanTrace =
        serde_json::from_value(serde_json::json!({ "spans": [] })).unwrap();
    assert_eq!(spantrace.to_string(), "");
}

// ─────────────────────────────────────────────────────────────────────────────
// GAP 56: TracingLevel <-> tracing::Level — bidirectional mapping for all 5 levels.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tracing_level_bidirectional_conversion() {
    let cases = [
        (TracingLevel::TRACE, tracing::Level::TRACE),
        (TracingLevel::DEBUG, tracing::Level::DEBUG),
        (TracingLevel::INFO, tracing::Level::INFO),
        (TracingLevel::WARN, tracing::Level::WARN),
        (TracingLevel::ERROR, tracing::Level::ERROR),
    ];
    for (erased, tracing_level) in cases {
        let forward: tracing::Level = erased.into();
        assert_eq!(forward, tracing_level, "TracingLevel -> tracing::Level");

        let back = TracingLevel::from(&tracing_level);
        assert_eq!(back, erased, "&tracing::Level -> TracingLevel");

        let roundtrip = TracingLevel::from(&tracing::Level::from(erased));
        assert_eq!(roundtrip, erased, "TracingLevel roundtrip must be identity");
    }
}
