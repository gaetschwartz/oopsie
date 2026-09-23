//! Trace reach through a type-erased source: the Provider API surfaces the
//! origin-most capture, while stable falls back to the wrap-site one.
#![cfg(all(feature = "fancy", feature = "tracing"))]
#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    clippy::all,
    reason = "integration test fixtures intentionally trip style lints"
)]

mod common;

use oopsie::{Report, Welp, oopsie};
use oopsie_core::{redact, snap_name_by_channel};

#[oopsie(traced)]
#[oopsie("inner failed: {message}")]
pub struct InnerError {
    message: String,
}

#[inline(never)]
fn origin_frame() -> InnerError {
    InnerOopsie {
        message: "disk offline".to_owned(),
    }
    .build()
}

#[inline(never)]
fn wrap_site(inner: InnerError) -> Welp {
    Welp::wrap(inner, "outer")
}

#[test]
fn test_welp_wrap_reaches_erased_source_trace() {
    if !common::backtrace_snapshot_tests_enabled() {
        return;
    }
    common::force_backtrace();
    let report = Report::new(wrap_site(origin_frame())).no_colors();
    redact!(backtrace, {
        insta::assert_snapshot!(snap_name_by_channel!("welp_wrap_erased_source"), report);
    });
}

mod provide_order {
    use core::num::NonZeroU8;

    use oopsie::{Diagnostic, ErrorCode, HelpText, Oopsie};

