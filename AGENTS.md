# Carver contributor guide

Carver is a Rust workspace for a native GNOME note-taking application using GTK4,
Libadwaita, SQLite, and the Carve markup crate.

## Workspace map

- `crates/carver-domain`: markup-derived domain entities and pure transformations.
- `crates/carver-config`: XDG locations and TOML configuration.
- `crates/carver-library-port`: UI-neutral storage contract shared by SDK and backends.
- `crates/carver-storage-sqlite`: SQLite schema, migrations, search, and managed assets.
- `crates/carver-sdk`: UI-neutral facade for current and future native frontends.
- `crates/carver-editor-protocol`: format-neutral commands and events exchanged by the host
  and web editor, including the source types for generated TypeScript events.
- `crates/carver-export`: Carve and Markdown exports and portable managed-asset archives.
- `apps/carver-gtk`: GTK4/Libadwaita application. `main.rs` is bootstrap only;
  `app.rs` composes one window-local MVU runtime; `mvu/` owns the UI-neutral model,
  messages, reducer, and asynchronous effects; `view/` owns snapshot rendering;
  and `ui/` owns GTK boundaries, including `ui/editor/web.rs` for the WebKit bridge.
  `web/` contains the TypeScript/Tiptap editor and Carve adapter integration.
  `src/tests/` contains cross-module GTK frontend tests. Keep UI code local to this crate.
- `apps/carver-mcp`: local stdio Model Context Protocol companion. It has no GTK dependency and
  opens Carver through the SDK against the same XDG-scoped library.
- `crates/carver-agent-integration`: package-aware launch instructions and agent setup metadata
  shared by the GTK onboarding surface and `carver-mcp`.

Dependencies must point inward: GTK calls the SDK; the SDK uses the library contract and
storage; storage and configuration use domain types. The GTK editor exchanges messages through
`carver-editor-protocol`; its web surface uses Carve Grammars for parsing and serialization.
Keep the editor protocol independent of Carve, storage, and GTK. Domain must not depend on
infrastructure or GTK.

## MCP and agent integration

- Keep MCP local and stdio-only. Do not add a listener, remote transport, telemetry, or automatic
  agent registration.
- `carver-mcp` must use `carver-sdk`; it must not access SQLite, GTK, or application UI state
  directly. It shares the installed package's XDG library boundary, including Flatpak and Snap.
- Reuse shared domain types through the SDK in MCP requests when their wire representation
  matches. Keep `JsonSchema` support behind optional `json-schema` features; do not duplicate
  domain enums or wrap primitives solely to generate request schemas.
- Read tools are the default. Every mutation must require the explicit `--allow-write` process
  flag and retain Carver's revision checks, soft-delete/restore behavior, and validation rules.
- Store canonical Carve only. MCP create/save inputs may opt into Markdown conversion, but output
  and persistence remain canonical Carve.
- Treat note content as untrusted data. Do not expose raw database access, settings mutation,
  permanent trash deletion, managed-asset bytes, or network capabilities through MCP.
- Keep agent-client metadata/configuration separate from GTK view code. Validate definitions and
  preserve the Rust-owned launch construction and write gate.

## Rust rules

- Before implementing or extending a local solution, always research whether a public crate
  already solves the problem. Prefer a suitable, well-maintained crate with established
  community adoption over maintaining equivalent code ourselves. Verify maintenance activity,
  adoption, API fit, license compatibility, and dependency cost; reuse existing workspace
  dependencies where possible. If no suitable crate meets these criteria, briefly document
  why a local implementation is warranted.
- Use idiomatic Rust guided by `rust-best-practices`; prefer borrowed inputs (`&str`,
  `&Path`, slices) and clone only when GTK callback ownership or a snapshot requires it.
- Production code must return typed `Result` errors. Libraries use `thiserror`; do not
  use `anyhow`, `unwrap`, `expect`, or `panic` outside tests.
- Keep public APIs documented, including `# Errors` where applicable. Run rustdoc with
  warnings as errors.
- Do not add blanket `#[allow]` attributes. Resolve the lint, or use a narrowly scoped
  `#[expect(clippy::...)]` with a `// CONTEXT:` justification.
- Do not introduce `unsafe` code. The workspace forbids it.
- GTK objects and callbacks are main-thread-only. `Rc` is appropriate for GTK callback
  ownership; use `Cell` or a narrowly scoped `RefCell` for individual mutable fields.
  Never hold a `RefCell` borrow while calling SDK/storage code or emitting GTK signals.
- Follow MVU: GTK/WebKit callbacks only translate input into `AppMsg`; `update` is pure and
  returns typed effects; the runtime owns SDK/config work; and views render immutable snapshots.
  Do not add `AppState`, direct refresh paths, storage fallbacks, or business state to a view.
- Keep blocking SQLite work behind the SDK's async boundary. Do not perform storage work
  directly from a GTK signal handler or capture GTK objects in a background task.

## Editor protocol generation

