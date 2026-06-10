#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie]
#[oopsie("all failed")]
pub enum E {
    #[oopsie("bad")]
    Bad { x: u32 },
}

fn main() {}
