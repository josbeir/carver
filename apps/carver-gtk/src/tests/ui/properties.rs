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
    assert!(widget_as::<adw::ExpanderRow>(dialog_root, "document-property-row-0").is_some());
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

pub(super) fn add_default_properties_should_offer_only_without_frontmatter() -> TestResult {
    let fixture = super::document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Preferences(
        PreferencesMsg::SetDocumentPropertiesEnabled(true),
    ));
    fixture
        .runtime
        .dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
            vec![carver_config::DocumentProperty {
                key: "author".to_owned(),
                kind: carver_domain::PropertyKind::Text,
                multiline: false,
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
    let add_defaults =
        widget_as::<gtk::Button>(root, "document-properties-add-defaults").ok_or("add defaults")?;
    add_defaults.emit_clicked();
    assert!(
        run_main_context_until(
            || widget_as::<adw::EntryRow>(root, "document-property-key-1").is_some()
        ),
        "the default property row should be added"
    );
    dialog.close();
    assert!(run_main_context_until(|| fixture
        .window
        .visible_dialog()
        .is_none()));

    let with_frontmatter = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: with_frontmatter.id,
        revision: with_frontmatter.revision,
        source: "---\nauthor: Existing\n---\nBody\n".to_owned(),
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
        widget_as::<gtk::Button>(dialog.upcast_ref(), "document-properties-add-defaults").is_none(),
        "a note with its own properties must not offer the defaults"
    );
    dialog.close();
    fixture.window.close();
    Ok(())
}
