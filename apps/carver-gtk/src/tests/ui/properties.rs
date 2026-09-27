//! Display-backed document-properties coverage.
use super::*;
use crate::mvu::{AppMsg, EditorMsg, PreferencesMsg};
use libadwaita::prelude::*;
use sourceview5::prelude::*;

/// Returns the preorder position of the first widget named `name`, for asserting row order.
fn widget_order(root: &gtk::Widget, name: &str) -> Option<usize> {
    fn walk(widget: &gtk::Widget, name: &str, order: &mut usize) -> Option<usize> {
        if widget.widget_name() == name {
            return Some(*order);
        }
        *order += 1;
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = walk(&current, name, order) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    let mut order = 0;
    walk(root, name, &mut order)
}

pub(super) fn document_properties_button_should_follow_mode_and_setting() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "# Title\n\nBody\n".to_owned(),
    }));

    let button = widget_as::<gtk::Button>(&fixture.surface, "document-properties-button")
        .ok_or("document properties button")?;
    assert!(
        !button.has_css_class("osd") && !button.has_css_class("flat"),
        "the floating button should use the theme background so it adapts to the color scheme"
    );
    let editor_stack = widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack")
        .ok_or("editor mode stack")?;

    editor_stack.set_visible_child_name("source");
    assert!(
        run_main_context_until(|| button.is_visible()),
        "the floating button should be visible in Source mode"
    );
    editor_stack.set_visible_child_name("rich");
    assert!(
        run_main_context_until(|| button.is_visible()),
        "the floating button should be visible in Edit mode"
    );
    editor_stack.set_visible_child_name("rendered");
    assert!(
        run_main_context_until(|| !button.is_visible()),
        "the floating button should be hidden in Preview mode"
    );
    editor_stack.set_visible_child_name("rich");
    assert!(run_main_context_until(|| button.is_visible()));

    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(
        run_main_context_until(|| fixture.window.visible_dialog().is_some_and(|dialog| {
            dialog.widget_name() == "document-properties-dialog" && dialog.is_mapped()
        })),
        "visible dialog: {:?}",
        fixture
            .window
            .visible_dialog()
            .map(|dialog| dialog.widget_name().clone())
    );
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let dialog_root = dialog.upcast_ref();
    assert!(widget_as::<gtk::TextView>(dialog_root, "document-properties-raw").is_none());
    // The group points users at Preferences for configuring defaults.
    assert!(
        widget_as::<adw::PreferencesGroup>(dialog_root, "document-properties-group")
            .and_then(|group| group.description())
            .is_some_and(|text| text.contains("Preferences")),
        "the dialog should introduce default properties via Preferences"
    );
    // The always-present title field is a simple text row, not an expander.
    assert!(widget_as::<adw::EntryRow>(dialog_root, "document-property-value-0").is_some());
    widget_as::<gtk::Button>(dialog_root, "document-properties-save")
        .ok_or("save button")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesFloatingButton(false),
    ));
    assert!(
        run_main_context_until(|| !button.is_visible()),
        "the floating button should be hidden when the setting is off"
    );

    fixture.window.close();
    Ok(())
}

pub(super) fn default_properties_dialog_should_persist_typed_entries() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let entries = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let dialog = crate::ui::editor::properties_dialog::show_defaults(
        Some(fixture.window.upcast_ref::<gtk::Window>()),
        &fixture.dispatcher,
        &entries,
    );
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |visible| visible.widget_name() == "document-properties-defaults-dialog"
        )));
    let root = dialog.upcast_ref();

    let add = widget_as::<adw::ButtonRow>(root, "document-property-add").ok_or("add property")?;
    add.emit_by_name::<()>("activated", &[]);
    assert!(
        run_main_context_until(
            || widget_as::<adw::EntryRow>(root, "document-property-key-0").is_some()
        ),
        "the added property row should be built"
    );
    let key = widget_as::<adw::EntryRow>(root, "document-property-key-0").ok_or("property key")?;
    key.set_text("author");
    let value =
        widget_as::<adw::EntryRow>(root, "document-property-value-0").ok_or("property value")?;
    value.set_text("Jane");
    dialog.close();

    let config_path = fixture.config_path.clone();
    assert!(
        run_main_context_until(|| {
            carver_config::load(&config_path).is_ok_and(|config| {
                config.document_properties.entries.len() == 1
                    && config.document_properties.entries[0].key == "author"
                    && config.document_properties.entries[0].value == serde_json::json!("Jane")
            })
        }),
        "config: {:?}",
        carver_config::load(&config_path).map(|config| config.document_properties.entries)
    );

    fixture.window.close();
    Ok(())
}

