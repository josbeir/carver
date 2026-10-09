//! Display-backed command discovery, selection preservation, and dialog handoffs.

use super::*;
use crate::mvu::{AppMsg, BasesMsg, NavigationMsg};
use libadwaita::prelude::AdwApplicationWindowExt;

fn fixture(id: &str) -> Result<(WindowFixture, carver_sdk::Note), Box<dyn std::error::Error>> {
    let fixture = window_fixture_seeded(
        id,
        |client, category| {
            let note = client.create_note(category)?;
            client.save_note(note.id, note.revision, "Hello selected world\n\nbodyneedle")?;
            Ok(())
        },
        |config| config.editor.last_mode = carver_config::EditorMode::Source,
    )?;
    // The shared fixture opens its dialog references for other surface tests.
    while let Some(dialog) = fixture.window.visible_dialog() {
        dialog.force_close();
    }
    fixture.window.present();
    assert!(run_main_context_until(|| fixture
        .note_list()
        .is_ok_and(|list| !browser_note_ids(&list).is_empty())));
    let id = browser_note_ids(&fixture.note_list()?)[0];
    let note = fixture.client.note(id)?.ok_or("note")?;
    Ok((fixture, note))
}

fn palette(window: &adw::ApplicationWindow) -> Result<adw::Dialog, Box<dyn std::error::Error>> {
    gtk::prelude::WidgetExt::activate_action(window, crate::ui::palette::ACTION, None)?;
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    let dialog = window.visible_dialog().ok_or("palette")?;
    assert_eq!(dialog.widget_name(), "command-palette-dialog");
    Ok(dialog)
}

fn row(dialog: &adw::Dialog, title: &str) -> Option<gtk::ListBoxRow> {
    let list = widget_as::<gtk::ListBox>(dialog.upcast_ref(), "palette-results")?;
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        let label = row
            .child()?
            .first_child()?
            .next_sibling()?
            .first_child()?
            .downcast::<gtk::Label>()
            .ok()?;
        if label.text() == title {
            return Some(row);
        }
        index += 1;
    }
    None
}

fn query(dialog: &adw::Dialog, text: &str) -> Result<gtk::SearchEntry, Box<dyn std::error::Error>> {
    let entry = widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "palette-search-entry")
        .ok_or("entry")?;
    assert!(entry.ancestor(adw::HeaderBar::static_type()).is_some());
    entry.set_text(text);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        dialog.upcast_ref(),
        "palette-status"
    )
    .is_some_and(|status| !status.is_visible())));
    Ok(entry)
}

fn command(window: &adw::ApplicationWindow, title: &str) -> TestResult {
    let dialog = palette(window)?;
    let entry = query(&dialog, title)?;
    assert!(row(&dialog, title).is_some(), "{title}");
    entry.emit_by_name::<()>("activate", &[]);
    assert!(
        run_main_context_until(|| window
            .visible_dialog()
            .is_none_or(|dialog| dialog.widget_name() != "command-palette-dialog")),
        "{title}"
    );
    Ok(())
}

