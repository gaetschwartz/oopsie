//! SpanTrace wrapper with `Capturable` support.
//!
//! This module provides a [`SpanTrace`] wrapper that integrates with oopsie's
//! implicit data generation, allowing automatic capture of tracing span context
//! in error types.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tracing_level::TracingLevel;

#[derive(Debug, Clone)]
pub struct SpanTrace {
    pub inner: SpanTraceInner,
}

#[derive(Debug, Clone)]
pub enum SpanTraceInner {
    Tracing(tracing_error::SpanTrace),
    Fallback(FallbackSpanTrace),
}

impl SpanTrace {
    /// Capture the current span trace.
    ///
    /// This captures the current tracing span context. For this to work,
    /// a subscriber with [`tracing_error::ErrorLayer`] must be installed.
    #[must_use]
    #[track_caller]
    pub fn capture() -> Self {
        Self {
            inner: SpanTraceInner::Tracing(tracing_error::SpanTrace::capture()),
        }
    }

    /// Create a new `SpanTrace` from an existing [`tracing_error::SpanTrace`].
    #[must_use]
    pub const fn new(inner: tracing_error::SpanTrace) -> Self {
        Self {
            inner: SpanTraceInner::Tracing(inner),
        }
    }

    /// Get the capture status of this span trace.
    ///
    /// Returns information about whether the span trace was successfully
    /// captured, is empty, or if the subscriber doesn't support span tracing.
    #[must_use]
    pub fn status(&self) -> tracing_error::SpanTraceStatus {
        match &self.inner {
            SpanTraceInner::Tracing(inner) => inner.status(),
            SpanTraceInner::Fallback(_) => tracing_error::SpanTraceStatus::CAPTURED,
        }
    }

    /// Consumes self and returns the inner [`tracing_error::SpanTrace`].
    #[must_use]
    pub fn into_span_trace(self) -> Option<tracing_error::SpanTrace> {
        match self.inner {
            SpanTraceInner::Tracing(inner) => Some(inner),
            SpanTraceInner::Fallback(_) => None,
        }
    }

    /// Extracts the span information from the error.
    #[must_use]
    #[inline]
    pub fn extract_from_error(err: &(impl crate::ErrorExt + ?Sized)) -> Option<&Self> {
        err.oopsie_spantrace()
    }

    pub fn with_spans<F>(&self, mut f: F)
    where
        F: FnMut(&ErasedMetadata, &str) -> bool,
    {
        match &self.inner {
            SpanTraceInner::Tracing(inner) => {
                inner.with_spans(|md, fields| {
                    let fallback_md: ErasedMetadata = md.into();
                    f(&fallback_md, fields)
                });
            }
            SpanTraceInner::Fallback(fallback) => {
                for FallbackSpan { metadata, fields } in &fallback.spans {
                    if !f(metadata, fields) {
                        break;
                    }
                }
            }
        }
    }
}

