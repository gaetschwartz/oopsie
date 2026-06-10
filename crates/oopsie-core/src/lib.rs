//! Core error types and utilities.

#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
// Doctests that use `#[derive(Oopsie)]` will see the macro emit a `provide`
// impl when the unstable feature is active; inject the corresponding language
// feature flag so those doctests compile under `--features unstable`.
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    doc(test(attr(feature(error_generic_member_access))))
)]

mod backtrace;
mod diagnostic;
pub mod spantrace;
#[cfg(feature = "test-utils")]
pub mod test_utils;
mod traits;
mod welp;

use std::borrow::{Borrow, Cow};
use std::ops::Deref;

pub use backtrace::{
    Backtrace, RustBacktrace, clear_rust_backtrace_override, rust_backtrace, rust_panic_backtrace,
    set_rust_backtrace_override, with_rust_backtrace_override,
};
pub use diagnostic::Diagnostic;

/// Private helpers used by macro-generated code. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::backtrace::{
        is_backtrace_capture_code, is_internal_frame, is_post_panic_code, is_runtime_init_code,
    };
    /// Autoref probe for capture deduplication.
    ///
    /// When the concrete source type implements `Diagnostic`, the high-priority
    /// `CaptureFromExt` impl is selected and tries to extract existing traces.
    /// For non-`Diagnostic` sources (e.g., `io::Error`), the low-priority
    /// `CaptureFromFallback` impl is selected via autoref and does fresh capture.
    pub struct CaptureProbe<'a, T: ?Sized>(pub &'a T);

    /// High-priority: source implements `Diagnostic` → try extraction.
    pub trait CaptureFromExt {
        fn resolve<C: crate::CaptureExt>(&self) -> C;
    }

    impl<T: crate::Diagnostic> CaptureFromExt for CaptureProbe<'_, T> {
        #[inline]
        #[track_caller]
        fn resolve<C: crate::CaptureExt>(&self) -> C {
            C::capture_or_extract(self.0)
        }
    }

    /// Low-priority: source doesn't implement `Diagnostic` → fresh capture.
    pub trait CaptureFromFallback {
        fn resolve<C: crate::Capturable>(&self) -> C;
    }

    impl<T: ?Sized> CaptureFromFallback for &CaptureProbe<'_, T> {
        #[inline]
        #[track_caller]
        fn resolve<C: crate::Capturable>(&self) -> C {
            C::capture()
        }
    }

    /// Accessor-time autoref probe for forwarding a transparent wrapper's
    /// `Diagnostic` accessors to its source.
    ///
    /// Unlike [`CaptureProbe`] (which runs at construction to decide
    /// extract-vs-capture), this runs when `oopsie_*` is called: a transparent
    /// variant delegates each accessor to its source's concrete type. When that
    /// type implements `Diagnostic` the high-priority [`DiagForwardExt`] impl
    /// forwards the call; otherwise the [`DiagForwardFallback`] impl (selected
    /// via autoref) yields `None`. Works on stable, where the Provider-API
    /// [`source_trace`] path would yield `None`.
    pub struct DiagProbe<'a, T: ?Sized>(pub &'a T);

    /// High-priority: source implements `Diagnostic` → forward the accessor.
    pub trait DiagForwardExt<'a> {
        fn fwd_code(&self) -> Option<crate::ErrorCode>;
        fn fwd_help(&self) -> Option<crate::HelpText>;
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace>;
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace>;
    }

    impl<'a, T: crate::Diagnostic + ?Sized> DiagForwardExt<'a> for DiagProbe<'a, T> {
        #[inline]
        fn fwd_code(&self) -> Option<crate::ErrorCode> {
            self.0.oopsie_error_code()
        }
        #[inline]
        fn fwd_help(&self) -> Option<crate::HelpText> {
            self.0.oopsie_help_text()
        }
        #[inline]
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace> {
            self.0.oopsie_backtrace()
        }
        #[inline]
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace> {
            self.0.oopsie_spantrace()
        }
    }

    /// Low-priority: source doesn't implement `Diagnostic` → nothing to forward.
    pub trait DiagForwardFallback<'a> {
        #[inline]
        fn fwd_code(&self) -> Option<crate::ErrorCode> {
            None
        }
        #[inline]
        fn fwd_help(&self) -> Option<crate::HelpText> {
            None
        }
        #[inline]
        fn fwd_backtrace(&self) -> Option<&'a crate::Backtrace> {
            None
        }
        #[inline]
        fn fwd_spantrace(&self) -> Option<&'a crate::SpanTrace> {
            None
        }
    }

    impl<'a, T: ?Sized> DiagForwardFallback<'a> for &DiagProbe<'a, T> {}

    /// Pull the deepest `T` reachable from a source error via the Provider API.
    ///
    /// The generated `provide()` forwards to the source before providing its
    /// own trace, and std's `Request` is first-wins, so the deepest provider in
    /// the chain fills the slot — this surfaces the origin-most trace rather
    /// than a wrap-site one. Derived stable accessors call this on their source
    /// so they agree with the provider path.
    ///
    /// Returns `None` without `unstable-error-generic-member-access`: descending
    /// into a type-erased `dyn Error` source is not portable there, and the
    /// caller falls back to the wrapper's own field.
    #[inline]
    #[must_use]
    pub fn source_trace<'a, T: 'static>(
        source: &'a (dyn std::error::Error + 'static),
    ) -> Option<&'a T> {
        #[cfg(feature = "unstable-error-generic-member-access")]
        {
            core::error::request_ref::<T>(source)
        }
        #[cfg(not(feature = "unstable-error-generic-member-access"))]
        {
            let _ = source;
            None
        }
    }
}

