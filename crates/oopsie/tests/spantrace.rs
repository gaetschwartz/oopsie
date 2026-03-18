#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(clippy::all)]

use oopsie::{Oopsie, SpanTrace};

#[derive(Debug, Oopsie)]
#[oopsie("Boxed spantrace error")]
#[oopsie(suffix, module(false))]
struct BoxedSpantraceError {
    #[oopsie(spantrace)]
    span: Box<SpanTrace>,
}

#[test]
fn test_extract_boxed_spantrace_via_provide_ref() {
    use oopsie::ErrorExt as _;
    let err = BoxedSpantraceOopsie.build();
    let extracted = err.oopsie_spantrace();
    assert!(extracted.is_some());
}

#[cfg(feature = "unstable")]
mod provide_test {
    use super::*;

    #[derive(Debug, Oopsie)]
    #[oopsie("Boxed spantrace error via provide")]
    #[oopsie(suffix, module(false))]
    #[oopsie(provide(ref, oopsie::SpanTrace => prov_span.as_ref()))]
    struct BoxedSpantraceProvideError {
        #[oopsie(spantrace)]
        prov_span: Box<SpanTrace>,
    }

    #[test]
    fn test_extract_boxed_spantrace_via_nightly_provide() {
        let err = BoxedSpantraceProvideOopsie.build();
        let extracted = core::error::request_ref::<SpanTrace>(&err);
        assert!(extracted.is_some());
    }
}
