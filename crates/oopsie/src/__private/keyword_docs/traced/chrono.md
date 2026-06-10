Inside `timestamp(...)`: uses `chrono::DateTime<Local>` instead of `SystemTime`
as the injected timestamp type (default: off). Opt-in, and needs oopsie's
`chrono` feature enabled.

Form: `chrono = true`.

### Example
```ignore
// `chrono = true` only compiles with oopsie's `chrono` feature, which this
// crate's own doctests don't enable.
#[oopsie::oopsie(traced(timestamp(chrono = true)))]
pub enum AppError {
    #[oopsie("boom")]
    Boom,
}
```
