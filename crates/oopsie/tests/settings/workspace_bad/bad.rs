// The malformed workspace naming knob must be reported against the workspace section.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(suffix)]
pub struct Thing {
    pub a: u8,
}

fn main() {}
