# Carver

[![Quality](https://github.com/josbeir/carver/actions/workflows/quality.yml/badge.svg)](https://github.com/josbeir/carver/actions/workflows/quality.yml)
[![codecov](https://codecov.io/gh/josbeir/carver/graph/badge.svg)](https://codecov.io/gh/josbeir/carver)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f.svg)](LICENSE)
[![MSRV: 1.98](https://img.shields.io/badge/MSRV-1.98-93450a.svg)](https://www.rust-lang.org/)

Carver is a beautiful native GNOME note-taking app written in Rust. Its optional local MCP
connection lets an AI agent work with the plans, ideas, and project notes you already keep in
Carver—turning them into a useful, organized project context instead of an isolated chat.

<p align="center">
  <img src="docs/Screenshot From 2026-09-08 19-36-38.png" alt="Carver editing in light mode" />
  <img src="docs/Screenshot From 2026-09-08 19-41-07.png" alt="Carver editing in dark mode" />
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
- **Media sidebar and attachments**<br>
  Browse and focus every image or managed file used in a note, add attachments, and preview them
  with GNOME's native viewers.
- **Organized, recoverable notes**<br>
  Categories, recent-note browsing, Trash restoration, and Undo.
- **Search, import, and export**<br>
  Full-text and in-note search; Carve/Markdown import; Carve, Markdown, and PDF export.
- **Preferences that respect your workflow**<br>
  Configure editing mode, source presentation, remote images, and the formatting toolbar.
- **[Local agent access](#agent-access)**<br>
  Let an agent use your notes as project context to find ideas, build plans, and organize work,
  with explicitly opt-in, reversible changes.
- **Native GNOME by design**<br>
  Responsive GTK4/Libadwaita design with light/dark themes and accessible controls.

## Tables of contents and collapsible details

**Enable enhanced Carve rendering** in Preferences is on by default. It renders
author-placed tables of contents, collapsible details, and bare HTTP(S) links in
Preview, source split preview, HTML export, and PDF/print. Author these constructs
in Source mode; Rich mode preserves their markup without dedicated insertion controls.

```carve
{depth="3"}
::: toc
:::

# My note

::: details "Supporting information"
Extra context and https://example.com
:::
```

Place attributes **before** the opening fence. Omit `depth` to include all six
heading levels, or use `{from="2" to="4"}` for a range. PDF/print expands details
so their contents are included. HTML exports retain the interactive disclosure.
Turning the preference off restores core rendering without editing the note.
Carve/Markdown exports and clipboard copies retain their existing behavior.

## Install

Download `carver-<version>-x86_64.flatpak` and its `.sha256` file from the
[latest release](https://github.com/josbeir/carver/releases/latest). Verify the
download, then install and launch Carver:

```sh
sha256sum -c carver-0.2.2-x86_64.flatpak.sha256
flatpak install --user --or-update --bundle ./carver-0.2.2-x86_64.flatpak
flatpak run io.github.josbeir.Carver
```

Carver needs the GNOME runtime. If Flatpak asks to add Flathub, accept the
prompt; otherwise add it once with:

```sh
flatpak remote-add --if-not-exists --user flathub \
  https://dl.flathub.org/repo/flathub.flatpakrepo
```

GitHub release bundles do not provide automatic updates yet. To update, download
the bundle from a newer release and run the same install command.

### AppImage builds

The release CI workflow produces `carver-<version>-x86_64.AppImage` and a matching
`.sha256` checksum on version tags and manual runs. Tagged builds attach them
to the release. The current baseline is Ubuntu 26.04 or a compatible newer
system, with `glycin-loaders` and Bubblewrap installed; use Flatpak on older
distributions. AppImage support is experimental pending CI and editor validation.

```sh
sha256sum -c carver-0.2.2-x86_64.AppImage.sha256
chmod +x carver-0.2.2-x86_64.AppImage
./carver-0.2.2-x86_64.AppImage --appimage-extract-and-run
```

Extraction works without FUSE. If FUSE is available, the image can also be
launched directly. AppImages use the native XDG data directories, separate from
the Flatpak library. See [packaging details](packaging/appimage/README.md).

## Develop from source

For development, Carver requires Rust 1.98 or newer, GTK 4.22+, Libadwaita 1.9+,
GtkSourceView 5, and WebKitGTK 6 development libraries.

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

## Architecture

Carver is a Cargo workspace with clear dependency boundaries:

| Layer | Package | Responsibility |
| --- | --- | --- |
| Domain | `carver-domain` | UI-independent note, category, revision, and search types; canonical Carve import and content-derived transformations. |
| Configuration | `carver-config` | XDG paths and durable TOML preferences. |
| Persistence contract | `carver-library-port` | UI-neutral library interface shared by storage implementations and clients. |
| Storage | `carver-storage-sqlite` | SQLite migrations, FTS5 search, notes, soft deletion, and managed assets. |
| Application SDK | `carver-sdk` | Asynchronous, UI-neutral facade over the installed library. |
| Editor bridge | `carver-editor-protocol` | Format-neutral message contract between the host and rich-text editor surfaces. |
| Export | `carver-export` | Carve and Markdown exports plus portable archives for managed images. |
| Agent integration | `carver-agent-integration` | Package-aware local MCP setup instructions and client metadata. |
| Desktop app | `carver-gtk` | GTK4/Libadwaita application with a window-local MVU runtime, source editor, sandboxed Tiptap rich editor, and native WebKit preview. The rich editor uses [Carve Grammars](https://github.com/markup-carve/carve-grammars) for faithful editing. |
| Local agent server | `carver-mcp` | Local stdio MCP server that opens Carver through the SDK and exposes controlled note and category access to agents. |

The UI calls the SDK; the SDK uses the library contract and storage; configuration and storage use
domain types. GTK/WebKit callbacks dispatch MVU messages, the reducer requests typed effects, and
views render immutable snapshots.

## Development and quality

The editor's `EditorEvent` and `SelectionState` TypeScript definitions are generated from
`carver-editor-protocol` using its optional `json-schema` feature. After changing those Rust
types, run `npm run protocol:generate --prefix apps/carver-gtk/web` and commit the generated
`protocol.generated.ts`. The web `npm run check` workflow rejects stale definitions.
Generation is development-only: normal builds use the committed types, which are erased
from the JavaScript bundle, and the GTK app does not enable the schema feature.

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
