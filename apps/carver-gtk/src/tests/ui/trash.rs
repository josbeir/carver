//! Display-backed regression coverage for the trash page composition.

use super::*;
use crate::mvu::{AppDispatcher, AppModel, LoadState, Route};
use crate::ui::trash::build_trash;

pub(super) fn trash_rows_should_keep_their_card_surface() -> TestResult {
    let (window, view, list, _) = trash_fixture();
    window.present();
    view.render(&trash_model());
    assert!(run_main_context_until(|| list.first_child().is_some()));

    let row = list
        .first_child()
        .and_then(|heading| heading.next_sibling())
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("trashed note row")?;
    assert!(list.has_css_class("trash-feed"));
    assert!(!list.has_css_class("note-feed"));
    assert!(
        row.has_css_class("card"),
        "row {} classes: {:?}",
        row.widget_name(),
        row.css_classes()
    );
    assert!(
        row.has_css_class("note-card"),
        "row {} classes: {:?}",
        row.widget_name(),
        row.css_classes()
    );
    window.close();
    Ok(())
}

pub(super) fn trash_contents_should_use_one_page_scroller() -> TestResult {
    let (window, view, _, pages) = trash_fixture();
    window.present();
    view.render(&trash_model());
    assert!(run_main_context_until(|| {
        pages.visible_child_name().as_deref() == Some("contents")
    }));

    let scroll = widget_as::<gtk::ScrolledWindow>(pages.upcast_ref(), "trash-content-scroll")
        .ok_or("trash content scroll")?;
    assert!(
        pages
            .visible_child()
            .and_downcast::<gtk::ScrolledWindow>()
            .is_some()
    );
    let clamp = widget_as::<adw::Clamp>(pages.upcast_ref(), "trash-content-clamp")
        .ok_or("trash content clamp")?;
    assert!(clamp.is_ancestor(&scroll));
    assert!(clamp.child().and_downcast::<gtk::ListBox>().is_some());
    window.close();
    Ok(())
}

fn trash_fixture() -> (gtk::Window, crate::view::ViewRefs, gtk::ListBox, gtk::Stack) {
    let dispatcher = AppDispatcher::default();
    let (trash, references) = build_trash(&dispatcher);
    let pages = gtk::Stack::new();
    pages.add_named(&trash, Some("trash"));
    let list = references.list.clone();
    let trash_pages = references.pages.clone();
    let view = crate::view::ViewRefs::new(
        pages.clone(),
        adw::StatusPage::new(),
        references.status.clone(),
    )
    .with_trash(references.list, references.pages, references.empty_button);
    let window = gtk::Window::builder()
        .default_width(760)
        .default_height(600)
        .child(&pages)
        .build();
    (window, view, list, trash_pages)
}

fn trash_model() -> AppModel {
    let mut model = AppModel::new(&carver_config::Config::default());
    model.route = Route::Trash;
    model.trash.state = LoadState::Ready(carver_sdk::TrashContents {
        categories: Vec::new(),
        notes: vec![carver_sdk::TrashedNoteSummary {
            id: carver_sdk::NoteId::new(),
            category_id: carver_sdk::CategoryId::new(),
            category_name: String::from("Notes"),
            title: String::from("Trashed note"),
            excerpt: String::from("A recoverable note"),
            trashed_at: time::OffsetDateTime::UNIX_EPOCH,
            has_images: false,
        }],
    });
    model
}
