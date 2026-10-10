use super::*;
use crate::mvu::{AppModel, LoadState, RequestId};
use crate::ui::tests::support::TestResult;

fn document() -> EditorDocument {
    let mut model = AppModel::new(&carver_config::Config::default());
    let _ = crate::mvu::update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: carver_sdk::NoteId::new(),
            revision: carver_sdk::Revision(1),
            source: "# First\n\nBody\n\n## Second\n\nEnd".to_owned(),
        }),
    );
    model.editor.unwrap_or_else(|| panic!("loaded document"))
}

pub(crate) fn links_should_preserve_rows_during_same_session_reload() {
    let dispatcher = AppDispatcher::default();
    let sidebar = DocumentSidebar::new(&gtk::Box::default(), gtk::ToggleButton::new(), &dispatcher);
    let mut document = document();
    let linked = carver_sdk::NoteSummary {
        id: carver_sdk::NoteId::new(),
        category_id: document.category_id,
        category_name: "Notes".to_owned(),
        title: "Linked note".to_owned(),
        excerpt: String::new(),
        revision: carver_sdk::Revision(1),
        is_favorite: false,
        updated_at: time::OffsetDateTime::UNIX_EPOCH,
        has_images: false,
    };
    let links = carver_sdk::NoteLinks {
        outgoing: vec![linked],
        backlinks: Vec::new(),
    };
    document.links.state = LoadState::Ready(links.clone());
    let render = |document: &EditorDocument| {
        sidebar.render(
            document,
            carver_config::DocumentSidebarPage::Links,
            &dispatcher,
        );
    };
    render(&document);
    let row = sidebar.linked_rows.borrow()[0].clone();
    assert_eq!(sidebar.links_stack_page.badge_number(), 1);
    document.links.state = LoadState::Loading(RequestId(10));
    render(&document);
    assert_eq!(sidebar.linked_rows.borrow()[0], row);
    assert_eq!(sidebar.links_stack_page.badge_number(), 1);
    document.links.state = LoadState::Ready(links);
    render(&document);
    assert_eq!(sidebar.linked_rows.borrow()[0], row);
    document.links.state = LoadState::Ready(carver_sdk::NoteLinks::default());
    render(&document);
    assert_ne!(sidebar.linked_rows.borrow()[0], row);
    assert_eq!(sidebar.links_stack_page.badge_number(), 0);
}

pub(crate) fn links_should_clear_previous_session_rows_while_loading() {
    let dispatcher = AppDispatcher::default();
    let sidebar = DocumentSidebar::new(&gtk::Box::default(), gtk::ToggleButton::new(), &dispatcher);
    let mut document = document();
    document.links.state = LoadState::Ready(carver_sdk::NoteLinks::default());
    sidebar.render(
        &document,
        carver_config::DocumentSidebarPage::Links,
        &dispatcher,
    );
    let row = sidebar.linked_rows.borrow()[0].clone();
    document.session = EditorSessionId(document.session.0 + 1);
    document.links.state = LoadState::Loading(RequestId(10));
    sidebar.render(
        &document,
        carver_config::DocumentSidebarPage::Links,
        &dispatcher,
    );
    assert_ne!(sidebar.linked_rows.borrow()[0], row);
    assert!(sidebar.rendered_links.borrow().is_none());
}

pub(crate) fn outline_should_reuse_rows_and_navigate_current_source() -> TestResult {
    let fixture = crate::ui::tests::ui::document_sidebar::fixture()?;
    let category = fixture.client.create_category("Outline reuse")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "# First\n\nBody\n\n## Second\n\nEnd".to_owned(),
    }));
    let root = &fixture.surface;
    let view = crate::ui::tests::support::widget_as::<gtk::ListView>(root, "editor-outline-list")
        .ok_or("outline")?;
    let model = view.model().ok_or("outline model")?;
    let first = model.item(0).ok_or("first row")?;
    let second = model.item(1).ok_or("second row")?;
    let source = crate::ui::tests::support::widget_as::<sourceview5::View>(root, "source-editor")
        .ok_or("source")?;
    let buffer = source.buffer();
    let mut cursor = buffer.iter_at_offset(9);
    buffer.insert(&mut cursor, "Longer body ");
    assert_eq!(model.item(0).as_ref(), Some(&first));
    assert_eq!(model.item(1).as_ref(), Some(&second));
    view.emit_by_name::<()>("activate", &[&1_u32]);
    assert!(crate::ui::tests::support::run_main_context_until(|| buffer
        .iter_at_mark(&buffer.get_insert())
        .line()
        == 4));
    let mut start = buffer.start_iter();
    buffer.insert(&mut start, "# New\n\n");
    assert_ne!(model.item(0).as_ref(), Some(&first));
    let rebuilt = model.item(0).ok_or("rebuilt row")?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string(),
    }));
    assert_ne!(model.item(0).as_ref(), Some(&rebuilt));
    fixture.window.destroy();
    Ok(())
}
