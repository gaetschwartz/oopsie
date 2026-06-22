// 32 bytes — over the manifest `max-size = 16`, but a per-type `size(..=128)`
// overrides the manifest cap, so this builds.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix, size(..=128))]
pub struct Big {
    pub data: [u8; 32],
}

fn main() {}
