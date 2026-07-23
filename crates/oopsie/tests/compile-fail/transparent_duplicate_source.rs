#[oopsie::oopsie]
pub enum DupTransparent {
    #[oopsie(transparent)]
    A { source: std::io::Error },
    #[oopsie(transparent)]
    B { source: std::io::Error },
}

fn main() {}
