// 32 bytes: under the workspace cap (64) but over the package cap (16); the
// package cap must win, so this fails citing the package section.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
pub struct Big {
    pub data: [u8; 32],
}

fn main() {}
