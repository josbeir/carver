//! Pause/resume regressions through the native buffer and the real `WebKit` editor.

use super::*;
use crate::mvu::{AppMsg, EditorMsg, EditorSaveState};

pub(super) fn autosave_should_defer_hidden_browser_rows_and_preserve_external_refreshes()
-> TestResult {
    let (fixture, note_id) = monitored_fixture()?;
    let root = fixture.root()?;
    let list = fixture.note_list()?;
    assert!(run_main_context_until(|| list
        .model()
        .is_some_and(|model| model.n_items() > 0)));
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(crate::mvu::NavigationMsg::OpenNote {
            note_id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    assert!(run_main_context_until(
        || note_tab_is_active(&root) && !note_tab_is_loading(&root)
    ));
    let source = fixture.source()?;
    let buffer = source.buffer();
    let model = list.model().ok_or("browser model")?;
    let changes = Rc::new(Cell::new(0));
    let changes_for_signal = Rc::clone(&changes);
    model.connect_items_changed(move |_, _, _, _| {
        changes_for_signal.set(changes_for_signal.get() + 1);
    });
    buffer.place_cursor(&buffer.end_iter());
    buffer.insert_at_cursor(" source edit");
    assert!(run_main_context_until(|| fixture
        .client
        .note(note_id)
        .is_ok_and(
            |note| note.is_some_and(|note| note.source.ends_with(" source edit"))
        )));
    let stack = fixture.editor_mode_stack()?;
    stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().endsWith(' source edit') && window.carverEditor.editor.commands.focus('end') && window.carverEditor.editor.commands.insertContent(' rich edit')",
    );
    assert!(run_main_context_until(|| fixture
        .client
        .note(note_id)
        .is_ok_and(
            |note| note.is_some_and(|note| note.source.ends_with(" rich edit"))
        )));
    // A separate writer must still be noticed while the Notes list is hidden.
    let saved = fixture.client.note(note_id)?.ok_or("saved note")?;
    fixture.client.save_note(
        note_id,
        saved.revision,
        "# Externally updated\n\nExternal content",
    )?;
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().endsWith('External content')",
    );
    assert_eq!(
        changes.get(),
        0,
        "hidden cards should not be rebuilt by autosave or monitor replies"
    );
    let tabs = widget_as::<adw::TabView>(&root, "workspace-tabs").ok_or("tabs")?;
    tabs.set_selected_page(&tabs.nth_page(0));
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        &root,
        &format!("note-title:{note_id}")
    )
    .is_some_and(|label| label.text() == "Externally updated")));
    assert!(
        changes.get() > 0,
        "returning to Notes should project the latest library data"
    );
    fixture.window.destroy();
    Ok(())
}

fn monitored_fixture() -> Result<(WindowFixture, carver_sdk::NoteId), Box<dyn std::error::Error>> {
    let seeded = Rc::new(Cell::new(None));
    let seeded_for_library = Rc::clone(&seeded);
    let mut source_text = String::new();
    for i in 0..60 {
        use std::fmt::Write;
        write!(
            source_text,
            "## Section {i}\n\nA paragraph with **bold** and *italic* text.\n\n- First item\n- [ ] Second item\n\n> Quoted paragraph\n\n"
        )?;
    }
    source_text.push_str("\nEnd");
    let fixture = window_fixture_seeded_with_monitor(
        "io.github.josbeir.Carver.MonitoredTyping",
        |client, category| {
            for i in 0..50 {
                let note = client.create_note(category)?;
                client.save_note(
                    note.id,
                    note.revision,
                    &format!("# Other {i}\n\nBackground note."),
                )?;
            }
            let note = client.create_note(category)?;
            let note = client.save_note(note.id, note.revision, &source_text)?;
            seeded_for_library.set(Some(note.id));
            Ok(())
        },
        |config| {
            config.editor.last_mode = carver_config::EditorMode::Source;
            config.editor.autosave_delay_ms = 500;
            config.editor.show_document_sidebar = true;
        },
        true,
    )?;
    let note_id = seeded.get().ok_or("seeded note")?;
    Ok((fixture, note_id))
}

fn fixture()
-> Result<(document_sidebar::SidebarFixture, carver_sdk::Note), Box<dyn std::error::Error>> {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Typing")?;
    let created = fixture.client.create_note(category.id)?;
    let note = fixture
        .client
        .save_note(created.id, created.revision, "Seed")?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: note.source.clone(),
    }));
    Ok((fixture, note))
}