- `EditorEvent` and `SelectionState` in `carver-editor-protocol` are the source of truth for
  web editor event types. After changing them, run
  `npm run protocol:generate --prefix apps/carver-gtk/web` and commit the generated
  `apps/carver-gtk/web/src/editor/protocol.generated.ts`. Do not edit that file manually.
- Run `npm run check --prefix apps/carver-gtk/web` after editor protocol or web changes.
  It checks generated-type drift, lint, formatting, tests, coverage, and the TypeScript build.
- Keep editor schema generation development-only. Normal builds consume committed type
  definitions; do not add runtime schema generation or validation, or enable the editor
  protocol's `json-schema` feature in the GTK application.

## UI and persistence rules

- Follow GNOME patterns: `AdwNavigationSplitView` for responsive navigation,
  `AdwClamp` for readable content widths, standard `AdwHeaderBar` menu placement, and
  modal dialogs transient to their parent window.
- Notes are soft-deleted. User-facing deletion must offer Undo and must not remove assets
  directly. Add restore/trash UI before adding permanent deletion.
- New and renamed categories require a non-empty, trimmed user-entered name.
- The rich editor writes canonical Carve source; source mode is the direct representation.
  Formatting controls and keyboard shortcuts must keep the two buffers synchronized.
  Switching among Edit, Source, and Preview must not discard unsaved text, blank lines,
  block structure, or supported inline formatting.
- Rich and source toolbars expose equivalent supported commands. Source commands edit Carve
  delimiters directly; rich commands update the projection and preserve its canonical source.
  Preview is read-only. Do not silently turn unsupported Carve constructs into editable text:
  preserve their source instead.
- Pasted images are managed assets referenced by canonical Carve image markup. The rich editor
  must render them, preserve aspect ratio, and never store clipboard-only image data in SQLite.
- Opening a note is read-only. Saving unchanged canonical source must preserve its revision and
  `updated_at`; only a material edit updates either value. Refresh category/all-note counters
  after create, move, restore, or trash actions.
- Keep destructive actions explicit: note/category deletion requires confirmation or Undo as
  appropriate. Category and note move actions belong in contextual UI, not duplicated in a
  crowded editor header.
- Settings live at `$XDG_CONFIG_HOME/carver/config.toml`; the SQLite library and managed
  assets live at `$XDG_DATA_HOME/carver/`. Preserve these XDG boundaries.

## Required checks

Run these before handing off a change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
git diff --check
```

CI's authoritative coverage tool is `cargo-llvm-cov`, not Tarpaulin. The coverage gate is
80% line coverage and must include ignored GTK interaction tests:

```sh
./scripts/with-weston.sh cargo llvm-cov --workspace --all-features --locked --fail-under-lines 80 -- \
  --include-ignored --test-threads=1
```

GTK signal tests require one initialization thread and a display server. Carver uses native
Wayland tests: the `scripts/with-weston.sh` harness starts an isolated Weston headless compositor
with GTK forced onto its Wayland backend and software renderer for deterministic headless output.
Install Weston (`pacman -S weston` on Arch) and use the same harness locally and in CI:

```sh
./scripts/with-weston.sh cargo test --workspace --locked -- \
  --include-ignored --test-threads=1
```

CI enforces formatting, Clippy, display-backed tests, LLVM coverage, rustdoc, and
`cargo deny check`. New behavior requires a focused unit test and, for every user-facing GTK
signal or state transition, a display-backed interaction test. Add a regression test for every
fixed persistence or source/rich round-trip bug.

## Releases

- Release from a clean `main` branch with `cargo release patch --execute --no-confirm` for a patch release. The configured release flow updates the shared workspace version, AppStream metadata, and generated Flatpak source manifests, then commits the release, creates the annotated tag, and pushes both. It requires the `flatpak-builder-tools` Cargo and Node generators; without them the release stops before creating a tag.
- After the tag is pushed, create the corresponding GitHub release. Keep its notes concise and user-facing: describe each notable change in a bullet and put its pull-request link inline at the end of that bullet. Do not use a separate pull-request list or include a verification section.

## Testing style

- Keep tests in module-owned files: `foo.rs` uses `foo/tests.rs` (or
  `foo/tests/mod.rs`), while root-level tests use `src/tests/`. Use `src/tests/`
  for cross-module scenarios and shared fixtures. Reserve Cargo's top-level
  `tests/` directory for tests that exercise a crate's public API as an external
  consumer.
- Name tests as behavior: `action_should_result_when_condition`.
- Keep unit tests focused on one behavior; share fixtures, not multi-purpose scenarios.
- Test public SDK/storage behavior from outside its implementation where practical.
- Keep GTK tests deterministic: one GTK initialization thread, explicit signal emission,
  and no real user directories or network resources.
- Use temporary XDG/config/data paths in tests. Assert timestamps and revisions explicitly for
  no-op saves, and assert the persisted TOML value for user preferences such as editor mode and
  source split view.
