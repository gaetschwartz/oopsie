#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

// `U: From<T>` names `T` in its bound, but `T` is used only by the `Seed`
// variant. Naming `T` in another parameter's bound does not constrain the
// `Convert` selector's `Contextual` impl, so the impl would leave `T`
// unconstrained (E0207); the guard rejects it with a clear message instead.
#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
enum ConvertError<T: std::fmt::Debug + Clone, U: From<T> + std::fmt::Debug, E: std::error::Error + 'static>
{
    #[oopsie("converted {value:?}")]
    Convert { source: E, value: U },
    #[oopsie("seed {seed:?}")]
    Seed { seed: T },
}

fn main() {}
