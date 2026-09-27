//! Opt-in capture of the application views used as screenshots on the docs site.
//!
//! This is a developer tool rather than an interaction scenario: it is called
//! from the display-backed orchestrator (so the suite keeps a single GTK entry
//! point) but does nothing unless `CARVER_SCREENSHOT_DIR` is set. Run it through
//! `scripts/capture-screenshots.sh`, which sets that variable.
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

/// The review note is the richest document, used by the editor and source scenes.
const WEEKLY_REVIEW: &str = "---\nstatus: In progress\ndone: false\ndue: 2026-10-02\n---\n\n\
    # Weekly review\n\n\
    A calm week is a sequence of small, finished things.\n\n\
    ## What moved forward\n\n\
    - [x] Triage the inbox\n\
    - [x] Ship the import fix\n\
    - [ ] Draft the release notes\n\
    - [ ] Review the Base columns\n\n\
    ## Carried over\n\n\
    - [ ] Answer the design review thread\n\
    - [ ] Resize the hero images\n\
      - light and dark variants\n\
      - keep the aspect ratio\n\n\
    ## Focus per day\n\n\
    | Day | Focus   | Notes           |\n\
    | --- | ------- | --------------- |\n\
    | Mon | Writing | draft the post  |\n\
    | Tue | Reviews | clear the queue |\n\
    | Wed | Build   | cut the release |\n\n\
    ## Useful commands\n\n\
    ```sh\n\
    cargo test --workspace --locked\n\
    ```\n\n\
    > Fewer, sharper tasks. Keep the list short enough to finish.\n";

/// Which seeded category a showcase note belongs to.
#[derive(Clone, Copy)]
enum Group {
    Notes,
    Ideas,
    Research,
    Personal,
}

/// One seeded note: its content, frontmatter properties, and age.
struct NoteSpec {
    days: i64,
    group: Group,
    trashed: bool,
    status: &'static str,
    done: bool,
    due: &'static str,
    title: &'static str,
    body: &'static str,
}

/// The showcase library. `status`, `done`, and `due` give the saved Bases text,
/// boolean, and date columns, and `days` back-dates each note so the browser's
/// relative timestamps and grouping look natural.
const SHOWCASE_NOTES: &[NoteSpec] = &[
    NoteSpec {
        days: 1,
        group: Group::Notes,
        trashed: false,
        status: "Planned",
        done: false,
        due: "2026-10-05",
        title: "Release checklist",
        body: "Cut the bundle, refresh the manifests, and verify the checksum before tagging.",
    },
    NoteSpec {
        days: 2,
        group: Group::Notes,
        trashed: false,
        status: "Done",
        done: true,
        due: "2026-09-22",
        title: "Import fix",
        body: "Carve and Markdown imports now keep the original line endings.",
    },
    NoteSpec {
        days: 3,
        group: Group::Notes,
        trashed: false,
        status: "Done",
        done: true,
        due: "2026-09-20",
        title: "Meeting notes",
        body: "Decisions from the design review, with the follow-ups we agreed to write down.",
    },
    NoteSpec {
        days: 5,
        group: Group::Notes,
        trashed: false,
        status: "In progress",
        done: false,
        due: "2026-10-01",
        title: "Printer regression",
        body: "Narrow down why the PDF export shifts the page margins.",
    },
    NoteSpec {
        days: 40,
        group: Group::Notes,
        trashed: true,
        status: "Done",
        done: true,
        due: "2026-09-10",
        title: "Old draft",
        body: "Superseded by newer work.",
    },
    NoteSpec {
        days: 8,
        group: Group::Ideas,
        trashed: false,
        status: "In progress",
        done: false,
        due: "2026-10-12",
        title: "Ideas backlog",
        body: "Rough ideas worth keeping, so they stop living only in a chat transcript.",
    },
    NoteSpec {
        days: 10,
        group: Group::Ideas,
        trashed: false,
        status: "Planned",
        done: false,
        due: "2026-10-18",
        title: "Base redesign",
        body: "Try a denser grid and a clearer property picker.",
    },
    NoteSpec {
        days: 44,
        group: Group::Ideas,
        trashed: true,
        status: "Done",
        done: true,
        due: "2026-09-05",
        title: "Parked idea",
        body: "Revisit if the scope changes.",
    },
    NoteSpec {
        days: 16,
        group: Group::Research,
        trashed: false,
        status: "Done",
        done: true,
        due: "2026-09-28",
        title: "Reading list",
        body: "Papers and posts to come back to when there is a quiet hour.",
    },
    NoteSpec {
        days: 19,
        group: Group::Research,
        trashed: false,
        status: "In progress",
        done: false,
        due: "2026-10-09",
        title: "Carve spec notes",
        body: "Notes written while re-reading the source format specification.",
    },
    NoteSpec {
        days: 24,
        group: Group::Research,
        trashed: false,
        status: "Done",
        done: true,
        due: "2026-09-15",
        title: "Paper highlights",
        body: "Passages worth keeping from the search paper.",
    },
    NoteSpec {
        days: 30,
        group: Group::Personal,
        trashed: false,
        status: "Planned",
        done: false,
        due: "2026-11-02",
        title: "Trip planning",
        body: "Cabin, train times, and a short packing list.",
    },
    NoteSpec {
        days: 33,
        group: Group::Personal,
        trashed: false,
        status: "In progress",
        done: false,
        due: "2026-10-25",
        title: "Home projects",
        body: "Fix the shelf, hang the mirror, sort the cables.",
    },
];

