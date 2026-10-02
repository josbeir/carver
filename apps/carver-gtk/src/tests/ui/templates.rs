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
    let button = widget_as::<gtk::Button>(manager.upcast_ref(), "new-template-button")
        .ok_or("new template")?;
    let button = if button.is_visible() {
        button
    } else {
        widget_as::<gtk::Button>(manager.upcast_ref(), "create-first-template")
            .ok_or("first template")?
    };
    button.emit_clicked();
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
    assert!(
        !widget_as::<gtk::Button>(draft.upcast_ref(), "template-save-button")
            .ok_or("save")?
            .is_sensitive()
    );
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
    glib::MainContext::default().block_on(
        fixture
            .client
            .create_template_async("Planning".into(), "---\ntype: planning\n---".into()),
    )?;
    assign_default(&fixture, template.id)?;
    assert!(run_main_context_until(|| widget_as::<adw::SplitButton>(
        fixture.window.upcast_ref(),
        "new-note-button"
    )
    .is_some_and(
        |button| button.tooltip_text().as_deref() == Some("New Note · Meeting")
            && button.icon_name().as_deref() == Some("document-new-symbolic")
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
    assert_eq!(
        glib::MainContext::default().block_on(fixture.client.templates_async())?,
        [] as [carver_domain::NoteTemplate; 0]
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

pub(super) fn dismiss_initial_dialogs(window: &adw::ApplicationWindow) {
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
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let templates = glib::MainContext::default().block_on(fixture.client.templates_async())?;
    assert_eq!(templates[0].source, "# Unsaved draft\n\n## Agenda\n");
    assert_eq!(
        fixture.client.note(note.id)?.ok_or("original note")?.source,
        "# Original\n"
    );
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
    assert!(
        !widget_as::<gtk::Button>(manager.upcast_ref(), "new-template-button")
            .ok_or("header action")?
            .is_visible()
    );
    assert!(widget_as::<gtk::SearchEntry>(manager.upcast_ref(), "templates-search").is_none());
    let page = widget_as::<adw::StatusPage>(manager.upcast_ref(), "template-empty-state")
        .ok_or("onboarding page")?;
    assert!(page.has_css_class("compact"));
    assert!(
        page.ancestor(gtk::ScrolledWindow::static_type()).is_none(),
        "status page must own its scrolling without an outer scroller"
    );
    manager.set_content_height(220);
    assert!(run_main_context_until(
        || page.height() > 0 && page.height() < 300
    ));
    super::add::capture_dialog(&manager, "template-empty-short")?;
    widget_as::<gtk::Button>(manager.upcast_ref(), "create-first-template")
        .ok_or("first template action")?
        .emit_clicked();
    let editor = visible_dialog(&fixture.window, "template-editor-dialog")?;
    assert_eq!(
        widget_as::<adw::ActionRow>(editor.upcast_ref(), "template-properties-status")
            .ok_or("empty property guidance")?
            .title(),
        "No properties yet. Add default properties in Preferences."
    );
    assert!(
        widget_as::<adw::ExpanderRow>(editor.upcast_ref(), "template-properties-expander")
            .is_some()
    );
    editor.close();
    let returned_manager = visible_dialog(&fixture.window, "templates-dialog")?;
    returned_manager.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::Pick));
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    assert!(
        widget_as::<adw::NavigationSplitView>(picker.upcast_ref(), "template-preview-split")
            .is_none()
    );
    assert!(widget_as::<gtk::SearchEntry>(picker.upcast_ref(), "templates-search").is_none());
    assert!(widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note").is_none());
    assert!(widget_as::<gtk::Button>(picker.upcast_ref(), "create-first-template").is_some());
    picker.close();
    fixture.window.close();
    Ok(())
}

fn check_picker_layout(fixture: &WindowFixture, picker: &adw::Dialog) -> TestResult {
    assert!(
        widget_as::<webkit6::WebView>(picker.upcast_ref(), "template-content-preview").is_none()
    );
    let selector = widget_as::<adw::ComboRow>(picker.upcast_ref(), "template-picker-choice")
        .ok_or("searchable template choice")?;
    assert!(selector.enables_search());
    let create = widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("explicit create")?;
    selector.set_selected(1);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        picker.upcast_ref(),
        "template-property-value:type"
    )
    .is_some_and(|value| value.text() == "planning")));
    selector.set_selected(0);
    assert!(run_main_context_until(|| create.is_sensitive()));
    fixture.window.set_default_size(420, 720);
    assert!(run_main_context_until(
        || picker.width() < 560 && picker.width() <= fixture.window.width()
    ));
    let scroll = widget_as::<gtk::ScrolledWindow>(picker.upcast_ref(), "template-picker-scroll")
        .ok_or("single scroll body")?;
    assert_eq!(scroll.hscrollbar_policy(), gtk::PolicyType::Never);
    picker.set_content_height(220);
    assert!(run_main_context_until(|| {
        let adjustment = scroll.vadjustment();
        adjustment.upper() > adjustment.page_size()
    }));
    let adjustment = scroll.vadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    assert!(adjustment.value() > 0.0);
    picker.set_content_height(480);
    super::add::capture_dialog(picker, "template-picker-properties")?;
    Ok(())
}