pub(super) fn default_properties_should_always_show_without_removal() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesEnabled(true),
    ));
    fixture
        .runtime
        .dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
            vec![carver_config::DocumentProperty {
                key: "author".to_owned(),
                field_type: carver_config::DocumentPropertyType::Text,
                multiple: false,
                value: serde_json::json!("Jane"),
            }],
        )));
    let category = fixture.client.create_category("Properties")?;
    let empty = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: empty.id,
        revision: empty.revision,
        source: String::new(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();

    assert!(
        widget_as::<gtk::Button>(root, "document-properties-add-defaults").is_none(),
        "the add-defaults action should be gone"
    );
    // The configured default is always present, value-only, and not removable.
    let author = widget_as::<adw::EntryRow>(root, "document-property-value-1")
        .ok_or("default property row")?;
    assert_eq!(author.text(), "Jane");
    assert!(
        widget_as::<gtk::Button>(root, "document-property-remove-1").is_none(),
        "a configured default must not be removable"
    );
    assert!(
        widget_as::<adw::ButtonRow>(root, "document-properties-add").is_some(),
        "the add-property action should be a full-width button row"
    );

    dialog.close();
    fixture.window.close();
    Ok(())
}

fn list_default(multiple: bool) -> carver_config::DocumentProperty {
    carver_config::DocumentProperty {
        key: "status".to_owned(),
        field_type: carver_config::DocumentPropertyType::List,
        multiple,
        value: serde_json::json!(["active", "archived"]),
    }
}

fn configure_list_default(fixture: &super::document_sidebar::SidebarFixture, multiple: bool) {
    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesEnabled(true),
    ));
    fixture
        .runtime
        .dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
            vec![list_default(multiple)],
        )));
}

