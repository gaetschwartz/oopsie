#![cfg(feature = "serde")]
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

#[cfg(feature = "tracing")]
use oopsie::erased::TracingLevel;
use oopsie::erased::{ErasedBacktrace, ErasedError, ErasedMetadata, ErasedSpan, ErasedSpanTrace};
use oopsie::oopsie;
use oopsie_core::{redact, snap_name};

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

#[cfg(feature = "tracing")]
#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_erased_error_text() {
    let error = ErasedError::from_error(common::make_error());
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name!("erased_error_text"), error.to_text());
    });
}

#[cfg(feature = "tracing")]
#[test]
#[test_with::env(OOPSIE_BACKTRACE_SNAPSHOT_TESTS)]
fn test_erased_error_json() {
    let error = ErasedError::from_error(common::make_error());
    redact!(backtrace, {
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

#[cfg(feature = "tracing")]
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
    let diag = oopsie::erased::Diagnostics::default();
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
// ErasedFrame Display — name=Some, filename=None
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
// Clone — explicit clone() assertions on the erased types.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tracing")]
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
// PartialEq/Eq — direct equality assertions on the erased span types.
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
// ErasedSpanTrace Display — empty spans vector renders the empty string.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_erased_spantrace_display_empty_spans() {
    let spantrace: ErasedSpanTrace =
        serde_json::from_value(serde_json::json!({ "spans": [] })).unwrap();
    assert_eq!(spantrace.to_string(), "");
}

// ─────────────────────────────────────────────────────────────────────────────
// TracingLevel <-> tracing::Level — bidirectional mapping for all 5 levels.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tracing")]
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

// ─────────────────────────────────────────────────────────────────────────────
// Round trip: oopsie error → ErasedError → JSON → ErasedError → Report.
// ErasedError implements Diagnostic, so a deserialized error can be rendered
// through `Report` on the receiving side with code and help intact.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "fancy")]
#[test]
fn test_round_trip_through_json_renders_in_report() {
    let error = ErrorWithHelpOopsie {
        message: "connection refused",
    }
    .build();
    let erased = ErasedError::from_error(error);
    let code = erased
        .diagnostics
        .code()
        .expect("oopsie errors carry a code")
        .to_owned();

    let json = serde_json::to_string(&erased).expect("ErasedError serializes");
    let roundtripped: ErasedError = serde_json::from_str(&json).expect("ErasedError deserializes");

    let report = oopsie::Report::new(roundtripped);
    let rendered = report.to_string();

    assert!(
        rendered.contains(&format!("[{code}]")),
        "Report must render the transported error code, got:\n{rendered}"
    );
    assert!(
        rendered.contains("Something went wrong: connection refused"),
        "Report must render the transported message, got:\n{rendered}"
    );
    assert!(
        rendered.contains("Try restarting the service"),
        "Report must render the transported help text, got:\n{rendered}"
    );
}

#[test]
fn test_unknown_span_level_does_not_reject_payload() {
    let erased: ErasedError = serde_json::from_str(
        r#"{"message":"m","spantrace":{"spans":[{"metadata":{"name":"s","target":"t","level":"FATAL"},"fields":""}]},"backtrace":null}"#,
    )
    .expect("one unknown span level must not drop the whole error");
    assert!(erased.spantrace.is_some());
}

#[cfg(feature = "tracing")]
#[test]
fn test_round_trip_source_chain_survives_report_and_reerasure() {
    let erased = ErasedError::from_error(common::make_error());
    assert!(!erased.source_chain.is_empty(), "fixture must have a cause");

    let json = serde_json::to_string(&erased).unwrap();
    let roundtripped: ErasedError = serde_json::from_str(&json).unwrap();

    // Wire format unchanged by the cache field.
    assert_eq!(serde_json::to_string(&roundtripped).unwrap(), json);

    // Re-erasure reproduces the transported chain.
    let reerased = ErasedError::from_error_ref(&roundtripped);
    assert_eq!(reerased.source_chain, roundtripped.source_chain);

    // Report's source walk renders the cause line.
    #[cfg(feature = "fancy")]
    {
        let rendered = oopsie::Report::new(roundtripped).to_string();
        assert!(
            rendered.contains(&format!("╰─▶ {}", erased.source_chain[0])),
            "Report must render the transported cause, got:\n{rendered}"
        );
    }
}

// Display is intentionally just the message: chain renderers print each
// source on a single `├─▶` line, so a multi-line Display would corrupt the
// embedding error's report.
#[test]
fn test_display_stays_single_line_after_round_trip() {
    let error = ErrorWithHelpOopsie { message: "boom" }.build();
    let erased = ErasedError::from_error(error);
    let displayed = erased.to_string();
    assert_eq!(displayed, "Something went wrong: boom");
}

// Regression: `from_error_ref` eagerly walks `Error::source()`
// with `successors(...).collect()`. A user `source()` that returns itself (or an
// ancestor) is a cycle the std `Error` contract does not forbid, so the walk must
// be bounded — otherwise this serialization-facing API hangs / OOMs on a single
// ill-behaved foreign error.
#[test]
fn from_error_ref_terminates_on_cyclic_source() {
    use oopsie::Diagnostic;
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    struct Cyclic;
    impl fmt::Display for Cyclic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("cyclic")
        }
    }
    impl Error for Cyclic {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(self)
        }
    }
    impl Diagnostic for Cyclic {}

    let erased = ErasedError::from_error_ref(&Cyclic);
    assert!(
        erased.source_chain.len() <= 256,
        "source chain must be bounded on a cyclic source(), got {}",
        erased.source_chain.len()
    );
}
