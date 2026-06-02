#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(unused, clippy::all)]

use oopsie::Oopsie;

// ---- Test 1: default suffix adds "Oopsie" ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix, module(false))]
enum SuffixDefaultError {
    #[oopsie("variant alpha happened")]
    Alpha { value: u32 },
}

#[test]
fn default_oopsie_suffix() {
    let err = AlphaOopsie { value: 42u32 }.build();
    assert!(matches!(err, SuffixDefaultError::Alpha { value: 42 }));
    assert_eq!(err.to_string(), "variant alpha happened");
}

// ---- Test 2: custom suffix ----

#[derive(Debug, Oopsie)]
#[oopsie(suffix = "Ctx", module(false))]
enum SuffixCustomError {
    #[oopsie("beta failed")]
    Beta { msg: String },
}

#[test]
fn custom_suffix() {
    let err = BetaCtx { msg: "oops" }.build();
    assert!(matches!(err, SuffixCustomError::Beta { msg } if msg == "oops"));
}

// ---- Test 3: no suffix — selector = variant name ----

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NoSuffixError {
    #[oopsie("gamma occurred")]
    Gamma { code: i32 },
}

#[test]
fn no_suffix_uses_variant_name() {
    let err = Gamma { code: 7i32 }.build();
    assert!(matches!(err, NoSuffixError::Gamma { code: 7 }));
}
