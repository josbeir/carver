//! Imperative GTK rendering adapters for MVU model snapshots.

use std::cell::Cell;

use libadwaita as adw;

use crate::mvu::{AppModel, LoadState, Route};

/// GTK references used to render the high-level MVU resources.
///
/// This type intentionally owns widgets only. Application state lives in [`AppModel`].
pub struct ViewRefs {
    route_stack: gtk::Stack,
    sidebar_status: adw::StatusPage,
    browser_status: adw::StatusPage,
    trash_status: adw::StatusPage,
    rendering: Cell<bool>,
}

impl ViewRefs {
    /// Collects view references after a window has composed its GTK widget tree.
    #[must_use]
    pub fn new(
        route_stack: gtk::Stack,
        sidebar_status: adw::StatusPage,
        browser_status: adw::StatusPage,
        trash_status: adw::StatusPage,
    ) -> Self {
        Self {
            route_stack,
            sidebar_status,
            browser_status,
            trash_status,
            rendering: Cell::new(false),
        }
    }

    /// Renders one immutable model snapshot without invoking application actions.
    pub fn render(&self, model: &AppModel) {
        self.rendering.set(true);
        self.route_stack.set_visible_child_name(match model.route {
            Route::Browser => "browser",
            Route::Trash => "trash",
            Route::Editor => "editor",
        });
        render_resource(&self.sidebar_status, &model.sidebar, "No categories yet");
        render_resource(&self.browser_status, &model.browser.notes, "No notes yet");
        render_resource(&self.trash_status, &model.trash, "Trash is empty");
        self.rendering.set(false);
    }

    /// Reports whether a widget signal was caused by a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.rendering.get()
    }
}

fn render_resource<T>(status: &adw::StatusPage, resource: &LoadState<T>, empty_title: &str) {
    match resource {
        LoadState::Idle | LoadState::Ready(_) => status.set_title(empty_title),
        LoadState::Loading(_) => status.set_title("Loading…"),
        LoadState::Failed(error) => {
            status.set_title("Could not load content");
            status.set_description(Some(&error.message));
        }
    }
}
