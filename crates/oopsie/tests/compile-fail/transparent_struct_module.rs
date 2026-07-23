#[derive(Debug, oopsie::Oopsie)]
#[oopsie(transparent, module)]
struct Wrap {
    source: std::io::Error,
}

fn main() {}