pub(super) fn palette_should_search_destinations_and_preserve_note_metadata() -> TestResult {
    let (fixture, before) = fixture("io.github.josbeir.Carver.PaletteNavigation")?;
    let dialog = palette(&fixture.window)?;
    query(&dialog, "bodyneedle")?;
    let result = row(&dialog, "Hello selected world").ok_or("note result")?;
    let labels = result
        .child()
        .and_then(|child| child.first_child())
        .and_then(|icon| icon.next_sibling())
        .ok_or("note labels")?;
    let teaser = labels
        .first_child()
        .and_then(|title| title.next_sibling())
        .and_downcast::<gtk::Label>()
        .ok_or("note teaser")?;
    assert_eq!(teaser.text(), "bodyneedle");
    assert!(teaser.is_single_line_mode());
    assert!(teaser.next_sibling().is_none());
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let after = fixture.client.note(before.id)?.ok_or("note")?;
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.updated_at, before.updated_at);
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Projects")?;
    let destination = row(&dialog, "Projects").ok_or("category result")?;
    widget_as::<gtk::ListBox>(dialog.upcast_ref(), "palette-results")
        .ok_or("results")?
        .emit_by_name::<()>("row-activated", &[&destination]);
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let dialog = palette(&fixture.window)?;
    let entry = query(&dialog, "Hello selected world")?;
    entry.emit_by_name::<()>("activate", &[]);
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    assert_eq!(
        fixture.client.note(before.id)?.ok_or("note")?.revision,
        before.revision
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_should_remove_destinations_when_the_library_changes() -> TestResult {
    let (fixture, _) = fixture("io.github.josbeir.Carver.PaletteCatalogRefresh")?;
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Projects")?;
    assert!(row(&dialog, "Projects").is_some());
    fixture.client.trash_category(fixture.destination.id)?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::LibraryChangedExternally);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        dialog.upcast_ref(),
        "palette-status"
    )
    .is_some_and(|status| status.text() == "No results")));
    assert!(row(&dialog, "Projects").is_none());

    query(&dialog, "Review base")?;
    assert!(row(&dialog, "Review base").is_some());
    glib::MainContext::default().block_on(fixture.client.delete_base_async(fixture.base.id))?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::LibraryChangedExternally);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        dialog.upcast_ref(),
        "palette-status"
    )
    .is_some_and(|status| status.text() == "No results")));
    assert!(row(&dialog, "Review base").is_none());
    assert_eq!(dialog.widget_name(), "command-palette-dialog");
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_should_format_the_original_source_selection_and_restore_focus() -> TestResult
{
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteSource")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    let source = fixture.source()?;
    let buffer = source.buffer();
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("selected")));
    source.grab_focus();
    // A headless compositor may not grant global input focus to the window.
    assert!(run_main_context_until(|| source.is_focus()));
    buffer.select_range(&buffer.iter_at_offset(6), &buffer.iter_at_offset(14));
    assert!(buffer.selection_bounds().is_some());
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Bold")?;
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
        note.source
    );
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert!(run_main_context_until(|| source.is_focus()));
    assert_eq!(
        buffer
            .selection_bounds()
            .map(|(a, b)| buffer.text(&a, &b, false).to_string()),
        Some("selected".into())
    );
    command(&fixture.window, "Bold")?;
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("*selected*")));
    assert!(run_main_context_until(|| source.is_focus()));
    command(&fixture.window, "Insert table")?;
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "palette-table-dialog"
        )));
    fixture
        .window
        .visible_dialog()
        .ok_or("table dialog")?
        .close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    command(&fixture.window, "Move note…")?;
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "move-note-dialog"
        )));
    let move_dialog = fixture.window.visible_dialog().ok_or("move dialog")?;
    assert_eq!(move_dialog.widget_name(), "move-note-dialog");
    move_dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_should_preserve_rich_selection_and_keep_preview_read_only() -> TestResult {
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteRich")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    fixture.editor_mode_stack()?.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&fixture.root()?, "rich-editor").ok_or("rich")?;
    rich.grab_focus();
    assert_web_script_should_be_true(
        &rich,
        "(() => { const c = window.carverEditor; const e = c?.editor; return c?.source().includes('Hello selected world') && e.commands.setTextSelection({from:7,to:15}) && e.state.selection.from === 7 && e.state.selection.to === 15; })()",
    );
    command(&fixture.window, "Bold")?;
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().includes('*selected*') === true",
    );
    fixture
        .editor_mode_stack()?
        .set_visible_child_name("rendered");
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Export note…")?;
    assert!(row(&dialog, "Export note…").is_some());
    let entry = widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "palette-search-entry")
        .ok_or("entry")?;
    entry.set_text("Bold");
    assert!(run_main_context_until(|| row(&dialog, "Bold").is_none()));
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    command(&fixture.window, "Find in note")?;
    fixture.find_entry()?.set_text("selected");
    let root = fixture.root()?;
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        &root,
        "editor-find-count"
    )
    .is_some_and(|count| count.text() == "1 match")));
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_should_follow_base_and_trash_context_and_keep_delete_confirmation()
-> TestResult {
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteContexts")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Bases(BasesMsg::Open(fixture.base.id)));
    assert!(run_main_context_until(|| fixture.route_stack().is_ok_and(
        |stack| stack.visible_child_name().as_deref() == Some("base")
    )));
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Configure Base")?;
    assert!(row(&dialog, "Configure Base").is_some());
    assert!(row(&dialog, "New note").is_none());
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    command(&fixture.window, "Delete Base")?;
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "delete-base-confirmation"
        )));
    fixture
        .window
        .visible_dialog()
        .ok_or("confirmation")?
        .close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::ShowTrash));
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Refresh Trash")?;
    assert!(row(&dialog, "Refresh Trash").is_some());
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert!(fixture.client.note(note.id)?.is_some());
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_shortcut_should_work_and_suppress_nested_dialogs() -> TestResult {
    let (fixture, _) = fixture("io.github.josbeir.Carver.PaletteKeyboard")?;
    let shortcut = fixture
        .window
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|object| object.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("command-palette-shortcut"))
        .ok_or("shortcut")?;
    assert!(shortcut.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::P,
            &0_u32,
            &(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK)
        ]
    ));
    let dialog = fixture.window.visible_dialog().ok_or("palette")?;
    query(&dialog, "Preferences")?;
    gtk::prelude::WidgetExt::activate_action(&fixture.window, crate::ui::palette::ACTION, None)?;
    assert_eq!(fixture.window.visible_dialog(), Some(dialog.clone()));
    query(&dialog, "Preferences")?.emit_by_name::<()>("activate", &[]);
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|dialog| dialog.is::<adw::PreferencesDialog>())));
    let preferences = fixture.window.visible_dialog().ok_or("preferences")?;
    gtk::prelude::WidgetExt::activate_action(&fixture.window, crate::ui::palette::ACTION, None)?;
    assert_eq!(fixture.window.visible_dialog(), Some(preferences.clone()));
    preferences.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    fixture.window.set_default_size(360, 700);
    let dialog = palette(&fixture.window)?;
    query(&dialog, "Preferences")?;
    assert!(run_main_context_until(
        || dialog.width() <= fixture.window.width()
    ));
    query(&dialog, "")?;
    let navigation = dialog
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|object| object.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("palette-navigation"))
        .ok_or("palette navigation")?;
    let list =
        widget_as::<gtk::ListBox>(dialog.upcast_ref(), "palette-results").ok_or("results")?;
    assert!(run_main_context_until(|| list.height() > 0));
    for _ in 0..50 {
        assert!(navigation.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gtk::gdk::Key::Down,
                &0_u32,
                &gtk::gdk::ModifierType::empty()
            ]
        ));
    }
    let scroll = widget_as::<gtk::ScrolledWindow>(dialog.upcast_ref(), "palette-scroller")
        .ok_or("scroller")?;
    assert!(
        run_main_context_until(|| scroll.vadjustment().value() > 0.0),
        "selected={:?}, bounds={:?}, adjustment={}/{}, list height={}",
        list.selected_row().map(|row| row.index()),
        list.selected_row()
            .and_then(|row| row.compute_bounds(&list)),
        scroll.vadjustment().upper(),
        scroll.vadjustment().page_size(),
        list.height()
    );
    let selected = list.selected_row().ok_or("selected row")?;
    let bounds = selected.compute_bounds(&list).ok_or("row bounds")?;
    let bottom = f64::from(bounds.y() + bounds.height());
    assert!(bottom <= scroll.vadjustment().value() + scroll.vadjustment().page_size() + 1.0);
    // A viewport relayout can reset the value before publishing the new scroll range.
    let adjustment = scroll.vadjustment();
    adjustment.set_value(0.0);
    assert_eq!(adjustment.value(), 0.0);
    adjustment.emit_by_name::<()>("changed", &[]);
    assert!(adjustment.value() > 0.0);
    assert!(bottom <= adjustment.value() + adjustment.page_size() + 1.0);
    assert!(navigation.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Escape,
            &0_u32,
            &gtk::gdk::ModifierType::empty()
        ]
    ));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    fixture.window.close();
    Ok(())
}

