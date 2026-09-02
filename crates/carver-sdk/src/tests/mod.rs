use std::{
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use super::*;

#[derive(Debug, Error)]
#[error("test backend operation is unsupported")]
struct TestError;

struct TestBackend {
    categories: Mutex<Vec<Category>>,
}

impl TestBackend {
    fn unsupported<T>() -> Result<T, TestError> {
        Err(TestError)
    }
}

impl LibraryBackend for TestBackend {
    type Error = TestError;

    fn create_category(&self, name: &str, now: OffsetDateTime) -> Result<Category, Self::Error> {
        let mut categories = self.categories.lock().map_err(|_| TestError)?;
        let category = Category {
            id: CategoryId::new(),
            name: name.to_owned(),
            position: i64::try_from(categories.len()).map_err(|_| TestError)?,
            created_at: now,
            updated_at: now,
            trashed_at: None,
        };
        categories.push(category.clone());
        Ok(category)
    }

    fn categories(&self) -> Result<Vec<Category>, Self::Error> {
        self.categories
            .lock()
            .map(|categories| categories.clone())
            .map_err(|_| TestError)
    }

    fn note_count(&self, _category_id: CategoryId) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn rename_category(
        &self,
        _category_id: CategoryId,
        _name: &str,
        _now: OffsetDateTime,
    ) -> Result<Category, Self::Error> {
        Self::unsupported()
    }

    fn trash_category(
        &self,
        _category_id: CategoryId,
        _now: OffsetDateTime,
    ) -> Result<(), Self::Error> {
        Self::unsupported()
    }

    fn restore_category(
        &self,
        _category_id: CategoryId,
        _now: OffsetDateTime,
    ) -> Result<(), Self::Error> {
        Self::unsupported()
    }

    fn create_note(
        &self,
        _category_id: CategoryId,
        _now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::unsupported()
    }

    fn note(&self, _note_id: NoteId) -> Result<Option<Note>, Self::Error> {
        Self::unsupported()
    }

    fn save_note(
        &self,
        _note_id: NoteId,
        _revision: Revision,
        _source: &str,
        _now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::unsupported()
    }

    fn move_note(
        &self,
        _note_id: NoteId,
        _category_id: CategoryId,
        _now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::unsupported()
    }

    fn trash_note(&self, _note_id: NoteId, _now: OffsetDateTime) -> Result<(), Self::Error> {
        Self::unsupported()
    }

    fn restore_note(&self, _note_id: NoteId) -> Result<(), Self::Error> {
        Self::unsupported()
    }

    fn trash_contents(&self) -> Result<TrashContents, Self::Error> {
        Self::unsupported()
    }

    fn empty_trash(&self) -> Result<TrashPurgeResult, Self::Error> {
        Self::unsupported()
    }

    fn recent_notes(
        &self,
        _category_id: Option<CategoryId>,
        _limit: usize,
        _offset: usize,
    ) -> Result<Vec<NoteSummary>, Self::Error> {
        Self::unsupported()
    }

    fn search(
        &self,
        _query: &str,
        _category_id: Option<CategoryId>,
        _limit: usize,
    ) -> Result<Vec<SearchHit>, Self::Error> {
        Self::unsupported()
    }

    fn store_asset(
        &self,
        _note_id: NoteId,
        _extension: &str,
        _bytes: &[u8],
    ) -> Result<String, Self::Error> {
        Self::unsupported()
    }

    fn note_asset_bytes(
        &self,
        _note_id: NoteId,
        _relative_path: &str,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Self::unsupported()
    }
}

#[test]
fn async_requests_are_serialized_by_the_backend_worker() -> Result<(), LibraryError<TestError>> {
    let client = LibraryClient::spawn(TestBackend {
        categories: Mutex::new(Vec::new()),
    })?;

    let created = block_on(client.create_category_async("Projects".to_owned()))?;
    let categories = block_on(client.categories_async())?;

    assert_eq!(categories, vec![created]);
    Ok(())
}

#[test]
fn async_facade_propagates_backend_failures_without_blocking() -> Result<(), LibraryError<TestError>>
{
    let client = LibraryClient::spawn(TestBackend {
        categories: Mutex::new(Vec::new()),
    })?;
    let category_id = CategoryId::new();
    let note_id = NoteId::new();

    assert_eq!(block_on(client.note_count_async(category_id))?, 0);
    assert_backend_error(&block_on(
        client.rename_category_async(category_id, "Renamed".to_owned()),
    ));
    assert_backend_error(&block_on(client.trash_category_async(category_id)));
    assert_backend_error(&block_on(client.restore_category_async(category_id)));
    assert_backend_error(&block_on(client.create_note_async(category_id)));
    assert_backend_error(&block_on(client.note_async(note_id)));
    assert_backend_error(&block_on(client.save_note_async(
        note_id,
        Revision(0),
        "Updated source".to_owned(),
    )));
    assert_backend_error(&block_on(client.move_note_async(note_id, category_id)));
    assert_backend_error(&block_on(client.trash_note_async(note_id)));
    assert_backend_error(&block_on(client.restore_note_async(note_id)));
    assert_backend_error(&block_on(client.trash_contents_async()));
    assert_backend_error(&block_on(client.empty_trash_async()));
    assert_backend_error(&block_on(client.recent_notes_async(None, 10, 0)));
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

fn assert_backend_error<T>(result: &Result<T, LibraryError<TestError>>) {
    assert!(matches!(result, Err(LibraryError::Backend(_))));
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
