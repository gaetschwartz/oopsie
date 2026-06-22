// `module = false` flattens an enum's selectors (normally module-wrapped), so the
// selector is nameable as a bare `Boom`.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
pub enum AppError {
    #[oopsie("boom")]
    Boom { code: u8 },
}

fn main() {
    let _ = Boom { code: 0u8 };
}
