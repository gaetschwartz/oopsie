Inside `timestamp(...)`: also exposes the captured timestamp through the
`std::error::Request` provider API (active with the `unstable` feature), so
callers can retrieve it with `Error::request_value` (default: off). Opt-in.

Form: `provide = true`.

### Example
```
#[oopsie::oopsie(traced(timestamp(provide = true)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
