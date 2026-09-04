# MVU refactor plan

## Purpose

Refactor `apps/carver-gtk` to a complete Model-View-Update architecture while preserving the workspace dependency direction, GTK main-thread rules, canonical Carve editing, managed assets, and the existing SQLite SDK boundary.

GTK and WebKit remain imperative rendering adapters. The application model, messages, transitions, and effects remain UI-neutral.

```text
GTK / WebKit event
        |
        v
      AppMsg
        |
        v
update(&mut AppModel, AppMsg) -> Vec<Effect>
        |                              |
        v                              v
render(ViewRefs, snapshot)      GLib effect runner
                                       |
                                       v
                              completion AppMsg
```

Do not introduce Tokio. GLib remains the UI executor and `carver-sdk` retains the dedicated, serialized SQLite worker.

## Workspace-wide test layout

Apply this policy to every crate.

```text
<crate>/
|- src/
|  |- lib.rs / main.rs
|  |- feature.rs
|  |- feature/
|  |  `- tests.rs        # private unit tests for feature.rs
|  `- tests/
|     |- mod.rs          # private cross-module tests
|     `- support.rs
|- tests/
|  |- public_api.rs      # external-consumer integration tests
|  `- common/
|     `- mod.rs
`- test-data/            # only stable checked-in fixtures
```

- Module-owned tests live beside private implementation: `foo.rs` uses `foo/tests.rs` or `foo/tests/mod.rs`.
- Root `src/tests/` is for private cross-module scenarios and shared fixtures.
- Top-level `tests/` is reserved for public APIs and is compiled by Cargo as an external consumer.
- `test-data/` is created only for stable, readable fixtures. SQLite libraries, XDG directories, and mutable asset fixtures use `tempfile`.
- Do not widen production visibility solely to support a test.

### Per-crate application

| Crate | Module/private tests | External-consumer tests |
| --- | --- | --- |
| `carver-domain` | Derivation and canonicalization | Domain type/API contracts |
| `carver-config` | TOML parsing, defaults, migrations | XDG/config public API |
| `carver-editor-protocol` | Serialization details | Host protocol round trips |
| `carver-sdk` | Worker lifecycle and errors | `LibraryClient` client contract |
| `carver-storage-sqlite` | SQL mapping and migration internals | `SqliteLibrary` persistence contract |
| `carver-gtk` | MVU reducer and Weston UI scenarios in `src/tests/` | Binary smoke tests using `CARGO_BIN_EXE_carver-gtk` |

`carver-gtk` is a binary, so its top-level integration tests are black-box process tests. Internal GTK types and MVU state remain private and are tested under `src/tests/`.

## Target GTK architecture

Create the following layout.

```text
apps/carver-gtk/src/
|- mvu/
|  |- mod.rs
|  |- model.rs
|  |- msg.rs
|  |- effect.rs
|  |- update.rs
|  |- runtime.rs
|  `- tests/
`- view/
   |- mod.rs
   |- browser.rs
   |- sidebar.rs
   |- trash.rs
   |- editor.rs
   `- dialogs.rs
```

The completed architecture has no `AppState`, no direct `refresh_browser`, `refresh_sidebar`, or `refresh_trash` calls, and no GTK or WebKit objects in the application model.

### Core types

`AppModel` owns route, selected category, browser/sidebar/trash resources, preferences, notices, and the active editor session. `ViewRefs` owns GTK and WebKit references only.

Use nested messages to avoid an oversized, unrelated message enum.

```rust
enum AppMsg {
    Navigation(NavigationMsg),
    Browser(BrowserMsg),
    Sidebar(SidebarMsg),
    Trash(TrashMsg),
    Editor(EditorMsg),
    Preferences(PreferencesMsg),
    Library(LibraryReply),
}
```

Use small `Copy` newtypes for `RequestId`, `EditorSessionId`, and `TimerId`. Every asynchronous completion contains the identity that created it; the reducer ignores stale completions.

Use these reusable model concepts rather than feature-specific Boolean flags:

- `LoadState<T>`: `Idle`, `Loading(RequestId)`, `Ready(T)`, or `Failed(UiError)`.
- One resource-reload policy: at most one active and one requested reload.
- A typed `UiError` conversion from SDK/config errors to user-visible status content or toasts.
- One centralized invalidation mapping after mutations.

Do not over-generalize widgets: browser rows, categories, and trash rows keep separate render functions because their presentation rules differ.

### Editor state

The model owns canonical Carve source. Source `TextBuffer` and rich WebKit editor are projections of it.

```text
Clean -> Dirty -> Saving -> Clean
                 |
                 +- source changed: one follow-up save
                 |
                 `- save failure: Failed -> retry -> Dirty
```

