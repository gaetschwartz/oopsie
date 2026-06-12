Marks this field as the captured span trace, surfaced via
`Diagnostic::oopsie_spantrace`. The field's type must have `SpanTrace` as its
last path segment; it is auto-captured (and excluded from the selector). Marking
a field is only needed when its type isn't recognized automatically.

Forms: `spantrace`, `spantrace = true`, `spantrace = false`.
