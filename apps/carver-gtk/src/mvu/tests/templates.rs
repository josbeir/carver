use super::*;
use crate::mvu::{TemplatePurpose, TemplatesMsg};

#[test]
fn template_load_should_ignore_stale_completions() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Manage));
    let previous = model.template_request.unwrap_or(RequestId(0));
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Manage));
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::Loaded {
                request_id: previous,
                purpose: TemplatePurpose::Manage,
                result: Ok(Vec::new())
            })
        )
        .is_empty()
    );
}
#[test]
fn template_save_should_reject_invalid_properties_before_persistence() {
    let mut config = Config::default();
    config.document_properties.entries = vec![carver_config::DocumentProperty {
        key: "count".into(),
        field_type: carver_config::DocumentPropertyType::Number,
        multiple: false,
        value: serde_json::json!(1),
    }];
    let mut model = AppModel::new(&config);
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Save {
            request_id: id,
            original: None,
            name: "Template".into(),
            source: "---\ncount: text\n---".into(),
        }),
    );
    assert!(
        matches!(&effects[..], [Effect::FinishTemplateEdit { error: Some(error), .. }] if error.message.contains("count"))
    );
}
#[test]
fn template_save_should_accept_explicit_empty_values() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let source = "---\ntext: ''\nlist: []\nflag: false\nnumber: 0\nempty: null\n---";
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Save {
            request_id: id,
            original: None,
            name: " Template ".into(),
            source: source.into(),
        }),
    );
    assert!(
        matches!(&effects[..], [Effect::SaveTemplate { name, source: saved, .. }] if name == "Template" && saved == source)
    );
}
#[test]
fn template_save_should_ignore_a_closed_editor() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let id = model.template_editor.unwrap_or(RequestId(0));
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::EditorClosed(id)),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::Saved {
                request_id: id,
                result: Ok(())
            })
        )
        .is_empty()
    );
}
#[test]
fn ordinary_note_creation_should_resolve_category_template_in_the_runtime() {
    let mut model = AppModel::new(&Config::default());
    let id = CategoryId::new();
    model.selected_category = Some(id);
    assert_eq!(
        update(&mut model, AppMsg::Navigation(NavigationMsg::CreateNote)),
        vec![Effect::CreateCategoryNote { category_id: id }]
    );
}

#[test]
fn new_category_should_load_choices_for_the_shared_creation_form() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(&mut model, AppMsg::Templates(TemplatesMsg::NewCategory));
    assert!(matches!(
        &effects[..],
        [Effect::LoadTemplates {
            purpose: TemplatePurpose::NewCategory,
            ..
        }]
    ));
}

#[test]
fn category_creation_should_forward_the_selected_template() {
    let mut model = AppModel::new(&Config::default());
    let template_id = carver_sdk::TemplateId::new();
    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategoryWithAppearance {
            name: " Work ".into(),
            appearance: carver_sdk::CategoryAppearance::default(),
            template_id: Some(template_id),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::CreateCategoryWithAppearance { name, template_id: Some(id), .. } if name == "Work" && *id == template_id)));
}

#[test]
fn template_deletion_should_acknowledge_revision_without_reloading_browser_on_wakeup() {
    let mut model = AppModel::new(&Config::default());
    model.library_revision = Some(carver_sdk::LibraryRevision(10));
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Changed {
            id: carver_sdk::TemplateId::new(),
            result: Ok(()),
        }),
    );
    let request_id = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadLibraryRevision { request_id } => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("local revision acknowledgment"));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadBrowser { .. }))
    );
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
                request_id,
                result: Ok(carver_sdk::LibraryRevision(11)),
            })
        )
        .is_empty()
    );
    let effects = update(&mut model, AppMsg::LibraryChangedExternally);
    let request_id = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadLibraryRevision { request_id } => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("focus revision check"));
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
                request_id,
                result: Ok(carver_sdk::LibraryRevision(11)),
            })
        )
        .is_empty()
    );
}

#[test]
fn template_deletion_should_clear_only_matching_category_assignments_without_reloading_sidebar() {
    let mut model = AppModel::new(&Config::default());
    let id = carver_sdk::TemplateId::new();
    let category = carver_sdk::Category {
        id: CategoryId::new(),
        name: "Work".into(),
        appearance: carver_sdk::CategoryAppearance::default(),
        position: 0,
        created_at: time::OffsetDateTime::UNIX_EPOCH,
        updated_at: time::OffsetDateTime::UNIX_EPOCH,
        trashed_at: None,
        default_template_id: Some(id),
    };
    let mut other = category.clone();
    other.id = CategoryId::new();
    other.default_template_id = Some(carver_sdk::TemplateId::new());
    model.sidebar.state = LoadState::Ready(vec![
        carver_sdk::CategorySummary {
            category,
            note_count: 2,
        },
        carver_sdk::CategorySummary {
            category: other.clone(),
            note_count: 3,
        },
    ]);
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Changed { id, result: Ok(()) }),
    );
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::LoadSidebar { .. } | Effect::LoadBrowser { .. }
    )));
    let LoadState::Ready(categories) = model.sidebar.state else {
        panic!("ready sidebar");
    };
    assert_eq!(categories[0].category.default_template_id, None);
    assert_eq!(categories[0].note_count, 2);
    assert_eq!(categories[1].category, other);
}