fn saved(fixture: &document_sidebar::SidebarFixture, note: &carver_sdk::Note, suffix: &str) {
    assert!(run_main_context_until(|| {
        fixture.runtime.model().editor.is_some_and(|document| {
            document.source.ends_with(suffix) && document.save_state == EditorSaveState::Clean
        }) && fixture
            .client
            .note(note.id)
            .is_ok_and(|note| note.is_some_and(|note| note.source.ends_with(suffix)))
    }));
}

fn pause_after(timestamp: time::OffsetDateTime) {
    // SQLite timestamps have second precision; cross that boundary before the next material edit.
    assert!(run_main_context_until(|| time::OffsetDateTime::now_utc()
        .unix_timestamp()
        > timestamp.unix_timestamp()));
}

pub(super) fn source_typing_should_debounce_and_resume_after_autosave() -> TestResult {
    let (fixture, note) = fixture()?;
    let source =
        widget_as::<sourceview5::View>(&fixture.surface, "source-editor").ok_or("source")?;
    let buffer = source.buffer();
    buffer.place_cursor(&buffer.end_iter());
    pause_after(note.updated_at);
    let before = fixture.runtime.render_count();
    for _ in 0..20 {
        buffer.insert_at_cursor("a");
    }
    assert_eq!(fixture.runtime.render_count() - before, 20);
    assert_eq!(fixture.runtime.pending_editor_timer_count(), 2);
    saved(&fixture, &note, &"a".repeat(20));
    let first = fixture.client.note(note.id)?.ok_or("saved note")?;
    assert_eq!(first.revision, carver_sdk::Revision(note.revision.0 + 1));
    assert!(first.updated_at > note.updated_at);
    pause_after(first.updated_at);
    let before = fixture.runtime.render_count();
    buffer.insert_at_cursor(" resumed");
    assert_eq!(fixture.runtime.render_count() - before, 1);
    assert_eq!(buffer.iter_at_mark(&buffer.get_insert()), buffer.end_iter());
    assert!(buffer.can_undo());
    saved(&fixture, &note, " resumed");
    let second = fixture.client.note(note.id)?.ok_or("resumed note")?;
    assert_eq!(second.revision, carver_sdk::Revision(first.revision.0 + 1));
    assert!(second.updated_at > first.updated_at);
    buffer.undo();
    assert!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .ends_with(&"a".repeat(20))
    );
    fixture.window.destroy();
    Ok(())
}

pub(super) fn rich_typing_should_resume_after_autosave_without_reloading() -> TestResult {
    let (fixture, note) = fixture()?;
    let stack =
        widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack").ok_or("modes")?;
    stack.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(&rich, "window.carverEditor?.source() === 'Seed'");
    pause_after(note.updated_at);
    assert_web_script_should_be_true(
        &rich,
        "(() => { const e=window.carverEditor.editor; e.commands.focus('end'); window.typingEditor=e; window.typingLoads=0; const load=window.carverEditor.load.bind(window.carverEditor); window.carverEditor.load=(...args)=>{ window.typingLoads++; return load(...args); }; for(let i=0;i<20;i++) e.commands.insertContent('a'); return true; })()",
    );
    saved(&fixture, &note, &"a".repeat(20));
    let first = fixture.client.note(note.id)?.ok_or("saved note")?;
    assert_eq!(first.revision, carver_sdk::Revision(note.revision.0 + 1));
    assert!(first.updated_at > note.updated_at);
    pause_after(first.updated_at);
    assert_web_script_should_be_true(
        &rich,
        "(() => { const e=window.carverEditor.editor; return e===window.typingEditor && window.typingLoads===0 && e.commands.insertContent(' resumed') && e.state.selection.from===e.state.doc.content.size-1; })()",
    );
    saved(&fixture, &note, " resumed");
    let second = fixture.client.note(note.id)?.ok_or("resumed note")?;
    assert_eq!(second.revision, carver_sdk::Revision(first.revision.0 + 1));
    assert!(second.updated_at > first.updated_at);
    assert_web_script_should_be_true(
        &rich,
        "window.typingLoads===0 && window.carverEditor.editor.commands.undo() && !window.carverEditor.source().includes(' resumed')",
    );
    let ghost =
        widget_as::<gtk::Label>(&fixture.surface, "source-marker-placeholder").ok_or("ghost")?;
    assert!(!ghost.is_visible());
    fixture.window.destroy();
    Ok(())
}