pub(super) fn template_property_values_should_stay_on_one_line_at_all_dialog_widths() -> TestResult
{
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplatePropertyLayoutTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.set_default_size(1000, 900);
    fixture.window.present();
    let editor = new_draft(&fixture)?;
    let date = "2026-09-30T22:07:42+02:00";
    write_draft(
        &editor,
        "Layout",
        "---\ntype: draft\npost_date: '2026-09-30T22:07:42+02:00'\n---",
    )?;
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        editor.upcast_ref(),
        "template-property-value:post_date"
    )
    .is_some()));
    widget_as::<adw::ExpanderRow>(editor.upcast_ref(), "template-properties-expander")
        .ok_or("property disclosure")?
        .set_expanded(true);
    let short = widget_as::<gtk::Label>(editor.upcast_ref(), "template-property-value:type")
        .ok_or("short value")?;
    let timestamp =
        widget_as::<gtk::Label>(editor.upcast_ref(), "template-property-value:post_date")
            .ok_or("date value")?;
    assert!(run_main_context_until(
        || timestamp.is_mapped() && timestamp.width() > 0
    ));
    assert_eq!(short.layout().line_count(), 1);
    assert!(!short.layout().is_ellipsized());
    assert!(!short.is_selectable());
    assert!(!timestamp.is_selectable());
    assert_eq!(timestamp.layout().line_count(), 1);
    assert!(
        !timestamp.layout().is_ellipsized(),
        "a timestamp should fit in a wide dialog"
    );
    assert_eq!(timestamp.tooltip_text().as_deref(), Some(date));
    let body = widget_as::<gtk::ScrolledWindow>(editor.upcast_ref(), "template-editor-scroll")
        .ok_or("editor scrolling fallback")?;
    assert!(run_main_context_until(|| {
        let a = body.vadjustment();
        a.upper() <= a.page_size() + 1.0
    }));
    editor.set_content_width(360);
    fixture.window.set_default_size(420, 720);
    assert!(run_main_context_until(|| editor.width() < 760));
    assert_eq!(short.layout().line_count(), 1);
    assert_eq!(timestamp.layout().line_count(), 1);
    assert_eq!(timestamp.ellipsize(), gtk::pango::EllipsizeMode::End);
    assert_eq!(timestamp.text(), date);
    assert_eq!(timestamp.tooltip_text().as_deref(), Some(date));
    editor.force_close();
    fixture.window.close();
    Ok(())
}

pub(super) fn template_properties_should_scroll_inside_a_bounded_panel_when_many() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplateManyPropertiesTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.set_default_size(1000, 900);
    fixture.window.present();
    let editor = new_draft(&fixture)?;
    let source = (0..30).fold(String::from("---\n"), |mut source, i| {
        use std::fmt::Write;
        let _ = writeln!(source, "field_{i}: value_{i}");
        source
    }) + "---";
    write_draft(&editor, "Many properties", &source)?;
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        editor.upcast_ref(),
        "template-property-value:field_29"
    )
    .is_some()));
    widget_as::<adw::ExpanderRow>(editor.upcast_ref(), "template-properties-expander")
        .ok_or("disclosure")?
        .set_expanded(true);
    let panel = widget_as::<gtk::ScrolledWindow>(editor.upcast_ref(), "template-properties-scroll")
        .ok_or("bounded properties")?;
    assert!(run_main_context_until(|| panel.is_mapped()
        && panel.height() > 0
        && panel.vadjustment().upper()
            > panel.vadjustment().page_size()));
    assert!(panel.height() <= 180);
    let adjustment = panel.vadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    assert!(adjustment.value() > 0.0);
    let body = widget_as::<gtk::ScrolledWindow>(editor.upcast_ref(), "template-editor-scroll")
        .ok_or("outer body")?;
    assert!(run_main_context_until(|| {
        let a = body.vadjustment();
        a.upper() <= a.page_size() + 1.0
    }));
    editor.force_close();
    fixture.window.close();
    Ok(())
}

