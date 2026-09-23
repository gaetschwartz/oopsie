use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
pub enum DepError {
    #[oopsie("boom {n}")]
    Boom { n: u64 },
}

pub fn g() -> DepError {
    dep_oopsies::Boom { n: 1u64 }.build()
}
