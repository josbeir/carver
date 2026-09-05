# Carver

[![Quality](https://github.com/josbeir/carver/actions/workflows/quality.yml/badge.svg)](https://github.com/josbeir/carver/actions/workflows/quality.yml)
[![codecov](https://codecov.io/gh/josbeir/carver/graph/badge.svg)](https://codecov.io/gh/josbeir/carver)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f.svg)](LICENSE)
[![MSRV: 1.92](https://img.shields.io/badge/MSRV-1.92-93450a.svg)](https://www.rust-lang.org/)

Carver is a native GNOME home for notes that last. Capture ideas in a polished writing
space, keep them organized without ceremony, and reach for the
[Carve](https://github.com/markup-carve) source whenever you want full control.

<p align="center">
  <img src="docs/Screenshot%20From%202026-09-03%2000-36-17.png" alt="Carver notes view" width="49%" />
  <img src="docs/Screenshot%20From%202026-09-03%2000-36-28.png" alt="Carver source and preview view" width="49%" />
</p>

## Notes, without the clutter

- **Rich-text writing** — Write naturally with headings, inline formatting, lists, tasks, links,
  code blocks, tables, definition lists, and familiar keyboard shortcuts.
- **Carve source, preview, and split view** — Move between rich editing, canonical `.crv` source,
  read-only preview, or a synchronized source-and-preview workspace without losing your place.
- **A source editor built for markup** — GtkSourceView-powered Carve highlighting, current-node
  breadcrumbs, line numbers, current-line highlighting, in-note search, configurable monospace
  font and size, and a quieter writing-focus syntax style when colour would distract.
- **Images that travel with the note** — Paste or add images, resize them from the editor, and
  keep managed assets referenced by canonical Carve markup. Remote-image loading is optional.
- **Organized, recoverable notes** — Create and move categories, browse recent notes, and use
  Trash to restore notes or categories (with Undo after moving a note to Trash).
- **Search, copy, and share** — Use full-text library search or `Ctrl+F` within an editor; copy a
  rendered note; import Carve (`.crv`) or Markdown; export Carve, Markdown, or PDF; bundle source
  exports with managed assets; or print through the native print dialog.
- **Preferences that respect your workflow** — Choose the editing surface, source-editor
  presentation, remote-image behavior, and whether the formatting toolbar is visible.
- **Native GNOME by design** — A responsive GTK4/Libadwaita application built in Rust, with
  light/dark-theme support, accessible controls, contextual menus, and a keyboard-shortcuts guide.

## Architecture

Carver is a Cargo workspace with clear dependency boundaries:

- `carver-domain` contains domain types and pure transformations.
- `carver-config` handles XDG paths and TOML configuration.
- `carver-storage-sqlite` owns migrations, FTS5 search, notes, and managed assets.
- `carver-sdk` exposes a UI-neutral asynchronous facade.
- `carver-editor-protocol` defines the small, format-neutral host/editor bridge.
- `carver-gtk` is the GTK4/Libadwaita desktop application.
  Its sandboxed Tiptap surface uses [Carve Grammars](https://github.com/markup-carve/carve-grammars)
  for faithful Carve editing, while native WebKit preview uses the canonical Carve renderer.
  Its window-local Model-View-Update runtime keeps application state UI-neutral: GTK/WebKit
  callbacks dispatch messages, a pure reducer requests typed effects, and views render snapshots.

## Getting started

Carver currently runs from source. It requires Rust 1.92 or newer, GTK 4.22+, Libadwaita
1.9+, GtkSourceView 5, and WebKitGTK 6 development libraries.

```sh
git clone https://github.com/josbeir/carver.git
cd carver
cargo run -p carver-gtk
```

To make a source-tree run appear in GNOME with Carver's icon and name, install
the local desktop assets once before launching it:

```bash
./scripts/install-dev-assets.sh
```

## Development and quality

Run the standard checks before contributing:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
```

GTK interaction tests run serially against an isolated, native Wayland compositor. Install
[Weston](https://gitlab.freedesktop.org/wayland/weston) (`pacman -S weston` on Arch; CI installs
the `weston` package) and use the included harness:

```sh
./scripts/with-weston.sh cargo test --workspace --locked -- --include-ignored --test-threads=1
```

Coverage is measured with `cargo-llvm-cov`; CI enforces at least 80% line coverage and uploads
the LCOV report to Codecov:

```sh
./scripts/with-weston.sh cargo llvm-cov --workspace --all-features --locked --fail-under-lines 80 -- \
  --include-ignored --test-threads=1
```

## Data locations

Carver follows the XDG base-directory convention:

- Configuration: `$XDG_CONFIG_HOME/carver/config.toml`
- Library: `$XDG_DATA_HOME/carver/library.sqlite3`
- Managed image assets: `$XDG_DATA_HOME/carver/assets/`
- Remote image cache: `$XDG_CACHE_HOME/carver/remote-images/`

## License

Carver is distributed under the [MIT License](LICENSE).
