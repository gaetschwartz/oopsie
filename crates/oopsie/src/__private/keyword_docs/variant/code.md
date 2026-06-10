Attaches an error code to the variant or struct, surfaced via
`Diagnostic::oopsie_error_code` and shown by `Report`. The code follows
`format!` semantics, so it can interpolate the item's fields. On a `traced`
type it replaces the auto-generated code for this item.

Forms: `code = "..."`, `code("...")`, `code("... {}", expr)`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(display("bad request: {status}"), code("E{}", status))]
    Http { status: u16 },
}

let err = Http { status: 404u16 }.build();
assert_eq!(err.oopsie_error_code().as_deref(), Some("E404"));
```
