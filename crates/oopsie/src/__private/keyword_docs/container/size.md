Asserts the size of the error type at compile time, so an accidental growth of
the error fails the build instead of silently bloating every `Result` that
carries it. Accepts an exact size or a range, in bytes.

Forms: `size(N)`, `size(..=N)`, `size(N..)`, `size(N..=M)`.

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
