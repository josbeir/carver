//! Decorative marker guidance, using GTK's native scrolling overlay without buffer edits.
//!
//! CONTEXT: `GtkSourceView` snippets insert real source. The existing `GtkTextView` overlay
//! supplies the presentation boundary we need without another crate or grammar change.

use std::{cell::Cell, rc::Rc};

use gettextrs::gettext;
use gtk::prelude::*;

use super::source_context::SourceContextCache;
use crate::mvu::SourcePlaceholder;

/// A transient projection of the active source marker, never part of note content.
#[derive(Clone)]
pub(super) struct SourcePlaceholderView {
    view: glib::WeakRef<gtk::TextView>,
    label: glib::WeakRef<gtk::Label>,
    context: SourceContextCache,
    preediting: Rc<Cell<bool>>,
    active: Rc<Cell<bool>>,
    pending: Rc<Cell<bool>>,
    position: Rc<Cell<Option<(i32, i32)>>>,
}

impl SourcePlaceholderView {
    /// Attaches a passive label that GTK scrolls and clips with the source text.
    pub(super) fn new(
        view: &gtk::TextView,
        scroll: &gtk::ScrolledWindow,
        context: &SourceContextCache,
    ) -> Self {
        let label = gtk::Label::builder()
            .accessible_role(gtk::AccessibleRole::Presentation)
            .can_target(false)
            .can_focus(false)
            .visible(false)
            .build();
        label.set_widget_name("source-marker-placeholder");
        label.add_css_class("dim-label");
        view.add_overlay(&label, 0, 0);
        let projection = Self {
            view: view.downgrade(),
            label: label.downgrade(),
            context: context.clone(),
            preediting: Rc::new(Cell::new(false)),
            active: Rc::new(Cell::new(false)),
            pending: Rc::new(Cell::new(false)),
            position: Rc::new(Cell::new(None)),
        };
        let for_mark = projection.clone();
        view.buffer().connect_mark_set(move |_, _, mark| {
            if matches!(mark.name().as_deref(), Some("insert" | "selection_bound")) {
                for_mark.refresh();
            }
        });
        let for_change = projection.clone();
        view.buffer().connect_changed(move |_| for_change.refresh());
        let for_preedit = projection.clone();
        view.connect_preedit_changed(move |_, text| {
            for_preedit.preediting.set(!text.is_empty());
            for_preedit.refresh();
        });
        let for_map = projection.clone();
        view.connect_map(move |_| for_map.refresh());
        for adjustment in [scroll.hadjustment(), scroll.vadjustment()] {
            let for_resize = projection.clone();
            // Page size and range changes also cover wrapping and viewport allocation.
            adjustment.connect_changed(move |_| for_resize.refresh());
        }
        projection
    }

    /// Limits guidance work to the source projection selected by the model.
    pub(super) fn set_active(&self, active: bool) {
        self.active.set(active);
        self.refresh();
    }

    /// Coalesces layout work after GTK has applied text, font, and allocation changes.
    pub(super) fn refresh(&self) {
        let Some(label) = self.label.upgrade() else {
            return;
        };
        if !self.active.get() {
            label.set_visible(false);
            return;
        }
        // Hide immediately when real content or an IME preedit replaces the guidance.
        if self.preediting.get() || self.context.placeholder().is_none() {
            label.set_visible(false);
        }
        if self.pending.replace(true) {
            return;
        }
        let projection = self.clone();
        glib::idle_add_local_once(move || {
            projection.pending.set(false);
            projection.render();
        });
    }

    fn render(&self) {
        let (Some(view), Some(label)) = (self.view.upgrade(), self.label.upgrade()) else {
            return;
        };
        let hint = (self.active.get() && !self.preediting.get())
            .then(|| self.context.placeholder())
            .flatten();
        let Some(hint) = hint else {
            label.set_visible(false);
            return;
        };
        label.set_label(&match hint {
            SourcePlaceholder::ListItem => gettext("List item"),
            SourcePlaceholder::Task => gettext("Task"),
        });
        // As a native text-view child the label inherits the source font and theme.
        let cursor = view.buffer().iter_at_mark(&view.buffer().get_insert());
        let location = view.iter_location(&cursor);
        // A small gap keeps the decorative text clear of the insertion caret, including
        // a checkbox whose author has not typed its trailing separator yet.
        let position = (location.x() + 3, location.y());
        if self.position.replace(Some(position)) != Some(position) {
            view.move_overlay(&label, position.0, position.1);
        }
        label.set_visible(true);
    }
}
