An explicit on/off switch accepted in any settings block, e.g.
`backtrace(enabled = false, r#type = ...)`. It lets a block both carry other
settings and state whether the part is active; a settings block without it counts
as enabled.

Forms: `enabled`, `enabled = true`, `enabled = false`.

### Example
```
#[oopsie::oopsie(traced(backtrace(enabled = false)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
