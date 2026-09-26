use super::*;

fn text_property(key: &str, value: &str) -> carver_config::DocumentProperty {
    carver_config::DocumentProperty {
        key: key.to_owned(),
        kind: carver_domain::PropertyKind::Text,
        multiline: false,
        value: serde_json::Value::String(value.to_owned()),
    }
}

fn load_editor(model: &mut AppModel, source: &str) -> (EditorSessionId, Revision) {
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
        .map_or((EditorSessionId(0), Revision(0)), |document| {
            (document.session, document.revision)
        })
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
fn apply_frontmatter_should_update_the_source_and_bump_the_generation() {
    let mut model = AppModel::new(&Config::default());
    let (session, revision) = load_editor(&mut model, "Body\n");
    let document = carver_domain::FrontmatterDocument {
        format: carver_domain::FrontmatterFormat::Yaml,
        fields: vec![carver_domain::FrontmatterField::new(
            "author",
            carver_domain::FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            revision,
            edit: FrontmatterEdit::Parsed(document),
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
}

#[test]
fn apply_frontmatter_should_be_a_noop_for_unchanged_source() {
    let source = "---\ntitle: Hello\n---\nBody\n";
    let mut model = AppModel::new(&Config::default());
    let (session, revision) = load_editor(&mut model, source);
    let document = carver_domain::parse_frontmatter_document(source);
    assert!(document.is_some());
    let Some(document) = document else {
        return;
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            revision,
            edit: FrontmatterEdit::Parsed(document),
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
fn apply_frontmatter_should_ignore_a_stale_revision() {
    let mut model = AppModel::new(&Config::default());
    let (session, revision) = load_editor(&mut model, "Body\n");
    let document = carver_domain::FrontmatterDocument {
        format: carver_domain::FrontmatterFormat::Yaml,
        fields: vec![carver_domain::FrontmatterField::new(
            "author",
            carver_domain::FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            revision: Revision(revision.0 + 1),
            edit: FrontmatterEdit::Parsed(document),
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
    let (session, revision) = load_editor(&mut model, "Body\n");
    let document = carver_domain::FrontmatterDocument {
        format: carver_domain::FrontmatterFormat::Yaml,
        fields: vec![carver_domain::FrontmatterField::new(
            "author",
            carver_domain::FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session: EditorSessionId(session.0 + 1),
            revision,
            edit: FrontmatterEdit::Parsed(document),
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