pub(super) fn list_default_should_render_a_dropdown_when_single() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    configure_list_default(&fixture, false);
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nstatus: active\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();

    assert!(
        widget_as::<adw::ComboRow>(root, "document-property-value-1").is_some(),
        "a single-select list property should render a dropdown"
    );
    let combo =
        widget_as::<adw::ComboRow>(root, "document-property-value-1").ok_or("list dropdown")?;
    assert_eq!(combo.selected(), 0);
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn list_default_should_render_switches_when_multiple() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    configure_list_default(&fixture, true);
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nstatus:\n  - active\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();

    assert!(
        widget_as::<adw::ExpanderRow>(root, "document-property-value-1").is_some(),
        "a multi-select list property should render an option expander"
    );
    assert_eq!(
        widget_as::<adw::SwitchRow>(root, "document-property-value-2").map(|row| row.is_active()),
        None,
        "the switch rows are nested inside the expander, not named by index"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn list_default_settings_should_offer_options_and_multiple() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let entries = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let dialog = crate::ui::editor::properties_dialog::show_defaults(
        Some(fixture.window.upcast_ref::<gtk::Window>()),
        &fixture.dispatcher,
        &entries,
    );
    let root = dialog.upcast_ref();
    widget_as::<adw::ButtonRow>(root, "document-property-add")
        .ok_or("add property")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-0"
    )
    .is_some()));

    let kind = widget_as::<adw::ComboRow>(root, "document-property-kind-0").ok_or("kind")?;
    kind.set_selected(4);
    assert!(run_main_context_until(|| widget_as::<adw::SwitchRow>(
        root,
        "document-property-multiple-0"
    )
    .is_some()));
    widget_as::<adw::EntryRow>(root, "document-property-key-0")
        .ok_or("property key")?
        .set_text("status");
    widget_as::<adw::EntryRow>(root, "document-property-value-0")
        .ok_or("options entry")?
        .set_text("active, archived");
    widget_as::<adw::SwitchRow>(root, "document-property-multiple-0")
        .ok_or("multiple switch")?
        .set_active(true);
    dialog.close();

    let config_path = fixture.config_path.clone();
    assert!(
        run_main_context_until(|| {
            carver_config::load(&config_path).is_ok_and(|config| {
                config.document_properties.entries.len() == 1
                    && config.document_properties.entries[0].multiple
                    && config.document_properties.entries[0].value
                        == serde_json::json!(["active", "archived"])
            })
        }),
        "config: {:?}",
        carver_config::load(&config_path).map(|config| config.document_properties.entries)
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn date_default_should_render_a_picker_and_disable_invalid_values() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesEnabled(true),
    ));
    fixture
        .runtime
        .dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
            vec![carver_config::DocumentProperty {
                key: "due".to_owned(),
                field_type: carver_config::DocumentPropertyType::Date,
                multiple: false,
                value: serde_json::json!(""),
            }],
        )));
    let category = fixture.client.create_category("Properties")?;

    let empty = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: empty.id,
        revision: empty.revision,
        source: "---\ndue: 2024-01-15\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    assert!(
        widget_as::<gtk::MenuButton>(dialog.upcast_ref(), "document-property-value-1-picker")
            .is_some(),
        "a valid date should render the calendar picker"
    );
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let invalid = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: invalid.id,
        revision: invalid.revision,
        source: "---\ndue: not a date\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();
    assert!(
        widget_as::<gtk::MenuButton>(root, "document-property-value-1-picker").is_none(),
        "an invalid date should not render the picker"
    );
    assert!(
        widget_as::<adw::ActionRow>(root, "document-property-value-1").is_some(),
        "an invalid date renders the read-only row"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn date_time_default_settings_should_persist_the_field_type() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let entries = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let dialog = crate::ui::editor::properties_dialog::show_defaults(
        Some(fixture.window.upcast_ref::<gtk::Window>()),
        &fixture.dispatcher,
        &entries,
    );
    let root = dialog.upcast_ref();
    widget_as::<adw::ButtonRow>(root, "document-property-add")
        .ok_or("add property")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-0"
    )
    .is_some()));

    let kind = widget_as::<adw::ComboRow>(root, "document-property-kind-0").ok_or("kind")?;
    kind.set_selected(6);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        root,
        "document-property-value-0"
    )
    .is_some()));
    widget_as::<adw::EntryRow>(root, "document-property-key-0")
        .ok_or("property key")?
        .set_text("post_date");
    dialog.close();

    let config_path = fixture.config_path.clone();
    assert!(
        run_main_context_until(|| {
            carver_config::load(&config_path).is_ok_and(|config| {
                config.document_properties.entries.len() == 1
                    && config.document_properties.entries[0].field_type
                        == carver_config::DocumentPropertyType::DateTime
                    && config.document_properties.entries[0].value == serde_json::json!("")
            })
        }),
        "config: {:?}",
        carver_config::load(&config_path).map(|config| config.document_properties.entries)
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn ad_hoc_date_property_should_reopen_as_date() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nmydate: 2026-09-16\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();
    assert_eq!(
        widget_as::<adw::ComboRow>(root, "document-property-kind-1").map(|combo| combo.selected()),
        Some(5),
        "an ISO date should reopen as the Date field type"
    );
    let picker = widget_as::<gtk::MenuButton>(root, "document-property-value-1-picker")
        .ok_or("calendar picker")?;
    // The picker stays on the collapsed row, so a date is picked without expanding it.
    let row = widget_as::<adw::ExpanderRow>(root, "document-property-row-1").ok_or("date row")?;
    assert!(
        !row.is_expanded(),
        "the custom date row should start collapsed"
    );
    assert!(
        run_main_context_until(|| picker.is_mapped()),
        "the calendar picker should be visible while the row is collapsed"
    );
    assert!(
        widget_as::<adw::EntryRow>(root, "document-property-key-1")
            .is_some_and(|key| !key.is_mapped()),
        "the expander content should stay hidden while collapsed"
    );
    // The remove button stays the last suffix, after the compact picker.
    assert!(
        widget_order(root, "document-property-remove-1")
            > widget_order(root, "document-property-value-1-picker"),
        "the remove button should follow the calendar picker"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}

/// Picking a day from the collapsed custom date picker persists it on save.
pub(super) fn ad_hoc_collapsed_date_picker_should_save_the_picked_value() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nmydate: 2026-09-16\n---\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    // Open the picker from the collapsed suffix, choose another day, and save.
    let picker = widget_as::<gtk::MenuButton>(root, "document-property-value-1-picker")
        .ok_or("calendar picker")?;
    picker.popup();
    let calendar =
        widget_as::<gtk::Calendar>(root, "document-property-value-1-calendar").ok_or("calendar")?;
    calendar.set_day(20);
    calendar.emit_by_name::<()>("day-selected", &[]);
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("2026-09-20"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn changing_a_property_type_should_keep_the_row_expanded() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "Body\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();

    widget_as::<adw::ButtonRow>(root, "document-properties-add")
        .ok_or("add property")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-1"
    )
    .is_some()));
    assert!(
        widget_as::<adw::ExpanderRow>(root, "document-property-row-1")
            .is_some_and(|row| row.is_expanded()),
        "a newly added property should be expanded"
    );

    widget_as::<adw::ComboRow>(root, "document-property-kind-1")
        .ok_or("kind")?
        .set_selected(5);
    assert!(run_main_context_until(|| widget_as::<gtk::MenuButton>(
        root,
        "document-property-value-1-picker"
    )
    .is_some()));
    assert!(
        widget_as::<adw::ExpanderRow>(root, "document-property-row-1")
            .is_some_and(|row| row.is_expanded()),
        "changing the type should keep the property expanded"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn date_picker_should_offer_clear_and_done_controls() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nmydate: 2026-09-16\n---\nBody\n".to_owned(),
    }));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_some_and(
            |dialog| dialog.widget_name() == "document-properties-dialog"
        )));
    let dialog = fixture
        .window
        .visible_dialog()
        .ok_or("document properties dialog")?;
    let root = dialog.upcast_ref();

    let picker = widget_as::<gtk::MenuButton>(root, "document-property-value-1-picker")
        .ok_or("date picker")?;
    let popover = picker.popover().ok_or("picker popover")?;
    let clear =
        widget_as::<gtk::Button>(root, "document-property-value-1-clear").ok_or("clear button")?;
    let done =
        widget_as::<gtk::Button>(root, "document-property-value-1-done").ok_or("done button")?;
    assert!(
        clear.is_ancestor(&popover) && done.is_ancestor(&popover),
        "the clear and done controls should live in the picker popover"
    );
    assert_eq!(done.label().as_deref(), Some("Done"));
    // Clicking Done dismisses the popover. Headless popovers do not map, so this exercises the
    // wired handler without relying on mapped visibility.
    done.emit_clicked();
    dialog.close();
    fixture.window.close();
    Ok(())
}

