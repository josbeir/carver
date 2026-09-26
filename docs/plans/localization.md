# Plan: Localize Carver (GNOME/GTK/Rust gettext)

Localize the Carver GTK application with GNU gettext following standard GNOME,
GTK, and Rust practice: `gettext-rs` for lookup, `formatx` for runtime template
formatting (GNU gettext's recommended Rust approach), `po/` catalogs committed
to the repo, `.mo` compiled at package time, and translated desktop/metainfo
merged at install time.

## Goal

- All user-visible text in `apps/carver-gtk` comes from gettext catalogs.
- Desktop entry and AppStream metainfo are translatable and installed translated.
- Dates and relative times are locale-aware.
- Catalogs live in `po/` with a stable POT and can move to a translation platform
  later without code or build changes.
- Flatpak, AppImage, and source-tree development all load the right locale dir.

## Decisions (settled)

| Question | Decision |
| --- | --- |
| Text domain | `io.github.josbeir.Carver` (matches app id, Flathub-friendly) |
| Lookup library | `gettext-rs` 0.8, `gettext-system` feature (glibc libintl; no bundled LGPL static link) |
| Runtime formatting | `formatx` 0.4 (`{}` and `{name}`), per GNU gettext manual's Rust guidance |
| Scope | **GTK crate only for the first pass**; library crates and MCP stay English |
| Library-origin text | `carver-sdk`/`carver-storage-sqlite`/`carver-config` detail stays English, wrapped in localized UI context |
| Out-of-scope crates | `carver-export`, `carver-agent-integration`, `carver-mcp` remain English (revisit later) |
| Application name | "Carver" stays untranslated (brand) |
| Seed languages | `nl`, `de`, `fr`, `es`, `it`, `zh_CN` in `po/LINGUAS` (English is the source; no `en.po`) |
| Chinese variant | Simplified (`zh_CN`); Traditional (`zh_TW`) optional later |
| Translation ownership | No external platform; catalogs authored in-repo, and the agent produces the translations |
| Contributor guideline | `AGENTS.md` gains a mandatory localization rule: always account for localization in changes and refresh the gettext catalogs when strings change |

## Non-goals

- No translation of `carver-mcp` tool titles/descriptions or the CLI `about`
  string (agent protocol output should be deterministic and locale-independent).
- No typed error-enum refactor of library crates.
- No changes to the TypeScript editor (GTK owns all editor labels/tooltips; the
  web bundle has no user-facing strings).
- No new runtime dependency on schema generation or network translation services.

---

## Verified toolchain facts (this environment, GNU gettext 1.0)

- `xgettext --language=Rust` understands `gettext`, `ngettext`, `pgettext`,
  `npgettext` by default and marks `{…}` placeholders `rust-format`; `msgfmt -c`
  can then validate translator placeholders.
- `msgfmt --desktop -d po` reads `po/LINGUAS` and emits `Comment[de]=…`,
  `Keywords[de]=…`. It **requires** `po/LINGUAS`.
- `msgfmt --xml -d po --template=…metainfo.xml` locates `metainfo.its`
  automatically from the `.metainfo.xml` filename. `--its` is not accepted.
- `xgettext --its=/usr/share/gettext/its/metainfo.its` (without `--language`)
  extracts metainfo strings.
- `desktop-file-validate` rejects `_Comment` keys, so the source desktop file
  must stay underscore-free; translation is merged into a separate installed file.

---

## 1. Dependencies

Workspace `Cargo.toml`:

```toml
gettext-rs = { version = "0.8", default-features = false, features = ["gettext-system"] }
formatx = "0.4"
```

`apps/carver-gtk/Cargo.toml`:

```toml
gettext-rs.workspace = true
formatx.workspace = true
```

`deny.toml`: no change expected (gettext-rs MIT, gettext-sys bindings MIT,
formatx MIT OR Apache-2.0). Confirm `cargo deny check` still passes.

## 2. i18n module (`apps/carver-gtk/src/i18n.rs`)

```rust
pub const DOMAIN: &str = "io.github.josbeir.Carver";

/// Initializes locale and gettext. Call once, first thing in `main`.
pub fn init() { /* setlocale(LcAll, "") + bindtextdomain + codeset + textdomain */ }

/// Resolves the message catalog directory for system, Flatpak, AppImage, dev.
fn locate_localedir() -> PathBuf { /* CARVER_LOCALEDIR / TEXTDOMAINDIR
    -> FLATPAK_ID or /.flatpak-info => /app/share/locale
    -> /usr/share/locale */ }

/// Formats a translated runtime template, falling back to the untranslated
/// template when a translation has malformed placeholders.
#[macro_export]
macro_rules! tr_fmt {
    ($template:expr $(, $($args:tt)*)?) => {
        formatx::formatx!($template $(, $($args)*)?).unwrap_or_else(|_| $template.into())
    };
}
```

Rules:

- The literal `gettext("…")` must appear at the call site so `xgettext` finds it;
  `tr_fmt!` only wraps formatting, e.g.
  `tr_fmt!(gettext("Moved {count} notes"), count = count)`.
- `gettextrs` results from `bindtextdomain`/`textdomain` are handled (logged or
  ignored), never `unwrap`/`expect` (workspace lint).
- No `setlocale`/bind calls in library crates; init runs in the GTK binary only.

## 3. Bootstrap wiring

- `main.rs`: `mod i18n;` and `i18n::init()` before `app::run()`.
- `app.rs`: leave `glib::set_application_name("Carver")` and the window title as
  the brand; localize `show_startup_error` title and description.

## 4. Catalogs (`po/`)

```
po/
  LINGUAS                 # nl, de, fr, es, it, zh_CN
  POTFILES.in             # non-Rust sources + documentation of coverage
  io.github.josbeir.Carver.pot
  nl.po
  de.po
  fr.po
  es.po
  it.po
  zh_CN.po
```

- Add `*.mo` to `.gitignore`; keep `.pot`/`.po` committed.
- Seed the catalogs with `msginit -i po/io.github.josbeir.Carver.pot -l <lang>
  -o po/<lang>.po --no-translator`, after the POT first exists, then complete the
  `msgstr`s (agent-authored).
- Rust sources are collected by the script with
  `find apps/carver-gtk/src -name '*.rs'`; `POTFILES.in` documents the desktop
  and metainfo inputs and the intent.

## 5. Extraction and compilation scripts

`scripts/update-translations.sh` (developer/CI):

```sh
xgettext --language=Rust --from-code=UTF-8 --add-comments \
  --package-name=Carver -o po/io.github.josbeir.Carver.pot $(find apps/carver-gtk/src -name '*.rs')
xgettext --language=Desktop --from-code=UTF-8 -j -o po/io.github.josbeir.Carver.pot \
  apps/carver-gtk/resources/io.github.josbeir.Carver.desktop
xgettext --its=/usr/share/gettext/its/metainfo.its --from-code=UTF-8 -j \
  -o po/io.github.josbeir.Carver.pot \
  apps/carver-gtk/resources/io.github.josbeir.Carver.metainfo.xml
for po in po/*.po; do msgmerge --update --backup=none "$po" po/io.github.josbeir.Carver.pot; done
```

`scripts/compile-translations.sh <destdir>` (packaging):

```sh
for lang in $(grep -v '^#' po/LINGUAS); do
  install -d "$destdir/locale/$lang/LC_MESSAGES"
  msgfmt --check -c -o "$destdir/locale/$lang/LC_MESSAGES/io.github.josbeir.Carver.mo" "po/$lang.po"
done
msgfmt --desktop --template=apps/carver-gtk/resources/io.github.josbeir.Carver.desktop \
  -d po -o "$destdir/applications/io.github.josbeir.Carver.desktop"
msgfmt --xml --template=apps/carver-gtk/resources/io.github.josbeir.Carver.metainfo.xml \
  -d po -o "$destdir/metainfo/io.github.josbeir.Carver.metainfo.xml"
```

## 6. Packaging

- **Flatpak** (`packaging/flatpak/io.github.josbeir.Carver.json`): replace the
  direct desktop/metainfo `install` lines with
  `sh scripts/compile-translations.sh /app/share`. Verify `msgfmt` exists in the
  `org.gnome.Sdk`; if not, add a `gettext` module to the manifest.
- **AppImage** (`packaging/appimage/build.sh`): run
  `scripts/compile-translations.sh "$app_dir/usr/share"`; pass the merged desktop
  file to `linuxdeploy --desktop-file`. `packaging/appimage/AppRun`: export
  `CARVER_LOCALEDIR="$app_dir/usr/share/locale"`.
- **Source-tree dev** (`scripts/install-dev-assets.sh`): optionally compile
  `.mo` into a dev locale dir and document `CARVER_LOCALEDIR=… cargo run`
  (or `LANGUAGE=de cargo run`).

## 7. CI

`.github/workflows/quality.yml`:

- Install `gettext` alongside the GTK packages.
- Run `msgfmt --check -c --strict` over `po/*.po`.
- Regenerate the POT and fail on `git diff --exit-code po/` (catches strings
  that were changed/added without wrapping or retired msgids).
- Keep `desktop-file-validate`/`appstreamcli validate` on the underscore-free
  source files (release workflow already does this).

## 8. Localization pass (GTK crate)

Wrap strings with `gettext(...)`; convert interpolated user text from `format!`
to `tr_fmt!(gettext("…"), …)`. Order by impact:

1. Shell/startup and shared states: `app.rs`, `view/mod.rs` (status pages,
   empty/loading/error, banners, reload/close-deleted dialogs, toasts).
2. Dialogs/toasts: `ui/dialogs.rs`, `ui/add.rs`, `ui/trash.rs` (delete/restore +
   Undo), `ui/search.rs`.
3. Editors: `ui/editor/toolbar.rs`, `ui/editor/mod.rs`, `find.rs`, `source*.rs`,
   `document_sidebar/*`, `preview.rs`, `formatting.rs`.
4. Sidebar/browser/bases: `ui/sidebar.rs`, `ui/browser.rs`, `ui/bases.rs`,
   `ui/bases/actions.rs`, `ui/bases/field_picker.rs`.
5. Conventions:
   - `pgettext` for ambiguous short strings (e.g. "Notes", "Today").
   - `ngettext` for counts ("1 note" / "{count} notes").
   - `// Translators:` comments above non-obvious strings (`--add-comments`).
   - No concatenation of translated fragments; whole sentences with placeholders.

## 9. Dates and relative times (`ui/browser.rs`)

- Replace `elapsed_label`/`relative_update_time` English with `ngettext` for
  seconds/minutes/hours and a localized "Yesterday"/"Today".
- Replace `format_description!("[month repr:short] …")` with
  `glib::DateTime::format("%b %e")` / `"%b %e, %Y"` (locale-aware month names).
- Keep `NoteDateGroup` classification logic; localize its display labels.

## 10. Docs, contributor workflow, and AGENTS.md guideline

- Add a "Localization" section to `AGENTS.md` so the rule governs all future
  work. It must state that contributors always account for localization:
  - Every user-visible string goes through gettext: `gettext("…")`, `ngettext`
    for counts, `pgettext` for ambiguous short strings, and
    `tr_fmt!(gettext("…"), …)` for interpolated templates. Never concatenate
    user-facing text or feed it through bare `format!`.
  - Whenever a user-facing string is added, changed, or removed, run
    `scripts/update-translations.sh` to regenerate the POT and refresh
    `po/*.po`, then update the maintained translations (`nl`, `de`, `fr`, `es`,
    `it`, `zh_CN`) and commit them in the same change.
  - Never edit `msgid`s in `po/*.po`; fix the English source and regenerate.
  - Localize dates and relative times via `glib::DateTime`/`ngettext`, not
    hardcoded English.
  - MCP and library output stays English by design.
  - CI enforces POT/PO drift and `msgfmt -c`, so a missing catalog update fails
    the build.
- Update the README with the same user-facing summary and the
  `msginit`/`LANGUAGE=xx cargo run` workflow.
- Add `po/README` for translators. Catalogs live in-repo; a platform (Weblate or
  GNOME Damned Lies) can be added later without code or build changes.
- The six catalogs are produced directly in-repo: run
  `scripts/update-translations.sh` and complete the `msgstr`s.

## 11. Tests

- Unit test `locate_localedir()` precedence (env override, Flatpak, default).
- Regression note: tests assert English strings and rely on the C locale
  returning the `msgid`; keep `i18n::init` out of library test paths.
- New user-facing GTK behavior still needs a display-backed interaction test;
  pure `gettext` wrapping does not by itself.
- Verify a translation end-to-end with `LANGUAGE=de` against a fixture catalog
  (manual or an ignored test under `apps/carver-gtk/src/tests/`).

## 12. Validation commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings -D clippy::perf
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
git diff --check
cargo deny check
msgfmt --check -c --strict -o /dev/null po/<lang>.po   # per translation
```

## Risks / watch items

- `setlocale` is process-global; call once in `main` only. Tests run with the C
  locale and keep passing unchanged.
- `formatx` strict formatting can fail on a malformed translation; `tr_fmt!`
  falls back to the untranslated template and `msgfmt -c` catches most cases at
  build time.
- Confirm `msgfmt` availability in the Flatpak SDK and AppImage CI image.
- `msgfmt --desktop`/`--xml` require `po/LINGUAS`; commit it early.
- Text domain contains dots; confirm Flathub/Damned Lies accept
  `io.github.josbeir.Carver.mo`.

## Suggested commit sequence

1. Infrastructure: deps, `i18n.rs`, bootstrap, `po/` skeleton, scripts, CI.
2. Shell/startup + shared states.
3. Dialogs, toasts, trash, search.
4. Editor surfaces.
5. Sidebar, browser, bases.
6. Dates/relative times.
7. Desktop + metainfo packaging.
8. Seed and complete `nl`, `de`, `fr`, `es`, `it`, `zh_CN` catalogs from the POT.
9. Add the `AGENTS.md` localization guideline and README notes.
10. Translator README and final docs.