pub(super) fn palette_icons_should_use_shared_glyphs_and_bundled_task_icon() -> TestResult {
    for (icon, glyph) in [
        ("format-text-highlight-symbolic", "H"),
        ("format-text-superscript-symbolic", "Aˣ"),
        ("format-text-subscript-symbolic", "Aₓ"),
    ] {
        assert_eq!(crate::ui::icons::formatting_glyph(icon), Some(glyph));
        let widget = crate::ui::icons::palette_icon(icon);
        assert!(widget.is::<gtk::Label>() || crate::ui::icons::available(icon));
    }
    assert!(crate::ui::icons::available("carver-list-todo-symbolic"));
    let fallback = crate::ui::icons::palette_icon("carver-nonexistent-test-symbolic")
        .downcast::<gtk::Image>()
        .map_err(|_| "fallback image")?;
    assert_eq!(
        fallback.icon_name().as_deref(),
        Some("document-edit-symbolic")
    );
    Ok(())
}

pub(super) fn quote_should_toggle_from_the_palette_and_toolbar_in_both_editable_modes() -> TestResult
{
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteQuote")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    let source = fixture.source()?;
    let buffer = source.buffer();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == note.source));
    source.grab_focus();
    buffer.select_range(&buffer.iter_at_offset(6), &buffer.iter_at_offset(14));
    command(&fixture.window, "Block quote")?;
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .starts_with("> Hello selected world")));
    let root = fixture.root()?;
    let toolbar = widget_as::<gtk::Box>(&root, "formatting-toolbar-desktop").ok_or("toolbar")?;
    let quote = widget_as::<gtk::ToggleButton>(toolbar.upcast_ref(), "format-quote-button")
        .ok_or("quote button")?;
    assert!(run_main_context_until(|| quote.is_active()));
    quote.emit_clicked();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == note.source
        && !quote.is_active()));
    fixture.editor_mode_stack()?.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich")?;
    rich.grab_focus();
    assert_web_script_should_be_true(
        &rich,
        "(() => { const c = window.carverEditor; return c?.source().includes('Hello selected world') && c.editor.commands.setTextSelection({from:7,to:15}); })()",
    );
    command(&fixture.window, "Block quote")?;
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().startsWith('> Hello selected world') === true",
    );
    assert!(run_main_context_until(|| quote.is_active()));
    quote.emit_clicked();
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().startsWith('Hello selected world') === true",
    );
    assert!(run_main_context_until(|| !quote.is_active()));
    fixture
        .editor_mode_stack()?
        .set_visible_child_name("source");
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .trim()
        == note.source));
    fixture
        .editor_mode_stack()?
        .set_visible_child_name("rendered");
    assert!(!toolbar.is_sensitive());
    fixture.window.close();
    Ok(())
}

