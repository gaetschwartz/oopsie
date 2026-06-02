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
