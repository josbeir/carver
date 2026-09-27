//! Opt-in capture of the application views used as screenshots on the docs site.
//!
//! This is a developer tool, not an interaction scenario: it only runs when
//! `CARVER_SCREENSHOT_DIR` is set, and returns immediately otherwise, so the
//! normal display-backed suite and CI (which run the ignored tests without that
//! variable) are unaffected. Drive it through `scripts/capture-screenshots.sh`.
//!
//! Each view is captured in both themes at 2x with `gtk::WidgetPaintable`, so
//! the output is deterministic in size, theme, and content.

use std::path::{Path, PathBuf};
use std::time::Duration;

use libadwaita::prelude::*;

use crate::ui::tests::support::TestLibraryClient;

use super::*;

/// Logical window size for every capture; large enough for the desktop layout.
const WINDOW_WIDTH: i32 = 1280;
const WINDOW_HEIGHT: i32 = 800;
/// Device scale for the captured PNGs.
const SCALE: f32 = 2.0;

#[test]
#[ignore = "captures docs screenshots; set CARVER_SCREENSHOT_DIR and run under a display"]
fn capture_docs_screenshots() -> TestResult {
    let Some(directory) = std::env::var_os("CARVER_SCREENSHOT_DIR").map(PathBuf::from) else {
        return Ok(());
    };

    gtk::disable_portals();
    glib::set_application_name("Carver screenshots");
    gtk::init()?;
    crate::app::load_styles();
    std::fs::create_dir_all(&directory)?;

    let review = std::cell::RefCell::new(None);
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.Screenshots",
        |client, category| {
            *review.borrow_mut() = Some(seed_showcase(client, category)?);
            Ok(())
        },
        // Screenshots should show the application's default document font, not
        // the deterministic font the interaction suite configures.
        |_| {},
    )?;
    let review = review.into_inner().ok_or("review note")?;
    capture_scenes(&fixture, &directory, &review)
}

/// Seeds a small, neutral library so every view has consistent content.
///
/// Notes are back-dated so the browser's relative timestamps and grouping look
/// natural instead of all reading "just now".
fn seed_showcase(
    client: &TestLibraryClient,
    category: carver_sdk::CategoryId,
) -> Result<carver_sdk::NoteId, Box<dyn std::error::Error>> {
    let review = seed_note(
        client,
        category,
        "---\nstatus: In progress\n---\n\n# Weekly review\n\n\
         A calm week is a sequence of small, finished things.\n\n\
         - [x] Triage the inbox\n\
         - [ ] Draft the release notes\n\
         - [ ] Review the Base columns\n\n\
         | Day | Focus   |\n| --- | ------- |\n| Mon | Writing |\n| Tue | Reviews |\n\n\
         > Fewer, sharper tasks.\n",
        0,
    )?;

    for (days, status, title, body) in [
        (
            1,
            "Planned",
            "Release checklist",
            "Cut the bundle, refresh the manifests, and verify the checksum before tagging.",
        ),
        (
            3,
            "Done",
            "Meeting notes",
            "Decisions from the design review, with the follow-ups we agreed to write down.",
        ),
        (
            8,
            "In progress",
            "Ideas backlog",
            "Rough ideas worth keeping, so they stop living only in a chat transcript.",
        ),
        (
            16,
            "Done",
            "Reading list",
            "Papers and posts to come back to when there is a quiet hour.",
        ),
    ] {
        seed_note(
            client,
            category,
            &format!("---\nstatus: {status}\n---\n\n# {title}\n\n{body}\n"),
            days,
        )?;
    }

    let trashed = seed_note(
        client,
        category,
        "---\nstatus: Done\n---\n\n# Old draft\n\nSuperseded by the weekly review.\n",
        24,
    )?;
    client.trash_note(trashed.id)?;

    Ok(review.id)
}

/// Creates one note and back-dates both timestamps by `days_ago`.
fn seed_note(
    client: &TestLibraryClient,
    category: carver_sdk::CategoryId,
    source: &str,
    days_ago: i64,
) -> Result<carver_sdk::Note, Box<dyn std::error::Error>> {
    let note = client.create_note_with_source(category, source)?;
    let timestamp = time::OffsetDateTime::now_utc() - time::Duration::days(days_ago);
    let saved = glib::MainContext::default().block_on(client.update_note_timestamps_async(
        note.id,
        note.revision,
        timestamp,
        timestamp,
    ))?;
    Ok(saved)
}

