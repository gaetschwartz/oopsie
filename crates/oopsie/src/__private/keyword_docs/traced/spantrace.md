Inside `traced(...)`: enables and tunes capture of the span trace (default: on).
Mentioning this part never disables the others. `spantrace(false)` drops the
span trace, and a settings block tunes its type, boxing, and on/off state.

Forms: `spantrace`, `spantrace = true`, `spantrace = false`,
`spantrace(true)`, `spantrace(false)`,
`spantrace(r#type = Path, boxed = ..., enabled = ...)`.

### Example
```
#[oopsie::oopsie(traced(spantrace, backtrace(false)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
