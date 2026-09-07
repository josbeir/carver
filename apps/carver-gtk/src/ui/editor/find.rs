//! Native find-in-note controls for the editable editor surfaces.

use std::{cell::Cell, rc::Rc};

use carver_config::EditorMode;
use gtk::prelude::*;
use sourceview5::prelude::*;
use webkit6::prelude::*;

const FIND_OPTIONS: webkit6::FindOptions =
    webkit6::FindOptions::CASE_INSENSITIVE.union(webkit6::FindOptions::WRAP_AROUND);

/// An editor-local find bar backed by the native search engine for each surface.
#[derive(Clone)]
pub(crate) struct FindController {
    bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
    count: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
    close: gtk::Button,
    source_view: sourceview5::View,
    source_settings: sourceview5::SearchSettings,
    source_context: sourceview5::SearchContext,
    rich_view: webkit6::WebView,
    mode: Rc<Cell<EditorMode>>,
}

impl FindController {
    /// Builds the find bar for a source and rich-text editor pair.
    pub(crate) fn new(
        source_editor: &super::SourceEditor,
        rich_view: &webkit6::WebView,
        capture_widget: &impl IsA<gtk::Widget>,
    ) -> Self {
        let bar = gtk::SearchBar::new();
        bar.set_widget_name("editor-find-bar");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_margin_start(12);
        row.set_margin_end(12);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        let entry = gtk::SearchEntry::new();
        entry.set_widget_name("editor-find-entry");
        entry.set_placeholder_text(Some("Find in note"));
        entry.set_hexpand(true);
        let count = gtk::Label::new(None);
        count.set_widget_name("editor-find-count");
        count.add_css_class("dim-label");
        let previous = gtk::Button::from_icon_name("go-up-symbolic");
        previous.set_widget_name("editor-find-previous");
        previous.set_tooltip_text(Some("Previous match"));
        let next = gtk::Button::from_icon_name("go-down-symbolic");
        next.set_widget_name("editor-find-next");
        next.set_tooltip_text(Some("Next match"));
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.set_widget_name("editor-find-close");
        close.set_tooltip_text(Some("Close find"));
        row.append(&entry);
        row.append(&count);
        row.append(&previous);
        row.append(&next);
        row.append(&close);
        bar.set_child(Some(&row));
        bar.connect_entry(&entry);
        bar.set_key_capture_widget(Some(capture_widget));

        let source_view = source_editor.view().clone();
        let source_settings = sourceview5::SearchSettings::builder()
            .case_sensitive(false)
            .regex_enabled(false)
            .wrap_around(true)
            .build();
        let source_context =
            sourceview5::SearchContext::new(source_editor.native_buffer(), Some(&source_settings));
        source_context.set_highlight(true);
        let controller = Self {
            bar,
            entry,
            count,
            previous,
            next,
            close,
            source_view,
            source_settings,
            source_context,
            rich_view: rich_view.clone(),
            mode: Rc::new(Cell::new(EditorMode::Rich)),
        };
        controller.connect_signals(capture_widget);
        controller
    }

    /// Returns the find-bar widget for the editor toolbar view.
    pub(crate) fn widget(&self) -> &gtk::SearchBar {
        &self.bar
    }

    /// Synchronizes the active native search target with the visible editor mode.
    pub(crate) fn set_mode(&self, mode: EditorMode) {
        if self.mode.replace(mode) == mode {
            return;
        }
        self.finish_searches();
        if mode == EditorMode::Rendered {
            self.bar.set_search_mode(false);
        } else if self.bar.is_search_mode() {
            self.search();
        }
    }

    /// Closes a find session before a different note is projected into the editor.
    pub(crate) fn reset(&self) {
        self.finish_searches();
        self.entry.set_text("");
        self.bar.set_search_mode(false);
    }

    /// Refreshes the visible match count after either editable projection changes.
    pub(crate) fn refresh_after_document_change(&self) {
        if !self.bar.is_search_mode() || self.entry.text().is_empty() {
            return;
        }
        match self.mode.get() {
            EditorMode::Source => self.set_count(self.source_context.occurrences_count()),
            EditorMode::Rich => {
                if let Some(context) = self.rich_view.find_controller() {
                    context.count_matches(
                        self.entry.text().as_str(),
                        FIND_OPTIONS.bits(),
                        u32::MAX,
                    );
                }
            }
            EditorMode::Rendered => {}
        }
    }

