//! Display-backed document-properties coverage.
use super::*;
use crate::mvu::{AppMsg, EditorMsg, PreferencesMsg};
use libadwaita::prelude::*;

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
    let source_mode =
        widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-source").ok_or("source")?;
    let rich_mode =
        widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-rich").ok_or("rich")?;
    let rendered_mode = widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-rendered")
        .ok_or("rendered")?;

    source_mode.set_active(true);
    assert!(
        run_main_context_until(|| button.is_visible()),
        "the floating button should be visible in Source mode"
    );
    rich_mode.set_active(true);
    assert!(
        run_main_context_until(|| button.is_visible()),
        "the floating button should be visible in Edit mode"
    );
    rendered_mode.set_active(true);
    assert!(
        run_main_context_until(|| !button.is_visible()),
        "the floating button should be hidden in Preview mode"
    );
    rich_mode.set_active(true);
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
    assert!(
        widget_as::<gtk::MenuButton>(root, "document-property-value-1-picker").is_some(),
        "an inferred date should render the calendar picker"
    );
    dialog.close();
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
