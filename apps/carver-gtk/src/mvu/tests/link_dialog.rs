use super::*;
use crate::mvu::LinkDialogOrigin;

fn loaded_model() -> AppModel {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("# Note"),
        }),
    );
    model
}

fn open(model: &mut AppModel, origin: LinkDialogOrigin) -> RequestId {
    let _ = update(
        model,
        AppMsg::Editor(EditorMsg::LinkDialogRequested { origin }),
    );
    let Some(dialog) = model.editor_link_dialog.as_ref() else {
        panic!("the link dialog should open for an active editor");
    };
    dialog.dialog_id
}

#[test]
fn requesting_the_link_dialog_should_present_it() {
    let mut model = loaded_model();
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogRequested {
            origin: LinkDialogOrigin::Rich {
                text: String::new(),
                destination: String::new(),
            },
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowLinkDialog { .. }]
    ));
    assert!(model.editor_link_dialog.is_some());
}

#[test]
fn link_dialog_query_should_debounce_then_search() {
    let mut model = loaded_model();
    let _ = open(
        &mut model,
        LinkDialogOrigin::Rich {
            text: String::new(),
            destination: String::new(),
        },
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogQueryChanged(String::from("rel"))),
    );
    let timer_id = match effects.as_slice() {
        [Effect::ScheduleLinkSearch { timer_id }] => *timer_id,
        _ => panic!("a query should schedule one debounced search"),
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogSearchElapsed { timer_id }),
    );
    let request_id = match effects.as_slice() {
        [Effect::SearchLinkCandidates { request_id, .. }] => *request_id,
        _ => panic!("the search timer should start one candidate search"),
    };

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::LinkCandidatesLoaded {
            request_id,
            result: Ok(Vec::new()),
        }),
    );
    assert!(matches!(
        model
            .editor_link_dialog
            .as_ref()
            .map(|dialog| &dialog.candidates.state),
        Some(LoadState::Ready(_))
    ));
}

#[test]
fn an_empty_link_dialog_query_should_not_search() {
    let mut model = loaded_model();
    let _ = open(
        &mut model,
        LinkDialogOrigin::Rich {
            text: String::new(),
            destination: String::new(),
        },
    );
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogQueryChanged(String::new())),
    );
    let timer_id = match effects.as_slice() {
        [Effect::ScheduleLinkSearch { timer_id }] => *timer_id,
        _ => panic!("a query change should schedule a debounce"),
    };
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogSearchElapsed { timer_id }),
    );
    assert!(effects.is_empty());
}

#[test]
fn confirming_a_rich_link_dialog_should_insert_it() {
    let mut model = loaded_model();
    let dialog_id = open(
        &mut model,
        LinkDialogOrigin::Rich {
            text: String::from("Doc"),
            destination: String::new(),
        },
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogConfirmed {
            dialog_id,
            text: String::from("Doc"),
            destination: String::from("https://example.com"),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::ApplyRichEditorCommand { .. }]
    ));
    assert!(model.editor_link_dialog.is_none());
}

#[test]
fn confirming_a_source_link_dialog_should_apply_a_source_command() {
    let mut model = loaded_model();
    let dialog_id = open(
        &mut model,
        LinkDialogOrigin::Source {
            selection: 0..0,
            text: String::new(),
        },
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogConfirmed {
            dialog_id,
            text: String::from("Carver"),
            destination: String::from("https://example.com"),
        }),
    );

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::SelectEditorSource { .. }))
    );
    assert!(
        model
            .editor
            .as_ref()
            .is_some_and(|document| document.source.contains("[Carver](https://example.com)"))
    );
}

#[test]
fn dismissing_the_link_dialog_should_clear_it() {
    let mut model = loaded_model();
    let dialog_id = open(
        &mut model,
        LinkDialogOrigin::Rich {
            text: String::new(),
            destination: String::new(),
        },
    );
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::LinkDialogDismissed(dialog_id)),
    );
    assert!(effects.is_empty());
    assert!(model.editor_link_dialog.is_none());
}
