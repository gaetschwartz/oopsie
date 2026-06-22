// Over the cap; the diagnostic should blame the largest variant.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum BigEnum {
    Heavy { data: [u8; 32] },
}

fn main() {}
