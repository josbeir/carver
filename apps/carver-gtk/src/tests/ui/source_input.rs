//! Structural source input goes through real GTK signals, native undo, and MVU rendering.

use super::*;
use crate::mvu::{AppMsg, EditorMsg};
use libadwaita::prelude::AdwApplicationWindowExt;

struct InputFixture {
    window: document_sidebar::SidebarFixture,
    source: sourceview5::View,
    keys: gtk::EventControllerKey,
}

fn source_keys(
    source: &impl IsA<gtk::Widget>,
) -> Result<gtk::EventControllerKey, Box<dyn std::error::Error>> {
    source
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("source-format-shortcuts"))
        .ok_or_else(|| "source keys".into())
}

impl InputFixture {
    fn new(text: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let window = document_sidebar::fixture()?;
        window.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
            note_id: carver_sdk::NoteId::new(),
            revision: carver_sdk::Revision(1),
            source: text.into(),
        }));
        let source =
            widget_as::<sourceview5::View>(&window.surface, "source-editor").ok_or("source")?;
        let keys = source_keys(&source)?;
        source.grab_focus();
        source.buffer().place_cursor(&source.buffer().end_iter());
        Ok(Self {
            window,
            source,
            keys,
        })
    }

    fn reset(&self, text: &str) {
        let buffer = self.source.buffer();
        buffer.set_enable_undo(false);
        buffer.set_text(text);
        buffer.place_cursor(&buffer.end_iter());
        buffer.set_enable_undo(true);
    }

    fn key(&self, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
        self.keys
            .emit_by_name::<bool>("key-pressed", &[&key, &0_u32, &modifiers])
    }

    fn text(&self) -> String {
        let buffer = self.source.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }
}

pub(super) fn enter_should_continue_and_exit_source_lists_with_native_undo() -> TestResult {
    let fixture = InputFixture::new("- café")?;
    let plain = gtk::gdk::ModifierType::empty();
    for (before, after) in [
        ("- café", "- café\n- "),
        ("* [X] task", "* [X] task\n* [ ] "),
        (". item", ". item\n. "),
        ("9) item", "9) item\n10) "),
        ("h. one\ni. two", "h. one\ni. two\nj. "),
        ("iv. one\nv. two", "iv. one\nv. two\nvi. "),
        ("- parent\n  * ", "- parent\n- "),
        ("> - ", "> "),
        ("> > ", "> "),
        ("- ", ""),
    ] {
        fixture.reset(before);
        assert!(fixture.key(gtk::gdk::Key::Return, plain));
        assert_eq!(fixture.text(), after);
        let buffer = fixture.source.buffer();
        assert_eq!(
            buffer.iter_at_mark(&buffer.get_insert()).offset(),
            i32::try_from(after.chars().count())?
        );
        assert_eq!(
            fixture
                .window
                .runtime
                .model()
                .editor
                .ok_or("editor")?
                .source,
            after
        );
        assert!(buffer.can_undo());
        buffer.undo();
        assert_eq!(fixture.text(), before);
        assert_eq!(
            buffer.iter_at_mark(&buffer.get_insert()).offset(),
            i32::try_from(before.chars().count())?
        );
        assert!(!buffer.can_undo(), "one key should create one undo action");
        buffer.redo();
        assert_eq!(fixture.text(), after);
        assert_eq!(
            buffer.iter_at_mark(&buffer.get_insert()).offset(),
            i32::try_from(after.chars().count())?
        );
    }
    fixture.reset("- task");
    assert!(fixture.key(gtk::gdk::Key::KP_Enter, plain));
    assert_eq!(fixture.text(), "- task\n- ");
    fixture.window.window.close();
    Ok(())
}

pub(super) fn tab_should_move_source_subtrees_and_restore_selection() -> TestResult {
    let fixture = InputFixture::new("- first\n- second\n  * child")?;
    let buffer = fixture.source.buffer();
    fixture.reset("- first\n- second\n  * child");
    buffer.place_cursor(&buffer.iter_at_offset(16));
    assert!(fixture.key(gtk::gdk::Key::Tab, gtk::gdk::ModifierType::empty()));
    assert_eq!(fixture.text(), "- first\n  - second\n    * child");
    assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).offset(), 18);
    assert!(fixture.key(
        gtk::gdk::Key::ISO_Left_Tab,
        gtk::gdk::ModifierType::SHIFT_MASK
    ));
    assert_eq!(fixture.text(), "- first\n- second\n  * child");
    assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).offset(), 16);
    buffer.undo();
    assert_eq!(fixture.text(), "- first\n  - second\n    * child");
    buffer.undo();
    assert_eq!(fixture.text(), "- first\n- second\n  * child");
    fixture.reset("- first\n- one\n- two\n- last");
    buffer.select_range(&buffer.iter_at_offset(8), &buffer.iter_at_offset(20));
    assert!(fixture.key(gtk::gdk::Key::Tab, gtk::gdk::ModifierType::empty()));
    assert_eq!(fixture.text(), "- first\n  - one\n  - two\n- last");
    let (start, end) = buffer.selection_bounds().ok_or("selection")?;
    assert_eq!((start.offset(), end.offset()), (10, 24));
    fixture.reset("- first\n- ");
    assert!(fixture.key(gtk::gdk::Key::Tab, gtk::gdk::ModifierType::empty()));
    assert_eq!(fixture.text(), "- first\n  - ");
    fixture.reset("- first");
    assert!(fixture.key(gtk::gdk::Key::Tab, gtk::gdk::ModifierType::empty()));
    assert!(
        !buffer.can_undo(),
        "invalid nesting must be a consumed no-op"
    );
    fixture.window.window.close();
    Ok(())
}

