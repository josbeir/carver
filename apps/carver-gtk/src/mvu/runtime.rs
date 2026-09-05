//! `GLib` runtime for executing reducer effects.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

use carver_sdk::{LibraryBackend, LibraryClient};
use carver_storage_sqlite::change_notification_files;
use gtk::gio::{
    self, FileMonitor, FileMonitorEvent,
    prelude::{FileExt, FileMonitorExt},
};

use crate::view::ViewRefs;

use super::{
    ActionKey, AppModel, AppMsg, EditorExportFormat, Effect, LibraryReply, TrashMutation, UiError,
    update,
};

type DispatchCallback = Rc<dyn Fn(AppMsg) -> bool>;

/// A weak, window-local route for GTK/WebKit adapters to submit MVU messages.
#[derive(Clone, Default)]
pub struct AppDispatcher {
    callback: Rc<RefCell<Option<DispatchCallback>>>,
}

impl AppDispatcher {
    /// Sends a message while the window runtime is still alive.
    #[must_use]
    pub fn dispatch(&self, message: AppMsg) -> bool {
        let callback = self.callback.borrow().clone();
        let Some(callback) = callback else {
            return false;
        };
        callback(message)
    }

    fn bind<B: LibraryBackend>(&self, runtime: &AppRuntime<B>) {
        let inner = Rc::downgrade(&runtime.inner);
        self.callback.replace(Some(Rc::new(move |message| {
            let Some(inner) = Weak::upgrade(&inner) else {
                return false;
            };
            AppRuntime { inner }.dispatch(message);
            true
        })));
    }
}

/// Main-thread dispatcher that renders model snapshots and executes typed effects.
///
/// A runtime is intentionally local to one GTK window. Its model is borrowed only while the
/// reducer runs; rendering and effect execution happen after that borrow is released.
pub struct AppRuntime<B: LibraryBackend> {
    inner: Rc<RuntimeInner<B>>,
}

struct RuntimeInner<B: LibraryBackend> {
    client: LibraryClient<B>,
    config_path: Option<PathBuf>,
    model: RefCell<AppModel>,
    view: ViewRefs,
    prepared_exports: RefCell<BTreeMap<u64, PreparedExport>>,
    library_monitor: RefCell<Option<FileMonitor>>,
}

struct PreparedExport {
    artifact: carver_export::ExportArtifact,
    target_uri: String,
}

impl<B: LibraryBackend> Clone for AppRuntime<B> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<B: LibraryBackend> AppRuntime<B> {
    /// Creates a local runtime from a worker-backed SDK client and GTK view references.
    #[must_use]
    pub fn new(client: LibraryClient<B>, model: AppModel, view: ViewRefs) -> Self {
        Self::new_with_config_path(client, model, view, None)
    }

