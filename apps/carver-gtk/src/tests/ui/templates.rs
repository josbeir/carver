//! Display-backed native template management and creation.
use super::*;
use crate::mvu::{AppMsg, NavigationMsg, TemplatesMsg};
use adw::prelude::*;

fn visible_dialog(
    window: &adw::ApplicationWindow,
    name: &str,
) -> Result<adw::Dialog, Box<dyn std::error::Error>> {
    assert!(
        run_main_context_until(|| window
            .visible_dialog()
            .is_some_and(|d| d.widget_name() == name && d.is_mapped())),
        "dialog {name} should be visible"
    );
    Ok(window.visible_dialog().ok_or("dialog missing")?)
}
fn new_draft(fixture: &WindowFixture) -> Result<adw::Dialog, Box<dyn std::error::Error>> {
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::Manage));
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    widget_as::<gtk::Button>(manager.upcast_ref(), "new-template-button")
        .ok_or("new template")?
        .emit_clicked();
    visible_dialog(&fixture.window, "template-editor-dialog")
}
fn write_draft(dialog: &adw::Dialog, name: &str, source: &str) -> TestResult {
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "template-name-entry")
        .ok_or("template name")?
        .set_text(name);
    widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("template source")?
        .buffer()
        .set_text(source);
    Ok(())
}
fn save_draft(dialog: &adw::Dialog) -> TestResult {
    let save = widget_as::<gtk::Button>(dialog.upcast_ref(), "template-save-button")
        .ok_or("template save")?;
    assert!(save.is_sensitive());
    save.emit_clicked();
    Ok(())
}

pub(super) fn templates_should_manage_validate_duplicate_and_delete() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplateManagerTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let draft = new_draft(&fixture)?;
    write_draft(&draft, "Meeting", "![Photo](assets/photo.png)")?;
    save_draft(&draft)?;
    let error =
        widget_as::<gtk::Label>(draft.upcast_ref(), "template-error").ok_or("template error")?;
    assert!(run_main_context_until(|| error.is_visible()));
    assert!(error.text().contains("managed"));
    write_draft(&draft, "Meeting", "---\ntype: meeting\n---\n\n# Agenda\n\n")?;
    save_draft(&draft)?;
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    let templates = glib::MainContext::default().block_on(fixture.client.templates_async())?;
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].name, "Meeting");
    let search = widget_as::<gtk::SearchEntry>(manager.upcast_ref(), "templates-search")
        .ok_or("template search")?;
    let row = widget_as::<adw::ActionRow>(
        manager.upcast_ref(),
        &format!("template-row-{}", templates[0].id),
    )
    .ok_or("template row")?;
    search.set_text("not found");
    assert!(run_main_context_until(|| !row.is_visible()));
    search.set_text("meeting");
    assert!(run_main_context_until(|| row.is_visible()));
    row.emit_by_name::<()>("activated", &[]);
    let editor = visible_dialog(&fixture.window, "template-editor-dialog")?;
    assert!(
        !widget_as::<gtk::Button>(editor.upcast_ref(), "template-save-button")
            .ok_or("save")?
            .is_sensitive()
    );
    write_draft(&editor, "Meeting", "# Revised agenda\n")?;
    save_draft(&editor)?;
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    let updated = glib::MainContext::default()
        .block_on(fixture.client.template_async(templates[0].id))?
        .ok_or("updated template")?;
    assert_eq!(updated.revision.0, templates[0].revision.0 + 1);
    let row = widget_as::<adw::ActionRow>(
        manager.upcast_ref(),
        &format!("template-row-{}", updated.id),
    )
    .ok_or("template row")?;
    let menu = find_widget(row.upcast_ref(), "template-actions").ok_or("template actions")?;
    menu.activate_action("template.duplicate", None)?;
    let duplicate = visible_dialog(&fixture.window, "template-editor-dialog")?;
    save_draft(&duplicate)?;
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    let list = glib::MainContext::default().block_on(fixture.client.templates_async())?;
    assert_eq!(list.len(), 2);
    let copy = list.iter().find(|t| t.id != updated.id).ok_or("copy")?;
    let row =
        widget_as::<adw::ActionRow>(manager.upcast_ref(), &format!("template-row-{}", copy.id))
            .ok_or("copy row")?;
    find_widget(row.upcast_ref(), "template-actions")
        .ok_or("actions")?
        .activate_action("template.delete", None)?;
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|d| d.is::<adw::AlertDialog>())));
    let confirmation = fixture
        .window
        .visible_dialog()
        .ok_or("delete confirmation")?
        .downcast::<adw::AlertDialog>()
        .map_err(|_| "alert")?;
    confirmation.emit_by_name_with_details::<()>(
        "response",
        glib::Quark::from_str("delete"),
        &[&"delete"],
    );
    confirmation.close();
    let _ = visible_dialog(&fixture.window, "templates-dialog")?;
    assert!(run_main_context_until(|| glib::MainContext::default()
        .block_on(fixture.client.templates_async())
        .is_ok_and(|t| t.len() == 1)));
    fixture.window.close();
    Ok(())
}