pub(super) fn quote_should_unwrap_lazy_continuations_from_palette_and_toolbar() -> TestResult {
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteLazyQuote")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    let source = fixture.source()?;
    let buffer = source.buffer();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == note.source));
    source.grab_focus();
    buffer.set_text("> first\ncontinued");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    command(&fixture.window, "Block quote")?;
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == "first\ncontinued"));

    let root = fixture.root()?;
    let toolbar = widget_as::<gtk::Box>(&root, "formatting-toolbar-desktop").ok_or("toolbar")?;
    let quote = widget_as::<gtk::ToggleButton>(toolbar.upcast_ref(), "format-quote-button")
        .ok_or("quote button")?;
    buffer.set_text("> > first\n> continued\n> > last");
    buffer.place_cursor(&buffer.iter_at_offset(5));
    assert!(run_main_context_until(|| quote.is_active()));
    quote.emit_clicked();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == "> first\n> continued\n> last"));

    buffer.set_text("> Hélène\n続き");
    buffer.place_cursor(&buffer.iter_at_offset(10));
    assert!(run_main_context_until(|| quote.is_active()));
    quote.emit_clicked();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        false
    ) == "Hélène\n続き"
        && !quote.is_active()));
    fixture.editor_mode_stack()?.set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "window.carverEditor?.source().trim() === 'Hélène\\n続き' && !document.querySelector('.ProseMirror blockquote')",
    );
    fixture
        .editor_mode_stack()?
        .set_visible_child_name("source");
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .trim()
        == "Hélène\n続き"));
    fixture.window.close();
    Ok(())
}

