#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    named_arguments_used_positionally,
    clippy::uninlined_format_args,
    reason = "each expected `format!` mirrors its fixture's display args verbatim"
)]

use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NamedPositionalError {
    #[oopsie("{} {}", a, other = b)]
    Pair { a: u32, b: u32 },
    #[oopsie("{0}", x = a)]
    Index { a: u32 },
    #[oopsie("[{:1$}]", a, w = 6)]
    Width { a: u32 },
    #[oopsie("[{:.*}]", 2, v = 1.5f64)]
    Precision,
}

#[test]
fn named_arg_fills_implicit_positional_slot() {
    let (a, b) = (3u32, 4u32);
    assert_eq!(
        Pair { a, b }.build().to_string(),
        format!("{} {}", a, other = b)
    );
}

#[test]
fn named_arg_fills_explicit_index() {
    let a = 3u32;
    assert_eq!(Index { a }.build().to_string(), format!("{0}", x = a));
}

#[test]
fn named_arg_fills_width_count() {
    let a = 3u32;
    assert_eq!(
        Width { a }.build().to_string(),
        format!("[{:1$}]", a, w = 6)
    );
}

#[test]
fn named_arg_fills_star_precision_value() {
    assert_eq!(
        Precision.build().to_string(),
        format!("[{:.*}]", 2, v = 1.5f64)
    );
}
