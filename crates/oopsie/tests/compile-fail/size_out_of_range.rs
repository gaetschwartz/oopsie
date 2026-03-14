use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false), size(1..=2))]
enum OutOfRangeError {
    #[oopsie("has data: {data}")]
    HasData { data: String },
}

fn main() {}
