#[oopsie::oopsie]
pub enum E {
    #[oopsie(display("validation failed"), help = "static advice")]
    Invalid {
        #[oopsie(help)]
        suggestion: String,
    },
}

fn main() {}