impl PartialEq for SpanTrace {
    fn eq(&self, other: &Self) -> bool {
        match (&self.inner, &other.inner) {
            (SpanTraceInner::Tracing(a), SpanTraceInner::Tracing(b)) => {
                // Comparing only the first (innermost) span is sufficient:
                // tracing_error::SpanTrace stores a single Span and with_spans
                // walks up the ancestor tree. Two traces with the same innermost
                // span have identical full traces.
                let mut eq = false;
                a.with_spans(|a_md, a_fields| {
                    b.with_spans(|b_md, b_fields| {
                        if a_md == b_md && a_fields == b_fields {
                            eq = true;
                        }
                        false
                    });
                    false
                });
                eq
            }
            (SpanTraceInner::Fallback(a), SpanTraceInner::Fallback(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for SpanTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            SpanTraceInner::Tracing(inner) => fmt::Display::fmt(inner, f),
            SpanTraceInner::Fallback(fallback) => fmt::Display::fmt(fallback, f),
        }
    }
}

impl crate::Capturable for SpanTrace {
    #[track_caller]
    fn capture() -> Self {
        Self::capture()
    }
}

impl crate::CaptureExt for SpanTrace {
    fn capture_or_extract(source: &dyn crate::ErrorExt) -> Self {
        source
            .oopsie_spantrace()
            .cloned()
            .unwrap_or_else(Self::capture)
    }
}

impl Serialize for SpanTrace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match &self.inner {
            SpanTraceInner::Tracing(span_trace) => {
                let mut spans = Vec::new();
                span_trace.with_spans(|metadata, fields| {
                    spans.push(FallbackSpan {
                        metadata: ErasedMetadata {
                            name: metadata.name().into(),
                            target: metadata.target().into(),
                            level: metadata.level().into(),
                            module_path: metadata.module_path().map(Box::from),
                            file: metadata.file().map(Box::from),
                            line: metadata.line(),
                        },
                        fields: fields.into(),
                    });
                    true // continue iterating
                });

                let serialized = FallbackSpanTrace { spans };
                serialized.serialize(serializer)
            }
            SpanTraceInner::Fallback(f) => f.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SpanTrace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let de = FallbackSpanTrace::deserialize(deserializer)?;
        Ok(Self {
            inner: SpanTraceInner::Fallback(de),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FallbackSpanTrace {
    spans: Vec<FallbackSpan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct FallbackSpan {
    metadata: ErasedMetadata,
    fields: Box<str>,
}

impl FallbackSpanTrace {
    pub fn with_spans<F>(&self, mut f: F)
    where
        F: FnMut(&ErasedMetadata, &str) -> bool,
    {
        for FallbackSpan { metadata, fields } in &self.spans {
            if !f(metadata, fields) {
                break;
            }
        }
    }
}

impl<'a> From<&'a tracing::Metadata<'_>> for ErasedMetadata {
    fn from(meta: &'a tracing::Metadata<'_>) -> Self {
        Self {
            name: meta.name().into(),
            target: meta.target().into(),
            level: meta.level().into(),
            module_path: meta.module_path().map(Box::from),
            file: meta.file().map(Box::from),
            line: meta.line(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErasedMetadata {
    name: Box<str>,
    target: Box<str>,
    level: TracingLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_path: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
}

impl ErasedMetadata {
    /// Returns the span name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the span target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Returns the span level.
    #[must_use]
    pub const fn level(&self) -> TracingLevel {
        self.level
    }

    /// Returns the module path, if available.
    #[must_use]
    pub fn module_path(&self) -> Option<&str> {
        self.module_path.as_deref()
    }

    /// Returns the source file path, if available.
    #[must_use]
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// Returns the line number, if available.
    #[must_use]
    pub const fn line(&self) -> Option<u32> {
        self.line
    }
}

impl fmt::Display for FallbackSpanTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut err = Ok(());
        let mut span = 0;

        macro_rules! try_bool {
            ($e:expr, $dest:ident) => {{
                let ret = $e.unwrap_or_else(|e| $dest = Err(e));

                if $dest.is_err() {
                    return false;
                }

                ret
            }};
        }

        self.with_spans(|metadata, fields| {
            if span > 0 {
                try_bool!(write!(f, "\n",), err);
            }

            try_bool!(
                write!(f, "{:>4}: {}::{}", span, metadata.target, metadata.name),
                err
            );

            if !fields.is_empty() {
                try_bool!(write!(f, "\n           with {}", fields), err);
            }

            if let Some((file, line)) = metadata
                .file
                .as_deref()
                .and_then(|file| metadata.line.map(|line| (file, line)))
            {
                try_bool!(write!(f, "\n             at {}:{}", file, line), err);
            }

            span += 1;
            true
        });

        err
    }
}

/// A wrapper around `Option<SpanTrace>` that implements [`Capturable`].
///
/// This type only captures a span trace if the capture was successful
/// (i.e., there was an active span and the subscriber supports it).
/// Use this when you want to optionally include span traces.
#[derive(Clone, Debug, Default)]
pub struct OptionalSpanTrace(Option<SpanTrace>);

impl OptionalSpanTrace {
    /// Create an empty `OptionalSpanTrace`.
    #[must_use]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Create an `OptionalSpanTrace` from an existing `SpanTrace`.
    #[must_use]
    pub const fn some(trace: SpanTrace) -> Self {
        Self(Some(trace))
    }

    /// Returns the inner `Option<SpanTrace>`.
    #[must_use]
    pub fn into_inner(self) -> Option<SpanTrace> {
        self.0
    }

    /// Returns a reference to the inner `SpanTrace` if present.
    #[must_use]
    pub const fn as_ref(&self) -> Option<&SpanTrace> {
        self.0.as_ref()
    }

    /// Returns true if this contains a span trace.
    #[must_use]
    pub const fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Returns true if this does not contain a span trace.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Display for OptionalSpanTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(trace) => fmt::Display::fmt(trace, f),
            None => Ok(()),
        }
    }
}

impl crate::Capturable for OptionalSpanTrace {
    #[track_caller]
    fn capture() -> Self {
        let trace = SpanTrace::capture();
        match trace.status() {
            tracing_error::SpanTraceStatus::CAPTURED => Self(Some(trace)),
            _ => Self(None),
        }
    }
}

impl From<SpanTrace> for OptionalSpanTrace {
    fn from(trace: SpanTrace) -> Self {
        Self(Some(trace))
    }
}

impl From<Option<SpanTrace>> for OptionalSpanTrace {
    fn from(opt: Option<SpanTrace>) -> Self {
        Self(opt)
    }
}

#[cfg(test)]
mod tests {
    use crate::Capturable;

    use super::*;

    #[test]
    fn test_span_trace_capture() {
        // Without ErrorLayer, status will be UNSUPPORTED, but capture shouldn't panic
        let trace = SpanTrace::capture();
        let _ = trace.status();
    }

    #[test]
    fn test_generate_implicit_data() {
        let trace: SpanTrace = Capturable::capture();
        let _ = trace.status();
    }

    #[test]
    fn test_optional_span_trace() {
        let trace: OptionalSpanTrace = Capturable::capture();
        // May or may not be Some depending on subscriber
        let _ = trace;
    }

    #[test]
    fn test_fallback_spantrace_deserialization() {
        let json = serde_json::json!({
            "spans": [
                {
                    "metadata": {
                        "name": "inner_span",
                        "target": "my_crate::module",
                        "level": "INFO",
                        "module_path": "my_crate::module",
                        "file": "src/lib.rs",
                        "line": 42
                    },
                    "fields": "request_id=123"
                },
                {
                    "metadata": {
                        "name": "outer_span",
                        "target": "my_crate",
                        "level": "DEBUG"
                    },
                    "fields": ""
                }
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        assert!(matches!(spantrace.inner, SpanTraceInner::Fallback(_)));
    }

    #[test]
    fn test_fallback_spantrace_serialization_roundtrip() {
        let json = serde_json::json!({
            "spans": [
                {
                    "metadata": {
                        "name": "test_span",
                        "target": "test_target",
                        "level": "WARN",
                        "file": "test.rs",
                        "line": 100
                    },
                    "fields": "key=value"
                }
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        let serialized = serde_json::to_string(&spantrace).unwrap();
        let roundtrip: SpanTrace = serde_json::from_str(&serialized).unwrap();

        // Verify roundtrip by serializing again
        let serialized2 = serde_json::to_string(&roundtrip).unwrap();
        assert_eq!(serialized, serialized2);
    }

    #[test]
    fn test_fallback_spantrace_display() {
        let json = serde_json::json!({
            "spans": [
                {
                    "metadata": {
                        "name": "inner_function",
                        "target": "my_module",
                        "level": "INFO",
                        "file": "src/lib.rs",
                        "line": 42
                    },
                    "fields": ""
                },
                {
                    "metadata": {
                        "name": "outer_function",
                        "target": "my_crate",
                        "level": "DEBUG"
                    },
                    "fields": ""
                }
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();

        insta::assert_snapshot!(display, @r"
           0: my_module::inner_function
                     at src/lib.rs:42
           1: my_crate::outer_function
        ");
    }

    #[test]
    fn test_fallback_spantrace_display_with_fields() {
        let json = serde_json::json!({
            "spans": [
                {
                    "metadata": {
                        "name": "process_request",
                        "target": "server",
                        "level": "INFO",
                        "file": "src/server.rs",
                        "line": 100
                    },
                    "fields": "user_id=42, method=GET"
                }
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();

        insta::assert_snapshot!(display, @"
        0: server::process_request
                with user_id=42, method=GET
                  at src/server.rs:100
        ");
    }

    #[test]
    fn test_fallback_spantrace_with_spans_iteration() {
        let json = serde_json::json!({
            "spans": [
                {"metadata": {"name": "span_a", "target": "target_a", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "span_b", "target": "target_b", "level": "DEBUG"}, "fields": ""},
                {"metadata": {"name": "span_c", "target": "target_c", "level": "TRACE"}, "fields": ""}
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();

        let mut collected = Vec::new();
        spantrace.with_spans(|metadata, _fields| {
            collected.push(format!("{}::{}", metadata.target(), metadata.name()));
            true // continue
        });

        assert_eq!(
            collected,
            vec!["target_a::span_a", "target_b::span_b", "target_c::span_c"]
        );
    }

    #[test]
    fn test_fallback_spantrace_with_spans_early_exit() {
        let json = serde_json::json!({
            "spans": [
                {"metadata": {"name": "a", "target": "t", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "b", "target": "t", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "c", "target": "t", "level": "INFO"}, "fields": ""}
            ]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();

        let mut count = 0;
        spantrace.with_spans(|_metadata, _fields| {
            count += 1;
            count < 2 // stop after second span
        });

        assert_eq!(count, 2);
    }

    // ── ErasedMetadata getters ──────────────────────────────────────────

    #[test]
    fn test_erased_metadata_getters_with_optional_fields() {
        let json = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "my_span",
                    "target": "my_crate::module",
                    "level": "INFO",
                    "module_path": "my_crate::module",
                    "file": "src/lib.rs",
                    "line": 42
                },
                "fields": ""
            }]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        spantrace.with_spans(|metadata, _fields| {
            assert_eq!(metadata.module_path(), Some("my_crate::module"));
            assert_eq!(metadata.file(), Some("src/lib.rs"));
            assert_eq!(metadata.line(), Some(42));
            false
        });
    }

    #[test]
    fn test_erased_metadata_getters_without_optional_fields() {
        let json = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "bare_span",
                    "target": "some_target",
                    "level": "DEBUG"
                },
                "fields": ""
            }]
        });

        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        spantrace.with_spans(|metadata, _fields| {
            assert_eq!(metadata.module_path(), None);
            assert_eq!(metadata.file(), None);
            assert_eq!(metadata.line(), None);
            false
        });
    }

    // ── OptionalSpanTrace ───────────────────────────────────────────────

    #[test]
    fn test_optional_span_trace_some_into_inner() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.into_inner().is_some());
    }

    #[test]
    fn test_optional_span_trace_none_into_inner() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.into_inner().is_none());
    }

    #[test]
    fn test_optional_span_trace_as_ref_some() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.as_ref().is_some());
    }

    #[test]
    fn test_optional_span_trace_as_ref_none() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.as_ref().is_none());
    }

    #[test]
    fn test_optional_span_trace_is_some() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let opt = OptionalSpanTrace::some(trace);
        assert!(opt.is_some());
        assert!(!opt.is_none());
    }

    #[test]
    fn test_optional_span_trace_is_none() {
        let opt = OptionalSpanTrace::none();
        assert!(opt.is_none());
        assert!(!opt.is_some());
    }

    #[test]
    fn test_optional_span_trace_display_some() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "test_span",
                    "target": "test_target",
                    "level": "INFO"
                },
                "fields": ""
            }]
        }))
        .unwrap();
        let opt = OptionalSpanTrace::some(trace);
        let display = opt.to_string();
        assert!(!display.is_empty());
    }

