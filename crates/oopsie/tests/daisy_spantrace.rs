#![cfg_attr(feature = "unstable", feature(error_generic_member_access))]
#![allow(clippy::all)]

use oopsie::{Oopsie, SpanTrace};

#[cfg(feature = "unstable")]
mod provide_test {
    use super::*;

    #[derive(Debug, Oopsie)]
    #[oopsie("Boxed spantrace error")]
    #[oopsie(suffix, module(false))]
    #[oopsie(provide(ref, oopsie::SpanTrace => span.as_ref()))]
    struct BoxedSpantraceError {
        #[oopsie(auto)]
        span: Box<SpanTrace>,
    }

    #[test]
    fn test_extract_boxed_spantrace_via_provide_ref() {
        let err = BoxedSpantraceOopsie.build();
        let extracted = SpanTrace::extract_from_error(&err);
        assert!(extracted.is_some());
    }
}
