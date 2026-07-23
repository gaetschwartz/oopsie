Inside `traced(...)`: injects an auto-captured timestamp field (default: off). A
bare `timestamp` records a `SystemTime`; a settings block opts into the `chrono`
type or the provider exposure. Needs oopsie's `std` feature — under `no_std`,
`timestamp` fails to compile.

Forms: `timestamp`, `timestamp = true`, `timestamp = false`,
`timestamp(true)`, `timestamp(false)`,
`timestamp(chrono = ..., provide = ..., enabled = ...)`.

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
