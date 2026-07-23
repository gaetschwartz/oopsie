#[derive(Debug, oopsie::Oopsie)]
#[oopsie(vis = 42)]
enum E {
    #[oopsie("leaf")]
    Leaf,
}

fn main() {}
