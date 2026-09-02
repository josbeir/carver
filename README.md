# Carver

[![Quality](https://github.com/josbeir/carver/actions/workflows/quality.yml/badge.svg)](https://github.com/josbeir/carver/actions/workflows/quality.yml)
[![codecov](https://codecov.io/gh/josbeir/carver/graph/badge.svg)](https://codecov.io/gh/josbeir/carver)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f.svg)](LICENSE)
[![MSRV: 1.92](https://img.shields.io/badge/MSRV-1.92-93450a.svg)](https://www.rust-lang.org/)

Carver is a fast, native GNOME note-taking application built with Rust, GTK4, and
Libadwaita. It keeps Carve markup as the durable source of every note while providing a
natural rich-text editing experience for everyday writing.

## Features

- A responsive GNOME interface with a collapsible category sidebar, recent notes grouped by
  day, and a focused full-text search powered by SQLite FTS5.
- Categories containing multiple notes, live note counters, contextual note moves, and a
  recoverable Trash.
- Three complementary note surfaces: rich-text Edit, canonical Carve Source, and a rendered
  Preview. Source mode can show a synchronized rendered split preview.
- WYSIWYG support for common Carve formatting: headings, inline marks, ordered/bulleted/task
  lists, links, inline code, and fenced code blocks. Familiar shortcuts such as Ctrl+B and
  Ctrl+I work in both editable surfaces.
- Images pasted directly from the clipboard are saved as managed local assets and rendered in
  notes. Remote image loading is configurable.
- Automatic, debounced saves with optimistic-concurrency revisions. Opening or leaving an
  unchanged note does not alter its modification date.
- TOML preferences for editor mode, split preview, image behavior, search scope, and window
  state.
- A UI-neutral Rust SDK layered above SQLite, making future native frontends practical without
  coupling them to GTK.

## Architecture

Carver is a Cargo workspace with clear dependency boundaries:

- `carver-domain` contains domain types and pure transformations.
- `carver-config` handles XDG paths and TOML configuration.
- `carver-storage-sqlite` owns migrations, FTS5 search, notes, and managed assets.
- `carver-sdk` exposes a UI-neutral asynchronous facade.
- `carver-richtext` projects supported Carve markup into the native editor.
- `carver-gtk` is the GTK4/Libadwaita desktop application.

## Getting started

Carver currently runs from source. It requires Rust 1.92 or newer, GTK 4.22+, Libadwaita
1.9+, and WebKitGTK 6 development libraries.

```sh
git clone https://github.com/josbeir/carver.git
cd carver
cargo run -p carver-gtk
```

## Development and quality

Run the standard checks before contributing:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
```

GTK interaction tests need a display server and run serially. On a Linux CI runner or desktop
with Xvfb available:

```sh
xvfb-run -a cargo test --workspace --locked -- --include-ignored --test-threads=1
```

Coverage is measured with `cargo-llvm-cov`; CI enforces at least 80% line coverage and uploads
the LCOV report to Codecov:

```sh
xvfb-run -a cargo llvm-cov --workspace --all-features --locked --fail-under-lines 80 -- \
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
