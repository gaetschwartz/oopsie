use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(..=1))]
enum TooLargeError {
    #[oopsie("has data: {data}")]
    HasData { data: String },
}

fn main() {}
