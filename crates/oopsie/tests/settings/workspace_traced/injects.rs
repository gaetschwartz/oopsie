// Workspace `traced = true` injects diagnostic fields, so constructing this with
// only its declared field fails with a `missing field` for the injected ones.
#[oopsie::oopsie]
pub struct E {
    pub a: u8,
}

fn main() {
    let _ = E { a: 0 };
}