    #[cfg_attr(
        not(feature = "unstable-error-generic-member-access"),
        expect(dead_code, reason = "only the nightly provider path constructs it")
    )]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Tag(u32);

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("inner failed")]
    #[oopsie(code = "app::inner", help = "inner help", exit_code = 5)]
    #[oopsie(provide(Tag => Tag(7)))]
    pub struct InnerErr {
        n: u32,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("outer failed")]
    #[oopsie(code = "app::outer")]
    pub struct OuterErr {
        source: InnerErr,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie(transparent)]
    pub struct SeeThrough {
        source: InnerErr,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("middle failed")]
    pub struct Shield {
        source: InnerErr,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("top failed")]
    #[oopsie(code = "app::top")]
    pub struct TopErr {
        source: Shield,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("top-of-transparent failed")]
    #[oopsie(code = "app::top")]
    pub struct TopOverSeeThrough {
        source: SeeThrough,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    pub enum EnumLayer {
        #[oopsie(transparent)]
        Through { source: InnerErr },
        #[oopsie("enum shield")]
        Shielded { source: InnerErr },
    }

    const fn inner() -> InnerErr {
        InnerErr { n: 1 }
    }

    type Meta = (Option<ErrorCode>, Option<HelpText>, Option<u8>);

    /// The stable accessors, asserted equal to the Provider-API view when the
    /// nightly path is compiled in.
    fn meta<E: Diagnostic + 'static>(err: &E) -> Meta {
        let stable = (
            err.oopsie_error_code(),
            err.oopsie_help_text(),
            err.oopsie_exit_code().map(NonZeroU8::get),
        );
        #[cfg(feature = "unstable-error-generic-member-access")]
        {
            let provided = (
                core::error::request_value::<ErrorCode>(err),
                core::error::request_value::<HelpText>(err),
                core::error::request_value::<NonZeroU8>(err).map(NonZeroU8::get),
            );
            assert_eq!(stable, provided, "stable accessors and provider disagree");
        }
        stable
    }

    #[test]
    fn own_code_beats_source_code() {
        let outer = OuterErr { source: inner() };
        assert_eq!(
            meta(&outer),
            (Some(ErrorCode::from("app::outer")), None, Some(5))
        );
    }

    #[test]
    fn transparent_layer_surfaces_source_metadata() {
        let through = SeeThrough { source: inner() };
        assert_eq!(
            meta(&through),
            (
                Some(ErrorCode::from("app::inner")),
                Some(HelpText::from_static("inner help")),
                Some(5)
            )
        );
        let top = TopOverSeeThrough { source: through };
        assert_eq!(
            meta(&top),
            (Some(ErrorCode::from("app::top")), None, Some(5))
        );
    }

    #[test]
    fn message_layer_shields_code_and_help_but_not_exit() {
        let shield = Shield { source: inner() };
        assert_eq!(meta(&shield), (None, None, Some(5)));
        let top = TopErr { source: shield };
        assert_eq!(
            meta(&top),
            (Some(ErrorCode::from("app::top")), None, Some(5))
        );
    }

    #[test]
    fn enum_layers_follow_the_same_rule() {
        let through = EnumLayer::Through { source: inner() };
        assert_eq!(
            meta(&through),
            (
                Some(ErrorCode::from("app::inner")),
                Some(HelpText::from_static("inner help")),
                Some(5)
            )
        );
        let shielded = EnumLayer::Shielded { source: inner() };
        assert_eq!(meta(&shielded), (None, None, Some(5)));
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn welp_from_error_reports_outer_code() {
        let welp = oopsie::Welp::from_error(OuterErr { source: inner() });
        assert_eq!(
            meta(&welp),
            (Some(ErrorCode::from("app::outer")), None, Some(5))
        );
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn welp_wrap_shields_code_and_help_but_forwards_exit_and_values() {
        let wrapped = oopsie::Welp::wrap(inner(), "mid");
        assert_eq!(meta(&wrapped), (None, None, Some(5)));
        assert_eq!(core::error::request_value::<Tag>(&wrapped), Some(Tag(7)));

        let welp = oopsie::Welp::from_error(wrapped);
        assert_eq!(meta(&welp), (None, None, Some(5)));
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn welp_over_transparent_chain_reaches_origin() {
        let welp = oopsie::Welp::from_error(SeeThrough { source: inner() });
        assert_eq!(
            meta(&welp),
            (
                Some(ErrorCode::from("app::inner")),
                Some(HelpText::from_static("inner help")),
                Some(5)
            )
        );
        assert_eq!(core::error::request_value::<Tag>(&welp), Some(Tag(7)));
    }
}

mod ref_provides {
    use oopsie::{Diagnostic as _, ErrorCode, HelpText, Oopsie};

    #[derive(Debug, PartialEq, Eq)]
    pub struct Label(&'static str);

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("inner failed")]
    #[oopsie(provide(ref, ErrorCode => code))]
    #[oopsie(provide(ref, HelpText => hint))]
    #[oopsie(provide(ref, Label => label))]
    pub struct RefInner {
        code: ErrorCode,
        hint: HelpText,
        label: Label,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("outer shields")]
    pub struct RefOuter {
        source: RefInner,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    pub enum RefEnum {
        #[oopsie("variant failed")]
        #[oopsie(provide(ref, ErrorCode => code))]
        #[oopsie(provide(ref, HelpText => hint))]
        Coded { code: ErrorCode, hint: HelpText },
    }

    fn inner() -> RefInner {
        RefInner {
            code: ErrorCode::from("app::inner"),
            hint: HelpText::from_static("inner hint"),
            label: Label("inner label"),
        }
    }

    fn variant() -> RefEnum {
        RefEnum::Coded {
            code: ErrorCode::from("app::variant"),
            hint: HelpText::from_static("variant hint"),
        }
    }

    #[test]
    fn stable_accessors_surface_ref_provided_code_and_help() {
        let inner = inner();
        assert_eq!(
            inner.oopsie_error_code(),
            Some(ErrorCode::from("app::inner"))
        );
        assert_eq!(
            inner.oopsie_help_text(),
            Some(HelpText::from_static("inner hint"))
        );
        let variant = variant();
        assert_eq!(
            variant.oopsie_error_code(),
            Some(ErrorCode::from("app::variant"))
        );
        assert_eq!(
            variant.oopsie_help_text(),
            Some(HelpText::from_static("variant hint"))
        );
    }

    #[test]
    fn message_layer_shields_ref_provided_code_and_help() {
        let outer = RefOuter { source: inner() };
        assert_eq!(outer.oopsie_error_code(), None);
        assert_eq!(outer.oopsie_help_text(), None);
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn ref_provided_code_and_help_are_also_provided_by_value() {
        let inner = inner();
        assert_eq!(
            core::error::request_value::<ErrorCode>(&inner),
            Some(ErrorCode::from("app::inner"))
        );
        assert_eq!(
            core::error::request_value::<HelpText>(&inner),
            Some(HelpText::from_static("inner hint"))
        );
        let variant = variant();
        assert_eq!(
            core::error::request_value::<ErrorCode>(&variant),
            Some(ErrorCode::from("app::variant"))
        );
        assert_eq!(
            core::error::request_value::<HelpText>(&variant),
            Some(HelpText::from_static("variant hint"))
        );

        let welp = oopsie::Welp::from_error(inner);
        assert_eq!(
            welp.oopsie_error_code(),
            Some(ErrorCode::from("app::inner"))
        );
        assert_eq!(
            welp.oopsie_help_text(),
            Some(HelpText::from_static("inner hint"))
        );
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn message_layer_blocks_ref_requests_for_code_and_help() {
        let outer = RefOuter { source: inner() };
        assert_eq!(core::error::request_ref::<ErrorCode>(&outer), None);
        assert_eq!(core::error::request_ref::<HelpText>(&outer), None);
        assert_eq!(core::error::request_value::<ErrorCode>(&outer), None);
        assert_eq!(core::error::request_value::<HelpText>(&outer), None);

        let wrapped = oopsie::Welp::wrap(inner(), "mid");
        assert_eq!(core::error::request_ref::<ErrorCode>(&wrapped), None);
        assert_eq!(core::error::request_ref::<HelpText>(&wrapped), None);
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn message_layer_forwards_other_ref_provides() {
        let outer = RefOuter { source: inner() };
        assert_eq!(
            core::error::request_ref::<Label>(&outer),
            Some(&Label("inner label"))
        );
        let wrapped = oopsie::Welp::wrap(inner(), "mid");
        assert_eq!(
            core::error::request_ref::<Label>(&wrapped),
            Some(&Label("inner label"))
        );
    }
}

mod foreign_meta_types {
    use oopsie::{Diagnostic, Oopsie};

    #[derive(Debug, PartialEq, Eq)]
    pub struct HelpText(&'static str);

    #[derive(Debug, PartialEq, Eq)]
    pub struct ErrorCode(&'static str);

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("foreign by ref")]
    #[oopsie(provide(ref, HelpText => hint))]
    #[oopsie(provide(ref, ErrorCode => code))]
    pub struct ByRef {
        hint: HelpText,
        code: ErrorCode,
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("foreign by value")]
    #[oopsie(provide(HelpText => HelpText("value hint")))]
    #[oopsie(provide(self::ErrorCode => self::ErrorCode("value::code")))]
    pub struct ByValue;

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    pub enum Variants {
        #[oopsie("foreign variant")]
        #[oopsie(provide(ref, HelpText => hint))]
        #[oopsie(provide(ErrorCode => ErrorCode("variant::code")))]
        V { hint: HelpText },
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("real inner")]
    #[oopsie(code = "real::inner", help = "real help")]
    pub struct RealInner;

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie(transparent)]
    #[oopsie(provide(ref, ErrorCode => &FOREIGN_CODE))]
    #[oopsie(provide(HelpText => HelpText("foreign help")))]
    pub struct Through {
        source: RealInner,
    }

    static FOREIGN_CODE: ErrorCode = ErrorCode("foreign::code");

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("borrowed {name}")]
    #[oopsie(provide(ref, oopsie::HelpText => *hint))]
    #[oopsie(provide(ref, HelpText => *foreign))]
    pub struct Borrowed<'a> {
        name: &'a str,
        hint: &'a oopsie::HelpText,
        foreign: &'a HelpText,
    }

    fn assert_no_meta<E: Diagnostic + 'static>(err: &E) {
        assert_eq!(err.oopsie_error_code(), None);
        assert_eq!(err.oopsie_help_text(), None);
        #[cfg(feature = "unstable-error-generic-member-access")]
        {
            assert_eq!(core::error::request_value::<oopsie::ErrorCode>(err), None);
            assert_eq!(core::error::request_value::<oopsie::HelpText>(err), None);
        }
    }

    #[test]
    fn same_named_user_types_are_not_surfaced_as_code_or_help() {
        let by_ref = ByRef {
            hint: HelpText("ref hint"),
            code: ErrorCode("ref::code"),
        };
        assert_no_meta(&by_ref);
        assert_no_meta(&ByValue);
        assert_no_meta(&Variants::V {
            hint: HelpText("variant hint"),
        });
    }

    #[test]
    fn borrowed_ref_provides_resolve_by_type() {
        let hint = oopsie::HelpText::from_static("borrowed hint");
        let foreign = HelpText("foreign");
        let name = String::from("x");
        let err = Borrowed {
            name: &name,
            hint: &hint,
            foreign: &foreign,
        };
        assert_eq!(err.oopsie_help_text(), Some(hint.clone()));
        assert_eq!(err.oopsie_error_code(), None);
    }

    #[test]
    fn transparent_layer_forwards_past_same_named_user_types() {
        let through = Through { source: RealInner };
        assert_eq!(
            through.oopsie_error_code(),
            Some(oopsie::ErrorCode::from("real::inner"))
        );
        assert_eq!(
            through.oopsie_help_text(),
            Some(oopsie::HelpText::from_static("real help"))
        );
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn same_named_user_types_are_provided_as_plain_values() {
        let by_ref = ByRef {
            hint: HelpText("ref hint"),
            code: ErrorCode("ref::code"),
        };
        assert_eq!(
            core::error::request_ref::<HelpText>(&by_ref),
            Some(&HelpText("ref hint"))
        );
        assert_eq!(
            core::error::request_ref::<ErrorCode>(&by_ref),
            Some(&ErrorCode("ref::code"))
        );
        assert_eq!(
            core::error::request_value::<HelpText>(&ByValue),
            Some(HelpText("value hint"))
        );
        let through = Through { source: RealInner };
        assert_eq!(
            core::error::request_ref::<ErrorCode>(&through),
            Some(&ErrorCode("foreign::code"))
        );
        assert_eq!(
            core::error::request_value::<oopsie::ErrorCode>(&through),
            Some(oopsie::ErrorCode::from("real::inner"))
        );
    }
}

mod meta_attr_precedence {
    use oopsie::{Diagnostic as _, ErrorCode, HelpText, Oopsie};

    #[cfg_attr(
        not(feature = "unstable-error-generic-member-access"),
        expect(
            dead_code,
            reason = "the stable accessors never read a shadowed provide"
        )
    )]
    static REF_CODE: ErrorCode = ErrorCode::from_static("ref::code");
    #[cfg_attr(
        not(feature = "unstable-error-generic-member-access"),
        expect(
            dead_code,
            reason = "the stable accessors never read a shadowed provide"
        )
    )]
    static REF_HELP: HelpText = HelpText::from_static("ref help");

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    pub enum CodeAndRef {
        #[oopsie(display("v"), code = "attr::code", help = "attr help")]
        #[oopsie(provide(ref, ErrorCode => &REF_CODE))]
        #[oopsie(provide(ref, HelpText => &REF_HELP))]
        V {},
    }

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false))]
    #[oopsie("s")]
    #[oopsie(code = "attr::code")]
    #[oopsie(provide(ref, oopsie::ErrorCode => &REF_CODE))]
    #[oopsie(provide(ref, HelpText => &REF_HELP))]
    pub struct StructCodeAndRef {
        #[oopsie(help)]
        hint: String,
    }

    #[test]
    fn own_code_and_help_beat_ref_provides_on_stable() {
        let v = CodeAndRef::V {};
        assert_eq!(v.oopsie_error_code(), Some(ErrorCode::from("attr::code")));
        assert_eq!(
            v.oopsie_help_text(),
            Some(HelpText::from_static("attr help"))
        );
        let s = StructCodeAndRef {
            hint: "field help".to_owned(),
        };
        assert_eq!(s.oopsie_error_code(), Some(ErrorCode::from("attr::code")));
        assert_eq!(
            s.oopsie_help_text(),
            Some(HelpText::from_static("field help"))
        );
    }

    #[cfg(feature = "unstable-error-generic-member-access")]
    #[test]
    fn own_code_and_help_suppress_ref_provides_of_the_same_type() {
        let v = CodeAndRef::V {};
        assert_eq!(
            core::error::request_value::<ErrorCode>(&v),
            Some(ErrorCode::from("attr::code"))
        );
        assert_eq!(core::error::request_ref::<ErrorCode>(&v), None);
        assert_eq!(
            core::error::request_value::<HelpText>(&v),
            Some(HelpText::from_static("attr help"))
        );
        assert_eq!(core::error::request_ref::<HelpText>(&v), None);
        let s = StructCodeAndRef {
            hint: "field help".to_owned(),
        };
        assert_eq!(
            core::error::request_value::<ErrorCode>(&s),
            Some(ErrorCode::from("attr::code"))
        );
        assert_eq!(core::error::request_ref::<ErrorCode>(&s), None);
        assert_eq!(
            core::error::request_value::<HelpText>(&s),
            Some(HelpText::from_static("field help"))
        );
        assert_eq!(core::error::request_ref::<HelpText>(&s), None);
    }
}
