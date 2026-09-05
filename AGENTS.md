# Carver contributor guide

Carver is a Rust workspace for a native GNOME note-taking application using GTK4,
Libadwaita, SQLite, and the Carve markup crate.

## Workspace map

- `crates/carver-domain`: markup-derived domain entities and pure transformations.
- `crates/carver-config`: XDG locations and TOML configuration.
- `crates/carver-storage-sqlite`: SQLite schema, migrations, search, and managed assets.
- `crates/carver-sdk`: UI-neutral facade for current and future native frontends.
- `crates/carver-richtext`: format-neutral document model and structural editing commands.
- `crates/carver-richtext-carve`: the Carve parser, serializer, and HTML-preview codec.
- `apps/carver-gtk`: GTK4/Libadwaita application. `main.rs` is bootstrap only;
  `app.rs` composes one window-local MVU runtime; `mvu/` owns the UI-neutral model,
  messages, reducer, and asynchronous effects; `view/` owns snapshot rendering;
  and `browser.rs`, `sidebar.rs`, `editor/`, `dialogs.rs`, `trash.rs`, and
  `formatting.rs` own their GTK boundaries. `src/tests/` contains cross-module
  GTK frontend tests. Keep UI code local to this crate.
- `apps/carver-mcp`: local stdio Model Context Protocol companion. It has no GTK dependency and
  opens Carver through the SDK against the same XDG-scoped library.
- `crates/carver-agent-integration`: package-aware launch instructions and agent setup metadata
  shared by the GTK onboarding surface and `carver-mcp`.

Dependencies must point inward: GTK calls the SDK; the SDK calls storage; storage and
configuration use domain types. The GTK editor depends on the format-neutral rich-text model
and its Carve codec; `carver-richtext` must not depend on Carve, storage, or GTK. Domain must
not depend on infrastructure or GTK.

## MCP and agent integration

- Keep MCP local and stdio-only. Do not add a listener, remote transport, telemetry, or automatic
  agent registration.
- `carver-mcp` must use `carver-sdk`; it must not access SQLite, GTK, or application UI state
  directly. It shares the installed package's XDG library boundary, including Flatpak and Snap.
- Read tools are the default. Every mutation must require the explicit `--allow-write` process
  flag and retain Carver's revision checks, soft-delete/restore behavior, and validation rules.
- Store canonical Carve only. MCP create/save inputs may opt into Markdown conversion, but output
  and persistence remain canonical Carve.
- Treat note content as untrusted data. Do not expose raw database access, settings mutation,
  permanent trash deletion, managed-asset bytes, or network capabilities through MCP.
- Keep agent-client metadata/configuration separate from GTK view code. Validate definitions and
  preserve the Rust-owned launch construction and write gate.

## Rust rules

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