    fn connect_signals(&self, capture_widget: &impl IsA<gtk::Widget>) {
        let controller = self.clone();
        self.entry
            .connect_search_changed(move |_| controller.search());
        let controller = self.clone();
        self.entry
            .connect_activate(move |_| controller.next_match());
        let controller = self.clone();
        self.previous
            .connect_clicked(move |_| controller.previous_match());
        let controller = self.clone();
        self.next.connect_clicked(move |_| controller.next_match());
        let controller = self.clone();
        self.close
            .connect_clicked(move |_| controller.bar.set_search_mode(false));
        let controller = self.clone();
        self.bar.connect_search_mode_enabled_notify(move |bar| {
            if bar.is_search_mode() {
                controller.search();
            } else {
                controller.finish_searches();
                controller.restore_focus();
            }
        });

        let controller = self.clone();
        self.source_context
            .connect_occurrences_count_notify(move |context| {
                if controller.mode.get() == EditorMode::Source && controller.bar.is_search_mode() {
                    controller.set_count(context.occurrences_count());
                }
            });
        if let Some(context) = self.rich_view.find_controller() {
            let controller = self.clone();
            context.connect_counted_matches(move |_, count| {
                if controller.mode.get() == EditorMode::Rich && controller.bar.is_search_mode() {
                    controller.set_count(i32::try_from(count).unwrap_or(i32::MAX));
                }
            });
            let controller = self.clone();
            context.connect_failed_to_find_text(move |_| {
                if controller.mode.get() == EditorMode::Rich && controller.bar.is_search_mode() {
                    controller.set_count(0);
                }
            });
        }

        let shortcut = gtk::EventControllerKey::new();
        shortcut.set_name(Some("editor-find-shortcuts"));
        shortcut.set_propagation_phase(gtk::PropagationPhase::Capture);
        let controller = self.clone();
        shortcut.connect_key_pressed(move |_, key, _, modifiers| {
            let control = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if control && key == gtk::gdk::Key::f && controller.mode.get() != EditorMode::Rendered {
                controller.open();
                return glib::Propagation::Stop;
            }
            if !controller.bar.is_search_mode() {
                return glib::Propagation::Proceed;
            }
            if key == gtk::gdk::Key::Escape {
                controller.bar.set_search_mode(false);
                return glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::F3 || (control && key == gtk::gdk::Key::g) {
                if shift {
                    controller.previous_match();
                } else {
                    controller.next_match();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        capture_widget.add_controller(shortcut);
    }

    fn open(&self) {
        self.bar.set_search_mode(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    fn search(&self) {
        if !self.bar.is_search_mode() {
            return;
        }
        let query = self.entry.text();
        if query.is_empty() || self.mode.get() == EditorMode::Rendered {
            self.finish_searches();
            return;
        }
        self.finish_searches();
        match self.mode.get() {
            EditorMode::Source => {
                self.source_settings.set_search_text(Some(query.as_str()));
                self.set_count(self.source_context.occurrences_count());
                self.select_source_match(true);
            }
            EditorMode::Rich => {
                let Some(context) = self.rich_view.find_controller() else {
                    self.set_count(0);
                    return;
                };
                context.search(query.as_str(), FIND_OPTIONS.bits(), u32::MAX);
                context.count_matches(query.as_str(), FIND_OPTIONS.bits(), u32::MAX);
            }
            EditorMode::Rendered => {}
        }
    }

    fn previous_match(&self) {
        if self.entry.text().is_empty() {
            return;
        }
        match self.mode.get() {
            EditorMode::Source => self.select_source_match(false),
            EditorMode::Rich => {
                if let Some(context) = self.rich_view.find_controller() {
                    context.search_previous();
                }
            }
            EditorMode::Rendered => {}
        }
    }

    fn next_match(&self) {
        if self.entry.text().is_empty() {
            return;
        }
        match self.mode.get() {
            EditorMode::Source => self.select_source_match(true),
            EditorMode::Rich => {
                if let Some(context) = self.rich_view.find_controller() {
                    context.search_next();
                }
            }
            EditorMode::Rendered => {}
        }
    }

    fn select_source_match(&self, forward: bool) {
        let buffer = self.source_view.buffer();
        let start = buffer.selection_bounds().map_or_else(
            || buffer.iter_at_mark(&buffer.get_insert()),
            |(selection_start, selection_end)| {
                if forward {
                    selection_end
                } else {
                    selection_start
                }
            },
        );
        let matched = if forward {
            self.source_context.forward(&start)
        } else {
            self.source_context.backward(&start)
        };
        if let Some((mut match_start, match_end, _)) = matched {
            buffer.select_range(&match_start, &match_end);
            let _ = self
                .source_view
                .scroll_to_iter(&mut match_start, 0.2, false, 0.0, 0.0);
        }
    }

    fn finish_searches(&self) {
        self.source_settings.set_search_text(None);
        if let Some(context) = self.rich_view.find_controller() {
            context.search_finish();
        }
        self.set_count(0);
    }

    fn set_count(&self, count: i32) {
        self.count.set_text(&Self::match_count_label(count));
        let has_matches = count > 0;
        self.previous.set_sensitive(has_matches);
        self.next.set_sensitive(has_matches);
    }

    fn match_count_label(count: i32) -> String {
        match count {
            value if value < 0 => String::from("Searching…"),
            0 => String::from("No matches"),
            1 => String::from("1 match"),
            value => format!("{value} matches"),
        }
    }

    fn restore_focus(&self) {
        match self.mode.get() {
            EditorMode::Source => {
                self.source_view.grab_focus();
            }
            EditorMode::Rich => {
                self.rich_view.grab_focus();
            }
            EditorMode::Rendered => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FindController;

    #[test]
    fn match_count_should_use_human_readable_labels() {
        assert_eq!(FindController::match_count_label(-1), "Searching…");
        assert_eq!(FindController::match_count_label(0), "No matches");
        assert_eq!(FindController::match_count_label(1), "1 match");
        assert_eq!(FindController::match_count_label(2), "2 matches");
    }
}
