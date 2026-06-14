#![cfg_attr(
    feature = "unstable-error-generic-member-access",
    feature(error_generic_member_access)
)]

use oopsie::{Oopsie, ResultExt as _};

#[derive(Debug, Oopsie)]
enum LoadError {
    #[oopsie("read failed for {file}: {source}", file = file.display())]
    Read {
        source: std::io::Error,
        file: std::path::PathBuf,
    },
}

fn main() {
    // The source type matches (`io::Error`), but the `file` field is given an
    // `i32` that does not `Into`-convert to `PathBuf`. Because the generated
    // `Contextual` impl matches structurally and only its `Into` where-clause
    // fails, rustc reports the precise `PathBuf: From<{integer}>` bound rather
    // than the `Contextual` `on_unimplemented` note — that note fires only when
    // no impl matches (a wrong source type or a leaf selector on a `Result`).
    let err: Result<(), std::io::Error> = Err(std::io::Error::other("oops"));
    let _ = err.context(load_oopsies::Read { file: 1 });
}
