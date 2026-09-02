//! Portable, asynchronous application API for Carver frontends.

#![forbid(unsafe_code)]

use std::{error::Error, thread};

use async_channel::{Receiver, Sender};
pub use carver_domain::{
    Category, CategoryId, Note, NoteId, NoteSummary, Revision, SearchHit, TrashContents,
    TrashPurgeResult, TrashedCategorySummary, TrashedNoteSummary,
};
use thiserror::Error;
use time::OffsetDateTime;

/// Persistence port implemented by local and future remote Carver backends.
///
/// Implementations are owned by one [`LibraryClient`] worker thread. They must not retain GTK
/// objects or call UI code.
#[expect(
    clippy::missing_errors_doc,
    reason = "the backend trait documents its single associated error contract once"
)]
pub trait LibraryBackend: Send + 'static {
    /// Backend-specific error returned by an operation.
    type Error: Error + Send + Sync + 'static;

    /// Creates a category at the supplied time.
    fn create_category(&self, name: &str, now: OffsetDateTime) -> Result<Category, Self::Error>;
    /// Lists active categories in their display order.
    fn categories(&self) -> Result<Vec<Category>, Self::Error>;
    /// Counts active notes in a category.
    fn note_count(&self, category_id: CategoryId) -> Result<usize, Self::Error>;
    /// Renames a category at the supplied time.
    fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error>;
    /// Moves a category to trash at the supplied time.
    fn trash_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error>;
    /// Restores a category from trash at the supplied time.
    fn restore_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error>;
    /// Creates a blank note at the supplied time.
    fn create_note(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Reads one note, excluding trashed notes.
    fn note(&self, note_id: NoteId) -> Result<Option<Note>, Self::Error>;
    /// Saves source guarded by its revision at the supplied time.
    fn save_note(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Moves an active note to an active category without changing its content timestamp.
    fn move_note(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Moves a note to trash at the supplied time.
    fn trash_note(&self, note_id: NoteId, now: OffsetDateTime) -> Result<(), Self::Error>;
    /// Restores a note from trash.
    fn restore_note(&self, note_id: NoteId) -> Result<(), Self::Error>;
    /// Lists recoverable trash contents.
    fn trash_contents(&self) -> Result<TrashContents, Self::Error>;
    /// Permanently removes trashed content and unreferenced managed assets.
    fn empty_trash(&self) -> Result<TrashPurgeResult, Self::Error>;
    /// Returns recent active notes, optionally filtered by category.
    fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, Self::Error>;
    /// Searches active notes by title and body.
    fn search(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, Self::Error>;
    /// Stores managed image bytes and returns their relative Carve path.
    fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, Self::Error>;
    /// Reads managed image bytes for one note.
    fn note_asset_bytes(
        &self,
        note_id: NoteId,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
}

type Job<B> = Box<dyn FnOnce(&B) + Send + 'static>;

/// Cloneable client that serializes storage work on a dedicated backend thread.
///
/// Use the `*_async` methods from a UI. The synchronous counterparts exist for short-lived
/// bootstrap code and deterministic tests; calling them from a UI callback defeats the worker.
pub struct LibraryClient<B> {
    requests: Sender<Job<B>>,
}

impl<B> Clone for LibraryClient<B> {
    fn clone(&self) -> Self {
        Self {
            requests: self.requests.clone(),
        }
    }
}

/// Failures surfaced by a [`LibraryClient`].
#[derive(Debug, Error)]
pub enum LibraryError<E: Error + Send + Sync + 'static> {
    /// The backend rejected an operation.
    #[error(transparent)]
    Backend(E),
    /// Starting the dedicated backend worker failed.
    #[error("could not start library storage worker: {0}")]
    WorkerStart(#[source] std::io::Error),
    /// The dedicated backend worker exited before it could answer.
    #[error("library storage worker is unavailable")]
    Unavailable,
}

#[expect(
    clippy::missing_errors_doc,
    reason = "all facade operations return the documented LibraryError contract"
)]
impl<B: LibraryBackend> LibraryClient<B> {
    /// Starts a dedicated worker that owns `backend`.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot start the worker thread.
    pub fn spawn(backend: B) -> Result<Self, LibraryError<B::Error>> {
        let (sender, receiver) = async_channel::unbounded();
        thread::Builder::new()
            .name("carver-library".to_owned())
            .spawn(move || run_worker(backend, receiver))
            .map_err(LibraryError::WorkerStart)?;
        Ok(Self { requests: sender })
    }

    /// Creates a category without blocking the caller.
    pub async fn create_category_async(
        &self,
        name: String,
    ) -> Result<Category, LibraryError<B::Error>> {
        self.request(move |backend| backend.create_category(&name, OffsetDateTime::now_utc()))
            .await
    }

    /// Lists sidebar categories without blocking the caller.
    pub async fn categories_async(&self) -> Result<Vec<Category>, LibraryError<B::Error>> {
        self.request(LibraryBackend::categories).await
    }

    /// Counts category notes without blocking the caller.
    pub async fn note_count_async(
        &self,
        category_id: CategoryId,
    ) -> Result<usize, LibraryError<B::Error>> {
        self.request(move |backend| backend.note_count(category_id))
            .await
    }

    /// Renames a category without blocking the caller.
    pub async fn rename_category_async(
        &self,
        category_id: CategoryId,
        name: String,
    ) -> Result<Category, LibraryError<B::Error>> {
        self.request(move |backend| {
            backend.rename_category(category_id, &name, OffsetDateTime::now_utc())
        })
        .await
    }

    /// Moves a category to trash without blocking the caller.
    pub async fn trash_category_async(
        &self,
        category_id: CategoryId,
    ) -> Result<(), LibraryError<B::Error>> {
        self.request(move |backend| backend.trash_category(category_id, OffsetDateTime::now_utc()))
            .await
    }

    /// Restores a category without blocking the caller.
    pub async fn restore_category_async(
        &self,
        category_id: CategoryId,
    ) -> Result<(), LibraryError<B::Error>> {
        self.request(move |backend| {
            backend.restore_category(category_id, OffsetDateTime::now_utc())
        })
        .await
    }

    /// Creates a blank note without blocking the caller.
    pub async fn create_note_async(
        &self,
        category_id: CategoryId,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| backend.create_note(category_id, OffsetDateTime::now_utc()))
            .await
    }

    /// Loads a note without blocking the caller.
    pub async fn note_async(
        &self,
        note_id: NoteId,
    ) -> Result<Option<Note>, LibraryError<B::Error>> {
        self.request(move |backend| backend.note(note_id)).await
    }

    /// Saves note source without blocking the caller.
    pub async fn save_note_async(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: String,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| {
            backend.save_note(note_id, revision, &source, OffsetDateTime::now_utc())
        })
        .await
    }

    /// Moves a note to an active category without blocking the caller.
    pub async fn move_note_async(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| {
            backend.move_note(note_id, category_id, OffsetDateTime::now_utc())
        })
        .await
    }

    /// Moves a note to trash without blocking the caller.
    pub async fn trash_note_async(&self, note_id: NoteId) -> Result<(), LibraryError<B::Error>> {
        self.request(move |backend| backend.trash_note(note_id, OffsetDateTime::now_utc()))
            .await
    }

    /// Restores a note without blocking the caller.
    pub async fn restore_note_async(&self, note_id: NoteId) -> Result<(), LibraryError<B::Error>> {
        self.request(move |backend| backend.restore_note(note_id))
            .await
    }

    /// Lists recoverable trash contents without blocking the caller.
    pub async fn trash_contents_async(&self) -> Result<TrashContents, LibraryError<B::Error>> {
        self.request(LibraryBackend::trash_contents).await
    }

    /// Permanently removes trash without blocking the caller.
    pub async fn empty_trash_async(&self) -> Result<TrashPurgeResult, LibraryError<B::Error>> {
        self.request(LibraryBackend::empty_trash).await
    }

    /// Returns recent active notes without blocking the caller.
    pub async fn recent_notes_async(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, LibraryError<B::Error>> {
        self.request(move |backend| backend.recent_notes(category_id, limit, offset))
            .await
    }

    /// Searches active notes without blocking the caller.
    pub async fn search_async(
        &self,
        query: String,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, LibraryError<B::Error>> {
        self.request(move |backend| backend.search(&query, category_id, limit))
            .await
    }

    /// Stores an image asset without blocking the caller.
    pub async fn store_asset_async(
        &self,
        note_id: NoteId,
        extension: String,
        bytes: Vec<u8>,
    ) -> Result<String, LibraryError<B::Error>> {
        self.request(move |backend| backend.store_asset(note_id, &extension, &bytes))
            .await
    }

    /// Reads an image asset without blocking the caller.
    pub async fn note_asset_bytes_async(
        &self,
        note_id: NoteId,
        relative_path: String,
    ) -> Result<Option<Vec<u8>>, LibraryError<B::Error>> {
        self.request(move |backend| backend.note_asset_bytes(note_id, &relative_path))
            .await
    }

    /// Creates a category synchronously for bootstrap code and tests.
    pub fn create_category(&self, name: &str) -> Result<Category, LibraryError<B::Error>> {
        let name = name.to_owned();
        self.blocking(move |backend| backend.create_category(&name, OffsetDateTime::now_utc()))
    }

    /// Lists categories synchronously for bootstrap code and tests.
    pub fn categories(&self) -> Result<Vec<Category>, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::categories)
    }

    /// Counts category notes synchronously for bootstrap code and tests.
    pub fn note_count(&self, category_id: CategoryId) -> Result<usize, LibraryError<B::Error>> {
        self.blocking(move |backend| backend.note_count(category_id))
    }

    /// Renames a category synchronously for bootstrap code and tests.
    pub fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
    ) -> Result<Category, LibraryError<B::Error>> {
        let name = name.to_owned();
        self.blocking(move |backend| {
            backend.rename_category(category_id, &name, OffsetDateTime::now_utc())
        })
    }

    /// Moves a category to trash synchronously for bootstrap code and tests.
    pub fn trash_category(&self, category_id: CategoryId) -> Result<(), LibraryError<B::Error>> {
        self.blocking(move |backend| backend.trash_category(category_id, OffsetDateTime::now_utc()))
    }

    /// Restores a category synchronously for bootstrap code and tests.
    pub fn restore_category(&self, category_id: CategoryId) -> Result<(), LibraryError<B::Error>> {
        self.blocking(move |backend| {
            backend.restore_category(category_id, OffsetDateTime::now_utc())
        })
    }

    /// Creates a blank note synchronously for bootstrap code and tests.
    pub fn create_note(&self, category_id: CategoryId) -> Result<Note, LibraryError<B::Error>> {
        self.blocking(move |backend| backend.create_note(category_id, OffsetDateTime::now_utc()))
    }

    /// Loads a note synchronously for bootstrap code and tests.
    pub fn note(&self, note_id: NoteId) -> Result<Option<Note>, LibraryError<B::Error>> {
        self.blocking(move |backend| backend.note(note_id))
    }

    /// Saves source synchronously for bootstrap code and tests.
    pub fn save_note(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: &str,
    ) -> Result<Note, LibraryError<B::Error>> {
        let source = source.to_owned();
        self.blocking(move |backend| {
            backend.save_note(note_id, revision, &source, OffsetDateTime::now_utc())
        })
    }

    /// Moves a note synchronously for bootstrap code and tests.
    pub fn move_note(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.blocking(move |backend| {
            backend.move_note(note_id, category_id, OffsetDateTime::now_utc())
        })
    }

    /// Moves a note to trash synchronously for bootstrap code and tests.
    pub fn trash_note(&self, note_id: NoteId) -> Result<(), LibraryError<B::Error>> {
        self.blocking(move |backend| backend.trash_note(note_id, OffsetDateTime::now_utc()))
    }

    /// Restores a note synchronously for bootstrap code and tests.
    pub fn restore_note(&self, note_id: NoteId) -> Result<(), LibraryError<B::Error>> {
        self.blocking(move |backend| backend.restore_note(note_id))
    }

    /// Lists trash synchronously for bootstrap code and tests.
    pub fn trash_contents(&self) -> Result<TrashContents, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::trash_contents)
    }

    /// Permanently removes trash synchronously for bootstrap code and tests.
    pub fn empty_trash(&self) -> Result<TrashPurgeResult, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::empty_trash)
    }

    /// Returns recent notes synchronously for bootstrap code and tests.
    pub fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, LibraryError<B::Error>> {
        self.blocking(move |backend| backend.recent_notes(category_id, limit, offset))
    }

    /// Searches notes synchronously for bootstrap code and tests.
    pub fn search(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, LibraryError<B::Error>> {
        let query = query.to_owned();
        self.blocking(move |backend| backend.search(&query, category_id, limit))
    }

    /// Stores an image asset synchronously for bootstrap code and tests.
    pub fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, LibraryError<B::Error>> {
        let extension = extension.to_owned();
        let bytes = bytes.to_owned();
        self.blocking(move |backend| backend.store_asset(note_id, &extension, &bytes))
    }

    /// Reads an image asset synchronously for bootstrap code and tests.
    pub fn note_asset_bytes(
        &self,
        note_id: NoteId,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError<B::Error>> {
        let relative_path = relative_path.to_owned();
        self.blocking(move |backend| backend.note_asset_bytes(note_id, &relative_path))
    }

    async fn request<T>(
        &self,
        operation: impl FnOnce(&B) -> Result<T, B::Error> + Send + 'static,
    ) -> Result<T, LibraryError<B::Error>>
    where
        T: Send + 'static,
    {
        let (reply_sender, reply_receiver) = async_channel::bounded(1);
        self.requests
            .send(Box::new(move |backend| {
                let _ =
                    reply_sender.send_blocking(operation(backend).map_err(LibraryError::Backend));
            }))
            .await
            .map_err(|_| LibraryError::Unavailable)?;
        reply_receiver
            .recv()
            .await
            .map_err(|_| LibraryError::Unavailable)?
    }

    fn blocking<T>(
        &self,
        operation: impl FnOnce(&B) -> Result<T, B::Error> + Send + 'static,
    ) -> Result<T, LibraryError<B::Error>>
    where
        T: Send + 'static,
    {
        let (reply_sender, reply_receiver) = async_channel::bounded(1);
        self.requests
            .send_blocking(Box::new(move |backend| {
                let _ =
                    reply_sender.send_blocking(operation(backend).map_err(LibraryError::Backend));
            }))
            .map_err(|_| LibraryError::Unavailable)?;
        reply_receiver
            .recv_blocking()
            .map_err(|_| LibraryError::Unavailable)?
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the worker must own both the backend and receiver for its full lifetime"
)]
fn run_worker<B: LibraryBackend>(backend: B, receiver: Receiver<Job<B>>) {
    while let Ok(job) = receiver.recv_blocking() {
        job(&backend);
    }
}

#[cfg(test)]
mod tests {
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

        fn create_category(
            &self,
            name: &str,
            now: OffsetDateTime,
        ) -> Result<Category, Self::Error> {
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
    fn async_requests_are_serialized_by_the_backend_worker() -> Result<(), LibraryError<TestError>>
    {
        let client = LibraryClient::spawn(TestBackend {
            categories: Mutex::new(Vec::new()),
        })?;

        let created = block_on(client.create_category_async("Projects".to_owned()))?;
        let categories = block_on(client.categories_async())?;

        assert_eq!(categories, vec![created]);
        Ok(())
    }

    #[test]
    fn async_facade_propagates_backend_failures_without_blocking()
    -> Result<(), LibraryError<TestError>> {
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
}