/// The seeded category ids a showcase note can belong to.
#[derive(Clone, Copy)]
struct Groups {
    notes: carver_sdk::CategoryId,
    ideas: carver_sdk::CategoryId,
    research: carver_sdk::CategoryId,
    personal: carver_sdk::CategoryId,
}

impl Groups {
    fn category(self, group: Group) -> carver_sdk::CategoryId {
        match group {
            Group::Notes => self.notes,
            Group::Ideas => self.ideas,
            Group::Research => self.research,
            Group::Personal => self.personal,
        }
    }
}

/// Ids the capture scenes need from the seeded library.
struct Showcase {
    review: carver_sdk::NoteId,
    media: carver_sdk::NoteId,
    focus: carver_sdk::NoteId,
    base: carver_sdk::BaseId,
}

/// A prose note for the distraction-free writing view.
const FOCUS_NOTE: &str = "---\nstatus: Done\n---\n\n\
    # On keeping a notebook\n\n\
    The point of a notebook is not to be tidy. It is to be there when a thought arrives, \
    so the thought has somewhere to land before it drifts off again.\n\n\
    Most entries will never be read twice, and that is fine. Their value was in the writing, \
    and a few of them will turn out to matter far more than they seemed to at the time.\n\n\
    ## What to keep\n\n\
    Keep the awkward first version. Keep the question you could not answer. Keep the small \
    detail that felt important for no clear reason, since those are usually the ones that grow \
    into something later.\n";

/// Captures the docs screenshots when `CARVER_SCREENSHOT_DIR` is set, and does
/// nothing otherwise, so the ignored suite and CI stay unaffected.
///
/// Runs at the end of the orchestrator, which owns GTK initialisation.
pub(super) fn capture_docs_screenshots() -> TestResult {
    let Some(directory) = std::env::var_os("CARVER_SCREENSHOT_DIR").map(PathBuf::from) else {
        return Ok(());
    };
    std::fs::create_dir_all(&directory)?;

    let showcase = std::cell::RefCell::new(None);
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.Screenshots",
        |client, category| {
            *showcase.borrow_mut() = Some(seed_showcase(client, category)?);
            Ok(())
        },
        // Screenshots should show the application's default document font, not
        // the deterministic font the interaction suite configures.
        |_| {},
    )?;
    let showcase = showcase.into_inner().ok_or("showcase")?;
    capture_scenes(&fixture, &directory, &showcase)
}

