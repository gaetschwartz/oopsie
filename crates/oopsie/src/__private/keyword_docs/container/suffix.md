Controls the suffix appended to generated selector names. Structs default to a
`"Oopsie"` suffix (e.g. `QueryError` → selector `QueryOopsie`); enums default to
no suffix. The selector base name is always the variant or struct name with a
trailing `Error` stripped first.

Forms: `suffix`, `suffix("X")`, `suffix(false)`.

### Example
```
#[oopsie::oopsie]
#[oopsie(suffix("Ctx"))]
pub struct QueryError {
    query: String,
}

let err = QueryCtx { query: "SELECT 1".to_owned() }.build();
assert_eq!(err.to_string(), "QueryError");
```
