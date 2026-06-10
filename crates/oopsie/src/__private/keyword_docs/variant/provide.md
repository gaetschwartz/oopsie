Provides a value or reference for the variant or struct through the
`std::error::Request` provider API (active with the `unstable` feature), letting
callers retrieve typed data with `Error::request_ref`/`request_value`. The
expression is evaluated against the item's fields; `ref` provides a borrow.

Forms: `provide(Type => expr)`, `provide(ref, Type => expr)`.

### Example
```
use oopsie::ErrorCode;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie(display("query failed"))]
    #[oopsie(provide(ErrorCode => ErrorCode::from_static("E_QUERY")))]
    Query,
}

let err = Query.build();
assert_eq!(err.to_string(), "query failed");
```
