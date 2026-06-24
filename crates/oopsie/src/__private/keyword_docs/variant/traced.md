Toggles traced injection for this enum variant when the enclosing enum uses
`#[oopsie::oopsie(traced)]`. The variant-level toggle is boolean-only: it either
keeps traced injection enabled for the variant or disables it entirely.

The toggle is only meaningful on a traced enum; writing any `traced` form on a
variant of an enum that isn't traced is a compile error.

Forms: `traced`, `traced = true`, `traced = false`.

### Example
```
#[oopsie::oopsie(traced)]
pub enum AppError {
    #[oopsie(traced = false)]
    #[oopsie("fast path")]
    Fast,

    #[oopsie("slow path")]
    Slow,
}

fn main() {
    let fast = app_oopsies::Fast.build();
    let slow = app_oopsies::Slow.build();
    assert_eq!(fast.to_string(), "fast path");
    assert_eq!(slow.to_string(), "slow path");
}
```