fn assign_default(fixture: &WindowFixture, template_id: carver_sdk::TemplateId) -> TestResult {
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::SelectCategory(Some(
            fixture.category.id,
        ))));
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::EditCategory(
            fixture.category.clone(),
        )));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|d| widget_as::<adw::ComboRow>(
            d.upcast_ref(),
            "category-default-template"
        )
        .is_some())));
    let dialog = fixture.window.visible_dialog().ok_or("category dialog")?;
    let selector = widget_as::<adw::ComboRow>(dialog.upcast_ref(), "category-default-template")
        .ok_or("category template selector")?;
    assert_eq!(selector.selected(), 0);
    selector.set_selected(1);
    let save =
        widget_as::<gtk::Button>(dialog.upcast_ref(), "category-save").ok_or("header Save")?;
    assert!(save.ancestor(adw::HeaderBar::static_type()).is_some());
    let name = widget_as::<adw::EntryRow>(dialog.upcast_ref(), "category-name-entry")
        .ok_or("category name")?;
    let previous_name = name.text();
    name.set_text("  ");
    assert!(!save.is_sensitive());
    name.set_text(&previous_name);
    save.emit_clicked();
    assert!(run_main_context_until(|| fixture
        .client
        .categories()
        .is_ok_and(|c| c.iter().any(
            |c| c.id == fixture.category.id && c.default_template_id == Some(template_id)
        ))));
    Ok(())
}

pub(super) fn category_template_should_seed_notes_and_allow_blank_override() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.CategoryTemplateTests",
        |_, _| Ok(()),
        |config| {
            config.document_properties.enabled = true;
            config.document_properties.entries = vec![carver_config::DocumentProperty {
                key: "author".into(),
                field_type: carver_config::DocumentPropertyType::Text,
                multiple: false,
                value: serde_json::json!("Jos"),
            }];
        },
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let template = glib::MainContext::default().block_on(fixture.client.create_template_async(
        "Meeting".into(),
        "---\ntype: meeting\n---\n\n# Agenda\n".into(),
    ))?;
    assign_default(&fixture, template.id)?;
    let before = category_notes(&fixture)?.len();
    fixture
        .window
        .lookup_action("new-note")
        .ok_or("new note action")?
        .activate(None);
    assert!(run_main_context_until(
        || category_notes(&fixture).is_ok_and(|notes| notes.len() == before + 1)
    ));
    let notes = category_notes(&fixture)?;
    let created = notes
        .iter()
        .find(|n| n.title == "Agenda")
        .ok_or("created note")?;
    let note = fixture.client.note(created.id)?.ok_or("note")?;
    assert!(has_text_property(&note.source, "author", "Jos"));
    assert!(has_text_property(&note.source, "type", "meeting"));
    fixture
        .window
        .lookup_action("new-blank-note")
        .ok_or("blank action")?
        .activate(None);
    assert!(run_main_context_until(
        || category_notes(&fixture).is_ok_and(|notes| notes.len() == before + 2)
    ));
    assert!(
        category_notes(&fixture)?
            .iter()
            .any(|n| fixture.client.note(n.id).is_ok_and(|n| n
                .is_some_and(|n| has_text_property(&n.source, "author", "Jos")
                    && !has_text_property(&n.source, "type", "meeting"))))
    );
    fixture
        .window
        .lookup_action("new-from-template")
        .ok_or("picker action")?
        .activate(None);
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    widget_as::<adw::ActionRow>(
        picker.upcast_ref(),
        &format!("template-row-{}", template.id),
    )
    .ok_or("picker row")?
    .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(
        || category_notes(&fixture).is_ok_and(|notes| notes.len() == before + 3)
    ));
    fixture.window.close();
    Ok(())
}

