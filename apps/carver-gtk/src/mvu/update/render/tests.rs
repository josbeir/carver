use super::*;
use crate::mvu::{EditorSessionId, TimerId};

fn fixture() -> AppModel {
    let mut model = AppModel::new(&carver_config::Config::default());
    let _ = super::super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: carver_sdk::NoteId::new(),
            revision: carver_sdk::Revision(1),
            source: "# Heading\n\nBody".into(),
        }),
    );
    model
}

#[test]
fn autosave_request_should_schedule_work_without_rendering() {
    let mut model = fixture();
    let (effects, render) =
        update_for_view(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    assert!(!render);
    assert!(matches!(
        effects.as_slice(),
        [Effect::ScheduleEditorSave { .. }]
    ));
}

#[test]
fn autosave_elapsed_should_render_when_it_starts_saving() {
    let mut model = fixture();
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Changed".into())),
    );
    let (effects, _) = update_for_view(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let [
        Effect::ScheduleEditorSave {
            session, timer_id, ..
        },
    ] = effects.as_slice()
    else {
        panic!("save timer")
    };
    let (effects, render) = update_for_view(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed {
            session: *session,
            timer_id: *timer_id,
        }),
    );
    assert!(render);
    assert!(matches!(effects.as_slice(), [Effect::SaveNote { .. }]));
    assert!(matches!(
        model.editor.as_ref().map(|document| &document.save_state),
        Some(crate::mvu::EditorSaveState::Saving(_))
    ));
}

#[test]
fn selection_should_render_when_deferred_media_work_becomes_visible() {
    let mut model = fixture();
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("![Image](assets/test.png)".into())),
    );
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("document"));
    document.document_sidebar = crate::mvu::model::DocumentSidebarVisibility::Visible;
    document.media_files.clear();
    document.media_file_kinds.clear();
    let (effects, render) = update_for_view(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceSelectionChanged { selection: 0..0 }),
    );
    assert!(render);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadMediaFile { .. }))
    );
}

#[test]
fn source_selection_should_render_only_when_navigation_changes() {
    let mut model = fixture();
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("editor snapshot"))
        .mode = carver_config::EditorMode::Source;
    for (offset, expected) in [(12, false), (2, true), (3, false), (12, true)] {
        let (effects, render) = update_for_view(
            &mut model,
            AppMsg::Editor(EditorMsg::SourceSelectionChanged {
                selection: offset..offset,
            }),
        );
        assert_eq!(effects, []);
        assert_eq!(render, expected);
    }
}

#[test]
fn stale_timers_should_leave_the_view_unchanged() {
    let mut model = fixture();
    for message in [
        EditorMsg::PreviewElapsed {
            session: EditorSessionId(1),
            timer_id: TimerId(999),
        },
        EditorMsg::AutosaveElapsed {
            session: EditorSessionId(1),
            timer_id: TimerId(999),
        },
    ] {
        let (effects, render) = update_for_view(&mut model, AppMsg::Editor(message));
        assert_eq!(effects, []);
        assert!(!render);
    }
}

#[test]
fn hidden_preview_should_update_its_snapshot_without_rendering() {
    let mut model = fixture();
    for (mode, split, expected) in [
        (carver_config::EditorMode::Rich, false, false),
        (carver_config::EditorMode::Source, false, false),
        (carver_config::EditorMode::Source, true, true),
        (carver_config::EditorMode::Rendered, false, true),
    ] {
        model
            .editor
            .as_mut()
            .unwrap_or_else(|| panic!("editor snapshot"))
            .mode = mode;
        model.preferences.source_split_view = split;
        let Some(Effect::SchedulePreview { session, timer_id }) =
            super::super::schedule_preview(&mut model)
        else {
            panic!("preview timer")
        };
        let (_, render) = update_for_view(
            &mut model,
            AppMsg::Editor(EditorMsg::PreviewElapsed { session, timer_id }),
        );
        assert_eq!(render, expected);
        assert_eq!(
            model
                .editor_preview
                .as_ref()
                .unwrap_or_else(|| panic!("editor snapshot"))
                .source,
            "# Heading\n\nBody"
        );
    }
}

#[test]
fn rich_selection_should_render_only_when_formatting_changes() {
    let mut model = fixture();
    let document = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor snapshot"));
    let session = document.session;
    let source = document.source.clone();
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("editor snapshot"))
        .mode = carver_config::EditorMode::Rich;
    for (revision, (bold, expected)) in [(false, false), (true, true), (true, false), (false, true)]
        .into_iter()
        .enumerate()
    {
        let formatting = carver_editor_protocol::SelectionState {
            active: if bold {
                vec!["bold".into()]
            } else {
                Vec::new()
            },
            revision: u64::try_from(revision).unwrap_or_else(|_| panic!("revision")),
            navigation_epoch: u64::try_from(revision)
                .unwrap_or_else(|_| panic!("navigation epoch")),
            ..Default::default()
        };
        let (_, render) = update_for_view(
            &mut model,
            AppMsg::Editor(EditorMsg::DocumentSelectionChanged {
                session,
                source: source.clone().into(),
                mode: carver_config::EditorMode::Rich,
                heading: None,
                media: None,
                formatting: Some(formatting),
            }),
        );
        assert_eq!(render, expected);
    }
}
