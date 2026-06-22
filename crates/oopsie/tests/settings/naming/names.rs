// `module.suffix` renames the module to `app_api` and `default-suffix` makes the
// selector `BoomCtx`. Naming `app_api::BoomCtx` proves both renames took effect.
use oopsie::Oopsie;

#[derive(Debug, Oopsie)]
pub enum AppError {
    #[oopsie("boom")]
    Boom { code: u8 },
}

fn main() {
    let _ = app_api::BoomCtx { code: 0u8 };
}
