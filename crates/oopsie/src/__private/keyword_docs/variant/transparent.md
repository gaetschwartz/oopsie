Delegates `Display` and the error `source` to the wrapped error, and generates a
`From<Inner>` impl instead of a context selector — so the inner error converts
directly with `?`. A `transparent` variant or struct must have exactly one field:
the source.

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