/// Seeds a small, neutral library so every view has consistent content.
fn seed_showcase(
    client: &TestLibraryClient,
    notes: carver_sdk::CategoryId,
) -> Result<Showcase, Box<dyn std::error::Error>> {
    let groups = Groups {
        notes,
        ideas: seed_category(
            client,
            "Ideas",
            carver_sdk::CategoryIcon::Lightbulb,
            carver_sdk::CategoryColor::Yellow,
        )?,
        research: seed_category(
            client,
            "Research",
            carver_sdk::CategoryIcon::Book,
            carver_sdk::CategoryColor::Teal,
        )?,
        personal: seed_category(
            client,
            "Personal",
            carver_sdk::CategoryIcon::Heart,
            carver_sdk::CategoryColor::Purple,
        )?,
    };

    let review = seed_note(client, groups.notes, WEEKLY_REVIEW, 0)?;

    for spec in SHOWCASE_NOTES {
        let source = note_source(spec.status, spec.done, spec.due, spec.title, spec.body);
        let note = seed_note(client, groups.category(spec.group), &source, spec.days)?;
        if spec.trashed {
            client.trash_note(note.id)?;
        }
    }

    Ok(Showcase {
        review: review.id,
        media: seed_media_note(client, groups.notes)?,
        focus: seed_note(client, groups.notes, FOCUS_NOTE, 6)?.id,
        base: seed_bases(client)?.id,
    })
}

/// Seeds a note with managed image assets so the document sidebar has content.
fn seed_media_note(
    client: &TestLibraryClient,
    category: carver_sdk::CategoryId,
) -> Result<carver_sdk::NoteId, Box<dyn std::error::Error>> {
    let note = client.create_note(category)?;
    let study = client.store_asset(note.id, "png", &sample_png(0x2f8f_7aff, 0x1d5f_8fff)?)?;
    let draft = client.store_asset(note.id, "png", &sample_png(0xe893_5aff, 0x7d63_d8ff)?)?;

    let source = format!(
        "---\nstatus: In progress\n---\n\n# Sketchbook\n\n\
         Two files live beside this note.\n\n\
         ![Colour study]({study})\n\n![Layout draft]({draft})\n"
    );
    let saved = client.save_note(note.id, note.revision, &source)?;
    let timestamp = time::OffsetDateTime::now_utc() - time::Duration::days(4);
    let _ = glib::MainContext::default().block_on(client.update_note_timestamps_async(
        saved.id,
        saved.revision,
        timestamp,
        timestamp,
    ))?;
    Ok(saved.id)
}

/// A small striped PNG, so the media sidebar has something to display.
fn sample_png(base: u32, band: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    const WIDTH: i32 = 640;
    const HEIGHT: i32 = 400;
    const BAND: i32 = 88;

    let image =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, WIDTH, HEIGHT)
            .ok_or("pixbuf")?;
    image.fill(base);

    let stripe =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, WIDTH, BAND)
            .ok_or("stripe")?;
    stripe.fill(band);
    for top in [96, 264] {
        stripe.copy_area(0, 0, WIDTH, BAND, &image, 0, top);
    }

    Ok(image.save_to_bufferv("png", &[])?)
}

/// Creates the saved Bases; the roadmap's text, boolean, and date columns are
/// the ones the site shows.
fn seed_bases(
    client: &TestLibraryClient,
) -> Result<carver_sdk::BaseDefinition, Box<dyn std::error::Error>> {
    let roadmap = glib::MainContext::default().block_on(client.create_base_async(
        "Roadmap".to_owned(),
        vec![
            carver_sdk::BaseColumn::Name,
            carver_sdk::BaseColumn::Category,
            carver_sdk::BaseColumn::Updated,
            carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath("/status".to_owned())),
            carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath("/done".to_owned())),
            carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath("/due".to_owned())),
        ],
    ))?;
    let _ = glib::MainContext::default().block_on(client.create_base_async(
        "Reading list".to_owned(),
        vec![
            carver_sdk::BaseColumn::Name,
            carver_sdk::BaseColumn::Category,
            carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath("/done".to_owned())),
        ],
    ))?;
    Ok(roadmap)
}

/// Creates a category with an explicit icon and accent colour.
fn seed_category(
    client: &TestLibraryClient,
    name: &str,
    icon: carver_sdk::CategoryIcon,
    color: carver_sdk::CategoryColor,
) -> Result<carver_sdk::CategoryId, Box<dyn std::error::Error>> {
    Ok(client
        .create_category_with_appearance(name, carver_sdk::CategoryAppearance { icon, color })?
        .id)
}

