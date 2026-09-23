// Control: a real member is still capped by the workspace max-size, proving
// the override root itself isn't ignored outright.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
pub struct Big {
    pub n: u64,
}

fn main() {}