pub(super) fn control_click_should_choose_template_without_creating_a_note() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.ControlTemplateClickTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let before = fixture
        .client
        .recent_notes(
            None,
            carver_sdk::PageRequest {
                limit: 100,
                offset: 0,
            },
        )?
        .items
        .len();
    let button = adw::SplitButton::new();
    let template_click = std::rc::Rc::new(std::cell::Cell::new(true));
    crate::ui::browser::connect_new_note_activation(&button, &fixture.dispatcher, &template_click);
    // Exercise the actual clicked signal with the Ctrl state recorded by the capture controller.
    button.emit_clicked();
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    assert!(!template_click.get());
    assert_eq!(
        fixture
            .client
            .recent_notes(
                None,
                carver_sdk::PageRequest {
                    limit: 100,
                    offset: 0
                }
            )?
            .items
            .len(),
        before
    );
    picker.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert_eq!(
        fixture
            .client
            .recent_notes(
                None,
                carver_sdk::PageRequest {
                    limit: 100,
                    offset: 0
                }
            )?
            .items
            .len(),
        before
    );
    // Consuming the modifier state leaves the following ordinary click unchanged.
    button.emit_clicked();
    assert!(run_main_context_until(|| fixture
        .client
        .recent_notes(
            None,
            carver_sdk::PageRequest {
                limit: 100,
                offset: 0
            }
        )
        .is_ok_and(|page| page.items.len() == before + 1)));
    fixture.window.close();
    Ok(())
}

