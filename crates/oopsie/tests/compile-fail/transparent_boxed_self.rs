#[oopsie::oopsie]
pub enum SelfBoxed {
    #[oopsie(transparent)]
    Wrap { source: Box<SelfBoxed> },
    #[oopsie("leaf")]
    Leaf,
}

fn main() {}