/// Builds canonical Carve with the shared showcase frontmatter properties.
fn note_source(status: &str, done: bool, due: &str, title: &str, body: &str) -> String {
    format!("---\nstatus: {status}\ndone: {done}\ndue: {due}\n---\n\n# {title}\n\n{body}\n")
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

fn capture_scenes(fixture: &WindowFixture, directory: &Path, showcase: &Showcase) -> TestResult {
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
        run_main_context_until(|| note_list.model().is_some_and(|model| model.n_items() >= 8)),
        "note list items: {}",
        note_list.model().map_or(0, |model| model.n_items())
    );
    capture_theme_pair(fixture, directory, "library")?;

    // Browser search over the seeded notes.
    let search_bar = widget_as::<gtk::SearchBar>(&root, "note-search-bar").ok_or("search bar")?;
    let search_entry =
        widget_as::<gtk::SearchEntry>(&root, "note-search-entry").ok_or("search entry")?;
    let search_toggle =
        widget_as::<gtk::ToggleButton>(&root, "note-search-toggle").ok_or("search toggle")?;
    search_toggle.set_active(true);
    search_entry.set_text("notes");
    assert!(run_main_context_until(|| search_bar.is_search_mode()));
    assert!(
        run_main_context_until(|| note_list.model().is_some_and(|model| model.n_items() > 0)),
        "search results: {}",
        note_list.model().map_or(0, |model| model.n_items())
    );
    let _ = run_main_context_until_for(Duration::from_millis(300), || false);
    capture_theme_pair(fixture, directory, "search")?;
    search_entry.set_text("");
    search_toggle.set_active(false);
    let _ = run_main_context_until_for(Duration::from_millis(200), || false);

    // Rich editor: open the review note and wait for the web surface.
    assert!(activate_browser_note(&note_list, showcase.review));
    assert!(run_main_context_until(|| route_stack
        .visible_child_name()
        .as_deref()
        == Some("editor")));
    editor_stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(&rich, "document.body.innerText.includes('Weekly review')");
    scroll_web_view_to_top(&rich);
    capture_theme_pair(fixture, directory, "editor")?;

    // Carve source with the live preview split.
    editor_stack.set_visible_child_name("source");
    let split =
        widget_as::<gtk::ToggleButton>(&root, "source-split-toggle").ok_or("split toggle")?;
    split.set_active(true);
    if let Some(scroll) = fixture
        .source()?
        .parent()
        .and_downcast::<gtk::ScrolledWindow>()
    {
        scroll.vadjustment().set_value(0.0);
    }
    let split_preview =
        widget_as::<webkit6::WebView>(&root, "source-split-preview").ok_or("split preview")?;
    assert_web_script_should_be_true(&split_preview, "document.body.innerText.length > 0");
    scroll_web_view_to_top(&split_preview);
    capture_theme_pair(fixture, directory, "source")?;
    capture_media(fixture, directory, showcase.media)?;
    capture_focus(fixture, directory, showcase.focus)?;

    // Saved Base grid with text, boolean, and date columns.
    assert!(sidebar_select(
        &sidebar,
        &format!("base-count:{}", showcase.base)
    ));
    let bases_grid = widget_as::<gtk::ColumnView>(&root, "bases-grid").ok_or("bases grid")?;
    assert!(run_main_context_until(|| bases_grid
        .model()
        .is_some_and(|model| model.n_items() >= 8)));
    capture_theme_pair(fixture, directory, "bases")?;

    // Return to the notes browser so dialogs sit over a familiar surface.
    capture_dialogs(fixture, directory)?;

    // Leave the app in the light theme.
    apply_color_scheme(false);
    Ok(())
}

/// The document sidebar listing a note's managed image assets.
fn capture_media(
    fixture: &WindowFixture,
    directory: &Path,
    note: carver_sdk::NoteId,
) -> TestResult {
    let root = fixture.root()?;
    let editor_stack = fixture.editor_mode_stack()?;
    let split =
        widget_as::<gtk::ToggleButton>(&root, "source-split-toggle").ok_or("split toggle")?;

    assert!(open_note(fixture, note)?);
    editor_stack.set_visible_child_name("source");
    reset_source_scroll(fixture)?;
    split.set_active(false);

    let sidebar_toggle = widget_as::<gtk::ToggleButton>(&root, "editor-document-sidebar-toggle")
        .ok_or("document sidebar toggle")?;
    sidebar_toggle.set_active(true);
    assert!(run_main_context_until(|| find_widget(
        &root,
        "editor-media-item"
    )
    .is_some()));
    capture_theme_pair(fixture, directory, "media")?;

    sidebar_toggle.set_active(false);
    let _ = run_main_context_until_for(Duration::from_millis(200), || false);
    Ok(())
}

