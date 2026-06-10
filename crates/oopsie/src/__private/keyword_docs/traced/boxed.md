Inside `traced(...)`: boxes the injected trace field(s) (default: on), keeping
the error type small. At the pair level it applies to both traces; placed inside
`backtrace(...)` or `spantrace(...)` it tunes that one trace. `boxed = false`
stores the trace(s) inline.

Forms: `boxed`, `boxed = true`, `boxed = false`, `boxed(true)`,
`boxed(false)`.

### Example
```
#[oopsie::oopsie(traced(boxed = false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