pub(super) fn inserting_template_should_preserve_properties_and_undo_in_source() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.InsertTemplateSourceTests",
        |_, _| Ok(()),
        |config| {
            config.editor.last_mode = carver_config::EditorMode::Source;
            config.editor.autosave_delay_ms = 60_000;
            config.document_properties.enabled = true;
            config.document_properties.entries = vec![carver_config::DocumentProperty {
                key: "global_only".into(),
                field_type: carver_config::DocumentPropertyType::Text,
                multiple: false,
                value: serde_json::json!("default"),
            }];
        },
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let original = "---\nstatus: done\n---\n\n# Existing";
    let note = fixture.client.create_note(fixture.category.id)?;
    let note = fixture.client.save_note(note.id, note.revision, original)?;
    glib::MainContext::default().block_on(fixture.client.create_template_async(
        "Meeting".into(),
        "---\nstatus: draft\nkind: meeting\nwhen: '{{date}}'\n---\n\n# Agenda {{time}}\n".into(),
    ))?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok_and(
        |view| view
            .buffer()
            .text(&view.buffer().start_iter(), &view.buffer().end_iter(), true)
            == original
    )));
    let buffer = fixture.source()?.buffer();
    buffer.place_cursor(&buffer.end_iter());
    fixture
        .window
        .lookup_action("insert-template")
        .ok_or("insert action")?
        .activate(None);
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    assert_eq!(picker.title(), "Insert Template");
    assert!(widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:kind").is_some());
    assert!(widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:status").is_none());
    widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("insert confirmation")?
        .emit_clicked();
    assert!(run_main_context_until(|| buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .contains("# Agenda")));
    let inserted = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    assert!(has_text_property(&inserted, "status", "done"), "{inserted}");
    assert!(
        has_text_property(&inserted, "kind", "meeting"),
        "{inserted}"
    );
    assert!(!inserted.contains("global_only"));
    assert!(!inserted.contains("{{"));
    assert!(inserted.contains("# Existing\n\n# Agenda"), "{inserted}");
    buffer.undo();
    assert!(run_main_context_until(|| buffer.text(
        &buffer.start_iter(),
        &buffer.end_iter(),
        true
    ) == original));
    assert_eq!(
        fixture.client.note(note.id)?.ok_or("original note")?.source,
        original
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn inserting_property_only_template_should_undo_in_rich_mode() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.InsertTemplateRichTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let note = fixture.client.create_note(fixture.category.id)?;
    let note = fixture.client.save_note(
        note.id,
        note.revision,
        "---\nstatus: done\n---\n\n# Existing\n",
    )?;
    glib::MainContext::default().block_on(fixture.client.create_template_async(
        "Properties".into(),
        "---\nkind: meeting\nstatus: draft\ncreated: '{{datetime}}'\n---\n".into(),
    ))?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok_and(
        |view| view
            .buffer()
            .text(&view.buffer().start_iter(), &view.buffer().end_iter(), true)
            .contains("Existing")
    )));
    let web =
        widget_as::<webkit6::WebView>(&fixture.root()?, "rich-editor").ok_or("rich editor")?;
    super::window::assert_web_script_should_be_true(
        &web,
        "Boolean(window.carverEditor?.editor?.state?.doc?.textContent?.includes('Existing'))",
    );
    super::window::assert_web_script_should_be_true(
        &web,
        "(() => { let at = 0; window.carverEditor.editor.state.doc.descendants((node, pos) => { if (node.type.name === 'paragraph' && node.textContent === 'Existing') at = pos + 1; }); return window.carverEditor.editor.commands.setTextSelection({from: at, to: at + 8}); })()",
    );
    fixture
        .window
        .lookup_action("insert-template")
        .ok_or("insert action")?
        .activate(None);
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("insert confirmation")?
        .emit_clicked();
    let buffer = fixture.source()?.buffer();
    assert!(run_main_context_until(|| has_text_property(
        &buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "kind",
        "meeting"
    )));
    assert!(has_text_property(
        &buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "status",
        "done"
    ));
    assert!(
        !buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .contains("{{")
    );
    assert!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .contains("created:")
    );
    assert!(run_main_context_until(|| fixture
        .client
        .note(note.id)
        .is_ok_and(|saved| saved.is_some_and(|saved| saved
            .source
            .contains("Existing")
            && has_text_property(&saved.source, "kind", "meeting")))));
    super::window::assert_web_script_should_be_true(&web, "window.carverEditor.command('undo')");
    assert!(run_main_context_until(|| !has_text_property(
        &buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "kind",
        "meeting"
    )));
    assert!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .contains("Existing")
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn template_pattern_menu_should_insert_at_the_cursor_and_show_reference() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.PatternMenuTests",
        |_, _| Ok(()),
        |_| {},
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let dialog = new_draft(&fixture)?;
    let help =
        widget_as::<gtk::Label>(dialog.upcast_ref(), "template-help").ok_or("combined help")?;
    assert!(help.text().contains("Patterns are filled in when used."));
    assert!(help.text().contains("default properties"));
    write_draft(&dialog, "Dynamic", "# Meeting — ")?;
    let view = widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("source")?;
    let buffer = view.buffer();
    buffer.place_cursor(&buffer.end_iter());
    let menu = widget_as::<gtk::MenuButton>(dialog.upcast_ref(), "template-insert-pattern")
        .ok_or("pattern menu")?;
    assert!(menu.popover().is_some_and(|p| p.is::<gtk::PopoverMenu>()));
    menu.activate_action("pattern.date", None)?;
    assert_eq!(menu.menu_model().ok_or("menu model")?.n_items(), 3);
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "# Meeting — {{date}}"
    );
    for (action, token) in [
        ("pattern.time", "{{time}}"),
        ("pattern.datetime", "{{datetime}}"),
        ("pattern.category", "{{category}}"),
    ] {
        menu.activate_action(action, None)?;
        assert!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .ends_with(token)
        );
    }
    menu.activate_action("pattern.reference", None)?;
    let reference = visible_dialog(&fixture.window, "template-pattern-reference-dialog")?;
    reference.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|d| d == dialog)));
    buffer.set_text("{{unknown}}");
    let error = widget_as::<gtk::Label>(dialog.upcast_ref(), "template-error").ok_or("error")?;
    let save =
        widget_as::<gtk::Button>(dialog.upcast_ref(), "template-save-button").ok_or("save")?;
    assert!(run_main_context_until(
        || error.is_visible() && !save.is_sensitive()
    ));
    assert!(error.text().contains("unknown"));
    buffer.set_text("---\nwhen: '{{date}}'\n---\n# {{category}}");
    assert!(run_main_context_until(
        || !error.is_visible() && save.is_sensitive()
    ));
    let value = widget_as::<gtk::Label>(dialog.upcast_ref(), "template-property-value:when")
        .ok_or("resolved value")?;
    assert!(!value.text().contains("{{"));
    save_draft(&dialog)?;
    let _ = visible_dialog(&fixture.window, "templates-dialog")?;
    let template = glib::MainContext::default()
        .block_on(fixture.client.templates_async())?
        .into_iter()
        .next()
        .ok_or("saved template")?;
    assert!(template.source.contains("{{date}}"));
    fixture.window.close();
    Ok(())
}

