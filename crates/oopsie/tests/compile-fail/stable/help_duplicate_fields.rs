#[oopsie::oopsie]
pub enum E {
    #[oopsie("validation failed")]
    Invalid {
        #[oopsie(help)]
        first: String,
        #[oopsie(help)]
        second: String,
    },
}

fn main() {}
