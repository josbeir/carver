# Carver contributor guide

Carver is a Rust workspace for a native GNOME note-taking application using GTK4,
Libadwaita, SQLite, and the Carve markup crate.

## Workspace map

- `crates/carver-domain`: markup-derived domain entities and pure transformations.
- `crates/carver-config`: XDG locations and TOML configuration.
- `crates/carver-storage-sqlite`: SQLite schema, migrations, search, and managed assets.
- `crates/carver-sdk`: UI-neutral facade for current and future native frontends.
- `apps/carver-gtk`: GTK4/Libadwaita application. Keep UI code local to this crate.

Dependencies must point inward: GTK calls the SDK; the SDK calls storage; storage and
configuration use domain types. Domain must not depend on infrastructure or GTK.

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
- Keep persistence and business actions in typed helpers/SDK methods. GTK callbacks only
  gather widget input, invoke an action, refresh views, and surface an error/toast.

## UI and persistence rules

- Follow GNOME patterns: `AdwNavigationSplitView` for responsive navigation,
  `AdwClamp` for readable content widths, standard `AdwHeaderBar` menu placement, and
  modal dialogs transient to their parent window.
- Notes are soft-deleted. User-facing deletion must offer Undo and must not remove assets
  directly. Add restore/trash UI before adding permanent deletion.
- New and renamed categories require a non-empty, trimmed user-entered name.
- The rich editor writes canonical Carve source; source mode is the direct representation.
  Formatting controls must keep the two buffers synchronized.
- Settings live at `$XDG_CONFIG_HOME/carver/config.toml`; the SQLite library and managed
  assets live at `$XDG_DATA_HOME/carver/`. Preserve these XDG boundaries.

## Required checks

Run these before handing off a change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

GTK signal tests require a display and are marked ignored locally. Run the full suite
serially under a display server:

```sh
xvfb-run -a cargo test --workspace -- --include-ignored --test-threads=1
```

CI enforces formatting, Clippy, Xvfb-backed tests, 80% line coverage, rustdoc, and
dependency policy. New behavior requires a focused unit test and, for every user-facing
GTK signal, a display-backed interaction test.

## Testing style

- Name tests as behavior: `action_should_result_when_condition`.
- Keep unit tests focused on one behavior; share fixtures, not multi-purpose scenarios.
- Test public SDK/storage behavior from outside its implementation where practical.
- Keep GTK tests deterministic: one GTK initialization thread, explicit signal emission,
  and no real user directories or network resources.
