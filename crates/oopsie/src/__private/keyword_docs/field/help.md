Uses this field's runtime `Display` output as the error's dynamic help text,
surfaced via `Diagnostic::oopsie_help_text`. Unlike the variant-level
`help = "..."`, the text comes from the field value at runtime rather than a
fixed string. At most one `help` field per variant or struct.

Forms: `help`, `help = true`, `help = false`.

### Example
```
use oopsie::Diagnostic as _;

#[oopsie::oopsie]
#[oopsie(module(false))]
pub enum AppError {
    #[oopsie("config invalid")]
    BadConfig {
        #[oopsie(help)]
        hint: String,
    },
}

let err = BadConfig { hint: "set PORT in .env".to_owned() }.build();
assert_eq!(err.oopsie_help_text().as_deref(), Some("set PORT in .env"));
```
