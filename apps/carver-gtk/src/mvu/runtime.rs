//! `GLib` runtime for executing reducer effects.

use std::{cell::RefCell, rc::Rc};

use carver_sdk::{LibraryBackend, LibraryClient};

use crate::view::ViewRefs;

use super::{AppModel, AppMsg, Effect, LibraryReply, TrashMutation, UiError, update};

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
            Effect::ScheduleSearch { timer_id } => {
                let runtime = self.clone();
                glib::spawn_future_local(async move {
                    glib::timeout_future(std::time::Duration::from_millis(250)).await;
                    runtime.dispatch(AppMsg::Browser(super::BrowserMsg::SearchTimerFired(
                        timer_id,
                    )));
                });
            }
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
        }
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
