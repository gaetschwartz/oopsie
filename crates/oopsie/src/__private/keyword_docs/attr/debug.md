By default `#[oopsie::oopsie]` adds a `Debug` derive to the error type (a `Debug`
impl is required by `std::error::Error`). Set `debug = false` to skip that
injection when you supply a hand-written `impl Debug` instead. An existing
`Debug` derive on the type is always respected and never duplicated.

Forms: `debug`, `debug = false`.

### Example
```
use std::fmt;

#[oopsie::oopsie(debug = false)]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

impl fmt::Debug for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("custom debug")
    }
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(format!("{err:?}"), "custom debug");
}
```
