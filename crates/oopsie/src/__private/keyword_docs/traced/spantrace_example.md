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
