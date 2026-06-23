// 32 bytes; an excluded member inherits nothing, so the workspace cap (16) is ignored.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
pub struct Big {
    pub data: [u8; 32],
}

fn main() {}
