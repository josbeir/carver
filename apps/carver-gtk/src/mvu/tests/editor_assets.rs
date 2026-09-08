use super::*;

#[test]
fn trashing_the_open_editor_note_should_close_its_session_before_the_effect_runs() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Open note"),
        }),
    );

    let effects = update(&mut model, AppMsg::Action(ActionMsg::TrashNote(note_id)));

    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.editor, None);
    assert_eq!(effects, vec![Effect::TrashNote { note_id }]);
}

#[test]
fn a_successful_note_trash_should_offer_undo_until_the_note_is_restored() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(&mut model, AppMsg::Action(ActionMsg::TrashNote(note_id)));
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::TrashNote(note_id),
            result: Ok(()),
        }),
    );
    assert_eq!(model.undo_trash_note, Some(note_id));

    assert_eq!(
        update(&mut model, AppMsg::Trash(TrashMsg::RestoreNote(note_id))),
        vec![Effect::RestoreNote { note_id }]
    );
    assert_eq!(model.undo_trash_note, None);
}

#[test]
fn stale_editor_close_should_not_close_a_newer_document() {
    let mut model = AppModel::new(&Config::default());
    let first_note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: first_note_id,
            revision: Revision(1),
            source: "first".to_owned(),
        }),
    );
    let Some(first_session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(2),
            source: "second".to_owned(),
        }),
    );

    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(first_session)));

    assert_eq!(model.route, Route::Editor);
    assert_ne!(
        model.editor.as_ref().map(|document| document.session),
        Some(first_session)
    );
}

#[test]
fn source_event_after_editor_close_should_be_ignored() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "active".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(session)));

    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::SourceChanged("stale widget event".to_owned())),
        )
        .is_empty()
    );
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.editor, None);
}

#[test]
fn pasted_image_should_store_an_asset_and_update_the_current_document() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Before".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };

    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::PasteImage {
                extension: "png".to_owned(),
                bytes: vec![1, 2, 3],
            }),
        ),
        vec![Effect::StoreEditorAsset {
            image: true,
            session,
            note_id,
            extension: "png".to_owned(),
            bytes: vec![1, 2, 3],
            alt: "Pasted image".to_owned(),
            source_target: None,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: "Pasted image".to_owned(),
            source_target: None,
            result: Ok("assets/pasted.png".to_owned()),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [
            Effect::SchedulePreview { session: preview_session, .. },
            Effect::ScheduleEditorSave { session: save_session, .. },
            Effect::ReloadRichEditor { session: reload_session, source },
        ] if *preview_session == session
            && *save_session == session
            && *reload_session == session
            && source == "Before\n![Pasted image](assets/pasted.png)\n"
    ));
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before\n![Pasted image](assets/pasted.png)\n")
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(session)));
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::EditorAssetStored {
                image: true,
                session,
                alt: "Pasted image".to_owned(),
                source_target: None,
                result: Ok("assets/stale.png".to_owned()),
            }),
        )
        .is_empty()
    );
    assert_eq!(model.editor, None);
}

#[test]
fn source_image_paste_should_replace_the_captured_cursor_selection() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Before remove After".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let target = SourceImageTarget {
        source: String::from("Before remove After"),
        selection: 7..13,
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportImage {
            extension: "png".to_owned(),
            bytes: vec![1, 2, 3],
            alt: "Pasted image".to_owned(),
            source_target: Some(target.clone()),
        }),
    );
    assert!(matches!(effects.as_slice(), [Effect::StoreEditorAsset {
            image: true, source_target: Some(actual), .. }] if actual == &target));

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: "Pasted image".to_owned(),
            source_target: Some(target),
            result: Ok("assets/pasted.png".to_owned()),
        }),
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before ![Pasted image](assets/pasted.png) After")
    );
}

#[test]
fn file_import_should_insert_a_managed_link_and_refresh_media() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };

    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportFile {
            extension: String::from("pdf"),
            bytes: vec![1, 2, 3],
            name: String::from("Project brief.pdf"),
            source_target: None,
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: false,
            session,
            alt: String::from("Project brief.pdf"),
            source_target: None,
            result: Ok(String::from("assets/brief.pdf")),
        }),
    );

    let Some(document) = model.editor.as_ref() else {
        panic!("editor should remain open");
    };
    assert_eq!(document.source, "[Project brief.pdf](assets/brief.pdf)\n");
    assert_eq!(document.media.len(), 1);
}

#[test]
fn media_sidebar_should_focus_only_current_occurrences() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("![Diagram](assets/diagram.png)"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleMediaSidebar));
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should be open");
    };
    assert!(document.media_sidebar.is_visible());
    let selection = document.media[0].range.clone();
    let session = document.session;

    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::FocusMedia { selection })
        ),
        vec![Effect::FocusEditorMedia {
            session,
            selection: 0..30,
            path: String::from("assets/diagram.png"),
            occurrence: 0,
        }]
    );
}

