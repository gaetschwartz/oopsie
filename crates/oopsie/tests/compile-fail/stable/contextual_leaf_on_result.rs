#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::{Oopsie, ResultExt as _};

#[derive(Debug, Oopsie)]
enum ConfigError {
    #[oopsie("missing key: {key}")]
    Missing { key: String },
}

fn main() {
    // `Missing` is a leaf selector — it has no `source` field, so it only
    // builds from `NoSource` and belongs on `Option::context` / `.fail()`,
    // not on a `Result` carrying an `io::Error`.
    let opened: Result<(), std::io::Error> = Err(std::io::Error::other("oops"));
    let _ = opened.context(config_oopsies::Missing {
        key: "host".to_owned(),
    });
}