#[test]
fn picker_should_preview_selection_and_require_explicit_creation() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);
    let template = carver_sdk::NoteTemplate {
        id: carver_sdk::TemplateId::new(),
        name: "Meeting".into(),
        source: "# Agenda".into(),
        revision: carver_sdk::Revision(1),
        created_at: time::OffsetDateTime::UNIX_EPOCH,
        updated_at: time::OffsetDateTime::UNIX_EPOCH,
    };
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Pick));
    let request_id = model.template_request.unwrap_or(RequestId(0));
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Loaded {
            request_id,
            purpose: TemplatePurpose::Pick(category_id),
            result: Ok(vec![template.clone()]),
        }),
    );
    assert!(matches!(
        &update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::SelectPreview {
                request_id,
                id: Some(template.id)
            })
        )[..],
        [Effect::ShowTemplatePreview {
            preview: Some(Ok(_)),
            ..
        }]
    ));
    assert_eq!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::CreateSelected(request_id))
        ),
        vec![Effect::CreateTemplateNote {
            category_id,
            template_id: template.id
        }]
    );
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::PickerClosed(request_id)),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::CreateSelected(request_id))
        )
        .is_empty()
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::SelectPreview {
                request_id,
                id: Some(template.id)
            })
        )
        .is_empty()
    );
}
#[test]
fn draft_preview_should_ignore_a_closed_editor() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let request_id = model.template_editor.unwrap_or(RequestId(0));
    assert!(matches!(
        &update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::PreviewDraft {
                request_id,
                source: "# Draft".into()
            })
        )[..],
        [Effect::ShowDraftProperties { preview: Ok(_), .. }]
    ));
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::EditorClosed(request_id)),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::PreviewDraft {
                request_id,
                source: "# Draft".into()
            })
        )
        .is_empty()
    );
}

#[test]
fn saving_template_from_note_should_finish_without_opening_manager() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    model.template_editor_from_note = true;
    let request_id = model.template_editor.unwrap_or(RequestId(0));
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Saved {
            request_id,
            result: Ok(()),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::FinishTemplateEdit { error: None, .. }))
    );
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::LoadTemplates {
            purpose: TemplatePurpose::Manage,
            ..
        }
    )));
}

#[test]
fn template_insertion_should_ignore_confirmation_after_the_note_changes() -> Result<(), String> {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Original".into(),
        }),
    );
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::InsertCaptured(0..0)),
    );
    let Some(Effect::LoadTemplates {
        request_id,
        purpose,
    }) = effects.first()
    else {
        return Err("template load".into());
    };
    let request_id = *request_id;
    let purpose = purpose.clone();
    let template = carver_sdk::NoteTemplate {
        id: carver_sdk::TemplateId::new(),
        name: "Snippet".into(),
        source: "Inserted".into(),
        revision: Revision(1),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    };
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Loaded {
            request_id,
            purpose,
            result: Ok(vec![template]),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Changed".into())),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::CreateSelected(request_id))
        )
        .is_empty()
    );
    assert_eq!(
        model.editor.as_ref().map(|d| d.source.as_str()),
        Some("Changed")
    );
    Ok(())
}

#[test]
fn saving_as_template_from_a_background_note_should_copy_unsaved_source() -> Result<(), String> {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "# Unsaved background".into(),
        }),
    );
    let document = model.editor.take().ok_or("editor")?;
    model
        .tabs
        .background
        .insert(crate::mvu::model::TabId(42), document);
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::FromNote(note_id)),
    );
    assert!(
        matches!(&effects[..], [Effect::ShowTemplateEditor { source, .. }] if source == "# Unsaved background")
    );
    assert!(model.template_editor_from_note);
    Ok(())
}

#[test]
fn saving_as_template_from_an_unopened_note_should_load_its_source() -> Result<(), String> {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::FromNote(note_id)),
    );
    let Some(Effect::LoadTemplateNote {
        request_id,
        note_id: requested,
    }) = effects.first()
    else {
        return Err("load note".into());
    };
    assert_eq!(*requested, note_id);
    let request_id = *request_id;
    let note = Note {
        id: note_id,
        category_id: CategoryId::new(),
        title: "Stored note".into(),
        source: "# Stored note".into(),
        plain_text: "Stored note".into(),
        is_favorite: false,
        revision: Revision(2),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        trashed_at: None,
    };
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::NoteLoaded {
            request_id,
            result: Ok(note),
        }),
    );
    assert!(
        matches!(&effects[..], [Effect::ShowTemplateEditor { source, .. }] if source == "# Stored note")
    );
    Ok(())
}

#[test]
fn failed_note_loading_should_show_an_error_without_opening_an_editor() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::FromNote(NoteId::new())),
    );
    let request_id = model.template_request.unwrap_or(RequestId(0));
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::NoteLoaded {
                request_id,
                result: Err(UiError::new("missing note"))
            })
        )
        .is_empty()
    );
    assert_eq!(
        model.notice.as_ref().map(|n| n.message.as_str()),
        Some("missing note")
    );
    assert!(model.template_editor.is_none());
}

#[test]
fn stale_note_loading_should_not_open_an_editor() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::FromNote(NoteId::new())),
    );
    let request_id = model.template_request.unwrap_or(RequestId(0));
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Manage));
    assert!(
        update(
            &mut model,
            AppMsg::Templates(TemplatesMsg::NoteLoaded {
                request_id,
                result: Err(UiError::new("stale"))
            })
        )
        .is_empty()
    );
    assert!(model.notice.is_none());
}

#[test]
fn failed_template_save_should_keep_the_editor_and_restore_its_controls() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Templates(TemplatesMsg::Edit(None)));
    let request_id = model.template_editor.unwrap_or(RequestId(0));
    let effects = update(
        &mut model,
        AppMsg::Templates(TemplatesMsg::Saved {
            request_id,
            result: Err(UiError::new("revision conflict")),
        }),
    );
    assert_eq!(model.template_editor, Some(request_id));
    assert!(
        matches!(&effects[..], [Effect::FinishTemplateEdit { error: Some(error), .. }] if error.message == "revision conflict")
    );
}
