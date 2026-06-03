#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]
#![allow(
    unused,
    clippy::all,
    reason = "derive-macro test fixtures intentionally trip style lints"
)]

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

// ---- Test 4: container-level vis(pub) produces genuinely `pub` selectors ----
//
// Within one crate `pub` and `pub(crate)` are equally reachable, so reachability
// alone cannot prove the override took effect. `pub use` is the discriminator:
// re-exporting a `pub(crate)` item as `pub` is E0365 ("only public within the
// crate, cannot be re-exported"). So the re-export below compiles *only* because
// `vis(pub)` produced a truly-`pub` selector — a regression to the default vis
// would fail to compile this file.

mod container_vis {
    use oopsie::Oopsie;

    #[derive(Debug, Oopsie)]
    #[oopsie(vis(pub), module(false), suffix)]
    #[expect(
        unnameable_types,
        reason = "pub enum in a private module: deliberately reachable via the re-exported pub selector but not nameable at pub"
    )]
    pub enum WidgetError {
        #[oopsie("boom: {value}")]
        Boom { value: u32 },
    }
}

pub use container_vis::BoomOopsie;

#[test]
fn container_vis_pub_makes_selector_reexportable() {
    let err = BoomOopsie { value: 9u32 }.build();
    assert!(matches!(err, container_vis::WidgetError::Boom { value: 9 }));
}

// ---- Test 5: variant-level vis(pub) overrides the container default ----
//
// The container default is `pub(crate)`; only `Loud` overrides to `pub`, so only
// `LoudOopsie` is re-exportable. `QuietOopsie` keeps the default and would hit
// E0365 if re-exported — confirming the override is per-variant, not global.

mod variant_vis {
    use oopsie::Oopsie;

    #[derive(Debug, Oopsie)]
    #[oopsie(module(false), suffix)]
    #[expect(
        unnameable_types,
        reason = "pub enum in a private module: deliberately reachable via the re-exported pub selector but not nameable at pub"
    )]
    pub enum MixedError {
        #[oopsie("loud: {n}")]
        #[oopsie(vis(pub))]
        Loud { n: u32 },
        #[oopsie("quiet")]
        Quiet { n: u32 },
    }
}

pub use variant_vis::LoudOopsie;

#[test]
fn variant_vis_pub_overrides_container_default() {
    let err = LoudOopsie { n: 3u32 }.build();
    assert!(matches!(err, variant_vis::MixedError::Loud { n: 3 }));
}

// ---- Test 6: variant literally named `Error`, suffix stripping (no suffix) ----
//
// `selector_name` strips a trailing "Error" only when the remainder is
// non-empty (`.filter(|s| !s.is_empty())`). For a variant named exactly
// `Error`, stripping would leave "", which the filter rejects, so the base
// `"Error"` is kept. With no suffix (enum default), the selector struct is
// named `Error` — identical to the variant name, not an empty ident.

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum NestedSuffixError {
    #[oopsie("error variant: {detail}")]
    Error { detail: String },
}

#[test]
fn variant_named_error_keeps_name_without_suffix() {
    let err = Error {
        detail: "kaboom".to_owned(),
    }
    .build();
    assert_eq!(err.to_string(), "error variant: kaboom");
    assert!(matches!(err, NestedSuffixError::Error { detail } if detail == "kaboom"));
}

// ---- Test 7: variant named `Error` with a custom suffix appended ----
//
// Same base preservation as above, but with a suffix the selector becomes
// base + suffix = `Error` + `Oopsie` = `ErrorOopsie` (the "Error" is not
// stripped because stripping it would empty the base).

#[derive(Debug, Oopsie)]
#[oopsie(suffix, module(false))]
enum SuffixedErrorVariant {
    #[oopsie("suffixed error: {code}")]
    Error { code: i32 },
}

#[test]
fn variant_named_error_with_suffix_appends() {
    let err = ErrorOopsie { code: 13i32 }.build();
    assert!(matches!(err, SuffixedErrorVariant::Error { code: 13 }));
    assert_eq!(err.to_string(), "suffixed error: 13");
}
