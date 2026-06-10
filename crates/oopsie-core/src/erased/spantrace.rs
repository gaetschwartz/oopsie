//! Serializable span trace representation.

use std::fmt;

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// TracingLevel
// ─────────────────────────────────────────────────────────────────────────────

/// Unrecognized or unparseable level strings deserialize as
/// [`UNKNOWN`](Self::UNKNOWN) instead of rejecting the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, Serialize)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE", ascii_case_insensitive)]
#[repr(u8)]
pub enum TracingLevel {
    TRACE = 0,
    DEBUG = 1,
    INFO = 2,
    WARN = 3,
    ERROR = 4,
    UNKNOWN = 5,
}

impl<'de> Deserialize<'de> for TracingLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // A level is cosmetic metadata; a transport for error reports must
        // degrade on a bad value rather than drop the whole error.
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(s.parse().unwrap_or(Self::UNKNOWN))
    }
}

#[cfg(feature = "tracing")]
impl From<TracingLevel> for tracing::Level {
    #[inline]
    fn from(level: TracingLevel) -> Self {
        match level {
            TracingLevel::ERROR | TracingLevel::UNKNOWN => Self::ERROR,
            TracingLevel::WARN => Self::WARN,
            TracingLevel::INFO => Self::INFO,
            TracingLevel::DEBUG => Self::DEBUG,
            TracingLevel::TRACE => Self::TRACE,
        }
    }
}

#[cfg(feature = "tracing")]
impl From<&tracing::Level> for TracingLevel {
    #[inline]
    fn from(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::ERROR => Self::ERROR,
            tracing::Level::WARN => Self::WARN,
            tracing::Level::INFO => Self::INFO,
            tracing::Level::DEBUG => Self::DEBUG,
            tracing::Level::TRACE => Self::TRACE,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Erased span trace types
// ─────────────────────────────────────────────────────────────────────────────

/// A serializable, type-erased representation of a span trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErasedSpanTrace {
    spans: Vec<ErasedSpan>,
}

/// A single span in an erased span trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErasedSpan {
    pub metadata: ErasedMetadata,
    pub fields: Box<str>,
}

/// Serializable metadata from a tracing span.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    #[inline]
    pub fn target(&self) -> &str {
        &self.target
    }

    #[must_use]
    #[inline]
    pub const fn level(&self) -> TracingLevel {
        self.level
    }

    #[must_use]
    #[inline]
    pub fn module_path(&self) -> Option<&str> {
        self.module_path.as_deref()
    }

    #[must_use]
    #[inline]
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    #[must_use]
    #[inline]
    pub const fn line(&self) -> Option<u32> {
        self.line
    }
}

#[cfg(feature = "tracing")]
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

impl ErasedSpanTrace {
    /// Create an `ErasedSpanTrace` from a live `SpanTrace`.
    #[cfg(feature = "tracing")]
    #[must_use]
    pub fn from_spantrace_ref(st: &crate::SpanTrace) -> Self {
        let mut spans = Vec::new();
        st.as_span_trace().with_spans(|metadata, fields| {
            spans.push(ErasedSpan {
                metadata: ErasedMetadata::from(metadata),
                fields: fields.into(),
            });
            true
        });
        Self { spans }
    }

    /// Returns `true` when the trace contains no spans.
    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Iterate over spans with their metadata and fields.
    pub fn with_spans<F>(&self, mut f: F)
    where
        F: FnMut(&ErasedMetadata, &str) -> bool,
    {
        for ErasedSpan { metadata, fields } in &self.spans {
            if !f(metadata, fields) {
                break;
            }
        }
    }
}

#[cfg(feature = "tracing")]
impl From<&crate::SpanTrace> for ErasedSpanTrace {
    #[inline]
    fn from(st: &crate::SpanTrace) -> Self {
        Self::from_spantrace_ref(st)
    }
}

