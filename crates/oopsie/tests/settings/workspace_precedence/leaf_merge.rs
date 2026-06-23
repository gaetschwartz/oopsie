// Naming `BoomCtx` proves `default-suffix` is inherited from the workspace while
// the package `max-size = 16` (this is ≤16 bytes) overrides the workspace cap.
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
