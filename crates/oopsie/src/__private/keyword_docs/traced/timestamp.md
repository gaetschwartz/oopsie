Inside `traced(...)`: injects an auto-captured timestamp field (default: off). A
bare `timestamp` records a `SystemTime`; a settings block opts into the `chrono`
type or the provider exposure.

Forms: `timestamp`, `timestamp(chrono = ..., provide = ...)`.

### Example
```
#[oopsie::oopsie(traced(timestamp))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
