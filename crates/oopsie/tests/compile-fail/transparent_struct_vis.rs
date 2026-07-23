#[derive(Debug, oopsie::Oopsie)]
#[oopsie(transparent, vis(pub))]
struct Wrap {
    source: std::io::Error,
}

fn main() {}
