#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::{Oopsie, ResultExt as _};

#[derive(Debug, Oopsie)]
enum LoadError {
    #[oopsie("read failed: {source}")]
    Read { source: std::io::Error },
}

fn main() {
    // `Read` builds from an `io::Error`, but this `Result`'s error is a
    // `ParseIntError`: the source types don't match.
    let parsed: Result<i32, std::num::ParseIntError> = "x".parse();
    let _ = parsed.context(load_oopsies::Read);
}
