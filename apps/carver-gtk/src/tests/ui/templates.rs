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
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.TemplateManagerTests",
        |client, category| {
            client.create_note_with_source(category, "# Existing note")?;
            Ok(())
        },
        |_| {},
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let draft = new_draft(&fixture)?;
    write_draft(&draft, "Meeting", "---\nstatus: draft\n---\n# Content")?;
    assert!(run_main_context_until(|| widget_as::<adw::ActionRow>(
        draft.upcast_ref(),
        "template-property:status"
    )
    .is_some()));
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
    delete_template_and_preserve_browser(&fixture, &manager, copy.id)?;
    fixture.window.close();
    Ok(())
}

fn delete_template_and_preserve_browser(
    fixture: &WindowFixture,
    manager: &adw::Dialog,
    template_id: carver_sdk::TemplateId,
) -> TestResult {
    let row =
        widget_as::<adw::ActionRow>(manager.upcast_ref(), &format!("template-row-{template_id}"))
            .ok_or("copy row")?;
    let browser_model = fixture.note_list()?.model().ok_or("note feed model")?;
    assert!(run_main_context_until(|| browser_model.n_items() > 0));
    let browser_items: Vec<_> = (0..browser_model.n_items())
        .filter_map(|index| browser_model.item(index))
        .collect();
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
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::LibraryChangedExternally);
    let _ = run_main_context_until_for(std::time::Duration::from_millis(350), || false);
    let remaining_items: Vec<_> = (0..browser_model.n_items())
        .filter_map(|index| browser_model.item(index))
        .collect();
    assert_eq!(
        remaining_items, browser_items,
        "deleting a template must preserve the note feed"
    );
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
    widget_as::<gtk::Button>(dialog.upcast_ref(), "category-manage-templates")
        .ok_or("manage from category")?
        .emit_clicked();
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    manager.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|current| current == dialog)));
    assert_eq!(name.text(), previous_name);
    assert_eq!(selector.selected(), 1);
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
    assert!(run_main_context_until(|| widget_as::<adw::SplitButton>(
        fixture.window.upcast_ref(),
        "new-note-button"
    )
    .is_some_and(
        |button| button.label().as_deref() == Some("New Note · Meeting")
    )));
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
    assert_eq!(category_notes(&fixture)?.len(), before + 2);
    let author = widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:author")
        .ok_or("effective default")?;
    assert_eq!(
        author.subtitle().as_deref(),
        Some("From your default properties")
    );
    check_picker_layout(&fixture, &picker)?;
    let own = widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:type")
        .ok_or("template property")?;
    assert_eq!(own.subtitle().as_deref(), Some("From template"));
    widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("explicit create")?
        .emit_clicked();
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

pub(super) fn template_empty_state_should_teach_and_open_a_first_draft() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplateOnboardingTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::Manage));
    let manager = visible_dialog(&fixture.window, "templates-dialog")?;
    widget_as::<gtk::Button>(manager.upcast_ref(), "create-first-template")
        .ok_or("first template action")?
        .emit_clicked();
    let editor = visible_dialog(&fixture.window, "template-editor-dialog")?;
    assert!(
        widget_as::<adw::ExpanderRow>(editor.upcast_ref(), "template-properties-expander")
            .is_some()
    );
    editor.close();
    fixture.window.close();
    Ok(())
}

fn check_picker_layout(fixture: &WindowFixture, picker: &adw::Dialog) -> TestResult {
    let split =
        widget_as::<adw::NavigationSplitView>(picker.upcast_ref(), "template-preview-split")
            .ok_or("preview navigation")?;
    assert!(run_main_context_until(|| split.width() > 600));
    let search = widget_as::<gtk::SearchEntry>(picker.upcast_ref(), "templates-search")
        .ok_or("picker search")?;
    let create = widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("preview create")?;
    let row = widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:type")
        .ok_or("property preview")?;
    assert!(row.is_visible());
    search.set_text("no matching template");
    assert!(run_main_context_until(|| !create.is_sensitive()));
    search.set_text("");
    let list = widget_as::<gtk::ListBox>(picker.upcast_ref(), "template-picker-list")
        .ok_or("picker list")?;
    assert!(run_main_context_until(|| list
        .row_at_index(0)
        .is_some_and(|row| row.is_visible())));
    list.select_row(list.row_at_index(0).as_ref());
    assert!(run_main_context_until(|| create.is_sensitive()));
    let view = widget_as::<webkit6::WebView>(picker.upcast_ref(), "template-content-preview")
        .ok_or("read-only content")?;
    assert!(!view.is_editable());
    assert_web_script_should_be_true(
        &view,
        "document.body.innerText.includes('Agenda') && document.querySelector('h1') !== null",
    );
    super::add::capture_dialog(picker, "template-picker-wide")?;
    fixture.window.set_default_size(420, 720);
    assert!(run_main_context_until(|| split.is_collapsed()));
    assert!(split.width() <= fixture.window.width());
    split.set_show_content(true);
    assert!(run_main_context_until(|| split.shows_content()));
    assert_web_script_should_be_true(
        &view,
        "document.documentElement.scrollWidth <= window.innerWidth && parseFloat(getComputedStyle(document.querySelector('h1')).fontSize) >= 14",
    );
    super::add::capture_dialog(picker, "template-picker-narrow")?;
    Ok(())
}
