// `traced = true` injects diagnostic fields into this attribute error, so
// constructing it with only its declared field fails with a `missing field` for
// the injected one — a direct witness that the trace fields were generated.
#[oopsie::oopsie]
pub struct E {
    pub a: u8,
}

fn main() {
    let _ = E { a: 0 };
}