#[test]
fn source_image_import_should_not_replace_text_changed_while_asset_stores() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before remove After"),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let target = SourceImageTarget {
        source: String::from("Before remove After"),
        selection: 7..13,
    };

    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportImage {
            extension: String::from("png"),
            bytes: vec![1, 2, 3],
            alt: String::from("Pasted image"),
            source_target: Some(target.clone()),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from(
            "Before typed remove After",
        ))),
    );
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: String::from("Pasted image"),
            source_target: Some(target),
            result: Ok(String::from("assets/pasted.png")),
        }),
    );

    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before typed remove After\n![Pasted image](assets/pasted.png)\n")
    );
}

#[test]
fn source_change_should_update_the_canonical_document_and_mark_it_dirty() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let unsupported_source = "::: unsupported Carve block\\nverbatim".to_owned();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(4),
            source: "Initial source".to_owned(),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(unsupported_source.clone())),
    );

    assert_eq!(
        effects,
        vec![Effect::SchedulePreview {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }]
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));

    assert_eq!(
        effects,
        vec![Effect::ScheduleEditorSave {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
            delay_ms: 500,
        }]
    );
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should remain open");
    };
    assert_eq!(document.session, super::EditorSessionId(1));
    assert_eq!(document.note_id, note_id);
    assert_eq!(document.revision, Revision(4));
    assert_eq!(document.source, unsupported_source);
    assert_eq!(document.mode, carver_config::EditorMode::Rich);
    assert_eq!(document.save_state, super::EditorSaveState::Dirty);
}

#[test]
fn media_details_should_load_once_and_ignore_replies_from_a_closed_document() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("![One](assets/a.png)\n\n![Two](assets/a.png)"),
        }),
    );
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should exist");
    };
    let session = document.session;
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::ToggleMediaSidebar));
    assert_eq!(
        effects,
        vec![
            Effect::PersistConfig {
                config: model.config.clone()
            },
            Effect::LoadMediaFile {
                session,
                note_id,
                path: String::from("assets/a.png"),
                image: true
            },
        ]
    );
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should exist");
    };
    let selection = document.media[1].range.clone();
    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::FocusMedia {
                selection: selection.clone()
            })
        ),
        vec![Effect::FocusEditorMedia {
            session,
            selection,
            path: String::from("assets/a.png"),
            occurrence: 1
        },]
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Another document"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::MediaFileLoaded {
            image: true,
            session,
            path: String::from("assets/a.png"),
            file: Some(crate::mvu::MediaFile {
                size: 1024,
                preview: None,
            }),
        }),
    );
    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.media_files.is_empty())
    );
}

#[test]
fn preview_should_request_only_an_authored_managed_attachment() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("[Brief](assets/brief.pdf)"),
        }),
    );
    let Some(document) = model.editor.as_ref() else {
        panic!("document");
    };
    let selection = document.media[0].range.clone();
    let session = document.session;
    let before = document.clone();
    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::PreviewMedia { selection })
        ),
        vec![Effect::PrepareMediaPreview {
            session,
            note_id,
            path: String::from("assets/brief.pdf"),
            label: String::from("Brief")
        }]
    );
    assert_eq!(model.editor.as_ref(), Some(&before));
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::PreviewMedia {
                selection: 100..200
            })
        )
        .is_empty()
    );
}

#[test]
fn prepared_preview_should_be_ignored_after_switching_documents() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Another note"),
        }),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::MediaPreviewPrepared {
                session: EditorSessionId(999),
                result: Ok(std::path::PathBuf::from("/tmp/preview.pdf")),
            })
        )
        .is_empty()
    );
}

#[test]
fn editor_media_selection_should_track_occurrences_without_editing_source() {
    let mut model = AppModel::new(&Config::default());
    let source = "Before\n\n![One](assets/a.png)\n\n![Two](assets/a.png)";
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: source.into(),
        }),
    );
    let document = open_media_document(&model);
    let session = document.session;
    let mode = document.mode;
    let range = document.media[1].range.clone();
    let message = |session, mode, media| {
        AppMsg::Editor(EditorMsg::MediaSelected {
            session,
            mode,
            media,
        })
    };
    let media = carver_editor_protocol::MediaSelection {
        path: "assets/a.png".into(),
        occurrence: 1,
    };
    assert!(update(&mut model, message(session, mode, Some(media.clone()))).is_empty());
    assert_eq!(
        open_media_document(&model).selected_media,
        Some(range.clone())
    );
    let stale = EditorSessionId(session.0 + 1);
    let _ = update(&mut model, message(stale, mode, None));
    assert_eq!(open_media_document(&model).selected_media, Some(range));
    let _ = update(&mut model, message(session, mode, None));
    let document = open_media_document(&model);
    assert_eq!(document.selected_media, None);
    assert_eq!(document.source, source);
    assert_eq!(document.revision, Revision(1));
}

