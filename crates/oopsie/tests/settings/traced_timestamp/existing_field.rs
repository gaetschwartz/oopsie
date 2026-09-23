// A manifest-default `timestamp` must not reject a type that already carries a
// timestamp-typed field: `timestamp` was never written at this call site.
#[oopsie::oopsie(traced)]
pub struct Seen {
    pub last_seen: std::time::SystemTime,
}

fn main() {}
