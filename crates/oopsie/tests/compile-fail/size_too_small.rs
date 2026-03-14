use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(9999..))]
enum TooSmallError {
    #[oopsie("small")]
    Small,
}

fn main() {}
