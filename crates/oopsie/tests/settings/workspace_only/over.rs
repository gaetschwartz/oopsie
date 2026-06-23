// No package metadata; the cap is inherited from the workspace root.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum BigEnum {
    Heavy { data: [u8; 32] },
}

fn main() {}
