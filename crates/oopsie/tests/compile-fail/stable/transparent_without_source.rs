#[oopsie::oopsie]
pub enum E {
    #[oopsie(transparent)]
    NoSource { what: String },
}

fn main() {}
