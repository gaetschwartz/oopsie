Overrides the injected type for this part: the trace field type inside
`backtrace(...)` or `spantrace(...)`, or the error-code type inside `code(...)`.
Spelled with the raw identifier `r#type` since `type` is a keyword.

Form: `r#type = some::Path`.

### Example
```
#[oopsie::oopsie(traced(backtrace(r#type = ::oopsie::Backtrace)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
