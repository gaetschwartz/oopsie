use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, Serialize, Deserialize,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum TracingLevel {
    TRACE = 0,
    DEBUG = 1,
    INFO = 2,
    WARN = 3,
    ERROR = 4,
}

impl From<TracingLevel> for tracing::Level {
    fn from(level: TracingLevel) -> Self {
        match level {
            TracingLevel::ERROR => tracing::Level::ERROR,
            TracingLevel::WARN => tracing::Level::WARN,
            TracingLevel::INFO => tracing::Level::INFO,
            TracingLevel::DEBUG => tracing::Level::DEBUG,
            TracingLevel::TRACE => tracing::Level::TRACE,
        }
    }
}

impl From<&tracing::Level> for TracingLevel {
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
