//! `GLib` runtime for executing reducer effects.

use std::{cell::RefCell, future::Future, rc::Rc};

use carver_sdk::{LibraryBackend, LibraryClient};

use crate::view::ViewRefs;

use super::{ActionKey, AppModel, AppMsg, Effect, LibraryReply, TrashMutation, UiError, update};

/// Main-thread dispatcher that renders model snapshots and executes typed effects.
///
/// A runtime is intentionally local to one GTK window. Its model is borrowed only while the
/// reducer runs; rendering and effect execution happen after that borrow is released.
pub struct AppRuntime<B: LibraryBackend> {
    inner: Rc<RuntimeInner<B>>,
}

struct RuntimeInner<B: LibraryBackend> {
    client: LibraryClient<B>,
    model: RefCell<AppModel>,
    view: ViewRefs,
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
        Self {
            inner: Rc::new(RuntimeInner {
                client,
                model: RefCell::new(model),
                view,
            }),
        }
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
            Effect::ScheduleSearch { timer_id } => self.schedule_search(timer_id),
            Effect::ScheduleEditorSave {
                session,
                timer_id,
                delay_ms,
            } => self.schedule_editor_save(session, timer_id, delay_ms),
            Effect::SaveNote { request } => self.save_note(request),
            Effect::LoadSidebar { request_id } => {
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
            Effect::LoadBrowser {
                request_id,
                category_id,
                query,
            } => {
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
            Effect::RenameCategory { category_id, name } => {
                self.rename_category(category_id, name);
            }
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

    fn schedule_search(&self, timer_id: super::TimerId) {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            glib::timeout_future(std::time::Duration::from_millis(250)).await;
            runtime.dispatch(AppMsg::Browser(super::BrowserMsg::SearchTimerFired(
                timer_id,
            )));
        });
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

    fn create_category(&self, name: String) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::CreateCategory, async move {
            client.create_category_async(name).await.map(|_| ())
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

    fn rename_category(&self, category_id: carver_sdk::CategoryId, name: String) {
        let client = self.inner.client.clone();
        self.complete_action(ActionKey::RenameCategory(category_id), async move {
            client
                .rename_category_async(category_id, name)
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
