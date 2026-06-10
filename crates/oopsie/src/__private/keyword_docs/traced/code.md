With `traced`, attaches an auto-generated error code to every variant (or the
struct) that has no explicit `#[oopsie(code = "...")]`, surfaced via
`Diagnostic::oopsie_error_code`. The generated code is the item's path,
`module_path::Type::Variant`; `transparent` items are excluded.
`traced(code = false)` turns the auto code off, and `traced(code(r#type = Path))`
overrides the error-code type (default `ErrorCode`).

Forms: `traced(code = false)`, `traced(code(r#type = Path))`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie(traced(code(r#type = ::oopsie::ErrorCode)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert!(err.oopsie_error_code().is_some());
}
```
