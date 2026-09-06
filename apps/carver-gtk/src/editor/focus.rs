//! Restores keyboard focus to the active editable surface after native controls run.

use std::{cell::Cell, rc::Rc};

use carver_config::EditorMode;
use gtk::prelude::*;

/// The active editor surface that should receive input after toolbar interaction.
#[derive(Clone)]
pub(crate) struct EditorFocusRestorer {
    mode: Rc<Cell<EditorMode>>,
    source: gtk::TextView,
    rich: webkit6::WebView,
}

impl EditorFocusRestorer {
    /// Creates a restorer that follows the toolbar's active editor mode.
    pub(crate) fn new(
        mode: Rc<Cell<EditorMode>>,
        source: &gtk::TextView,
        rich: &webkit6::WebView,
    ) -> Self {
        Self {
            mode,
            source: source.clone(),
            rich: rich.clone(),
        }
    }

    /// Returns focus after GTK has completed the current button, popover, or dialog event.
    pub(crate) fn restore_later(&self) {
        let restorer = self.clone();
        glib::idle_add_local_once(move || restorer.restore());
    }

    fn restore(&self) {
        match self.mode.get() {
            EditorMode::Source => Self::focus_widget(self.source.upcast_ref()),
            EditorMode::Rich => Self::focus_widget(self.rich.upcast_ref()),
            EditorMode::Rendered => {}
        }
    }

    fn focus_widget(widget: &gtk::Widget) {
        if let Some(window) = widget.root().and_downcast::<gtk::Window>() {
            gtk::prelude::RootExt::set_focus(&window, Some(widget));
        } else {
            widget.grab_focus();
        }
    }
}