impl fmt::Display for ErasedSpanTrace {
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
                write!(
                    f,
                    "{:>3}: {}::{}",
                    span + 1,
                    metadata.target(),
                    metadata.name()
                ),
                err
            );

            if !fields.is_empty() {
                try_bool!(write!(f, "\n           with {}", fields), err);
            }

            if let Some((file, line)) = metadata.file().zip(metadata.line()) {
                try_bool!(write!(f, "\n             at {}:{}", file, line), err);
            }

            span += 1;
            true
        });

        err
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_level_tolerant_deserialization() {
        let level: TracingLevel = serde_json::from_value(serde_json::json!("info")).unwrap();
        assert_eq!(level, TracingLevel::INFO, "case-insensitive known level");

        let level: TracingLevel = serde_json::from_value(serde_json::json!("FATAL")).unwrap();
        assert_eq!(
            level,
            TracingLevel::UNKNOWN,
            "unknown level degrades to UNKNOWN"
        );

        // Canonical uppercase still round-trips byte-stably.
        let json = serde_json::to_string(&TracingLevel::WARN).unwrap();
        assert_eq!(json, r#""WARN""#);
        let back: TracingLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TracingLevel::WARN);

        // UNKNOWN itself round-trips.
        let json = serde_json::to_string(&TracingLevel::UNKNOWN).unwrap();
        assert_eq!(json, r#""UNKNOWN""#);
        let back: TracingLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TracingLevel::UNKNOWN);
    }

    #[test]
    fn test_erased_spantrace_deserialization() {
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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        assert_eq!(spantrace.spans.len(), 2);
    }

    #[test]
    fn test_erased_spantrace_serialization_roundtrip() {
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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        let serialized = serde_json::to_string(&spantrace).unwrap();
        let roundtrip: ErasedSpanTrace = serde_json::from_str(&serialized).unwrap();

        // Verify roundtrip by serializing again
        let serialized2 = serde_json::to_string(&roundtrip).unwrap();
        assert_eq!(serialized, serialized2);
    }

    #[cfg(feature = "test-utils")]
    #[test]
    fn test_erased_spantrace_display() {
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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();

        insta::assert_snapshot!(display, @"
        1: my_module::inner_function
                   at src/lib.rs:42
        2: my_crate::outer_function
        ");
    }

    #[cfg(feature = "test-utils")]
    #[test]
    fn test_erased_spantrace_display_with_fields() {
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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();

        insta::assert_snapshot!(display, @"
        1: server::process_request
                 with user_id=42, method=GET
                   at src/server.rs:100
        ");
    }

    #[test]
    fn test_erased_spantrace_with_spans_iteration() {
        let json = serde_json::json!({
            "spans": [
                {"metadata": {"name": "span_a", "target": "target_a", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "span_b", "target": "target_b", "level": "DEBUG"}, "fields": ""},
                {"metadata": {"name": "span_c", "target": "target_c", "level": "TRACE"}, "fields": ""}
            ]
        });

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();

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
    fn test_erased_spantrace_with_spans_early_exit() {
        let json = serde_json::json!({
            "spans": [
                {"metadata": {"name": "a", "target": "t", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "b", "target": "t", "level": "INFO"}, "fields": ""},
                {"metadata": {"name": "c", "target": "t", "level": "INFO"}, "fields": ""}
            ]
        });

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();

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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
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

        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        spantrace.with_spans(|metadata, _fields| {
            assert_eq!(metadata.module_path(), None);
            assert_eq!(metadata.file(), None);
            assert_eq!(metadata.line(), None);
            false
        });
    }

    // ── ErasedSpanTrace Display edge case ─────────────────────────────

    #[test]
    fn test_erased_spantrace_display_span_zero_has_no_leading_newline() {
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
        let spantrace: ErasedSpanTrace = serde_json::from_value(json).unwrap();
        let display = spantrace.to_string();
        // The first span must NOT start with a newline
        assert!(!display.starts_with('\n'));
        assert!(display.starts_with("  1:"));
    }
}
