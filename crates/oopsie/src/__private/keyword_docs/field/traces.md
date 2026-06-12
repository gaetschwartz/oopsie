Marks this field as the packed `(Backtrace, SpanTrace)` pair, surfacing both
through `Diagnostic::oopsie_backtrace` and `oopsie_spantrace` from one field. The
type must be `(Backtrace, SpanTrace)` (optionally boxed); it is auto-captured and
excluded from the selector. A packed `traces` field cannot coexist with separate
`backtrace`/`spantrace` fields.

Forms: `traces`, `traces = true`, `traces = false`.
