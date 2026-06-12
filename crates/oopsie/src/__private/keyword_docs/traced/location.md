Inside `traced(...)`: injects an auto-captured `&'static Location<'static>`
field recording the call site where the error was built (default: on). The
location is surfaced via `Diagnostic::oopsie_location` and rendered by `Report`
even when backtraces are disabled. Use `location = false` to opt out.

Forms: `location`, `location = true`, `location = false`,
`location(true)`, `location(false)`.

### Example
```
#[oopsie::oopsie(traced(location = false))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
