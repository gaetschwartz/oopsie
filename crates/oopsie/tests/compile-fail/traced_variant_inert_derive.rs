#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// A derive-only enum never injects traces, so any variant `traced` toggle —
// including `traced = false` — is inert and rejected up front.
#[derive(Oopsie)]
enum E {
    #[oopsie(traced = false)]
    #[oopsie("boom")]
    Boom,
}

fn main() {}
