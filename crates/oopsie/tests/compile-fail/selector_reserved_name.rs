#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("self error")]
    SelfError { info: String },
}

fn main() {}