    #[test]
    fn test_optional_span_trace_display_none() {
        let opt = OptionalSpanTrace::none();
        let display = opt.to_string();
        assert!(display.is_empty());
    }

    #[test]
    fn test_optional_span_trace_from_spantrace() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let opt: OptionalSpanTrace = trace.into();
        assert!(opt.is_some());
    }

    #[test]
    fn test_optional_span_trace_from_option_none() {
        let opt: OptionalSpanTrace = None::<SpanTrace>.into();
        assert!(opt.is_none());
    }

    #[test]
    fn test_optional_span_trace_from_option_some() {
        let trace: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let opt: OptionalSpanTrace = Some(trace).into();
        assert!(opt.is_some());
    }

    #[test]
    fn test_optional_span_trace_generate_without_subscriber() {
        // Without a subscriber with ErrorLayer, generate() should return None
        let opt: OptionalSpanTrace = Capturable::capture();
        assert!(opt.is_none());
    }

    // ── PartialEq ───────────────────────────────────────────────────────

    #[test]
    fn test_partial_eq_identical_fallback() {
        let json = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "span",
                    "target": "target",
                    "level": "INFO"
                },
                "fields": "key=val"
            }]
        });
        let a: SpanTrace = serde_json::from_value(json.clone()).unwrap();
        let b: SpanTrace = serde_json::from_value(json).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_partial_eq_different_fallback() {
        let json_a = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "span",
                    "target": "target_a",
                    "level": "INFO"
                },
                "fields": ""
            }]
        });
        let json_b = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "span",
                    "target": "target_b",
                    "level": "INFO"
                },
                "fields": ""
            }]
        });
        let a: SpanTrace = serde_json::from_value(json_a).unwrap();
        let b: SpanTrace = serde_json::from_value(json_b).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_partial_eq_fallback_vs_tracing() {
        let fallback: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        let tracing = SpanTrace::capture();
        assert_ne!(fallback, tracing);
    }

    // ── into_span_trace ─────────────────────────────────────────────────

    #[test]
    fn test_into_span_trace_fallback_returns_none() {
        let fallback: SpanTrace = serde_json::from_value(serde_json::json!({
            "spans": []
        }))
        .unwrap();
        assert!(fallback.into_span_trace().is_none());
    }

    #[test]
    fn test_into_span_trace_tracing_returns_some() {
        let tracing = SpanTrace::capture();
        assert!(tracing.into_span_trace().is_some());
    }

    // ── FallbackSpantrace Display edge case ─────────────────────────────

    #[test]
    fn test_fallback_display_span_zero_has_no_leading_newline() {
        let json = serde_json::json!({
            "spans": [{
                "metadata": {
                    "name": "only_span",
                    "target": "t",
                    "level": "INFO"
                },
                "fields": ""
            }]
        });
        let spantrace: SpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();
        // Span 0 must NOT start with a newline
        assert!(!display.starts_with('\n'));
        assert!(display.starts_with("   0:"));
    }
}