fn editor_source(fixture: &super::document_sidebar::SidebarFixture) -> String {
    fixture
        .runtime
        .model()
        .editor
        .as_ref()
        .map(|document| document.source.clone())
        .unwrap_or_default()
}

fn enable_defaults(
    fixture: &super::document_sidebar::SidebarFixture,
    entries: Vec<carver_config::DocumentProperty>,
) {
    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesEnabled(true),
    ));
    fixture
        .runtime
        .dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
            entries,
        )));
}

fn open_properties_dialog(
    fixture: &super::document_sidebar::SidebarFixture,
) -> Result<adw::Dialog, String> {
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::PropertiesDialogRequested));
    if !run_main_context_until(|| {
        fixture
            .window
            .visible_dialog()
            .is_some_and(|dialog| dialog.widget_name() == "document-properties-dialog")
    }) {
        return Err("document properties dialog did not open".to_owned());
    }
    fixture
        .window
        .visible_dialog()
        .ok_or_else(|| "document properties dialog".to_owned())
}

pub(super) fn date_time_default_should_edit_the_picker() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    enable_defaults(
        &fixture,
        vec![carver_config::DocumentProperty {
            key: "at".to_owned(),
            field_type: carver_config::DocumentPropertyType::DateTime,
            multiple: false,
            value: serde_json::json!(""),
        }],
    );
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nat: 2026-09-16T08:30:00Z\n---\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    let calendar =
        widget_as::<gtk::Calendar>(root, "document-property-value-1-calendar").ok_or("calendar")?;
    let hour_up =
        widget_as::<gtk::Button>(root, "document-property-value-1-hour-up").ok_or("hour up")?;
    let minute_down = widget_as::<gtk::Button>(root, "document-property-value-1-minute-down")
        .ok_or("minute down")?;

    // Drive the spinner to a fixed wall-clock time regardless of the seeded value or timezone.
    // (The display suite runs on a 24-hour clock.)
    let hour_label = widget_as::<gtk::Label>(root, "document-property-value-1-hour-label")
        .ok_or("hour label")?;
    let minute_label = widget_as::<gtk::Label>(root, "document-property-value-1-minute-label")
        .ok_or("minute label")?;
    for _ in 0..24 {
        if hour_label.text() == "09" {
            break;
        }
        hour_up.emit_clicked();
    }
    for _ in 0..60 {
        if minute_label.text() == "15" {
            break;
        }
        minute_down.emit_clicked();
    }
    assert_eq!(hour_label.text(), "09");
    assert_eq!(minute_label.text(), "15");
    calendar.set_day(20);
    calendar.emit_by_name::<()>("day-selected", &[]);
    widget_as::<gtk::Button>(root, "document-property-value-1-clear")
        .ok_or("clear")?
        .emit_clicked();
    widget_as::<gtk::Button>(root, "document-property-value-1-done")
        .ok_or("done")?
        .emit_clicked();

    // The retained time is applied again on save.
    calendar.emit_by_name::<()>("day-selected", &[]);
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("at:"), "source: {source}");
    assert!(source.contains("T09:15:"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn typed_defaults_should_save_edited_values() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    enable_defaults(
        &fixture,
        vec![
            carver_config::DocumentProperty {
                key: "notes".to_owned(),
                field_type: carver_config::DocumentPropertyType::LongText,
                multiple: false,
                value: serde_json::json!("first"),
            },
            carver_config::DocumentProperty {
                key: "count".to_owned(),
                field_type: carver_config::DocumentPropertyType::Number,
                multiple: false,
                value: serde_json::json!(2),
            },
            carver_config::DocumentProperty {
                key: "done".to_owned(),
                field_type: carver_config::DocumentPropertyType::Boolean,
                multiple: false,
                value: serde_json::json!(false),
            },
        ],
    );
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "Body\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    widget_as::<gtk::TextView>(root, "document-property-value-1")
        .ok_or("long text")?
        .buffer()
        .set_text("second");
    widget_as::<adw::EntryRow>(root, "document-property-value-2")
        .ok_or("number")?
        .set_text("7");
    widget_as::<adw::SwitchRow>(root, "document-property-value-3")
        .ok_or("boolean")?
        .set_active(true);
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("notes: second"), "source: {source}");
    assert!(source.contains("count: 7"), "source: {source}");
    assert!(source.contains("done: true"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn title_frontmatter_should_fill_the_title_row() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\ntitle: Hello\n---\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    assert_eq!(
        widget_as::<adw::EntryRow>(root, "document-property-value-0")
            .ok_or("title row")?
            .text(),
        "Hello"
    );
    widget_as::<gtk::Button>(root, "document-properties-cancel")
        .ok_or("cancel")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    fixture.window.close();
    Ok(())
}

pub(super) fn malformed_frontmatter_should_fall_back_to_raw_source() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---yaml\nkey: [\n---\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    let raw = widget_as::<gtk::TextView>(root, "document-properties-raw").ok_or("raw view")?;
    raw.buffer().set_text("author: Jane");
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("author: Jane"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn defaults_dialog_should_remove_a_property() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let entries = std::rc::Rc::new(std::cell::RefCell::new(vec![
        carver_config::DocumentProperty {
            key: "author".to_owned(),
            field_type: carver_config::DocumentPropertyType::Text,
            multiple: false,
            value: serde_json::json!("Jane"),
        },
        carver_config::DocumentProperty {
            key: "count".to_owned(),
            field_type: carver_config::DocumentPropertyType::Number,
            multiple: false,
            value: serde_json::json!(2),
        },
    ]));
    let dialog = crate::ui::editor::properties_dialog::show_defaults(
        Some(fixture.window.upcast_ref::<gtk::Window>()),
        &fixture.dispatcher,
        &entries,
    );
    assert!(run_main_context_until(|| widget_as::<gtk::Button>(
        dialog.upcast_ref(),
        "document-property-remove-0"
    )
    .is_some()));
    let root = dialog.upcast_ref();
    widget_as::<gtk::Button>(root, "document-property-remove-0")
        .ok_or("remove")?
        .emit_clicked();
    assert!(run_main_context_until(|| entries.borrow().len() == 1));
    assert_eq!(entries.borrow()[0].key, "count");
    dialog.close();
    fixture.window.close();
    Ok(())
}

pub(super) fn defaults_dialog_should_handle_date_and_typed_values() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let entries = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let dialog = crate::ui::editor::properties_dialog::show_defaults(
        Some(fixture.window.upcast_ref::<gtk::Window>()),
        &fixture.dispatcher,
        &entries,
    );
    let root = dialog.upcast_ref();

    // A Date default shows the dynamic-value hint instead of a fixed editor.
    widget_as::<adw::ButtonRow>(root, "document-property-add")
        .ok_or("add date")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-0"
    )
    .is_some()));
    widget_as::<adw::ComboRow>(root, "document-property-kind-0")
        .ok_or("date kind")?
        .set_selected(5);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        root,
        "document-property-value-0"
    )
    .is_some()));
    widget_as::<adw::EntryRow>(root, "document-property-key-0")
        .ok_or("date key")?
        .set_text("due");

    // A number and boolean default persist their typed values.
    widget_as::<adw::ButtonRow>(root, "document-property-add")
        .ok_or("add number")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-1"
    )
    .is_some()));
    widget_as::<adw::ComboRow>(root, "document-property-kind-1")
        .ok_or("number kind")?
        .set_selected(2);
    assert!(run_main_context_until(|| {
        widget_as::<adw::EntryRow>(root, "document-property-value-1")
            .is_some_and(|row| row.input_purpose() == gtk::InputPurpose::Number)
    }));
    widget_as::<adw::EntryRow>(root, "document-property-key-1")
        .ok_or("number key")?
        .set_text("count");
    widget_as::<adw::EntryRow>(root, "document-property-value-1")
        .ok_or("number value")?
        .set_text("5");

    widget_as::<adw::ButtonRow>(root, "document-property-add")
        .ok_or("add boolean")?
        .emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| widget_as::<adw::ComboRow>(
        root,
        "document-property-kind-2"
    )
    .is_some()));
    widget_as::<adw::ComboRow>(root, "document-property-kind-2")
        .ok_or("boolean kind")?
        .set_selected(3);
    assert!(run_main_context_until(|| widget_as::<adw::SwitchRow>(
        root,
        "document-property-value-2"
    )
    .is_some()));
    widget_as::<adw::EntryRow>(root, "document-property-key-2")
        .ok_or("boolean key")?
        .set_text("done");
    widget_as::<adw::SwitchRow>(root, "document-property-value-2")
        .ok_or("boolean value")?
        .set_active(true);
    dialog.close();

    let loaded = entries.borrow().clone();
    assert_eq!(loaded.len(), 3);
    assert_eq!(
        loaded[0].field_type,
        carver_config::DocumentPropertyType::Date
    );
    assert_eq!(loaded[1].value, serde_json::json!(5));
    assert_eq!(loaded[2].value, serde_json::json!(true));
    fixture.window.close();
    Ok(())
}