pub use spantrace::{OptionalSpanTrace, SpanTrace};
use tracing_error::ErrorLayer;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::registry::LookupSpan;
pub use traits::*;
pub use welp::{Welp, WelpOptionExt, WelpResultExt};

macro_rules! impl_string_newtypes {
    ($($(#[$meta:meta])* $ident:ident,)* $(,)?) => { $(
        #[derive(
            Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(transparent)]
        #[repr(transparent)]
        $(#[$meta])*
        pub struct $ident(Cow<'static, str>);

        impl $ident {
            #[doc = concat!("Build a new `", stringify!($ident), "` from a `&'static str`.")]
            #[must_use]
            #[inline]
            pub const fn from_static(s: &'static str) -> Self {
                Self(Cow::Borrowed(s))
            }

            #[doc = concat!("Build a new `", stringify!($ident), "` from a `String`.")]
            #[must_use]
            #[inline]
            pub const fn from_string(s: String) -> Self {
                Self(Cow::Owned(s))
            }

            /// Borrow the underlying string.
            #[must_use]
            #[inline]
            pub const fn as_str(&self) -> &str {
                match &self.0 {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => s.as_str(),
                }
            }

            #[doc = concat!("Consume the `", stringify!($ident), "` and return the underlying `Cow`.")]
            #[must_use]
            #[inline]
            pub fn into_inner(self) -> Cow<'static, str> {
                self.0
            }
        }

        impl Deref for $ident {
            type Target = str;

            #[inline]
            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl Borrow<str> for $ident {
            #[inline]
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl From<&'static str> for $ident {
            #[inline]
            fn from(s: &'static str) -> Self {
                Self::from_static(s)
            }
        }

        impl From<String> for $ident {
            #[inline]
            fn from(s: String) -> Self {
                Self::from_string(s)
            }
        }

        impl std::fmt::Display for $ident {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

    )* };
}

impl_string_newtypes!(
    /// An opaque error code, used for programmatic handling and matching.
    ErrorCode,
    /// User-facing help text, intended to be shown in diagnostics.
    HelpText,
);

/// Construct a `tracing_error::ErrorLayer` configured to format span fields
/// as JSON.
///
/// Equivalent to `ErrorLayer::new(JsonFields::default())` but doesn't require
/// the caller to depend on `tracing-subscriber` directly.
#[inline]
#[must_use]
pub fn json_error_layer<S>() -> ErrorLayer<S, JsonFields>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    ErrorLayer::new(JsonFields::default())
}
