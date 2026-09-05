# Carver

[![Quality](https://github.com/josbeir/carver/actions/workflows/quality.yml/badge.svg)](https://github.com/josbeir/carver/actions/workflows/quality.yml)
[![codecov](https://codecov.io/gh/josbeir/carver/graph/badge.svg)](https://codecov.io/gh/josbeir/carver)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f.svg)](LICENSE)
[![MSRV: 1.92](https://img.shields.io/badge/MSRV-1.92-93450a.svg)](https://www.rust-lang.org/)

Carver is a beautiful native GNOME note-taking app written in Rust. Its optional local MCP
connection lets an AI agent work with the plans, ideas, and project notes you already keep in
Carver—turning them into a useful, organized project context instead of an isolated chat.

<p align="center">
  <img src="docs/Screenshot%20From%202026-09-05%2016-43-51.png" alt="Carver editing Carve source in dark mode" width="49%" />
  <img src="docs/Screenshot%20From%202026-09-05%2016-44-25.png" alt="Carver rich-text editing view in dark mode" width="49%" />
</p>

<p align="center">
  <img src="docs/Screenshot%20From%202026-09-05%2016-45-29.png" alt="Carver Agents and MCP connection panel" width="49%" />
  <img src="docs/Screenshot%20From%202026-09-05%2016-45-35.png" alt="Carver preferences dialog" width="49%" />
</p>

<p align="center">
  <img src="docs/Screenshot%20From%202026-09-05%2016-45-50.png" alt="Carver editing Carve source in light mode" width="49%" />
  <img src="docs/Screenshot%20From%202026-09-05%2016-46-44.png" alt="Carver split source and preview view" width="49%" />
</p>

## Noteworthy features

- **Rich-text writing**<br>
  Headings, inline formatting, lists, tasks, links, tables, and keyboard shortcuts.
- **[Carve](https://github.com/markup-carve) source and preview**<br>
  Rich editing, canonical `.crv` source, read-only preview, and synchronized split view.
- **A source editor built for markup**<br>
  GtkSourceView highlighting, breadcrumbs, search, line controls, and configurable typography.
- **Images that travel with the note**<br>
  Paste, resize, and retain managed images alongside canonical source.
- **Organized, recoverable notes**<br>
  Categories, recent-note browsing, Trash restoration, and Undo.
- **Search, import, and export**<br>
  Full-text and in-note search; Carve/Markdown import; Carve, Markdown, and PDF export.
- **Preferences that respect your workflow**<br>
  Configure editing mode, source presentation, remote images, and the formatting toolbar.
- **Local agent access**<br>
  Let an agent use your notes as project context to find ideas, build plans, and organize work,
  with explicitly opt-in, reversible changes.
- **Native GNOME by design**<br>
  Responsive GTK4/Libadwaita design with light/dark themes and accessible controls.

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

## Agent access

Carver can expose its library to local AI clients through the `carver-mcp` stdio server. This is
especially handy when your notes are where thinking happens: write down rough ideas, meeting
notes, research, and project plans in Carver, then let an agent search and read that context when
helping you plan a project. With write access enabled, it can also draft or update plans, create
notes for new work, and organize notes and categories—so the useful result stays in your library
rather than disappearing into a chat transcript.

### Carve for writing, Markdown for interchange

Carver stores notes as [Carve](https://github.com/markup-carve), not Markdown. Markdown is a
great lowest-common-denominator interchange format, but its extensions and renderer-specific
dialects make complex documents ambiguous and difficult to round-trip reliably. Carve provides a
single canonical representation for the structured notes Carver edits—rich inline formatting,
tables, tasks, managed images, and target-specific content—so source mode, rich editing, and
preview can preserve the same document instead of guessing which Markdown dialect was intended.

That does not lock your notes in. Carver cleanly imports CommonMark and exports Markdown whenever
you need to share a note. MCP agents can pass Markdown to `create_note` or `save_note` with
`markdown: true`, and can pass `markdown: true` to `get_note` to receive a Markdown rendering in
the returned `source` field. Carver always keeps the canonical Carve source as the lossless
original, even when a Markdown export cannot represent every Carve construct exactly.

Open the **Connect an agent** entry in Carver's menu to choose Codex, Claude Code, GitHub Copilot
CLI, VS Code Copilot, or a generic stdio-MCP client and copy a user-level setup command. The setup
screen detects native, Flatpak, and Snap installs so the agent process opens the same private
library as Carver.

The server is read-only by default. Opt into reversible note changes explicitly with
`--allow-write`; permanent trash deletion, settings changes, raw database access, and managed
asset bytes are never exposed.

Agents can list categories and notes, search and read note source, and inspect the recoverable
trash. Category listings include each category's icon and accent colour. With write access
enabled, agents can create, rename, and update category appearance; create, save, move, trash,
restore, and adjust the creation and modification timestamps of notes; and trash or restore
categories. Every note write uses Carver's revision check, so an agent must reload a note after a
conflicting edit. `create_note` and `save_note` accept `markdown: true` to convert CommonMark
input into Carver's canonical source. `get_note` returns that canonical source, while
`get_note` with `markdown: true` returns a Markdown rendering for agents that prefer it.

`carver-mcp` is a local stdio process, not a network service. It opens the same XDG-scoped library
as the installed application, including the separate Flatpak or Snap data area when applicable.
Treat note contents returned to an agent as untrusted data, and review an agent's proposed changes
before enabling write access.

For headless setup, print the relevant command with:

```sh
carver-mcp configure codex
carver-mcp configure claude-code --allow-write
carver-mcp configure copilot
carver-mcp configure vscode
carver-mcp configure generic
```

## License

Carver is distributed under the [MIT License](LICENSE).
