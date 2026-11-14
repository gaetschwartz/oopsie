Delegates `Display` to the wrapped error and generates a `From<Inner>` impl
instead of a context selector — so the inner error converts directly with `?`.
A `transparent` variant or struct must have exactly one field: the source.

Because the message is the wrapped error's own, `source()` skips the wrapped
error and returns *its* source, so an error chain never repeats the message. The
wrapped error stays reachable through the field. With an explicit `display`, the
message differs from the wrapped error's, so `source()` still returns it.

Forms: `transparent`, `transparent = true`, `transparent = false`.

### Example
```
#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(transparent)]
    Io { source: std::io::Error },
}

fn read() -> Result<(), AppError> {
    let _ = std::fs::read("/no/such/file")?;
    Ok(())
}

let err = read().unwrap_err();
assert!(matches!(err, AppError::Io { .. }));
```
