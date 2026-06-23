Asserts the size of the error type at compile time, so an accidental growth of
the error fails the build instead of silently bloating every `Result` that
carries it. Accepts an exact size or a range, in bytes.

Forms: `size(N)`, `size(..=N)`, `size(N..)`, `size(N..=M)`.

Not available on a generic error type: its size depends on the type arguments
and has no single value to assert, so combining `size(...)` with generic
parameters is rejected.

### Example
```
#[oopsie::oopsie]
#[oopsie(size(..=64))]
pub enum AppError {
    #[oopsie("not found: {key}")]
    Missing { key: String },
}
# fn main() {}
```

### Project-wide default

With the `settings` feature, `max-size = N` under `[package.metadata.oopsie]` in
the crate's `Cargo.toml` caps every error derived in that crate, as if each
carried `#[oopsie(size(..=N))]`. The same key is also honored from
`[workspace.metadata.oopsie]` at the workspace root, applying to every member
crate that opts into the `settings` feature. A per-type `size(...)` always
overrides it, and `max-size = 0` is rejected (remove the key to disable the cap).