#[test]
fn source_caret_should_select_media_only_inside_its_range() {
    let mut config = Config::default();
    config.editor.last_mode = carver_config::EditorMode::Source;
    let mut model = AppModel::new(&config);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Before\n\n![One](assets/a.png)".into(),
        }),
    );
    let range = open_media_document(&model).media[0].range.clone();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceSelectionChanged {
            selection: range.start..range.start,
        }),
    );
    assert_eq!(
        open_media_document(&model).selected_media,
        Some(range.clone())
    );
    let session = open_media_document(&model).session;
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::MediaSelected {
            session,
            mode: carver_config::EditorMode::Rich,
            media: None,
        }),
    );
    assert_eq!(
        open_media_document(&model).selected_media,
        Some(range.clone())
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceSelectionChanged {
            selection: range.end..range.end,
        }),
    );
    assert_eq!(open_media_document(&model).selected_media, None);
}

fn open_media_document(model: &AppModel) -> &crate::mvu::model::EditorDocument {
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should be open");
    };
    document
}

#[test]
fn native_import_should_reject_a_different_document_and_preview_mode() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Untouched".into(),
        }),
    );
    let session = open_media_document(&model).session;
    let target = crate::mvu::ImportTarget {
        session: EditorSessionId(session.0 + 1),
        source: None,
    };
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ImportFiles {
                target: target.clone(),
                files: Vec::new()
            })
        )
        .is_empty()
    );
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ImportFilesStored {
                target,
                result: Ok(vec![crate::mvu::StoredMedia {
                    path: "assets/a.png".into(),
                    label: "A".into(),
                    image: true
                }])
            })
        )
        .is_empty()
    );
    let _ = update(
        &mut model,
        AppMsg::Preferences(PreferencesMsg::SetEditorMode(
            carver_config::EditorMode::Rendered,
        )),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ImportFiles {
                target: crate::mvu::ImportTarget {
                    session,
                    source: None
                },
                files: Vec::new()
            })
        )
        .is_empty()
    );
    assert_eq!(open_media_document(&model).source, "Untouched");
    assert_eq!(open_media_document(&model).revision, Revision(1));
}

#[test]
fn imported_files_should_replace_source_selection_once_in_selection_order() {
    let mut model = AppModel::new(&Config::default());
    let source = "Before replace After";
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: source.into(),
        }),
    );
    let session = open_media_document(&model).session;
    let target = crate::mvu::ImportTarget {
        session,
        source: Some(SourceImageTarget {
            source: source.into(),
            selection: 7..14,
        }),
    };
    let files = vec![
        crate::mvu::StoredMedia {
            path: "assets/first.bin".into(),
            label: "First".into(),
            image: true,
        },
        crate::mvu::StoredMedia {
            path: "assets/second.png".into(),
            label: "Second".into(),
            image: false,
        },
    ];
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportFilesStored {
            target,
            result: Ok(files),
        }),
    );
    assert_eq!(
        open_media_document(&model).source,
        "Before ![First](assets/first.bin)\n[Second](assets/second.png) After"
    );
}

#[test]
fn imported_attachment_label_should_escape_backslashes() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let target = crate::mvu::ImportTarget {
        session: open_media_document(&model).session,
        source: None,
    };
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportFilesStored {
            target,
            result: Ok(vec![crate::mvu::StoredMedia {
                path: "assets/report.bin".into(),
                label: "report\\".into(),
                image: false,
            }]),
        }),
    );
    let document = open_media_document(&model);
    assert_eq!(document.source, "[report\\\\](assets/report.bin)\n");
    assert_eq!(document.media.len(), 1);
}

#[test]
fn pasted_image_should_keep_image_markup_when_deduplication_returns_an_attachment_path() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let session = open_media_document(&model).session;
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            image: true,
            alt: "Pasted".into(),
            source_target: None,
            result: Ok("assets/image.bin".into()),
        }),
    );
    assert_eq!(
        open_media_document(&model).source,
        "![Pasted](assets/image.bin)\n"
    );
}

#[test]
fn changing_attachment_to_image_should_reload_details_and_reject_stale_results() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "[Photo](assets/a.png)".into(),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleMediaSidebar));
    let session = open_media_document(&model).session;
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("![Photo](assets/a.png)".into())),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadMediaFile { image: true, .. }))
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::MediaFileLoaded {
            session,
            image: false,
            path: "assets/a.png".into(),
            file: Some(crate::mvu::MediaFile {
                size: 123,
                preview: None,
            }),
        }),
    );
    assert_eq!(
        open_media_document(&model).media_files.get("assets/a.png"),
        Some(&None)
    );
}