fn finish_palette_matches(model: &mut crate::mvu::AppModel, effects: Vec<crate::mvu::Effect>) {
    for effect in effects {
        if let crate::mvu::Effect::MatchPalette {
            id,
            request,
            candidates,
            query,
        } = effect
        {
            let rows = crate::mvu::palette::match_rows(candidates, &query);
            let _ = crate::mvu::update(
                model,
                AppMsg::Palette(crate::mvu::palette::PaletteMsg::Matched { id, request, rows }),
            );
        }
    }
}

pub(super) fn palette_should_refresh_visible_commands_after_async_editor_replies() -> TestResult {
    use crate::mvu::{
        AppModel, EditorMsg, LibraryReply, UiError,
        palette::{CommandId, CommandLabel, PaletteMsg},
    };
    let (fixture, note) = fixture("io.github.josbeir.Carver.PaletteEditorReplies")?;
    let mut model = AppModel::new(&fixture.config);
    let _ = crate::mvu::update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: note.id,
            revision: note.revision,
            source: note.source,
        }),
    );
    model.editor.as_mut().ok_or("editor")?.pending_assets = 1;
    let _ = crate::mvu::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Changed text".into())),
    );
    let _ = crate::mvu::update(&mut model, AppMsg::Editor(EditorMsg::RetrySave));
    let crate::mvu::EditorSaveState::Saving(request) =
        model.editor.as_ref().ok_or("editor")?.save_state.clone()
    else {
        return Err("pending save".into());
    };
    let effects = crate::mvu::update(
        &mut model,
        AppMsg::Palette(PaletteMsg::Opened {
            labels: [
                (CommandId::Format(crate::mvu::FormatCommand::Bold), "Bold"),
                (CommandId::RetrySave, "Retry save"),
            ]
            .into_iter()
            .map(|(command, title)| CommandLabel {
                command,
                title: title.into(),
                aliases: String::new(),
                icon: "document-edit-symbolic".into(),
                shortcut: String::new(),
            })
            .collect(),
            source_selection: 0..0,
        }),
    );
    finish_palette_matches(&mut model, effects);
    let view = crate::ui::palette::PaletteView::default();
    let dispatcher = crate::mvu::AppDispatcher::default();
    view.render(&fixture.window, &dispatcher, &model);
    let dialog = fixture.window.visible_dialog().ok_or("palette")?;
    assert!(row(&dialog, "Bold").is_some_and(|row| !row.is_sensitive()));
    assert!(row(&dialog, "Retry save").is_none());

    let effects = crate::mvu::update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: request.clone(),
            move_error: None,
            result: Err(UiError::new("offline")),
        }),
    );
    finish_palette_matches(&mut model, effects);
    view.render(&fixture.window, &dispatcher, &model);
    assert!(row(&dialog, "Retry save").is_some_and(|row| row.is_sensitive()));

    let effects = crate::mvu::update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session: request.session,
            alt: String::new(),
            source_target: None,
            result: Ok("assets/image.png".into()),
        }),
    );
    finish_palette_matches(&mut model, effects);
    view.render(&fixture.window, &dispatcher, &model);
    assert!(row(&dialog, "Bold").is_some_and(|row| row.is_sensitive()));
    assert_eq!(fixture.window.visible_dialog(), Some(dialog.clone()));
    dialog.force_close();
    fixture.window.close();
    Ok(())
}
