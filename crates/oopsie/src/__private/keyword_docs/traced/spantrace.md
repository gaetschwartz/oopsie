Inside `traced(...)`: enables and tunes capture of the span trace (default: on).
Mentioning this part never disables the others. `spantrace(false)` drops the
span trace, and a settings block tunes its type, boxing, and on/off state.

Forms: `spantrace`, `spantrace = true`, `spantrace = false`,
`spantrace(true)`, `spantrace(false)`,
`spantrace(r#type = Path, boxed = ..., enabled = ...)`.