fn capture_scenes(
    fixture: &WindowFixture,
    directory: &Path,
    review: &carver_sdk::NoteId,
) -> TestResult {
    let window = fixture.window.clone();
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let route_stack = fixture.route_stack()?;
    let editor_stack = fixture.editor_mode_stack()?;

    // The fixture presents its dialogs eagerly; clear them so native views are visible.
    close_all_dialogs(&window);

    window.set_default_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    assert!(run_main_context_until(
        || window.is_mapped() && window.width() >= WINDOW_WIDTH
    ));

    // Library: the browser over the seeded notes, which the initial load already has.
    assert!(sidebar_select(&sidebar, "all-notes-count"));
    let note_list = fixture.note_list()?;
    assert!(
        run_main_context_until(|| note_list.model().is_some_and(|model| model.n_items() >= 4)),
        "note list items: {}",
        note_list.model().map_or(0, |model| model.n_items())
    );
    capture_theme_pair(fixture, directory, "library")?;

    // Rich editor: open the review note and wait for the web surface.
    assert!(activate_browser_note(&note_list, *review));
    assert!(run_main_context_until(|| route_stack
        .visible_child_name()
        .as_deref()
        == Some("editor")));
    editor_stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(&rich, "document.body.innerText.includes('Weekly review')");
    capture_theme_pair(fixture, directory, "editor")?;

    // Carve source with the live preview split.
    editor_stack.set_visible_child_name("source");
    let split =
        widget_as::<gtk::ToggleButton>(&root, "source-split-toggle").ok_or("split toggle")?;
    split.set_active(true);
    let split_preview =
        widget_as::<webkit6::WebView>(&root, "source-split-preview").ok_or("split preview")?;
    assert_web_script_should_be_true(&split_preview, "document.body.innerText.length > 0");
    capture_theme_pair(fixture, directory, "source")?;

    // Saved Base grid.
    assert!(sidebar_select(
        &sidebar,
        &format!("base-count:{}", fixture.base.id)
    ));
    let bases_grid = widget_as::<gtk::ColumnView>(&root, "bases-grid").ok_or("bases grid")?;
    assert!(run_main_context_until(|| bases_grid
        .model()
        .is_some_and(|model| model.n_items() >= 3)));
    capture_theme_pair(fixture, directory, "bases")?;

    // Agent setup dialog.
    let agent = crate::ui::dialogs::show_agent_setup_dialog_for_test(&window);
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    settle();
    capture_dialog_theme_pair(&agent, directory, "agent")?;
    agent.close();

    // Preferences dialog.
    let preferences = fixture.preferences_dialog.clone();
    preferences.present(Some(&window));
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    settle();
    capture_dialog_theme_pair(&preferences, directory, "settings")?;
    preferences.close();

    // Leave the app in the light theme.
    apply_color_scheme(false);
    Ok(())
}

/// Captures a full window in both themes.
fn capture_theme_pair(fixture: &WindowFixture, directory: &Path, name: &str) -> TestResult {
    let window = fixture.window.clone().upcast::<gtk::Widget>();
    for (suffix, dark) in [("light", false), ("dark", true)] {
        apply_color_scheme(dark);
        settle();
        capture_widget(&window, &directory.join(format!("{name}-{suffix}.png")))?;
    }
    Ok(())
}

/// Captures a presented dialog's content in both themes.
fn capture_dialog_theme_pair(
    dialog: &adw::PreferencesDialog,
    directory: &Path,
    name: &str,
) -> TestResult {
    let content = dialog.child().ok_or("dialog content")?;
    for (suffix, dark) in [("light", false), ("dark", true)] {
        apply_color_scheme(dark);
        settle();
        capture_widget(&content, &directory.join(format!("{name}-{suffix}.png")))?;
    }
    Ok(())
}

/// Renders a widget to a PNG at `SCALE`, matching the on-screen presentation.
fn capture_widget(widget: &gtk::Widget, path: &Path) -> TestResult {
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(SCALE, SCALE);
    paintable.snapshot(
        &snapshot,
        f64::from(widget.width()),
        f64::from(widget.height()),
    );
    let node = snapshot.to_node().ok_or("snapshot node")?;
    let renderer = widget
        .native()
        .and_then(|native| native.renderer())
        .ok_or("renderer")?;
    renderer.render_texture(&node, None).save_to_png(path)?;
    Ok(())
}

/// Forces the light or dark presentation. The app derives the web editor theme
/// from `AdwStyleManager`, so this also drives the embedded editor and preview.
fn apply_color_scheme(dark: bool) {
    adw::StyleManager::default().set_color_scheme(if dark {
        adw::ColorScheme::ForceDark
    } else {
        adw::ColorScheme::ForceLight
    });
}

/// Runs the loop long enough for layout, animations, and web renders to settle.
fn settle() {
    let _ = run_main_context_until_for(Duration::from_millis(600), || false);
}

/// Closes every dialog the fixture left presented.
fn close_all_dialogs(window: &adw::ApplicationWindow) {
    while let Some(dialog) = window.visible_dialog() {
        dialog.close();
        let _ = run_main_context_until_for(Duration::from_millis(50), || {
            window.visible_dialog().is_none()
        });
    }
}
