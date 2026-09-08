use super::*;
use carver_editor_protocol::DocumentTarget;

fn model() -> AppModel {
    let mut config = Config::default();
    config.editor.last_mode = carver_config::EditorMode::Source;
    let mut model = AppModel::new(&config);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(7),
            source: "# First\n\n## Same\n\nBody\n\n## Same".into(),
        }),
    );
    model
}

#[test]
fn heading_activation_should_focus_the_requested_occurrence_without_editing() {
    let mut model = model();
    let before = document(&model).clone();
    let target = DocumentTarget::Heading { occurrence: 2 };
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::FocusDocumentTarget {
            session: before.session,
            generation: before.source_generation,
            target: target.clone(),
        }),
    );
    let document = document(&model);
    let start = document.analysis.headings()[2].text_start;
    assert_eq!(
        effects,
        vec![Effect::FocusDocumentTarget {
            session: before.session,
            generation: 0,
            target,
            selection: start..start
        }]
    );
    assert_eq!(document.selected_heading, Some(2));
    assert_eq!(document.source, before.source);
    assert_eq!(document.revision, before.revision);
    assert_eq!(document.save_state, before.save_state);
    assert!(!document.document_sidebar.is_visible());
    assert!(std::sync::Arc::ptr_eq(&before.analysis, &document.analysis));
}

#[test]
fn source_selection_should_track_only_the_enclosing_heading() {
    let mut model = model();
    for (selection, expected) in [
        (12..12, Some(1)),
        (16..16, Some(1)),
        (31..31, Some(2)),
        (19..19, None),
        (2..25, None),
    ] {
        let _ = update(
            &mut model,
            AppMsg::Editor(EditorMsg::SourceSelectionChanged { selection }),
        );
        assert_eq!(document(&model).selected_heading, expected);
    }
}

#[test]
fn navigation_should_reject_rows_from_an_older_source_or_session() {
    let mut model = model();
    let before = document(&model).clone();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("# Replaced".into())),
    );
    for (session, generation) in [
        (before.session, 0),
        (EditorSessionId(before.session.0 + 1), 1),
    ] {
        assert!(
            update(
                &mut model,
                AppMsg::Editor(EditorMsg::FocusDocumentTarget {
                    session,
                    generation,
                    target: DocumentTarget::Heading { occurrence: 0 }
                })
            )
            .is_empty()
        );
    }
    assert_eq!(document(&model).selected_heading, None);
}

#[test]
fn projection_selection_should_reject_stale_source_and_track_duplicate_headings() {
    let mut model = model();
    let before = document(&model).clone();
    for (source, heading, expected) in [
        (before.source.as_str(), Some(2), Some(2)),
        ("# Old", Some(0), Some(2)),
        (before.source.as_str(), None, None),
    ] {
        let _ = update(
            &mut model,
            AppMsg::Editor(EditorMsg::DocumentSelectionChanged {
                session: before.session,
                mode: before.mode,
                source: std::rc::Rc::from(source),
                media: None,
                heading,
            }),
        );
        assert_eq!(document(&model).selected_heading, expected);
    }
}

#[test]
fn changing_source_should_rebuild_outline_and_clear_previous_selection() {
    let mut model = model();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceSelectionChanged { selection: 2..2 }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("### New\n\n# First".into())),
    );
    let document = document(&model);
    assert_eq!(
        document
            .analysis
            .headings()
            .iter()
            .map(|h| h.label.as_str())
            .collect::<Vec<_>>(),
        vec!["New", "First"]
    );
    assert_eq!(document.selected_heading, None);
    assert_eq!(document.source_generation, 1);
}

fn document(model: &AppModel) -> &crate::mvu::EditorDocument {
    let Some(document) = model.editor.as_ref() else {
        panic!("document should exist");
    };
    document
}