pub(super) fn source_input_should_preserve_native_keys_and_ime_composition() -> TestResult {
    let fixture = InputFixture::new("plain")?;
    let plain = gtk::gdk::ModifierType::empty();
    assert_eq!(
        fixture.keys.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    for text in ["plain", "---", "```\n- code", "---\ntitle: note\n---"] {
        fixture.reset(text);
        assert!(!fixture.key(gtk::gdk::Key::Return, plain));
        assert!(!fixture.key(gtk::gdk::Key::Tab, plain));
        assert_eq!(fixture.text(), text);
    }
    fixture.reset("- item");
    for modifiers in [
        gtk::gdk::ModifierType::CONTROL_MASK,
        gtk::gdk::ModifierType::ALT_MASK,
        gtk::gdk::ModifierType::SUPER_MASK,
    ] {
        assert!(!fixture.key(gtk::gdk::Key::Return, modifiers));
    }
    fixture.source.emit_preedit_changed("composing");
    assert!(!fixture.key(gtk::gdk::Key::Return, plain));
    assert!(!fixture.key(gtk::gdk::Key::Tab, plain));
    fixture.source.emit_preedit_changed("");
    assert!(fixture.key(gtk::gdk::Key::Return, gtk::gdk::ModifierType::SHIFT_MASK));
    assert_eq!(fixture.text(), "- item\\\n  ");
    let stack =
        widget_as::<adw::ViewStack>(&fixture.window.surface, "editor-mode-stack").ok_or("mode")?;
    stack.set_visible_child_name("rendered");
    assert!(!fixture.key(gtk::gdk::Key::Return, plain));
    fixture.window.window.close();
    Ok(())
}

pub(super) fn source_list_edits_should_round_trip_and_persist_canonical_content() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.SourceListInput",
        |client, category| {
            let note = client.create_note(category)?;
            let saved = client.save_note(
                note.id,
                note.revision,
                "# Lists\n\n- [ ] parent\n- [x] café",
            )?;
            let past = time::OffsetDateTime::now_utc() - time::Duration::days(1);
            glib::MainContext::default().block_on(client.update_note_timestamps_async(
                saved.id,
                saved.revision,
                past,
                past,
            ))?;
            Ok(())
        },
        |config| config.editor.last_mode = carver_config::EditorMode::Source,
    )?;
    while let Some(dialog) = fixture.window.visible_dialog() {
        dialog.force_close();
    }
    fixture.window.present();
    assert!(run_main_context_until(|| fixture
        .note_list()
        .is_ok_and(|list| !browser_note_ids(&list).is_empty())));
    let id = browser_note_ids(&fixture.note_list()?)[0];
    let before = fixture.client.note(id)?.ok_or("note")?;
    assert!(activate_browser_note(&fixture.note_list()?, id));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    let source = fixture.source()?;
    let buffer = source.buffer();
    buffer.place_cursor(&buffer.end_iter());
    let keys = source_keys(&source)?;
    assert!(keys.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::empty()
        ]
    ));
    buffer.insert_at_cursor("next");
    assert!(keys.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Tab,
            &0_u32,
            &gtk::gdk::ModifierType::empty()
        ]
    ));
    let expected = "# Lists\n\n- [ ] parent\n- [x] café\n  - [ ] next";
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
        expected
    );
    assert!(run_main_context_until(|| fixture
        .client
        .note(id)
        .is_ok_and(
            |note| note.is_some_and(|note| note.source == expected)
        )));
    let saved = fixture.client.note(id)?.ok_or("saved")?;
    assert!(saved.revision.0 > before.revision.0);
    assert!(saved.updated_at > before.updated_at);
    let stack = fixture.editor_mode_stack()?;
    stack.set_visible_child_name("rich");
    let root = fixture.root()?;
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich")?;
    let script = format!(
        "window.carverEditor?.source() === {} && document.querySelectorAll('li').length === 3",
        serde_json::to_string(expected)?
    );
    assert_web_script_should_be_true(&rich, &script);
    stack.set_visible_child_name("rendered");
    stack.set_visible_child_name("source");
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
        expected
    );
    let after = fixture.client.note(id)?.ok_or("saved")?;
    assert_eq!(after.revision, saved.revision);
    assert_eq!(after.updated_at, saved.updated_at);
    assert_note_should_reopen_without_saving(&fixture, &saved)?;
    fixture.window.close();
    Ok(())
}

fn assert_note_should_reopen_without_saving(
    fixture: &WindowFixture,
    saved: &carver_sdk::Note,
) -> TestResult {
    let root = fixture.root()?;
    assert!(
        fixture
            .dispatcher
            .dispatch(AppMsg::Tabs(crate::mvu::TabsMsg::CloseActive))
    );
    assert!(run_main_context_until(|| !note_tab_is_active(&root)));
    assert!(activate_browser_note(&fixture.note_list()?, saved.id));
    assert!(run_main_context_until(|| fixture.source().is_ok_and(
        |source| {
            let buffer = source.buffer();
            buffer.text(&buffer.start_iter(), &buffer.end_iter(), false) == saved.source
        }
    )));
    let reopened = fixture.client.note(saved.id)?.ok_or("reopened")?;
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    Ok(())
}
