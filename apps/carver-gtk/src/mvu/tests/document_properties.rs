use super::*;

fn text_property(key: &str, value: &str) -> carver_config::DocumentProperty {
    carver_config::DocumentProperty {
        key: key.to_owned(),
        field_type: carver_config::DocumentPropertyType::Text,
        multiple: false,
        value: serde_json::Value::String(value.to_owned()),
    }
}

fn load_editor(model: &mut AppModel, source: &str) -> EditorSessionId {
    let _ = update(
        model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: source.to_owned(),
        }),
    );
    model
        .editor
        .as_ref()
        .map_or(EditorSessionId(0), |document| document.session)
}

fn author_document() -> carver_domain::FrontmatterDocument {
    carver_domain::FrontmatterDocument {
        format: carver_domain::FrontmatterFormat::Yaml,
        fields: vec![carver_domain::FrontmatterField::new(
            "author",
            carver_domain::FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    }
}

#[test]
fn enabling_default_properties_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(PreferencesMsg::SetDocumentPropertiesEnabled(true)),
    );

    assert!(model.config.document_properties.enabled);
    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.document_properties.enabled)
    );
}

#[test]
fn floating_properties_button_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(PreferencesMsg::SetDocumentPropertiesFloatingButton(false)),
    );

    assert!(!model.config.document_properties.floating_button);
    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if !config.document_properties.floating_button)
    );
}

#[test]
fn set_document_properties_should_replace_entries_and_persist() {
    let mut model = AppModel::new(&Config::default());
    let entries = vec![
        text_property("author", "Jane"),
        text_property("tags", "rust"),
    ];

    let effects = update(
        &mut model,
        AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(entries.clone())),
    );

    assert_eq!(model.config.document_properties.entries, entries);
    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.document_properties.entries == entries)
    );
}

#[test]
fn create_note_should_seed_enabled_default_properties() {
    let mut config = Config::default();
    config.document_properties.enabled = true;
    config.document_properties.entries = vec![text_property("author", "Jane")];
    let expected = config.document_properties.default_source();
    let mut model = AppModel::new(&config);
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);

    let effects = update(&mut model, AppMsg::Navigation(NavigationMsg::CreateNote));

    assert!(!expected.is_empty());
    assert_eq!(
        effects,
        vec![Effect::CreateNote {
            category_id,
            source: expected,
        }]
    );
}

#[test]
fn create_note_should_not_seed_disabled_default_properties() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);

    let effects = update(&mut model, AppMsg::Navigation(NavigationMsg::CreateNote));

    assert_eq!(
        effects,
        vec![Effect::CreateNote {
            category_id,
            source: String::new(),
        }]
    );
}

#[test]
fn apply_frontmatter_should_update_the_source_and_reload_the_rich_projection() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(author_document()),
            category: None,
        }),
    );

    let document = model.editor.as_ref();
    assert!(
        document.is_some_and(|document| document.source.contains("author: Jane")),
        "source was not updated"
    );
    assert_eq!(document.map(|document| document.source_generation), Some(1));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ScheduleEditorSave { .. })),
        "applying frontmatter should autosave"
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ReloadRichEditor { .. })),
        "applying frontmatter should reload the rich projection"
    );
}

#[test]
fn apply_frontmatter_should_be_a_noop_for_unchanged_source() {
    let source = "---\ntitle: Hello\n---\nBody\n";
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, source);
    let document = carver_domain::parse_frontmatter_document(source);
    let Some(document) = document else {
        return;
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(document),
            category: None,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some(source)
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source_generation),
        Some(0)
    );
}

#[test]
fn apply_frontmatter_should_accept_a_local_autosave() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    // A local autosave advances the persisted revision while the dialog is open.
    if let Some(document) = model.editor.as_mut() {
        document.revision = Revision(2);
    }

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(author_document()),
            category: None,
        }),
    );

    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.source.contains("author: Jane")),
        "a local autosave must not discard the dialog save"
    );
    assert!(!effects.is_empty());
}

#[test]
fn apply_frontmatter_should_ignore_an_external_change() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    if let Some(document) = model.editor.as_mut() {
        document.external_change = Some(crate::mvu::model::ExternalChange::Edited(Revision(2)));
    }

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(author_document()),
            category: None,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source_generation),
        Some(0)
    );
}

#[test]
fn apply_frontmatter_should_ignore_a_stale_session() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session: EditorSessionId(session.0 + 1),
            edit: FrontmatterEdit::Parsed(author_document()),
            category: None,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source_generation),
        Some(0)
    );
}

#[test]
fn properties_dialog_request_without_an_editor_should_do_nothing() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PropertiesDialogRequested),
    );
    assert!(effects.is_empty());
}

