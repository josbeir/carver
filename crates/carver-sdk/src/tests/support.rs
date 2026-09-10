use std::{
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use super::*;

#[derive(Debug, Error)]
#[error("test backend operation is unsupported")]
pub(super) struct TestError;

pub(super) struct TestBackend {
    categories: Mutex<Vec<Category>>,
    gate: Option<WorkerGate>,
}

struct WorkerGate {
    started: async_channel::Sender<()>,
    release: async_channel::Receiver<()>,
}

impl TestBackend {
    pub(super) fn new() -> Self {
        Self {
            categories: Mutex::new(Vec::new()),
            gate: None,
        }
    }

    pub(super) fn gated(
        started: async_channel::Sender<()>,
        release: async_channel::Receiver<()>,
    ) -> Self {
        Self {
            categories: Mutex::new(Vec::new()),
            gate: Some(WorkerGate { started, release }),
        }
    }

    fn unsupported<T>() -> Result<T, TestError> {
        Err(TestError)
    }
}

impl LibraryBackend for TestBackend {
    type Error = TestError;

    fn change_revision(&self) -> Result<LibraryRevision, Self::Error> {
        self.categories
            .lock()
            .map_err(|_| TestError)
            .and_then(|categories| {
                u64::try_from(categories.len())
                    .map(LibraryRevision)
                    .map_err(|_| TestError)
            })
    }

    fn create_category(&self, name: &str, now: OffsetDateTime) -> Result<Category, Self::Error> {
        if let Some(gate) = &self.gate {
            gate.started.send_blocking(()).map_err(|_| TestError)?;
            gate.release.recv_blocking().map_err(|_| TestError)?;
        }
        let mut categories = self.categories.lock().map_err(|_| TestError)?;
        let category = Category {
            id: CategoryId::new(),
            name: name.to_owned(),
            appearance: CategoryAppearance::default(),
            position: i64::try_from(categories.len()).map_err(|_| TestError)?,
            created_at: now,
            updated_at: now,
            trashed_at: None,
        };
        categories.push(category.clone());
        Ok(category)
    }

    fn create_category_with_appearance(
        &self,
        name: &str,
        appearance: CategoryAppearance,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error> {
        let category = self.create_category(name, now)?;
        self.update_category(category.id, name, appearance, now)
    }

    fn categories(&self) -> Result<Vec<Category>, Self::Error> {
        self.categories
            .lock()
            .map(|categories| categories.clone())
            .map_err(|_| TestError)
    }

    fn categories_with_note_counts(&self) -> Result<Vec<CategorySummary>, Self::Error> {
        self.categories().map(|categories| {
            categories
                .into_iter()
                .map(|category| CategorySummary {
                    category,
                    note_count: 0,
                })
                .collect()
        })
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

    fn update_category(
        &self,
        category_id: CategoryId,
        name: &str,
        appearance: CategoryAppearance,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error> {
        let mut categories = self.categories.lock().map_err(|_| TestError)?;
        let category = categories
            .iter_mut()
            .find(|category| category.id == category_id)
            .ok_or(TestError)?;
        category.name = name.to_owned();
        category.appearance = appearance;
        category.updated_at = now;
        Ok(category.clone())
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

    fn create_note_with_source(
        &self,
        _category_id: CategoryId,
        _source: &str,
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

    fn set_note_favorite(
        &self,
        _note_id: NoteId,
        _revision: Revision,
        _is_favorite: bool,
        _now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::unsupported()
    }

    fn update_note_timestamps(
        &self,
        _note_id: NoteId,
        _revision: Revision,
        _created_at: OffsetDateTime,
        _updated_at: OffsetDateTime,
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

    fn create_base(
        &self,
        _name: &str,
        _columns: &[BaseColumn],
    ) -> Result<BaseDefinition, Self::Error> {
        Self::unsupported()
    }

    fn update_base(
        &self,
        _base_id: BaseId,
        _revision: Revision,
        _name: &str,
        _columns: &[BaseColumn],
        _filter_mode: BaseFilterMode,
        _filters: &[BaseFilter],
        _sorts: &[BaseSort],
    ) -> Result<BaseDefinition, Self::Error> {
        Self::unsupported()
    }

    fn bases(&self) -> Result<Vec<BaseDefinition>, Self::Error> {
        Self::unsupported()
    }

    fn delete_base(&self, _base_id: BaseId) -> Result<(), Self::Error> {
        Self::unsupported()
    }

    fn base_rows(&self, _base_id: BaseId) -> Result<Vec<BaseRow>, Self::Error> {
        Self::unsupported()
    }

    fn property_descriptors(&self) -> Result<Vec<PropertyDescriptor>, Self::Error> {
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

    fn favorite_notes(
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

    fn note_asset_size(
        &self,
        _note_id: NoteId,
        _relative_path: &str,
    ) -> Result<Option<u64>, Self::Error> {
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

pub(super) fn assert_backend_error<T>(result: &Result<T, LibraryError<TestError>>) {
    assert!(matches!(result, Err(LibraryError::Backend(_))));
}

pub(super) fn block_on<F: Future>(future: F) -> F::Output {
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
