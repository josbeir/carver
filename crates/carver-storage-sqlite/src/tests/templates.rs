use super::*;

#[test]
fn template_should_preserve_revision_and_time_when_save_is_unchanged() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    let template = library
        .insert_template(" Meeting ", "# Agenda\n", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let change = library
        .change_revision()
        .unwrap_or_else(|error| panic!("{error:?}"));
    let saved = library
        .update_template(
            template.id,
            template.revision,
            "Meeting",
            &template.source,
            now + time::Duration::hours(1),
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(saved, template);
    assert_eq!(
        library
            .change_revision()
            .unwrap_or_else(|error| panic!("{error:?}")),
        change
    );
}
#[test]
fn template_should_reject_stale_save_and_delete() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    let initial = library
        .insert_template("Meeting", "# Agenda", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let saved = library
        .update_template(
            initial.id,
            initial.revision,
            "Meeting",
            "# Changed",
            now + time::Duration::hours(1),
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(saved.revision, Revision(initial.revision.0 + 1));
    assert_eq!(saved.updated_at, now + time::Duration::hours(1));
    assert!(matches!(
        library.update_template(initial.id, initial.revision, "Meeting", "# Stale", now),
        Err(StorageError::TemplateConflict)
    ));
    assert!(matches!(
        library.remove_template(initial.id, initial.revision),
        Err(StorageError::TemplateConflict)
    ));
}
#[test]
fn deleting_template_should_clear_category_assignment_without_affecting_notes() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    let category = library
        .create_category("Meetings", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let template = library
        .insert_template("Meeting", "# Agenda", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    library
        .assign_category_template(category.id, Some(template.id), now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}"))[0]
            .default_template_id,
        Some(template.id)
    );
    let note = library
        .create_note_with_source(category.id, &template.source, now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    library
        .remove_template(template.id, template.revision)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}"))[0]
            .default_template_id,
        None
    );
    assert_eq!(
        library
            .note(note.id)
            .unwrap_or_else(|error| panic!("{error:?}")),
        Some(note)
    );
    assert!(
        library
            .list_templates()
            .unwrap_or_else(|error| panic!("{error:?}"))
            .is_empty()
    );
}
#[test]
fn templates_should_stay_outside_note_search_and_counts() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    let category = library
        .create_category("Projects", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    library
        .insert_template("UniqueTemplate", "# UniqueTemplate", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(
        library
            .search(
                "UniqueTemplate",
                None,
                PageRequest {
                    limit: 10,
                    offset: 0
                }
            )
            .unwrap_or_else(|error| panic!("{error:?}"))
            .items
            .is_empty()
    );
    assert_eq!(
        library
            .note_count(category.id)
            .unwrap_or_else(|error| panic!("{error:?}")),
        0
    );
    assert_eq!(
        library
            .list_category_summaries()
            .unwrap_or_else(|error| panic!("{error:?}"))[0]
            .note_count,
        0
    );
}
#[test]
fn category_update_should_roll_back_when_template_is_missing() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    let category = library
        .create_category("Original", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(
        library
            .update_template_category(
                category.id,
                "Changed",
                category.appearance,
                Some(TemplateId::new()),
                now
            )
            .is_err()
    );
    assert_eq!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}"))[0],
        category
    );
}
#[test]
fn template_should_reject_invalid_content_and_blank_names() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(
        library.insert_template(" ", "", now),
        Err(StorageError::InvalidTemplateName)
    ));
    assert!(
        library
            .insert_template("Bad", "---json\n[1]\n---", now)
            .is_err()
    );
    assert!(
        library
            .insert_template("Asset", "![Photo](assets/photo.png)", now)
            .is_err()
    );
    assert!(
        library
            .list_templates()
            .unwrap_or_else(|error| panic!("{error:?}"))
            .is_empty()
    );
}

#[test]
fn category_creation_should_persist_all_form_fields_together() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let template = library
        .insert_template("Meeting", "# Agenda", now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let appearance = CategoryAppearance {
        icon: CategoryIcon::Book,
        color: CategoryColor::Teal,
    };
    let category = library
        .create_category_with_template(" Work ", appearance, Some(template.id), now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(category.name, "Work");
    assert_eq!(category.appearance, appearance);
    assert_eq!(category.default_template_id, Some(template.id));
    assert_eq!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}")),
        vec![category]
    );
}

#[test]
fn category_creation_should_rollback_when_selected_template_is_unavailable() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let before = library
        .change_revision()
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(matches!(
        library.create_category_with_template(
            "Work",
            CategoryAppearance::default(),
            Some(TemplateId::new()),
            now
        ),
        Err(StorageError::TemplateConflict)
    ));
    assert!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}"))
            .is_empty()
    );
    assert_eq!(
        library
            .change_revision()
            .unwrap_or_else(|error| panic!("{error:?}")),
        before
    );
}

#[test]
fn category_creation_should_allow_no_default_template() {
    let (_dir, library) = library();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let category = library
        .create_category_with_template("Work", CategoryAppearance::default(), None, now)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(category.default_template_id, None);
    assert_eq!(
        library
            .list_categories()
            .unwrap_or_else(|error| panic!("{error:?}")),
        vec![category]
    );
}