pub(super) fn custom_template_format_should_preview_and_reject_invalid_formats() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.PatternFormatTests",
        |_, _| Ok(()),
        |_| {},
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let dialog = new_draft(&fixture)?;
    let menu = widget_as::<gtk::MenuButton>(dialog.upcast_ref(), "template-insert-pattern")
        .ok_or("pattern menu")?;
    menu.activate_action("pattern.custom", None)?;
    let formatting = visible_dialog(&fixture.window, "template-pattern-format-dialog")?;
    let entry =
        widget_as::<adw::EntryRow>(formatting.upcast_ref(), "template-pattern-format-entry")
            .ok_or("format")?;
    let insert =
        widget_as::<gtk::Button>(formatting.upcast_ref(), "template-pattern-format-insert")
            .ok_or("insert")?;
    entry.set_text("%Q");
    assert!(!insert.is_sensitive());
    entry.set_text("%H:%M:%S");
    assert!(insert.is_sensitive());
    let example =
        widget_as::<gtk::Label>(formatting.upcast_ref(), "template-pattern-format-example")
            .ok_or("example")?;
    assert_eq!(example.text().len(), 8);
    insert.emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(|d| d == dialog)));
    let buffer = widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("source")?
        .buffer();
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        "{{datetime:%H:%M:%S}}"
    );
    dialog.force_close();
    fixture.window.close();
    Ok(())
}

pub(super) fn category_templates_should_expand_patterns_at_note_creation() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.DynamicCategoryTests",
        |client, category| {
            let template = glib::MainContext::default().block_on(client.create_template_async(
                "Dynamic".into(),
                "---\nwhen: '{{date}}'\n---\n# {{category}} — {{time}}".into(),
            ))?;
            let category = client
                .categories()?
                .into_iter()
                .find(|entry| entry.id == category)
                .ok_or("category")?;
            glib::MainContext::default().block_on(client.update_category_with_template_async(
                category.id,
                category.name,
                category.appearance,
                Some(template.id),
            ))?;
            Ok(())
        },
        |_| {},
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let before = category_notes(&fixture)?.len();
    fixture
        .window
        .lookup_action("new-note")
        .ok_or("new note")?
        .activate(None);
    assert!(run_main_context_until(
        || category_notes(&fixture).is_ok_and(|notes| notes.len() == before + 1)
    ));
    let created = category_notes(&fixture)?
        .into_iter()
        .find(|note| note.title.starts_with(&fixture.category.name))
        .ok_or("created note")?;
    let source = fixture.client.note(created.id)?.ok_or("note")?.source;
    assert!(!source.contains("{{"));
    assert!(carver_domain::parse_frontmatter_document(&source).is_some());
    fixture.window.close();
    Ok(())
}