Back, autosave, retry, note changes, and trash all use this single coordinator. Save completions apply only when note ID, expected revision, and editor session still match. This prevents stale autosaves from reopening a trashed/old note and prevents Back from creating a second conflicting save.

## Development parts

Each part is independently reviewable and must preserve behavior unless it is explicitly correcting one of the audited failures.

### Part 1: Test-structure baseline

- Create public-API test entry points for each library crate.
- Keep private tests module-owned under `src`.
- Split the GTK monolithic interaction scenario into focused Weston tests.
- Add integration-test `common` helpers only when sharing external fixtures.
- Add `test-data/` only for real stable fixtures.

**Exit criteria:** no production visibility is widened for tests; every crate has an external-contract test location; existing tests retain their behavior.

### Part 2: Domain and storage category summaries

- Add `CategorySummary { category, note_count }` to `carver-domain`.
- Implement one SQLite `LEFT JOIN ... GROUP BY` category/count query.
- Add module-owned mapping/query tests and external storage API tests.

**Exit criteria:** category counts need one request, exclude trashed content, and retain current display semantics.

### Part 3: SDK request contract

- Add `categories_with_note_counts` to `LibraryBackend` and `LibraryClient`.
- Replace the SDK's unbounded job queue with bounded admission.
- Preserve the typed `LibraryError` contract.
- Test backpressure and worker/backend failure propagation.

**Exit criteria:** excessive UI work awaits capacity rather than allocating unlimited queued jobs; no interactive GTK signal uses synchronous SDK calls.

### Part 4: Pure MVU foundation

- Add `AppModel`, `Route`, `LoadState<T>`, IDs, `UiError`, `AppMsg`, and pure `Effect` values.
- Implement the pure `update` reducer and module-owned reducer tests.

**Exit criteria:** reducer tests need neither GTK, SQLite, nor a display; stale replies are ignored and load failures are modelled explicitly.

### Part 5: Runtime and rendering shell

- Add `AppRuntime`, dispatcher, effect execution, snapshots, and `ViewRefs`.
- Dispatch in this order: update, release model borrow, render, then execute effects.
- Add a narrow rendering guard for programmatic GTK changes.

**Exit criteria:** `glib::spawn_future_local` is limited to effect execution; GTK callbacks only convert widget input into `AppMsg`; callbacks are never called while holding a `RefCell` borrow.

### Part 6: Browser and sidebar migration

- Migrate selection, search, titles, empty/error states, and note list loading to resources in the model.
- Debounce search through model timer IDs.
- Coalesce repeated reloads.
- Render sidebar from `CategorySummary`.

**Required tests:** category selection reloads browser data; stale search results are ignored; failed loads render errors; repeated reloads are coalesced.

### Part 7: Trash and mutation invalidation

- Migrate trash resource/loading/error states.
- Define the central invalidation mapping for create, rename, move, trash, restore, undo, and empty-trash outcomes.
- Replace scattered refresh fan-out calls.

**Required tests:** restore/trash updates the right resources; empty-trash errors are visible; Undo restores its prior category and triggers one refresh per dependent resource.

### Part 8a: Category actions and trash admission

- Move category creation, rename, category trash, and browser note trash into typed messages and effects.
- Add action state to block duplicate rapid-click operations.
- Keep dialogs view-only: collect input and dispatch.

**Required tests:** duplicate actions do not duplicate mutations; invalid names surface errors; failed mutations do not invalidate successful resource data.

### Part 8b: Note moves and Undo

- Move note relocation and Undo into typed messages and effects.
- Keep the Undo affordance model-driven and route it through a window action rather than a business callback in a view.
- Move editor deletion to the same action pipeline when the editor model is introduced in Parts 9-10.

**Required tests:** move and Undo each invalidate dependent resources once; duplicate actions do not duplicate mutations; a failed Undo preserves the retryable move state.

### Part 9a: Pure source commands

