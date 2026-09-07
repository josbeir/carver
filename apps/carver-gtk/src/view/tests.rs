use carver_config::Config;
use carver_sdk::{
    Category, CategoryAppearance, CategoryColor, CategoryIcon, CategoryId, CategorySummary, NoteId,
    NoteSummary, Revision,
};
use time::{Duration, OffsetDateTime};

use super::{LoadState, browser_projection_snapshot, note_category_color};

#[test]
fn sidebar_snapshot_should_preserve_the_last_ready_projection_during_a_reload() {
    let mut model = crate::mvu::AppModel::new(&Config::default());
    model.sidebar.state = LoadState::Ready(Vec::new());
    let snapshot = super::SidebarSnapshot::from_model(&model);

    model.sidebar.state = LoadState::Loading(crate::mvu::RequestId(1));
    assert!(super::SidebarSnapshot::from_model(&model).is_none());

    model.sidebar.state = LoadState::Ready(Vec::new());
    assert_eq!(super::SidebarSnapshot::from_model(&model), snapshot);

    model.sidebar.state = LoadState::Failed(crate::mvu::UiError::new("offline"));
    assert!(super::SidebarSnapshot::from_model(&model).is_none());
}

#[test]
fn note_category_color_should_use_the_category_appearance() {
    let category_id = CategoryId::new();
    let note = NoteSummary {
        id: NoteId::new(),
        category_id,
        category_name: String::from("Ideas"),
        title: String::from("A note"),
        excerpt: String::new(),
        revision: Revision(1),
        is_favorite: false,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        has_images: false,
    };
    let sidebar = LoadState::Ready(vec![CategorySummary {
        category: Category {
            id: category_id,
            name: String::from("Ideas"),
            appearance: CategoryAppearance {
                icon: CategoryIcon::Folder,
                color: CategoryColor::Purple,
            },
            position: 0,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            trashed_at: None,
        },
        note_count: 1,
    }]);

    let color = note_category_color(&note, &sidebar);

    assert_eq!(color, Some(CategoryColor::Purple));
}

#[test]
fn browser_projection_snapshot_should_not_match_when_the_day_changes() {
    let model = crate::mvu::AppModel::new(&Config::default());
    let today = OffsetDateTime::UNIX_EPOCH.date();
    let tomorrow = today + Duration::DAY;
    let snapshot = browser_projection_snapshot(&model, today);

    assert!(!snapshot.matches(&model, tomorrow));
}

#[test]
fn browser_projection_snapshot_should_not_match_when_the_route_changes() {
    let mut model = crate::mvu::AppModel::new(&Config::default());
    let today = OffsetDateTime::UNIX_EPOCH.date();
    let snapshot = browser_projection_snapshot(&model, today);
    model.route = crate::mvu::Route::Trash;

    assert!(!snapshot.matches(&model, today));
}

#[test]
fn browser_projection_snapshot_should_match_an_unchanged_model() {
    let model = crate::mvu::AppModel::new(&Config::default());
    let today = OffsetDateTime::UNIX_EPOCH.date();
    let snapshot = browser_projection_snapshot(&model, today);

    assert!(snapshot.matches(&model, today));
}