    /// Creates a runtime that can persist configuration snapshots at `config_path`.
    #[must_use]
    pub fn new_with_config_path(
        client: LibraryClient<B>,
        model: AppModel,
        view: ViewRefs,
        config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: Rc::new(RuntimeInner {
                client,
                config_path,
                model: RefCell::new(model),
                view,
                prepared_exports: RefCell::new(BTreeMap::new()),
                library_monitor: RefCell::new(None),
            }),
        }
    }

    /// Binds a weak dispatcher for callbacks that are created before the runtime exists.
    pub fn bind_dispatcher(&self, dispatcher: &AppDispatcher) {
        dispatcher.bind(self);
    }

    /// Watches the shared library database and asks the reducer to verify semantic change state.
    ///
    /// # Errors
    ///
    /// Returns an error when the desktop cannot monitor `database_path`.
    pub fn monitor_library(
        &self,
        database_path: &Path,
        dispatcher: AppDispatcher,
    ) -> Result<(), glib::Error> {
        let Some(directory) = database_path.parent() else {
            return Err(glib::Error::new(
                gio::IOErrorEnum::InvalidArgument,
                "database path must have a parent directory",
            ));
        };
        let Some(watched_files) = change_notification_files(database_path) else {
            return Err(glib::Error::new(
                gio::IOErrorEnum::InvalidArgument,
                "database path must have a file name",
            ));
        };
        let directory = gio::File::for_path(directory);
        let monitor =
            directory.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)?;
        monitor.set_rate_limit(250);
        monitor.connect_changed(move |_, file, _, event| {
            let is_library_database = file
                .path()
                .is_some_and(|path| watched_files.contains(&path));
            if is_library_database
                && matches!(
                    event,
                    FileMonitorEvent::Changed | FileMonitorEvent::ChangesDoneHint
                )
            {
                let _ = dispatcher.dispatch(AppMsg::LibraryChangedExternally);
            }
        });
        self.inner.library_monitor.replace(Some(monitor));
        Ok(())
    }

    /// Reduces a message, renders the resulting snapshot, and starts any requested effects.
    pub fn dispatch(&self, message: AppMsg) {
        let (snapshot, effects) = {
            let mut model = self.inner.model.borrow_mut();
            let effects = update(&mut model, message);
            (model.clone(), effects)
        };
        self.inner.view.render(&snapshot);
        for effect in effects {
            self.run_effect(effect);
        }
    }

    /// Returns a clone of the current model for deterministic UI tests and snapshots.
    #[must_use]
    pub fn model(&self) -> AppModel {
        self.inner.model.borrow().clone()
    }

    /// Returns whether the view is currently applying a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.inner.view.is_rendering()
    }

    fn run_effect(&self, effect: Effect) {
        match effect {
            effect @ (Effect::PrepareEditorExport { .. }
            | Effect::WriteEditorExport { .. }
            | Effect::DiscardEditorExport { .. }) => self.run_editor_export_effect(effect),
            Effect::PersistConfig { config } => self.persist_config(&config),
            Effect::EnsureDefaultCategory => self.ensure_default_category(),
            Effect::CreateNote { category_id } => self.create_note(category_id),
            Effect::ImportNote {
                category_id,
                format,
                source,
            } => self.import_note(category_id, format, source),
            Effect::ScheduleSearch { timer_id } => self.schedule_search(timer_id),
            Effect::ScheduleEditorSave {
                session,
                timer_id,
                delay_ms,
            } => self.schedule_editor_save(session, timer_id, delay_ms),
            Effect::SchedulePreview { session, timer_id } => {
                self.schedule_preview(session, timer_id);
            }
            Effect::SaveNote { request } => self.save_note(request),
            Effect::StoreEditorAsset {
                session,
                note_id,
                extension,
                bytes,
                alt,
                source_selection,
            } => self.store_editor_asset(session, note_id, extension, bytes, alt, source_selection),
            Effect::LoadSidebar { request_id } => self.load_sidebar(request_id),
            Effect::LoadLibraryRevision { request_id } => self.load_library_revision(request_id),
            Effect::LoadBrowser {
                request_id,
                category_id,
                query,
            } => self.load_browser(request_id, category_id, query),
            Effect::LoadEditorNote {
                request_id,
                note_id,
            } => self.load_editor_note(request_id, note_id),
            Effect::LoadTrash { request_id } => {
                let client = self.inner.client.clone();
                let runtime = self.clone();
                glib::spawn_future_local(async move {
                    let result = client.trash_contents_async().await.map_err(display_error);
                    runtime.dispatch(AppMsg::Library(LibraryReply::TrashLoaded {
                        request_id,
                        result,
                    }));
                });
            }
            Effect::RestoreCategory { category_id } => {
                self.restore_category(category_id);
            }
            Effect::RestoreNote { note_id } => {
                self.restore_note(note_id);
            }
            Effect::EmptyTrash => {
                self.empty_trash();
            }
            Effect::CreateCategory { name } => {
                self.create_category(name);
            }
            Effect::CreateCategoryWithAppearance { name, appearance } => {
                self.create_category_with_appearance(name, appearance);
            }
            Effect::CreateCategoryAndMoveNote {
                action,
                name,
                note_id,
            } => {
                self.create_category_and_move_note(action, name, note_id);
            }
            Effect::RenameCategory { category_id, name } => {
                self.rename_category(category_id, name);
            }
            Effect::UpdateCategory {
                category_id,
                name,
                appearance,
            } => self.update_category(category_id, name, appearance),
            Effect::TrashCategory { category_id } => {
                self.trash_category(category_id);
            }
            Effect::MoveNote {
                action,
                note_id,
                category_id,
            } => {
                self.move_note(action, note_id, category_id);
            }
            Effect::TrashNote { note_id } => {
                self.trash_note(note_id);
            }
        }
    }

    fn run_editor_export_effect(&self, effect: Effect) {
        match effect {
            Effect::PrepareEditorExport {
                request_id,
                session,
                note_id,
                source,
                filename_stem,
                format,
                include_assets,
                target_uri,
            } => self.prepare_editor_export(
                request_id,
                session,
                note_id,
                source,
                filename_stem,
                format,
                include_assets,
                target_uri,
            ),
            Effect::WriteEditorExport { request_id } => self.write_editor_export(request_id),
            Effect::DiscardEditorExport { request_id } => {
                self.inner.prepared_exports.borrow_mut().remove(&request_id);
            }
            _ => {}
        }
    }

    fn schedule_search(&self, timer_id: super::TimerId) {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            glib::timeout_future(std::time::Duration::from_millis(250)).await;
            runtime.dispatch(AppMsg::Browser(super::BrowserMsg::SearchTimerFired(
                timer_id,
            )));
        });
    }

    fn ensure_default_category(&self) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = match client.categories_async().await {
                Ok(categories) if categories.is_empty() => client
                    .create_category_async(String::from("Notes"))
                    .await
                    .map(|_| ()),
                Ok(_) => Ok(()),
                Err(error) => Err(error),
            }
            .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::DefaultCategoryEnsured {
                result,
            }));
        });
    }

    fn create_note(&self, category_id: carver_sdk::CategoryId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .create_note_async(category_id)
                .await
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::NoteCreated { result }));
        });
    }

    fn import_note(
        &self,
        category_id: carver_sdk::CategoryId,
        format: carver_sdk::DocumentImportFormat,
        source: String,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .import_note_async(category_id, format, source)
                .await
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::NoteCreated { result }));
        });
    }

    fn load_browser(
        &self,
        request_id: super::RequestId,
        category_id: Option<carver_sdk::CategoryId>,
        query: String,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = if query.trim().is_empty() {
                client.recent_notes_async(category_id, 200, 0).await
            } else {
                client
                    .search_async(query, category_id, 200)
                    .await
                    .map(|matches| matches.into_iter().map(|hit| hit.note).collect())
            }
            .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::BrowserLoaded {
                request_id,
                result,
            }));
        });
    }

    fn load_sidebar(&self, request_id: super::RequestId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .categories_with_note_counts_async()
                .await
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::SidebarLoaded {
                request_id,
                result,
            }));
        });
    }

    fn persist_config(&self, config: &carver_config::Config) {
        let result = self.inner.config_path.as_deref().map_or(Ok(()), |path| {
            carver_config::save(path, config).map_err(display_error)
        });
        self.dispatch(AppMsg::Library(LibraryReply::ConfigPersisted { result }));
    }

    fn schedule_editor_save(
        &self,
        session: super::EditorSessionId,
        timer_id: super::TimerId,
        delay_ms: u64,
    ) {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            glib::timeout_future(std::time::Duration::from_millis(delay_ms)).await;
            runtime.dispatch(AppMsg::Editor(super::EditorMsg::AutosaveElapsed {
                session,
                timer_id,
            }));
        });
    }

    fn schedule_preview(&self, session: super::EditorSessionId, timer_id: super::TimerId) {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            glib::timeout_future(std::time::Duration::from_millis(120)).await;
            runtime.dispatch(AppMsg::Editor(super::EditorMsg::PreviewElapsed {
                session,
                timer_id,
            }));
        });
    }

    fn load_editor_note(&self, request_id: super::RequestId, note_id: carver_sdk::NoteId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .note_async(note_id)
                .await
                .map_err(display_error)
                .and_then(|note| note.ok_or_else(|| UiError::new("The note no longer exists")));
            runtime.dispatch(AppMsg::Library(LibraryReply::EditorLoaded {
                request_id,
                result,
            }));
        });
    }

    fn load_library_revision(&self, request_id: super::RequestId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client.change_revision_async().await.map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
                request_id,
                result,
            }));
        });
    }

    fn create_category(&self, name: String) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::CreateCategory, async move {
            client.create_category_async(name).await.map(|_| ())
        });
    }

    fn create_category_and_move_note(
        &self,
        action: ActionKey,
        name: String,
        note_id: carver_sdk::NoteId,
    ) {
        let client = self.inner.client.clone();
        self.complete_action(action, async move {
            let category = client.create_category_async(name).await?;
            client
                .move_note_async(note_id, category.id)
                .await
                .map(|_| ())
        });
    }

    fn save_note(&self, request: super::EditorSaveRequest) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .save_note_async(
                    request.note_id,
                    request.expected_revision,
                    request.source.clone(),
                )
                .await
                .map(|note| note.revision)
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::EditorSaved {
                request,
                result,
            }));
        });
    }

    fn store_editor_asset(
        &self,
        session: super::EditorSessionId,
        note_id: carver_sdk::NoteId,
        extension: String,
        bytes: Vec<u8>,
        alt: String,
        source_selection: Option<std::ops::Range<usize>>,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .store_asset_async(note_id, extension, bytes)
                .await
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::EditorAssetStored {
                session,
                alt,
                source_selection,
                result,
            }));
        });
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the effect is the fully typed immutable export snapshot"
    )]
    fn prepare_editor_export(
        &self,
        request_id: u64,
        session: super::EditorSessionId,
        note_id: carver_sdk::NoteId,
        source: String,
        filename_stem: String,
        format: EditorExportFormat,
        include_assets: bool,
        target_uri: String,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = async {
                let mut assets = Vec::new();
                if include_assets {
                    for path in carver_export::managed_asset_paths(&source) {
                        if let Some(bytes) = client
                            .note_asset_bytes_async(note_id, path.clone())
                            .await
                            .map_err(display_error)?
                        {
                            assets.push(carver_export::ManagedAsset { path, bytes });
                        }
                    }
                }
                let format = match format {
                    EditorExportFormat::Carve => carver_export::ExportFormat::Carve,
                    EditorExportFormat::Markdown => carver_export::ExportFormat::Markdown,
                    EditorExportFormat::Pdf => {
                        return Err(UiError::new(
                            "PDF export must be rendered by the GTK adapter.",
                        ));
                    }
                };
                carver_export::prepare_export(
                    &source,
                    &filename_stem,
                    format,
                    include_assets,
                    &assets,
                )
                .map_err(display_error)
            }
            .await;

            let result = result.map(|artifact| {
                let warnings = artifact
                    .warnings
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                runtime.inner.prepared_exports.borrow_mut().insert(
                    request_id,
                    PreparedExport {
                        artifact,
                        target_uri,
                    },
                );
                warnings
            });
            runtime.dispatch(AppMsg::Library(LibraryReply::EditorExportPrepared {
                request_id,
                session,
                result,
            }));
        });
    }

    fn write_editor_export(&self, request_id: u64) {
        let Some(prepared) = self.inner.prepared_exports.borrow_mut().remove(&request_id) else {
            self.dispatch(AppMsg::Library(LibraryReply::EditorExportWritten {
                request_id,
                result: Err(UiError::new("The prepared export is no longer available.")),
            }));
            return;
        };
        let file = gtk::gio::File::for_uri(&prepared.target_uri);
        let bytes = glib::Bytes::from_owned(prepared.artifact.bytes);
        let runtime = self.clone();
        file.replace_contents_bytes_async(
            &bytes,
            None,
            false,
            gtk::gio::FileCreateFlags::REPLACE_DESTINATION,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                runtime.dispatch(AppMsg::Library(LibraryReply::EditorExportWritten {
                    request_id,
                    result: result.map(|_| ()).map_err(display_error),
                }));
            },
        );
    }

    fn rename_category(&self, category_id: carver_sdk::CategoryId, name: String) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::RenameCategory(category_id), async move {
            client
                .rename_category_async(category_id, name)
                .await
                .map(|_| ())
        });
    }

    fn create_category_with_appearance(
        &self,
        name: String,
        appearance: carver_sdk::CategoryAppearance,
    ) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::CreateCategory, async move {
            client
                .create_category_with_appearance_async(name, appearance)
                .await
                .map(|_| ())
        });
    }

    fn update_category(
        &self,
        category_id: carver_sdk::CategoryId,
        name: String,
        appearance: carver_sdk::CategoryAppearance,
    ) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::UpdateCategory(category_id), async move {
            client
                .update_category_async(category_id, name, appearance)
                .await
                .map(|_| ())
        });
    }

    fn trash_category(&self, category_id: carver_sdk::CategoryId) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::TrashCategory(category_id), async move {
            client.trash_category_async(category_id).await
        });
    }

    fn move_note(
        &self,
        action: ActionKey,
        note_id: carver_sdk::NoteId,
        category_id: carver_sdk::CategoryId,
    ) {
        let client = self.inner.client.clone();
        self.complete_action(action, async move {
            client
                .move_note_async(note_id, category_id)
                .await
                .map(|_| ())
        });
    }

    fn trash_note(&self, note_id: carver_sdk::NoteId) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::TrashNote(note_id), async move {
            client.trash_note_async(note_id).await
        });
    }

    fn complete_action<F>(&self, action: ActionKey, operation: F)
    where
        F: Future<Output = Result<(), carver_sdk::LibraryError<B::Error>>> + 'static,
    {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = operation.await.map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::ActionFinished {
                action,
                result,
            }));
        });
    }

    fn restore_category(&self, category_id: carver_sdk::CategoryId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .restore_category_async(category_id)
                .await
                .map(|()| TrashMutation::CategoryRestored)
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::TrashMutationFinished {
                result,
            }));
        });
    }

    fn restore_note(&self, note_id: carver_sdk::NoteId) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .restore_note_async(note_id)
                .await
                .map(|()| TrashMutation::NoteRestored)
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::TrashMutationFinished {
                result,
            }));
        });
    }

    fn empty_trash(&self) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = client
                .empty_trash_async()
                .await
                .map(TrashMutation::Emptied)
                .map_err(display_error);
            runtime.dispatch(AppMsg::Library(LibraryReply::TrashMutationFinished {
                result,
            }));
        });
    }
}

fn display_error(error: impl std::fmt::Display) -> UiError {
    UiError::new(error.to_string())
}