- Refactor source commands into pure source/selection transforms.
- Retain a small GTK `TextBuffer` adapter for rendering and cursor handling.

**Required tests:** formatting transforms are pure module tests; GTK adapters preserve source and selection behavior.

### Part 9b: Canonical editor document model

- Add `EditorDocument`: session, persisted note/revision, canonical source, mode, and dirty/save state.
- Translate source changes to `EditorMsg::SourceChanged`.
- Make rich/source round trips use the canonical source as their only shared representation.

**Required tests:** rich/source round trips preserve canonical Carve; unsupported rich constructs remain source-preserved and preview-only.

### Part 10a: Editor save coordinator

- Add pure save-state transitions and revision/session-identified save effects.
- Model one in-flight save and one coalesced follow-up source snapshot.
- Preserve dirty source on save failures and expose a typed retry message.

**Required tests:** stale save completions are ignored; source changed while saving causes exactly one follow-up save; failures never discard source.

### Part 10b: Editor save UI handoff

- Replace `save_in_flight`, autosave generations, and direct save paths with the coordinator.
- Invalidate the editor session before returning, changing, or trashing notes.
- Surface save failures with a retry action.

**Required tests:** an old autosave cannot replace a new/trashed note; Back during autosave causes no revision conflict; save failure preserves source and offers retry.

### Part 11a.1: Editor load lifecycle

- Move note-card activation behind a typed `OpenNote` effect with request identity, so a stale
  completion cannot open the wrong note.
- Hydrate editor projections from the canonical MVU document rather than from a stack-notify
  callback and controller-owned `current_note`.

**Required tests:** activating a note with no legacy current-note value opens and hydrates the
correct document; an older open completion is ignored.

### Part 11a.2: Editor adapter dispatch boundary

- Give GTK/WebKit editor adapters a runtime-owned message dispatcher rather than access to
  `AppState` or the SDK.
- Move editor widget references into `ViewRefs` so model snapshots, not stack-notify callbacks,
  hydrate canonical source and the selected editor mode.
- Preserve the source/rich selection and scroll mechanics as view-only details.

**Required tests:** entering and leaving an editor hydrates the canonical document once; stale
editor widget events are ignored; a programmatic model render cannot dispatch a source change.

### Part 11b: Rich editor and managed-asset effects

- Translate WebKit `EditorEvent` values into typed editor messages.
- Use typed effects for managed image storage and route the completion back to the correct editor
  session before the view inserts the resulting canonical image markup.
- Keep WebKit selection and scrolling as view-adapter details only.

**Required tests:** rich paste stores a managed asset and canonical markup; stale WebKit events
are ignored; toolbar parity remains covered.

### Part 11c: Preview effects

- Model preview reload intent and debounce identity; let the view render the latest canonical
  source into source-split and read-only previews.
- Route remote-image policy and theme changes through messages/effects without a direct editor
  callback registry.

**Required tests:** preview honors remote-image policy; superseded preview work cannot render
an old source; source/rich/preview transitions preserve canonical source.

### Part 12a: Preferences and startup migration

- Route preference and window changes through messages/effects with visible persistence failures.
- Migrate startup/default category creation to runtime initialization.

**Required tests:** persisted editor and image preferences round-trip through effects and failure
states remain visible.

### Part 12b: Remove legacy controller paths

- Delete `AppState`, legacy controller helpers, direct refresh calls, and widget callback registries.
- Keep `main.rs` bootstrap-only.

**Exit criteria:** the model contains no GTK/WebKit objects; views contain no business state;
effects own SDK/config persistence; no direct SDK/config call or legacy refresh fallback remains in
a GTK/WebKit callback; dependency direction stays inward.

## Verification

Run these after every part.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace --locked
./scripts/with-weston.sh cargo test --workspace --locked -- --include-ignored --test-threads=1
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
git diff --check
```

Run the coverage gate after Parts 6, 10, and 12, and before merge.

```sh
./scripts/with-weston.sh cargo llvm-cov --workspace --all-features --locked --fail-under-lines 80 -- --include-ignored --test-threads=1
```

Every new reducer transition needs a focused module-owned unit test. Every user-facing GTK signal or state transition needs a focused Weston interaction test. Every library behavior promised to external callers needs an integration test under that crate's top-level `tests/` directory.