pub(super) fn date_default_picker_should_edit_and_save() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    enable_defaults(
        &fixture,
        vec![carver_config::DocumentProperty {
            key: "due".to_owned(),
            field_type: carver_config::DocumentPropertyType::Date,
            multiple: false,
            value: serde_json::json!(""),
        }],
    );
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\ndue: 2026-09-15\n---\nBody\n".to_owned(),
    }));

    // Saving an unchanged date exercises the preserve path.
    let dialog = open_properties_dialog(&fixture)?;
    widget_as::<gtk::Button>(dialog.upcast_ref(), "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    // Reopen and pick a different day.
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();
    let calendar =
        widget_as::<gtk::Calendar>(root, "document-property-value-1-calendar").ok_or("calendar")?;
    calendar.set_day(20);
    calendar.emit_by_name::<()>("day-selected", &[]);
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("2026-09-20"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn ad_hoc_boolean_property_should_toggle_and_save() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\ndone: true\n---\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();

    let toggle =
        widget_as::<gtk::Switch>(root, "document-property-value-1").ok_or("boolean switch")?;
    // The switch stays on the collapsed row, so the value is edited without expanding it.
    let row =
        widget_as::<adw::ExpanderRow>(root, "document-property-row-1").ok_or("boolean row")?;
    assert!(
        !row.is_expanded(),
        "the custom boolean row should start collapsed"
    );
    assert!(
        run_main_context_until(|| toggle.is_mapped()),
        "the boolean switch should be visible while the row is collapsed"
    );
    assert!(
        widget_as::<adw::EntryRow>(root, "document-property-key-1")
            .is_some_and(|key| !key.is_mapped()),
        "the expander content should stay hidden while collapsed"
    );
    // The remove button stays the last suffix, after the compact switch.
    assert!(
        widget_order(root, "document-property-remove-1")
            > widget_order(root, "document-property-value-1"),
        "the remove button should follow the boolean switch"
    );
    assert!(toggle.is_active());
    toggle.set_active(false);
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(source.contains("done: false"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn authored_frontmatter_order_should_survive_an_unchanged_save() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "---\nstatus: draft\nauthor: Jane\n---\nBody\n";
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: source.to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    widget_as::<gtk::Button>(dialog.upcast_ref(), "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert_eq!(editor_source(&fixture), source);
    fixture.window.close();
    Ok(())
}

pub(super) fn explicit_empty_value_should_survive_an_unchanged_save() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "---\nsummary: \"\"\n---\nBody\n";
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: source.to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    widget_as::<gtk::Button>(dialog.upcast_ref(), "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert_eq!(editor_source(&fixture), source);
    fixture.window.close();
    Ok(())
}

pub(super) fn complex_frontmatter_should_fall_back_to_raw_source() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    for (source, expected_language) in [
        ("---\nitems: [true, 2]\n---\nBody\n", "yaml"),
        ("---toml\nowner = { name = \"Grace\" }\n---\nBody\n", "toml"),
        (
            "---json\n{\n  \"owner\": {\"name\": \"Grace\"}\n}\n---\nBody\n",
            "json",
        ),
    ] {
        let note = fixture.client.create_note(category.id)?;
        fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
            note_id: note.id,
            revision: note.revision,
            source: source.to_owned(),
        }));
        let dialog = open_properties_dialog(&fixture)?;
        // A value the structured editor cannot represent falls back to the raw source view,
        // which highlights the block with the grammar for its declared format.
        let view = widget_as::<sourceview5::View>(dialog.upcast_ref(), "document-properties-raw")
            .ok_or("raw source view")?;
        assert_eq!(
            view.left_margin(),
            12,
            "the raw view should inset its text: {source}"
        );
        let buffer = view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .map_err(|_| "GtkSourceBuffer")?;
        assert!(buffer.is_highlight_syntax(), "source: {source}");
        assert_eq!(
            buffer.language().map(|language| language.id().to_string()),
            Some(expected_language.to_owned()),
            "source: {source}"
        );
        // The raw view inherits the Carve schemes, so it follows the app color scheme.
        let expected_scheme = if adw::StyleManager::default().is_dark() {
            "carve-dark"
        } else {
            "carve-light"
        };
        assert_eq!(
            buffer
                .style_scheme()
                .map(|scheme| scheme.id().to_string())
                .as_deref(),
            Some(expected_scheme),
            "source: {source}"
        );
        // The raw view re-derives its scheme while the dialog stays open.
        let style_manager = adw::StyleManager::default();
        let original_scheme = style_manager.color_scheme();
        style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
        assert_eq!(
            buffer
                .style_scheme()
                .map(|scheme| scheme.id().to_string())
                .as_deref(),
            Some("carve-dark")
        );
        style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
        assert_eq!(
            buffer
                .style_scheme()
                .map(|scheme| scheme.id().to_string())
                .as_deref(),
            Some("carve-light")
        );
        style_manager.set_color_scheme(original_scheme);
        dialog.close();
        assert!(run_main_context_until(|| fixture
            .window
            .visible_dialog()
            .is_none()));
    }
    fixture.window.close();
    Ok(())
}

