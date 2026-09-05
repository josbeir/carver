//! Portable, asynchronous application API for Carver frontends.

#![forbid(unsafe_code)]

use std::{error::Error, path::Path, thread};

use async_channel::{Receiver, Sender};
use carver_config::{AppPaths, ConfigError};
pub use carver_domain::{
    Category, CategoryId, CategorySummary, DocumentImportFormat, Note, NoteId, NoteSummary,
    Revision, SearchHit, TrashContents, TrashPurgeResult, TrashedCategorySummary,
    TrashedNoteSummary,
};
pub use carver_library_port::{LibraryBackend, LibraryRevision};
use carver_storage_sqlite::{SqliteLibrary, StorageError};
use thiserror::Error;
use time::OffsetDateTime;

type Job<B> = Box<dyn FnOnce(&B) + Send + 'static>;

/// Maximum number of storage operations that can wait behind the active worker operation.
///
/// A bounded queue applies asynchronous backpressure to frontends rather than allowing an
/// unlimited number of UI-triggered operations to accumulate in memory.
const REQUEST_QUEUE_CAPACITY: usize = 32;

/// Worker-backed client for the installed local SQLite library.
pub type InstalledLibraryClient = LibraryClient<SqliteLibrary>;

/// Errors opening the installed local library through the SDK.
#[derive(Debug, Error)]
pub enum OpenLibraryError {
    /// The XDG application locations could not be created.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The SQLite library could not be opened.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The SDK worker for the library could not be started.
    #[error(transparent)]
    Worker(#[from] LibraryError<StorageError>),
}

/// Opens the installed XDG-scoped Carver library behind the SDK worker boundary.
///
/// # Errors
///
/// Returns an error when XDG directories, SQLite storage, or the SDK worker cannot be prepared.
pub fn open_installed_library() -> Result<InstalledLibraryClient, OpenLibraryError> {
    let paths = AppPaths::discover();
    paths.ensure_exists()?;
    open_local_library(&paths.database_file(), &paths.assets_dir())
}

/// Opens a local SQLite library at explicit storage paths behind the SDK worker boundary.
///
/// This is intended for package integrations that supply their own XDG-compatible paths and for
/// deterministic tests. Most frontends should use [`open_installed_library`].
///
/// # Errors
///
/// Returns an error when the SQLite library or the SDK worker cannot be prepared.
pub fn open_local_library(
    database_path: &Path,
    assets_dir: &Path,
) -> Result<InstalledLibraryClient, OpenLibraryError> {
    let library = SqliteLibrary::open(database_path, assets_dir)?;
    Ok(LibraryClient::spawn(library)?)
}

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
        let (sender, receiver) = async_channel::bounded(REQUEST_QUEUE_CAPACITY);
        thread::Builder::new()
            .name("carver-library".to_owned())
            .spawn(move || run_worker(backend, receiver))
            .map_err(LibraryError::WorkerStart)?;
        Ok(Self { requests: sender })
    }

    /// Reads the current semantic library revision without blocking the caller.
    ///
    /// Frontends use this after a local change wake-up to decide whether their immutable read
    /// models require reloading.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend worker is unavailable or cannot read the revision.
    pub async fn change_revision_async(&self) -> Result<LibraryRevision, LibraryError<B::Error>> {
        self.request(LibraryBackend::change_revision).await
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

    /// Lists sidebar categories and their active-note counts without blocking the caller.
    pub async fn categories_with_note_counts_async(
        &self,
    ) -> Result<Vec<CategorySummary>, LibraryError<B::Error>> {
        self.request(LibraryBackend::categories_with_note_counts)
            .await
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

    /// Creates a note with canonical Carve source without blocking the caller.
    pub async fn create_note_with_source_async(
        &self,
        category_id: CategoryId,
        source: String,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| {
            backend.create_note_with_source(category_id, &source, OffsetDateTime::now_utc())
        })
        .await
    }

    /// Saves a note after converting the supplied document into canonical Carve source.
    pub async fn save_note_with_format_async(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        source: String,
        format: DocumentImportFormat,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| {
            let source = carver_domain::import_document(&source, format);
            backend.save_note(
                note_id,
                expected_revision,
                &source,
                OffsetDateTime::now_utc(),
            )
        })
        .await
    }

    /// Converts one supported source format and creates the resulting note without blocking.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend worker is unavailable or cannot persist the note.
    pub async fn import_note_async(
        &self,
        category_id: CategoryId,
        format: DocumentImportFormat,
        source: String,
    ) -> Result<Note, LibraryError<B::Error>> {
        self.request(move |backend| {
            let source = carver_domain::import_document(&source, format);
            backend.create_note_with_source(category_id, &source, OffsetDateTime::now_utc())
        })
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

    /// Reads the current semantic library revision synchronously for bootstrap code and tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend worker is unavailable or cannot read the revision.
    pub fn change_revision(&self) -> Result<LibraryRevision, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::change_revision)
    }

    /// Lists categories synchronously for bootstrap code and tests.
    pub fn categories(&self) -> Result<Vec<Category>, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::categories)
    }

    /// Lists categories and their active-note counts synchronously for bootstrap code and tests.
    pub fn categories_with_note_counts(
        &self,
    ) -> Result<Vec<CategorySummary>, LibraryError<B::Error>> {
        self.blocking(LibraryBackend::categories_with_note_counts)
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

    /// Creates a note with canonical Carve source synchronously for bootstrap code and tests.
    pub fn create_note_with_source(
        &self,
        category_id: CategoryId,
        source: &str,
    ) -> Result<Note, LibraryError<B::Error>> {
        let source = source.to_owned();
        self.blocking(move |backend| {
            backend.create_note_with_source(category_id, &source, OffsetDateTime::now_utc())
        })
    }

    /// Converts one supported source format and creates the resulting note synchronously.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend worker is unavailable or cannot persist the note.
    pub fn import_note(
        &self,
        category_id: CategoryId,
        format: DocumentImportFormat,
        source: &str,
    ) -> Result<Note, LibraryError<B::Error>> {
        let source = source.to_owned();
        self.blocking(move |backend| {
            let source = carver_domain::import_document(&source, format);
            backend.create_note_with_source(category_id, &source, OffsetDateTime::now_utc())
        })
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
mod tests;