pub(super) fn property_only_source_insertion_should_preserve_selection_persist_and_undo()
-> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.PropertyOnlySourceRegression",
        |_, _| Ok(()),
        |config| config.editor.last_mode = carver_config::EditorMode::Source,
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let original = "Before café 🦀 SELECT After\n";
    let note = fixture
        .client
        .create_note_with_source(fixture.category.id, original)?;
    glib::MainContext::default().block_on(
        fixture
            .client
            .create_template_async("Properties".into(), "---\nkind: meeting\n---\n".into()),
    )?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok_and(|v| v
        .buffer()
        .text(&v.buffer().start_iter(), &v.buffer().end_iter(), true)
        == original)));
    let buffer = fixture.source()?.buffer();
    let start = i32::try_from("Before café 🦀 ".chars().count())?;
    buffer.select_range(
        &buffer.iter_at_offset(start),
        &buffer.iter_at_offset(start + 6),
    );
    fixture
        .window
        .lookup_action("insert-template")
        .ok_or("insert")?
        .activate(None);
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("confirmation")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .client
        .note(note.id)
        .is_ok_and(|n| n.is_some_and(|n| n.source.ends_with(original)
            && has_text_property(&n.source, "kind", "meeting")
            && n.revision.0 == note.revision.0 + 1))));
    buffer.undo();
    assert!(run_main_context_until(|| fixture
        .client
        .note(note.id)
        .is_ok_and(|n| n.is_some_and(
            |n| n.source == original && n.revision.0 == note.revision.0 + 2
        ))));
    fixture.window.close();
    Ok(())
}

pub(super) fn invalid_insertion_template_should_show_error_and_allow_another_choice() -> TestResult
{
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.InvalidInsertionRegression",
        |client, _| {
            glib::MainContext::default().block_on(
                client.create_template_async("A Invalid".into(), "---\ncount: text\n---\n".into()),
            )?;
            glib::MainContext::default().block_on(
                client.create_template_async("B Valid".into(), "---\ncount: 3\n---\n".into()),
            )?;
            Ok(())
        },
        |config| {
            config.document_properties.entries = vec![carver_config::DocumentProperty {
                key: "count".into(),
                field_type: carver_config::DocumentPropertyType::Number,
                multiple: false,
                value: serde_json::json!(1),
            }];
        },
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let note = fixture
        .client
        .create_note_with_source(fixture.category.id, "Existing")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| fixture.source().is_ok()));
    fixture
        .window
        .lookup_action("insert-template")
        .ok_or("insert")?
        .activate(None);
    let picker = visible_dialog(&fixture.window, "templates-dialog")?;
    let status = widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-properties-status")
        .ok_or("visible error")?;
    widget_as::<adw::ComboRow>(picker.upcast_ref(), "template-picker-choice")
        .ok_or("choice")?
        .set_selected(0);
    assert!(run_main_context_until(
        || status.is_mapped() && status.title().contains("count")
    ));
    let confirm = widget_as::<gtk::Button>(picker.upcast_ref(), "template-create-note")
        .ok_or("confirmation")?;
    assert!(!confirm.is_sensitive());
    widget_as::<adw::ComboRow>(picker.upcast_ref(), "template-picker-choice")
        .ok_or("choice")?
        .set_selected(1);
    assert!(run_main_context_until(|| confirm.is_sensitive()));
    assert!(widget_as::<adw::ActionRow>(picker.upcast_ref(), "template-property:count").is_some());
    picker.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn template_save_conflict_should_preserve_draft_and_restore_controls() -> TestResult {
    let fixture = window_fixture_seeded(
        "io.github.josbeir.Carver.TemplateConflictRegression",
        |client, _| {
            glib::MainContext::default().block_on(
                client.create_template_async("Original".into(), "Original source".into()),
            )?;
            Ok(())
        },
        |_| {},
    )?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let template = glib::MainContext::default()
        .block_on(fixture.client.templates_async())?
        .into_iter()
        .next()
        .ok_or("template")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::Edit(Some(
            template.clone(),
        ))));
    let dialog = visible_dialog(&fixture.window, "template-editor-dialog")?;
    glib::MainContext::default().block_on(fixture.client.save_template_async(
        template.id,
        template.revision,
        "External".into(),
        "External source".into(),
    ))?;
    write_draft(&dialog, "Local", "Local unsaved source")?;
    save_draft(&dialog)?;
    let error = widget_as::<gtk::Label>(dialog.upcast_ref(), "template-error").ok_or("error")?;
    assert!(run_main_context_until(|| error.is_visible()));
    let name =
        widget_as::<adw::EntryRow>(dialog.upcast_ref(), "template-name-entry").ok_or("name")?;
    let source = widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("source")?;
    assert!(name.is_sensitive() && source.is_editable());
    assert_eq!(name.text(), "Local");
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            true
        ),
        "Local unsaved source"
    );
    assert!(
        widget_as::<gtk::Button>(dialog.upcast_ref(), "template-cancel")
            .is_some_and(|b| b.is_sensitive())
    );
    assert_eq!(
        glib::MainContext::default()
            .block_on(fixture.client.template_async(template.id))?
            .ok_or("persisted template")?
            .source,
        "External source"
    );
    dialog.force_close();
    fixture.window.close();
    Ok(())
}

