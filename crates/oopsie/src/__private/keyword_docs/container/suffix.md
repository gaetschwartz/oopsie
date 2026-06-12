Appends a suffix to generated selector names. The default is none for both enums
and structs — the selector base is the variant or struct name with a trailing
`Error` stripped. A suffix is mainly useful when the stripped name would clash
with the surrounding scope (e.g. a non-`Error`-suffixed struct under
`module(false)`, whose selector would otherwise share the type's name).

Forms: `suffix`, `suffix(true)`, `suffix(false)`, `suffix("X")`, `suffix = "X"`.
The flag forms (`suffix`/`suffix(true)`) use `"Oopsie"`.

### Example
```
#[oopsie::oopsie]
#[oopsie(suffix("Ctx"))]
pub struct QueryError {
    query: String,
}

fn main() {
    let err = query_oopsies::QueryCtx { query: "SELECT 1".to_owned() }.build();
    assert_eq!(err.to_string(), "QueryError");
}
```
