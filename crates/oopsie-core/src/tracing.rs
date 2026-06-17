//! `tracing` integration helpers.

use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format;
use tracing_subscriber::registry::LookupSpan;

/// Construct a `tracing_error::ErrorLayer` configured to format span fields
/// as JSON.
///
/// Equivalent to `ErrorLayer::new(JsonFields::default())` but doesn't require
/// the caller to depend on `tracing-subscriber` directly.
#[cfg(feature = "serde")]
#[inline]
#[must_use]
pub fn json_error_layer<S>() -> ErrorLayer<S, format::JsonFields>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    ErrorLayer::new(format::JsonFields::default())
}

/// Construct a `tracing_error::ErrorLayer` configured to format span fields using
/// the default format (i.e. `Debug`-formatting each field value).
#[inline]
#[must_use]
pub fn default_error_layer<S>() -> ErrorLayer<S, format::DefaultFields>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    ErrorLayer::new(format::DefaultFields::new())
}
