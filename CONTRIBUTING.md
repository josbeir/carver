# Contributing to Carver

See the [contributor guide](AGENTS.md) for workspace conventions and required checks.

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

| Layer                | Package                    | Responsibility                                                                                                                                                                                                                                   |
| -------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Domain               | `carver-domain`            | UI-independent note, category, revision, and search types; canonical Carve import and content-derived transformations.                                                                                                                           |
| Configuration        | `carver-config`            | XDG paths and durable TOML preferences.                                                                                                                                                                                                          |
| Persistence contract | `carver-library-port`      | UI-neutral library interface shared by storage implementations and clients.                                                                                                                                                                      |
| Storage              | `carver-storage-sqlite`    | SQLite migrations, FTS5 search, notes, soft deletion, and managed assets.                                                                                                                                                                        |
| Application SDK      | `carver-sdk`               | Asynchronous, UI-neutral facade over the installed library.                                                                                                                                                                                      |
| Editor bridge        | `carver-editor-protocol`   | Format-neutral message contract between the host and rich-text editor surfaces.                                                                                                                                                                  |
| Export               | `carver-export`            | Carve and Markdown exports plus portable archives for managed images.                                                                                                                                                                            |
| Agent integration    | `carver-agent-integration` | Package-aware local MCP setup instructions and client metadata.                                                                                                                                                                                  |
| Desktop app          | `carver-gtk`               | GTK4/Libadwaita application with a window-local MVU runtime, source editor, sandboxed Tiptap rich editor, and native WebKit preview. The rich editor uses [Carve Grammars](https://github.com/markup-carve/carve-grammars) for faithful editing. |
| Local agent server   | `carver-mcp`               | Local stdio MCP server that opens Carver through the SDK and exposes controlled note and category access to agents.                                                                                                                              |

The UI calls the SDK; the SDK uses the library contract and storage; configuration and storage use
domain types. GTK/WebKit callbacks dispatch MVU messages, the reducer requests typed effects, and
views render immutable snapshots.

## Development and quality

The editor's `EditorEvent`, `SelectionState`, `DocumentTarget`, `TableCommand`, and `LinkCommand`
TypeScript definitions are generated from `carver-editor-protocol` using its optional
`json-schema` feature. After changing those Rust types, run
`npm run protocol:generate --prefix apps/carver-gtk/web` and commit the generated
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

Coverage is measured with `cargo-llvm-cov`; CI enforces at least 90% line coverage and uploads
the LCOV report to Codecov:

```sh
./scripts/with-weston.sh cargo llvm-cov --workspace --all-features --locked --fail-under-lines 90 -- \
  --include-ignored --test-threads=1
```

## Translations

Carver uses GNU gettext. English is the source language, and the shipped catalogs live in
`po/` (`nl`, `de`, `fr`, `es`, `it`, `zh_CN`). After adding, changing, or removing a
user-visible string, regenerate the template and refresh the catalogs:

```sh
./scripts/update-translations.sh
```

Never edit `msgid`s by hand; fix the English source and regenerate. The script uses
GNU gettext's Rust parser when it is available (gettext 0.24+); older releases such as
Ubuntu's gettext 0.23.2 fall back to the C parser, which extracts the same messages but
cannot annotate Rust format strings for `msgfmt` validation. To try a translation
locally, compile the catalogs into a directory and point Carver at it:

```sh
./scripts/compile-translations.sh /tmp/carver-locale
CARVER_LOCALEDIR=/tmp/carver-locale/locale LANGUAGE=de cargo run -p carver-gtk
```

Packaging installs the `.mo` catalogs and translated desktop/metainfo through
`scripts/compile-translations.sh`.

## Data locations

Carver follows the XDG base-directory convention:

- Configuration: `$XDG_CONFIG_HOME/carver/config.toml`
- Library: `$XDG_DATA_HOME/carver/library.sqlite3`
- Managed image assets: `$XDG_DATA_HOME/carver/assets/`
- Remote image cache: `$XDG_CACHE_HOME/carver/remote-images/`