pub(super) fn template_editor_should_confirm_discard() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplateDiscardTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let draft = new_draft(&fixture)?;
    write_draft(&draft, "Draft", "# Unsaved")?;
    draft.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|d| d.is::<adw::AlertDialog>())));
    let confirmation = fixture
        .window
        .visible_dialog()
        .ok_or("discard")?
        .downcast::<adw::AlertDialog>()
        .map_err(|_| "alert")?;
    confirmation.emit_by_name_with_details::<()>(
        "response",
        glib::Quark::from_str("discard"),
        &[&"discard"],
    );
    confirmation.close();
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    assert!(
        glib::MainContext::default()
            .block_on(fixture.client.templates_async())?
            .is_empty()
    );
    manager.close();
    fixture.window.close();
    Ok(())
}

fn category_notes(
    fixture: &WindowFixture,
) -> Result<Vec<carver_sdk::NoteSummary>, Box<dyn std::error::Error>> {
    Ok(fixture
        .client
        .recent_notes(
            Some(fixture.category.id),
            carver_sdk::PageRequest {
                limit: 100,
                offset: 0,
            },
        )?
        .items)
}

fn dismiss_initial_dialogs(window: &adw::ApplicationWindow) {
    let dialogs = window.dialogs();
    let initial: Vec<_> = (0..dialogs.n_items())
        .filter_map(|i| dialogs.item(i))
        .filter_map(|d| d.downcast::<adw::Dialog>().ok())
        .collect();
    for dialog in initial {
        dialog.force_close();
    }
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));
}

fn has_text_property(source: &str, key: &str, value: &str) -> bool {
    carver_domain::parse_frontmatter_document(source).is_some_and(|d| {
        d.fields
            .iter()
            .any(|f| f.key == key && f.value == carver_domain::FrontmatterValue::Text(value.into()))
    })
}

pub(super) fn saving_as_template_should_copy_unsaved_editor_source() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.TemplateCopyTests",
        |_, _| Ok(()),
        |config| {
            config.editor.autosave_delay_ms = 60_000;
            config.editor.last_mode = carver_config::EditorMode::Source;
        },
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let note = fixture.client.create_note(fixture.category.id)?;
    let note = fixture
        .client
        .save_note(note.id, note.revision, "# Original\n")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok_and(|v| v
        .buffer()
        .text(&v.buffer().start_iter(), &v.buffer().end_iter(), true)
        .as_str()
        == note.source)));
    fixture
        .source()?
        .buffer()
        .set_text("# Unsaved draft\n\n## Agenda\n");
    fixture
        .window
        .lookup_action("save-as-template")
        .ok_or("save as template action")?
        .activate(None);
    let dialog = visible_dialog(&fixture.window, "template-editor-dialog")?;
    let source = widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("template source")?;
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            true
        ),
        "# Unsaved draft\n\n## Agenda\n"
    );
    save_draft(&dialog)?;
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    let templates = glib::MainContext::default().block_on(fixture.client.templates_async())?;
    assert_eq!(templates[0].source, "# Unsaved draft\n\n## Agenda\n");
    assert_eq!(
        fixture.client.note(note.id)?.ok_or("original note")?.source,
        "# Original\n"
    );
    manager.close();
    fixture.window.close();
    Ok(())
}
