// 32 bytes, over the manifest `max-size = 16`.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
pub struct Big {
    pub data: [u8; 32],
}

fn main() {}
