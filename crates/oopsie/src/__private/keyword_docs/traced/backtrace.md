Inside `traced(...)`: enables and tunes capture of the backtrace (default: on).
Mentioning this part never disables the others. `backtrace(false)` drops the
backtrace, and a settings block tunes its type, boxing, and on/off state.

Forms: `backtrace`, `backtrace(false)`,
`backtrace(r#type = Path, boxed = ..., enabled = ...)`.

### Example
```
#[oopsie::oopsie(traced(backtrace, spantrace(false)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
