#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie]
pub enum Config {
    #[oopsie("bad config")]
    Config,
    #[oopsie("other")]
    Other,
}

fn main() {}
