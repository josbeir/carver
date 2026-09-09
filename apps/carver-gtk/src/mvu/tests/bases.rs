use super::*;

#[test]
fn opening_a_base_should_change_route_and_load_its_rows() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();

    let effects = update(&mut model, AppMsg::Bases(BasesMsg::Open(base_id)));

    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
    assert!(
        matches!(effects.as_slice(), [Effect::LoadBaseRows { base_id: loaded, .. }] if *loaded == base_id)
    );
}

#[test]
fn created_base_should_reload_definitions_and_open_its_grid() {
    let mut model = AppModel::new(&Config::default());
    let base = BaseDefinition {
        id: BaseId::new(),
        name: "Projects".to_owned(),
        columns: vec![BaseColumn::Category],
        revision: Revision(1),
        row_count: 0,
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseCreated {
            result: Ok(base.clone()),
        }),
    );

    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base.id));
    assert!(
        matches!(effects.as_slice(), [Effect::LoadBases { .. }, Effect::LoadBaseRows { base_id, .. }] if *base_id == base.id)
    );
}

#[test]
fn closing_a_note_opened_from_a_base_should_restore_the_base_route() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let note_id = NoteId::new();

    let _ = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote(note_id)),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));

    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadBaseRows { base_id: loaded, .. }] if *loaded == base_id
    ));
    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
}
