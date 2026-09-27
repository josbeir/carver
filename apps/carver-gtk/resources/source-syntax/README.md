# Bundled syntax assets

- `carve.lang`, `carve-light.xml`, `carve-dark.xml`, `carve-writing-focus-light.xml`, and
  `carve-writing-focus-dark.xml` are Carver's own grammar and style schemes (MIT, see the
  repository `LICENSE`).
- `yaml.lang`, `toml.lang`, and `json.lang` are vendored unmodified from GtkSourceView
  (<https://gitlab.gnome.org/GNOME/gtksourceview>, `data/language-specs/`) so raw frontmatter
  highlighting never depends on the host's GtkSourceView data. They are licensed
  LGPL-2.1-or-later and keep their upstream copyright and license headers.

All assets are installed into `$XDG_DATA_HOME/carver/source-syntax` at startup and loaded by the
Carve source editor and the document-properties raw editor.
