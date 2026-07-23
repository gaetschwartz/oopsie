#[oopsie::oopsie]
pub enum E {
    #[oopsie(transparent, vis(pub))]
    Wrap { source: std::io::Error },
    #[oopsie("leaf")]
    Leaf,
}

fn main() {}
