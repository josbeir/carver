mod support;

use super::*;
use support::*;

#[test]
fn async_requests_are_serialized_by_the_backend_worker() -> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend::new())?;

    let created = block_on(client.create_category_async("Projects".to_owned()))?;
    let categories = block_on(client.categories_async())?;

    assert_eq!(categories, vec![created.clone()]);
    assert_eq!(
        block_on(client.categories_with_note_counts_async())?,
        vec![CategorySummary {
            category: created,
            note_count: 0,
        }]
    );
    Ok(())
}

#[test]
fn category_appearance_should_round_trip_through_the_async_facade()
-> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend::new())?;
    let initial_appearance = CategoryAppearance {
        icon: CategoryIcon::Heart,
        color: CategoryColor::Rose,
    };
    let updated_appearance = CategoryAppearance {
        icon: CategoryIcon::Briefcase,
        color: CategoryColor::Teal,
    };

    let created = block_on(
        client.create_category_with_appearance_async("Personal".to_owned(), initial_appearance),
    )?;
    let updated = block_on(client.update_category_async(
        created.id,
        "Projects".to_owned(),
        updated_appearance,
    ))?;

    assert_eq!(created.appearance, initial_appearance);
    assert_eq!(updated.name, "Projects");
    assert_eq!(updated.appearance, updated_appearance);
    Ok(())
}

#[test]
fn change_revision_should_be_available_through_the_async_facade()
-> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend::new())?;

    let revision = block_on(client.change_revision_async())?;

    assert_eq!(revision, LibraryRevision(0));
    Ok(())
}

#[test]
fn async_facade_propagates_backend_failures_without_blocking() -> Result<(), LibraryError<TestError>>
{
    let client = LibraryClient::spawn(TestBackend::new())?;
    let category_id = CategoryId::new();
    let note_id = NoteId::new();

    assert_eq!(block_on(client.note_count_async(category_id))?, 0);
    assert_backend_error(&block_on(
        client.rename_category_async(category_id, "Renamed".to_owned()),
    ));
    assert_backend_error(&block_on(client.trash_category_async(category_id)));
    assert_backend_error(&block_on(client.restore_category_async(category_id)));
    assert_backend_error(&block_on(client.create_note_async(category_id)));
    assert_backend_error(&block_on(client.import_note_async(
        category_id,
        DocumentImportFormat::Carve,
        String::from("# Imported"),
    )));
    assert_backend_error(&block_on(client.note_async(note_id)));
    assert_backend_error(&block_on(client.save_note_async(
        note_id,
        Revision(0),
        "Updated source".to_owned(),
    )));
    assert_backend_error(&block_on(client.set_note_favorite_async(
        note_id,
        Revision(0),
        true,
    )));
    assert_backend_error(&block_on(client.update_note_timestamps_async(
        note_id,
        Revision(0),
        OffsetDateTime::UNIX_EPOCH,
        OffsetDateTime::UNIX_EPOCH,
    )));
    assert_backend_error(&block_on(client.move_note_async(note_id, category_id)));
    assert_backend_error(&block_on(client.trash_note_async(note_id)));
    assert_backend_error(&block_on(client.restore_note_async(note_id)));
    assert_backend_error(&block_on(client.trash_contents_async()));
    assert_backend_error(&block_on(client.empty_trash_async()));
    assert_backend_error(&block_on(client.recent_notes_async(None, 10, 0)));
    assert_backend_error(&block_on(client.favorite_notes_async(None, 10, 0)));
    assert_backend_error(&block_on(client.search_async(
        "needle".to_owned(),
        None,
        10,
    )));
    assert_backend_error(&block_on(client.store_asset_async(
        note_id,
        "png".to_owned(),
        vec![1, 2, 3],
    )));
    assert_backend_error(&block_on(
        client.note_asset_bytes_async(note_id, "assets/example.png".to_owned()),
    ));
    Ok(())
}

#[test]
fn synchronous_favorite_requests_should_propagate_backend_errors()
-> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend::new())?;
    let note_id = NoteId::new();

    assert_backend_error(&client.set_note_favorite(note_id, Revision(0), true));
    assert_backend_error(&client.favorite_notes(None, 10, 0));
    Ok(())
}

#[test]
fn worker_queue_applies_backpressure_when_the_backend_is_busy()
-> Result<(), LibraryError<TestError>> {
    let (started_sender, started_receiver) = async_channel::bounded(1);
    let (release_sender, release_receiver) = async_channel::bounded(1);
    let client = LibraryClient::spawn(TestBackend::gated(started_sender, release_receiver))?;

    let blocking_client = client.clone();
    let blocking_request = std::thread::spawn(move || blocking_client.create_category("Blocked"));
    started_receiver
        .recv_blocking()
        .map_err(|_| LibraryError::Unavailable)?;

    for _ in 0..REQUEST_QUEUE_CAPACITY {
        assert!(
            client
                .requests
                .try_send(Box::new(|_: &TestBackend| {}))
                .is_ok()
        );
    }
    assert!(matches!(
        client.requests.try_send(Box::new(|_: &TestBackend| {})),
        Err(async_channel::TrySendError::Full(_))
    ));

    release_sender
        .send_blocking(())
        .map_err(|_| LibraryError::Unavailable)?;
    blocking_request
        .join()
        .map_err(|_| LibraryError::Unavailable)??;
    Ok(())
}

#[test]
fn asset_metadata_should_propagate_backend_errors() -> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend::new())?;
    assert_backend_error(&block_on(
        client.note_asset_size_async(NoteId::new(), "assets/a.pdf".into()),
    ));
    Ok(())
}
