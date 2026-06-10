With `traced` active, attaches an auto-generated error code to every variant (or
the struct) that has no explicit `#[oopsie(code = "...")]`, surfaced via
`Diagnostic::oopsie_error_code`. The generated code is the item's path,
`module_path::Type::Variant`; `transparent` items are excluded. `code = false`
turns the auto code off, and `code(r#type = Path)` overrides the error-code type.

Forms: `code = false`, `code(r#type = Path)`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert!(err.oopsie_error_code().is_some());
}
```