/// A distraction-free writing view: no category sidebar, no toolbar, no panels.
fn capture_focus(
    fixture: &WindowFixture,
    directory: &Path,
    note: carver_sdk::NoteId,
) -> TestResult {
    let root = fixture.root()?;
    let editor_stack = fixture.editor_mode_stack()?;
    let navigation = widget_as::<adw::BreakpointBin>(&root, "responsive-navigation-container")
        .and_then(|container| container.child())
        .and_downcast::<adw::NavigationSplitView>()
        .ok_or("navigation split view")?;

    assert!(open_note(fixture, note)?);
    editor_stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(&rich, "document.body.innerText.includes('notebook')");
    scroll_web_view_to_top(&rich);

    // The same result as the preferences and the header toggles, applied straight
    // to the widgets: those controls dispatch through runtimes the display
    // fixture does not wire to the editor.
    let toolbar = widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("formatting toolbar")?;
    toolbar.set_visible(false);
    let sidebar_toggle = widget_as::<gtk::ToggleButton>(&root, "editor-toggle-categories-button")
        .or_else(|| widget_as::<gtk::ToggleButton>(&root, "toggle-categories-button"))
        .ok_or("categories toggle")?;
    sidebar_toggle.set_active(false);
    assert!(run_main_context_until(|| navigation.is_collapsed()));

    capture_theme_pair(fixture, directory, "focus")?;

    sidebar_toggle.set_active(true);
    assert!(run_main_context_until(|| !navigation.is_collapsed()));
    toolbar.set_visible(true);
    Ok(())
}

/// The agent setup and preferences dialogs, over the notes browser.
fn capture_dialogs(fixture: &WindowFixture, directory: &Path) -> TestResult {
    let window = fixture.window.clone();
    let sidebar = fixture.sidebar()?;
    assert!(sidebar_select(&sidebar, "all-notes-count"));
    let _ = run_main_context_until_for(Duration::from_millis(300), || false);

    let agent = crate::ui::dialogs::show_agent_setup_dialog_for_test(&window);
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    settle();
    capture_theme_pair(fixture, directory, "agent")?;
    agent.close();
    let _ = run_main_context_until_for(Duration::from_millis(100), || {
        window.visible_dialog().is_none()
    });

    let preferences = fixture.preferences_dialog.clone();
    preferences.present(Some(&window));
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    settle();
    capture_theme_pair(fixture, directory, "settings")?;
    preferences.close();
    Ok(())
}

/// Selects all notes and opens the given note in the editor.
fn open_note(
    fixture: &WindowFixture,
    note: carver_sdk::NoteId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let sidebar = fixture.sidebar()?;
    let note_list = fixture.note_list()?;
    let route_stack = fixture.route_stack()?;

    Ok(sidebar_select(&sidebar, "all-notes-count")
        && run_main_context_until(|| note_list.model().is_some_and(|model| model.n_items() >= 8))
        && activate_browser_note(&note_list, note)
        && run_main_context_until(|| route_stack.visible_child_name().as_deref() == Some("editor")))
}

/// Scrolls the source editor back to the top of the buffer.
fn reset_source_scroll(fixture: &WindowFixture) -> TestResult {
    if let Some(scroll) = fixture
        .source()?
        .parent()
        .and_downcast::<gtk::ScrolledWindow>()
    {
        scroll.vadjustment().set_value(0.0);
    }
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

/// Scrolls an embedded web view back to the top of its document.
fn scroll_web_view_to_top(view: &webkit6::WebView) {
    assert_web_script_should_be_true(
        view,
        "(() => { \
            let node = document.querySelector('.ProseMirror'); \
            while (node) { node.scrollTop = 0; node = node.parentElement; } \
            window.scrollTo(0, 0); \
            if (document.scrollingElement) { document.scrollingElement.scrollTop = 0; } \
            return true; \
        })()",
    );
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