#[test]
fn apply_frontmatter_without_an_editor_should_do_nothing() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session: EditorSessionId(0),
            edit: FrontmatterEdit::Parsed(author_document()),
            category: None,
        }),
    );
    assert!(effects.is_empty());
}

#[test]
fn apply_raw_frontmatter_should_replace_the_block() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Raw {
                format: carver_domain::FrontmatterFormat::Yaml,
                content: "a: b".to_owned(),
            },
            category: None,
        }),
    );
    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.source.contains("a: b"))
    );
    assert!(!effects.is_empty());
}

#[test]
fn applying_a_category_change_should_move_the_note_on_save() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let category = carver_sdk::CategoryId::new();

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(author_document()),
            category: Some(category),
        }),
    );
    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.pending_category == Some(category))
    );

    let timer_id = effects.iter().find_map(|effect| match effect {
        Effect::ScheduleEditorSave { timer_id, .. } => Some(*timer_id),
        _ => None,
    });
    let Some(timer_id) = timer_id else {
        panic!("expected a scheduled save");
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed { session, timer_id }),
    );
    let move_to = effects.iter().find_map(|effect| match effect {
        Effect::SaveNote { request } => Some(request.move_to),
        _ => None,
    });
    assert_eq!(move_to, Some(Some(category)));
}

#[test]
fn a_category_only_change_should_still_schedule_a_save() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let category = carver_sdk::CategoryId::new();

    // An empty parsed document leaves "Body\n" byte-for-byte, so only the category changed.
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(carver_domain::FrontmatterDocument {
                format: carver_domain::FrontmatterFormat::Yaml,
                fields: Vec::new(),
                error: None,
            }),
            category: Some(category),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ScheduleEditorSave { .. })),
        "a category-only change should autosave"
    );
}

fn empty_document() -> carver_domain::FrontmatterDocument {
    carver_domain::FrontmatterDocument {
        format: carver_domain::FrontmatterFormat::Yaml,
        fields: Vec::new(),
        error: None,
    }
}

fn scheduled_timer(effects: &[Effect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ScheduleEditorSave { timer_id, .. } => Some(timer_id.0),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a scheduled save"))
}

fn scheduled_save(effects: &[Effect]) -> EditorSaveRequest {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SaveNote { request } => Some(request.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a save"))
}

#[test]
fn a_category_change_during_an_in_flight_save_should_be_saved_after() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let category = carver_sdk::CategoryId::new();

    // Start a save, then change only the category while it is in flight.
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Body changed\n".to_owned())),
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let timer_id = TimerId(scheduled_timer(&effects));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed { session, timer_id }),
    );
    let in_flight = scheduled_save(&effects);
    assert_eq!(in_flight.move_to, None);

    // An empty parsed document leaves the source unchanged, so only the category differs.
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(empty_document()),
            category: Some(category),
        }),
    );
    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.pending_category == Some(category))
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: in_flight,
            move_error: None,
            result: Ok(Revision(2)),
        }),
    );
    assert_eq!(scheduled_save(&effects).move_to, Some(category));
}

#[test]
fn a_failed_move_should_keep_the_content_save_and_report_a_notice() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let original = model.editor.as_ref().map(|document| document.category_id);
    let category = carver_sdk::CategoryId::new();

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(empty_document()),
            category: Some(category),
        }),
    );
    let timer_id = TimerId(scheduled_timer(&effects));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed { session, timer_id }),
    );
    let request = scheduled_save(&effects);

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            move_error: Some(UiError::new("move failed")),
            result: Ok(Revision(2)),
        }),
    );
    let document = model.editor.as_ref();
    assert_eq!(document.map(|document| document.category_id), original);
    assert_eq!(
        document.map(|document| document.revision),
        Some(Revision(2))
    );
    assert!(
        document.is_some_and(|document| { matches!(document.save_state, EditorSaveState::Clean) })
    );
    assert_eq!(
        model.notice.as_ref().map(|error| error.message.as_str()),
        Some("move failed")
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SaveNote { .. }))
    );
}

#[test]
fn an_editor_category_move_should_reload_the_sidebar() {
    let mut model = AppModel::new(&Config::default());
    let session = load_editor(&mut model, "Body\n");
    let category = carver_sdk::CategoryId::new();

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(empty_document()),
            category: Some(category),
        }),
    );
    let timer_id = TimerId(scheduled_timer(&effects));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed { session, timer_id }),
    );
    let request = scheduled_save(&effects);
    assert_eq!(request.move_to, Some(category));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            move_error: None,
            result: Ok(Revision(2)),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadSidebar { .. })),
        "a move should refresh category counts"
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadBrowser { .. })),
        "a move should refresh the note list"
    );
}