pub(super) fn heading_should_prefill_the_title_without_persisting() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    let source = "# Meeting\n\nBody\n";
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: source.to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();
    assert_eq!(
        widget_as::<adw::EntryRow>(root, "document-property-value-0")
            .ok_or("title row")?
            .text(),
        "Meeting"
    );
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    assert_eq!(editor_source(&fixture), source);
    fixture.window.close();
    Ok(())
}

pub(super) fn edited_prefilled_title_should_persist() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "# Meeting\n\nBody\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    let root = dialog.upcast_ref();
    widget_as::<adw::EntryRow>(root, "document-property-value-0")
        .ok_or("title row")?
        .set_text("Renamed");
    widget_as::<gtk::Button>(root, "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));
    let source = editor_source(&fixture);
    assert!(source.contains("title: Renamed"), "source: {source}");
    fixture.window.close();
    Ok(())
}

pub(super) fn edited_title_should_lead_the_block_on_reopen() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Properties")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "---\nauthor: Jane\n---\n\n# Meeting\n".to_owned(),
    }));
    let dialog = open_properties_dialog(&fixture)?;
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "document-property-value-0")
        .ok_or("title row")?
        .set_text("Renamed");
    widget_as::<gtk::Button>(dialog.upcast_ref(), "document-properties-save")
        .ok_or("save")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let source = editor_source(&fixture);
    assert!(
        source.starts_with("---\ntitle: \"Renamed\""),
        "source: {source}"
    );

    // Reopening keeps the title as the first field.
    let dialog = open_properties_dialog(&fixture)?;
    assert_eq!(
        widget_as::<adw::EntryRow>(dialog.upcast_ref(), "document-property-value-0")
            .ok_or("title row")?
            .text(),
        "Renamed"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}

