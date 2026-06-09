#[oopsie::oopsie]
pub enum E {
    #[oopsie(transparent)]
    Wrapped { source: std::io::Error, ctx: String },
}

fn main() {}
