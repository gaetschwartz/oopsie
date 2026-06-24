Injects trace-capture fields into every variant (or the struct): a backtrace, a
span trace, and the caller location, all enabled by default. The nested options
tune each part — mentioning one never disables the others — and `traced(false)`
opts the type out entirely. With `traced` active, an auto-generated error code is
also attached to each variant unless the nested `code` option disables it.

On enums, a variant can override tracing with `#[oopsie(traced)]` or
`#[oopsie(traced = false)]`. This toggle is boolean-only at variant scope: it
enables or disables the variant's traced injection as a whole. It only applies
when the enum itself is traced; a variant `traced` on an enum that isn't traced
is a compile error.

Forms: `traced`, `traced = true`, `traced = false`, `traced(true)`, `traced(false)`,
`traced(enabled = ...)`,
`traced(backtrace(...), spantrace(...), timestamp(...), location = ..., packed = ..., boxed = ..., code(...))`.

Variant forms (enum variants only): `traced`, `traced = true`, `traced = false`.

### Example
```
#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}

fn main() {
    let err = app_oopsies::Boom.build();
    assert_eq!(err.to_string(), "boom");
}
```
