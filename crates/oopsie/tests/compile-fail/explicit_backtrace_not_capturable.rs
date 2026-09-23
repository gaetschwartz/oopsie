#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::Oopsie;

#[derive(Debug)]
pub struct NotCapturable(oopsie::Backtrace);

impl core::borrow::Borrow<oopsie::Backtrace> for NotCapturable {
    fn borrow(&self) -> &oopsie::Backtrace {
        &self.0
    }
}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
#[oopsie("explicit backtrace of a non-capturable type")]
struct E {
    #[oopsie(backtrace)]
    backtrace: NotCapturable,
}

fn main() {}
