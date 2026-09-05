use carver_sdk::{
    Category, CategoryAppearance, CategoryColor, CategoryIcon, CategoryId, CategorySummary, NoteId,
    NoteSummary,
};
use time::OffsetDateTime;

use super::{LoadState, note_category_color};

#[test]
fn note_category_color_should_use_the_category_appearance() {
    let category_id = CategoryId::new();
    let note = NoteSummary {
        id: NoteId::new(),
        category_id,
        category_name: String::from("Ideas"),
        title: String::from("A note"),
        excerpt: String::new(),
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