pub(super) fn saving_an_unopened_note_as_template_should_return_to_the_browser() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.NoteTemplateOriginRegression")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let note = fixture
        .client
        .create_note_with_source(fixture.category.id, "# Browser note\n\nBody\n")?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Templates(TemplatesMsg::FromNote(note.id)));
    let dialog = visible_dialog(&fixture.window, "template-editor-dialog")?;
    let source = widget_as::<sourceview5::View>(dialog.upcast_ref(), "template-source-view")
        .ok_or("source")?;
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            true
        ),
        note.source
    );
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "template-name-entry")
        .ok_or("name")?
        .set_text("From browser");
    save_draft(&dialog)?;
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let templates = glib::MainContext::default().block_on(fixture.client.templates_async())?;
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].source, note.source);
    let unchanged = fixture.client.note(note.id)?.ok_or("note")?;
    assert_eq!(unchanged.revision, note.revision);
    assert_eq!(unchanged.updated_at, note.updated_at);
    fixture.window.close();
    Ok(())
}

pub(super) fn template_creation_should_report_missing_targets_without_creating_notes() -> TestResult
{
    let fixture = window_fixture_for("io.github.josbeir.Carver.MissingTemplateTargetRegression")?;
    dismiss_initial_dialogs(&fixture.window);
    let template = glib::MainContext::default().block_on(
        fixture
            .client
            .create_template_async("Deleted".into(), "Body".into()),
    )?;
    glib::MainContext::default().block_on(
        fixture
            .client
            .delete_template_async(template.id, template.revision),
    )?;
    let before = fixture.client.note_count(fixture.category.id)?;
    fixture
        .preferences_runtime
        .dispatch(AppMsg::Templates(TemplatesMsg::Create {
            category_id: fixture.category.id,
            template_id: template.id,
        }));
    assert!(run_main_context_until(|| fixture
        .preferences_runtime
        .model()
        .notice
        .is_some_and(|notice| notice
            .message
            .contains("template is no longer available"))));
    let template = glib::MainContext::default().block_on(
        fixture
            .client
            .create_template_async("Available".into(), "Body".into()),
    )?;
    fixture
        .preferences_runtime
        .dispatch(AppMsg::Templates(TemplatesMsg::Create {
            category_id: carver_sdk::CategoryId::new(),
            template_id: template.id,
        }));
    assert!(run_main_context_until(|| fixture
        .preferences_runtime
        .model()
        .notice
        .is_some_and(|notice| notice
            .message
            .contains("category is no longer available"))));
    assert_eq!(fixture.client.note_count(fixture.category.id)?, before);
    fixture.window.close();
    Ok(())
}

pub(super) fn template_shortcut_should_ignore_trash_and_base_contexts() -> TestResult {
    let fixture = window_fixture_for("io.github.josbeir.Carver.TemplateContextTests")?;
    dismiss_initial_dialogs(&fixture.window);
    fixture.window.present();
    let action = fixture
        .window
        .lookup_action("new-from-template")
        .ok_or("template action")?;
    for message in [
        AppMsg::Navigation(NavigationMsg::ShowTrash),
        AppMsg::Bases(crate::mvu::BasesMsg::Open(fixture.base.id)),
    ] {
        let _ = fixture.dispatcher.dispatch(message);
        action.activate(None);
        assert!(fixture.window.visible_dialog().is_none());
    }
    let note = fixture.client.create_note(fixture.category.id)?;
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: note.id,
            intent: crate::mvu::NoteOpenIntent::NewTab,
        }));
    assert!(run_main_context_until(|| note_tab_is_active(
        fixture.window.upcast_ref()
    ) && !note_tab_is_loading(
        fixture.window.upcast_ref()
    )));
    action.activate(None);
    assert!(fixture.window.visible_dialog().is_none());
    let _ = fixture
        .dispatcher
        .dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser));
    action.activate(None);
    visible_dialog(&fixture.window, "templates-dialog")?.close();
    fixture.window.close();
    Ok(())
}
