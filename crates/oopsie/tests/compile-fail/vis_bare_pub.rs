#[derive(Debug, oopsie::Oopsie)]
#[oopsie(vis = pub(crate))]
enum E {
    #[oopsie("leaf")]
    Leaf,
}

fn main() {}
