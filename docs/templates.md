# Dynamic note templates

Templates are reusable starting points. In the template editor, use **Insert Pattern**
to insert a current date, time, date and time, or category name at the cursor.
**Custom Date/Time Format…** offers common formats and a live example. **Pattern Reference**
is available in the same menu. Invalid patterns are shown inline and prevent saving.

## Supported patterns

| Pattern             | Example                     |
| ------------------- | --------------------------- |
| `{{date}}`          | `2026-10-01`                |
| `{{time}}`          | `14:30`                     |
| `{{datetime}}`      | `2026-10-01T14:30:12+02:00` |
| `{{category}}`      | `Meetings`                  |
| `{{date:%d/%m/%Y}}` | `01/10/2026`                |
| `{{time:%H:%M:%S}}` | `14:30:12`                  |

Patterns resolve when you create a note or insert a template into an existing note.
Each use captures one timestamp for all patterns and default date properties.
The host uses local time, falling back to UTC if the system offset is unavailable.
The property summary shows a sample using the dialog's captured time; confirmation
captures a fresh timestamp. The template itself keeps its patterns and revision.

Custom formats use the existing `time` crate's
[strftime format syntax](https://docs.rs/time/latest/time/format_description/fn.parse_strftime_borrowed.html).
Common tokens are `%Y` (year), `%m` (month), `%d` (day), `%H` (hour), `%M` (minute),
and `%S` (second). Named months and weekdays provided by this syntax use English.
Use ISO `{{date}}` or `{{datetime}}` for typed date/datetime properties; other
formats belong in text properties or the body.

## Example

```carve
---
meeting_date: "{{date}}"
created_at: "{{datetime}}"
category: "{{category}}"
---
# Meeting — {{date:%d/%m/%Y}}

Started at {{time}}.
```

Quote patterns in YAML, TOML, and JSON property values. Substitutions operate on
parsed string values, including nested lists and objects; property names and
non-string values stay unchanged. Category names are serialized safely as values.
For insertion, `{{category}}` uses the existing note's category; existing properties
are preserved and only missing template properties are added. New notes use their
destination category, with template properties overriding matching enabled defaults.

Patterns also expand inside code blocks. Write `\{{date}}` in body text to insert
literal `{{date}}`. In JSON or YAML double-quoted strings, use `"\\{{date}}"`;
YAML single-quoted strings can use `'\{{date}}'`. Expansion is single-pass: a
category containing pattern-like text is not expanded again. Unknown patterns and
unclosed delimiters are errors. Titles, prompted inputs, scripts, and arithmetic
are not supported.

## MCP

Use the same canonical template source with `create_template` or `save_template`.
`get_template` and `list_templates` return the unexpanded source. When `create_note`
omits `source`, a category's default template expands using the same SDK behavior
as GTK. Explicit note source is not treated as a template. Mutations still require
`--allow-write` and template saves retain revision checks.
