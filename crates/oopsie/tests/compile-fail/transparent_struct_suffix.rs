#[derive(Debug, oopsie::Oopsie)]
#[oopsie(transparent, suffix("Blah"))]
struct Wrap {
    source: std::io::Error,
}

fn main() {}
