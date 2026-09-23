#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

#[derive(Debug)]
pub struct NotCapturable(oopsie::Backtrace);

impl core::borrow::Borrow<oopsie::Backtrace> for NotCapturable {
    fn borrow(&self) -> &oopsie::Backtrace {
        &self.0
    }
}

#[oopsie::oopsie(traced(packed = false, backtrace(r#type = NotCapturable, boxed = false)))]
pub struct E {
    info: String,
}

fn main() {}