/// A 12-hour spinner shows a meridiem toggle and projects its hours to 1–12.
pub(super) fn time_spinner_should_follow_a_twelve_hour_clock() -> TestResult {
    let spinner = crate::ui::property::TimeSpinner::with_twelve_hour("clock", true);
    let window = adw::Window::new();
    window.set_content(Some(spinner.widget()));
    window.present();
    assert!(run_main_context_until(|| spinner.widget().is_mapped()));

    let hour = widget_as::<gtk::Label>(spinner.widget(), "clock-hour-label").ok_or("hour label")?;
    let minute =
        widget_as::<gtk::Label>(spinner.widget(), "clock-minute-label").ok_or("minute label")?;
    let meridiem =
        widget_as::<gtk::Button>(spinner.widget(), "clock-meridiem").ok_or("meridiem button")?;

    // Midnight reads as 12 with the locale's own AM label.
    spinner.set_time(0, 5);
    assert_eq!(hour.text(), "12");
    assert_eq!(minute.text(), "05");
    let am = meridiem
        .label()
        .map(|label| label.to_string())
        .ok_or("am label")?;
    assert!(
        !am.is_empty(),
        "the meridiem label should come from the locale"
    );

    // Afternoon reads as 1 with the opposite label.
    spinner.set_time(13, 30);
    assert_eq!(hour.text(), "01");
    assert_eq!(minute.text(), "30");
    let pm = meridiem
        .label()
        .map(|label| label.to_string())
        .ok_or("pm label")?;
    assert_ne!(am, pm, "a 12-hour clock must distinguish AM from PM");

    // Toggling flips the half of the day without changing the displayed hour.
    meridiem.emit_clicked();
    assert_eq!(hour.text(), "01");
    assert_eq!(
        meridiem.label().map(|label| label.to_string()).as_deref(),
        Some(am.as_str())
    );
    assert_eq!(spinner.time(), (1, 30));

    // Both chevrons wrap through the 12-hour projection.
    spinner.set_time(11, 59);
    widget_as::<gtk::Button>(spinner.widget(), "clock-hour-up")
        .ok_or("hour up")?
        .emit_clicked();
    widget_as::<gtk::Button>(spinner.widget(), "clock-minute-up")
        .ok_or("minute up")?
        .emit_clicked();
    assert_eq!(hour.text(), "12");
    assert_eq!(minute.text(), "00");
    widget_as::<gtk::Button>(spinner.widget(), "clock-hour-down")
        .ok_or("hour down")?
        .emit_clicked();
    assert_eq!(hour.text(), "11");

    window.close();
    Ok(())
}
