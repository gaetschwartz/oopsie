#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie]
pub enum AppError {
    #[oopsie("read failed")]
    Read,
    #[oopsie("read failed (io)")]
    ReadError,
}

fn main() {}
