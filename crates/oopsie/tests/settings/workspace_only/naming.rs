// Naming `BoomCtx` only compiles if the workspace `default-suffix = "Ctx"` applied.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom { code: u8 },
}

fn main() {
    let _ = BoomCtx { code: 0u8 };
}
