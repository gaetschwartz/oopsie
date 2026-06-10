Overrides the path to the `oopsie` crate used in the generated code, given as a
string. Needed when `oopsie` is re-exported under a different path or renamed in
`Cargo.toml`. Defaults to `::oopsie`.

Form: `path = "some::path"`.

### Example
```ignore
// A real custom path must name a crate that actually re-exports `oopsie`,
// which can't exist inside this crate's own doctest.
#[oopsie::oopsie]
#[oopsie(path = "my_crate::reexports::oopsie")]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}
```
