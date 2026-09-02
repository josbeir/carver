//! Rich/source note editor, formatting, image paste, and autosave.

use std::{rc::Rc, time::Duration as StdDuration};

use carver_richtext::parse_carve;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::refresh_browser, controller::AppState, formatting, sidebar::refresh_sidebar,
    trash::refresh_trash,
};

mod render;
mod source;

use render::connect_theme_colors;
pub(crate) use render::{install_image_paste, render_rich_markup};
pub(crate) use source::buffer_text;
use source::has_tag;

/// Builds the note editor and connects its user-facing actions.
pub(crate) fn build_editor(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-to-notes-button");
    back.set_tooltip_text(Some("Back to notes"));
    header.pack_start(&back);
    let title = adw::WindowTitle::new("Note", "Saved automatically");
    header.set_title_widget(Some(&title));
    let mode = gtk::ToggleButton::with_label("Carve Source");
    mode.set_widget_name("editor-mode-toggle");
    header.pack_end(&mode);
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name("delete-note-button");
    trash.set_tooltip_text(Some("Move Note to Trash"));
    trash.add_css_class("flat");
    header.pack_end(&trash);
    view.add_top_bar(&header);

    let format_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    format_bar.set_widget_name("formatting-toolbar");
    format_bar.add_css_class("toolbar");
    format_bar.set_margin_start(12);
    format_bar.set_margin_end(12);
    format_bar.set_margin_top(6);
    format_bar.set_margin_bottom(6);

    let editor_stack = gtk::Stack::new();
    let rich_buffer = gtk::TextBuffer::new(None);
    let rich = text_view(&rich_buffer, "rich-editor", false);
    let source_buffer = gtk::TextBuffer::new(None);
    let source = text_view(&source_buffer, "source-editor", true);
    formatting::append_controls(&format_bar, &rich_buffer);
    formatting::apply_theme_colors(&rich_buffer);
    connect_theme_colors(&rich_buffer);
    install_list_continuation(&rich, &rich_buffer);
    install_editor_shortcuts(&rich, &rich_buffer);
    add_editor_pages(&editor_stack, &rich, &source);
    view.add_top_bar(&format_bar);
    view.set_content(Some(&editor_stack));

    connect_mode_toggle(
        state,
        &mode,
        &editor_stack,
        &rich,
        &rich_buffer,
        &source_buffer,
    );
    connect_trash_action(state, stack, toast_overlay, &trash);
    connect_back_action(
        state,
        stack,
        toast_overlay,
        &back,
        &rich_buffer,
        &source_buffer,
    );
    connect_autosave(state, toast_overlay, &rich_buffer, &source_buffer);
    let _rich_image_paste = install_image_paste(&rich, &rich_buffer, state, toast_overlay);
    let _source_image_paste = install_image_paste(&source, &source_buffer, state, toast_overlay);
    connect_note_loading(
        state,
        stack,
        &mode,
        &editor_stack,
        &rich,
        &rich_buffer,
        &source_buffer,
    );
    view.upcast()
}

fn text_view(buffer: &gtk::TextBuffer, name: &str, monospace: bool) -> gtk::TextView {
    let view = gtk::TextView::with_buffer(buffer);
    view.set_widget_name(name);
    view.set_monospace(monospace);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_top_margin(24);
    view.set_bottom_margin(24);
    view.set_left_margin(24);
    view.set_right_margin(24);
    view
}

fn add_editor_pages(editor_stack: &gtk::Stack, rich: &gtk::TextView, source: &gtk::TextView) {
    let rich_scroll = gtk::ScrolledWindow::new();
    rich_scroll.set_child(Some(rich));
    let source_scroll = gtk::ScrolledWindow::new();
    source_scroll.set_child(Some(source));
    editor_stack.add_named(&rich_scroll, Some("rich"));
    editor_stack.add_named(&source_scroll, Some("source"));
    editor_stack.set_visible_child_name("rich");
}

fn connect_mode_toggle(
    state: &Rc<AppState>,
    mode: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    rich_view: &gtk::TextView,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
) {
    let state_for_mode = Rc::clone(state);
    let stack_for_mode = editor_stack.clone();
    let rich_for_mode = rich_buffer.clone();
    let rich_view_for_mode = rich_view.clone();
    let source_for_mode = source_buffer.clone();
    mode.connect_toggled(move |button| {
        let source_mode = button.is_active();
        state_for_mode.source_mode.set(source_mode);
        state_for_mode.synchronizing_editor.set(true);
        if source_mode {
            source_for_mode.set_text(&buffer_text(&rich_for_mode));
            stack_for_mode.set_visible_child_name("source");
            button.set_label("Rich Text");
        } else {
            render_rich_markup(
                &rich_view_for_mode,
                &rich_for_mode,
                &buffer_text(&source_for_mode),
                Some(&state_for_mode),
            );
            stack_for_mode.set_visible_child_name("rich");
            button.set_label("Carve Source");
        }
        state_for_mode.synchronizing_editor.set(false);
    });
}

fn connect_trash_action(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
    trash: &gtk::Button,
) {
    let state_for_trash = Rc::clone(state);
    let stack_for_trash = stack.clone();
    let toast_for_trash = toast_overlay.clone();
    trash.connect_clicked(move |_| {
        let note_for_undo = state_for_trash.current_note.borrow().clone();
        let Some(note) = note_for_undo.clone() else {
            return;
        };
        let state = Rc::clone(&state_for_trash);
        let stack = stack_for_trash.clone();
        let toast_overlay = toast_for_trash.clone();
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            match client.trash_note_async(note.id).await {
                Ok(()) => {
                    state.current_note.take();
                    refresh_browser(&state);
                    refresh_sidebar(&state);
                    refresh_trash(&state);
                    stack.set_visible_child_name("browser");
                    let toast = adw::Toast::new("Moved note to Trash");
                    toast.set_button_label(Some("Undo"));
                    let state_for_undo = Rc::clone(&state);
                    toast.connect_button_clicked(move |_| {
                        let Some(note) = note_for_undo.as_ref() else {
                            return;
                        };
                        let state = Rc::clone(&state_for_undo);
                        let client = state.client.clone();
                        let note_id = note.id;
                        glib::spawn_future_local(async move {
                            if client.restore_note_async(note_id).await.is_ok() {
                                refresh_browser(&state);
                                refresh_sidebar(&state);
                                refresh_trash(&state);
                            }
                        });
                    });
                    toast_overlay.add_toast(toast);
                }
                Err(error) => toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Could not move note to Trash: {error}"
                ))),
            }
        });
    });
}

fn connect_back_action(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
    back: &gtk::Button,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
) {
    let state_for_back = Rc::clone(state);
    let stack_for_back = stack.clone();
    let rich_for_back = rich_buffer.clone();
    let source_for_back = source_buffer.clone();
    let toast_for_back = toast_overlay.clone();
    back.connect_clicked(move |_| {
        if state_for_back.current_note.borrow().is_none() {
            stack_for_back.set_visible_child_name("browser");
            return;
        }
        let source = if state_for_back.source_mode.get() {
            buffer_text(&source_for_back)
        } else {
            buffer_text(&rich_for_back)
        };
        let Some(note) = state_for_back.current_note.borrow().clone() else {
            return;
        };
        state_for_back
            .autosave_generation
            .set(state_for_back.autosave_generation.get().saturating_add(1));
        let state = Rc::clone(&state_for_back);
        let stack = stack_for_back.clone();
        let toast = toast_for_back.clone();
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            match client
                .save_note_async(note.id, note.revision, source.to_string())
                .await
            {
                Ok(saved) => {
                    state.current_note.replace(Some(saved));
                    refresh_browser(&state);
                    stack.set_visible_child_name("browser");
                }
                Err(error) => {
                    toast.add_toast(adw::Toast::new(&format!("Could not save note: {error}")));
                }
            }
        });
    });
}

fn connect_autosave(
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
) {
    let state_for_rich_save = Rc::clone(state);
    let rich_for_save = rich_buffer.clone();
    let source_for_rich_save = source_buffer.clone();
    let toast_for_rich_save = toast_overlay.clone();
    rich_buffer.connect_changed(move |_| {
        if !state_for_rich_save.synchronizing_editor.get() {
            schedule_autosave(
                &state_for_rich_save,
                &rich_for_save,
                &source_for_rich_save,
                &toast_for_rich_save,
            );
        }
    });
    let state_for_source_save = Rc::clone(state);
    let rich_for_source_save = rich_buffer.clone();
    let source_for_source_save = source_buffer.clone();
    let toast_for_source_save = toast_overlay.clone();
    source_buffer.connect_changed(move |_| {
        if !state_for_source_save.synchronizing_editor.get() {
            schedule_autosave(
                &state_for_source_save,
                &rich_for_source_save,
                &source_for_source_save,
                &toast_for_source_save,
            );
        }
    });
}

fn connect_note_loading(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    mode: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    rich_view: &gtk::TextView,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
) {
    let state_for_visible = Rc::clone(state);
    let rich_for_visible = rich_buffer.clone();
    let rich_view_for_visible = rich_view.clone();
    let source_for_visible = source_buffer.clone();
    let mode_for_visible = mode.clone();
    let editor_stack_for_visible = editor_stack.clone();
    stack.connect_visible_child_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("editor")
            && let Some(note) = state_for_visible.current_note.borrow().as_ref()
        {
            state_for_visible.synchronizing_editor.set(true);
            if parse_carve(&note.source).is_ok() {
                render_rich_markup(
                    &rich_view_for_visible,
                    &rich_for_visible,
                    &note.source,
                    Some(&state_for_visible),
                );
            } else {
                mode_for_visible.set_active(true);
                editor_stack_for_visible.set_visible_child_name("source");
                state_for_visible.source_mode.set(true);
            }
            source_for_visible.set_text(&note.source);
            state_for_visible.synchronizing_editor.set(false);
        }
    });
}

/// Renders supported Carve source into the native rich text buffer.
pub(crate) fn install_list_continuation(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let buffer = buffer.clone();
    controller.connect_key_pressed(move |_controller, key, _keycode, _modifiers| {
        if key != gtk::gdk::Key::Return {
            return glib::Propagation::Proceed;
        }
        let insert = buffer.get_insert();
        let cursor = buffer.iter_at_mark(&insert);
        let mut line_start = cursor;
        line_start.set_line_offset(0);
        let marker = if has_tag(&line_start, "rich-list-bullet") {
            Some(("• ", "rich-list-bullet", 2))
        } else if has_tag(&line_start, "rich-list-ordered") {
            Some(("1. ", "rich-list-ordered", 3))
        } else if has_tag(&line_start, "rich-list-task") {
            Some(("☐ ", "rich-list-task", 2))
        } else {
            None
        };
        let Some((prefix, tag, prefix_width)) = marker else {
            return glib::Propagation::Proceed;
        };
        let mut line_end = line_start;
        line_end.forward_to_line_end();
        if line_is_empty_list_item(&line_start, &line_end) {
            buffer.remove_tag_by_name(tag, &line_start, &line_end);
            remove_structural_prefix(&buffer, &mut line_start, &mut line_end);
            return glib::Propagation::Stop;
        }
        let mut insertion = cursor;
        buffer.insert(&mut insertion, "\n");
        let start_offset = insertion.offset();
        buffer.insert(&mut insertion, prefix);
        let marker_start = buffer.iter_at_offset(start_offset);
        let marker_end = buffer.iter_at_offset(start_offset + prefix_width);
        buffer.apply_tag_by_name("rich-structural", &marker_start, &marker_end);
        buffer.apply_tag_by_name(tag, &marker_start, &insertion);
        glib::Propagation::Stop
    });
    view.add_controller(controller.clone());
    controller
}

/// Installs the standard keyboard shortcuts supported by the rich editor.
pub(crate) fn install_editor_shortcuts(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let buffer = buffer.clone();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }

        let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let handled = match key {
            gtk::gdk::Key::b if !shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-bold");
                true
            }
            gtk::gdk::Key::i if !shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-italic");
                true
            }
            gtk::gdk::Key::x if shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-strike");
                true
            }
            gtk::gdk::Key::_8 if shift => {
                formatting::toggle_selected_blocks(&buffer, "rich-list-bullet", "• ");
                true
            }
            gtk::gdk::Key::_7 if shift => {
                formatting::toggle_selected_blocks(&buffer, "rich-list-ordered", "1. ");
                true
            }
            _ => false,
        };

        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.add_controller(controller.clone());
    controller
}

fn line_is_empty_list_item(start: &gtk::TextIter, end: &gtk::TextIter) -> bool {
    let mut current = *start;
    while current.offset() < end.offset() {
        if !has_tag(&current, "rich-structural") && !current.char().is_whitespace() {
            return false;
        }
        current.forward_char();
    }
    true
}

fn remove_structural_prefix(
    buffer: &gtk::TextBuffer,
    start: &mut gtk::TextIter,
    end: &mut gtk::TextIter,
) {
    let mut prefix_end = *start;
    while prefix_end.offset() < end.offset() && has_tag(&prefix_end, "rich-structural") {
        prefix_end.forward_char();
    }
    if prefix_end.offset() > start.offset() {
        buffer.delete(start, &mut prefix_end);
        *end = *start;
        end.forward_to_line_end();
    }
}

fn schedule_autosave(
    state: &Rc<AppState>,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
    toast_overlay: &adw::ToastOverlay,
) {
    let delay = state.config.borrow().editor.autosave_delay_ms;
    let generation = state.autosave_generation.get().saturating_add(1);
    state.autosave_generation.set(generation);
    let state = Rc::clone(state);
    let rich_buffer = rich_buffer.clone();
    let source_buffer = source_buffer.clone();
    let toast_overlay = toast_overlay.clone();
    glib::timeout_add_local_once(StdDuration::from_millis(delay), move || {
        if state.autosave_generation.get() != generation {
            return;
        }
        let Some(note) = state.current_note.borrow().clone() else {
            return;
        };
        let source = if state.source_mode.get() {
            buffer_text(&source_buffer)
        } else {
            buffer_text(&rich_buffer)
        };
        if state.save_in_flight.get() {
            return;
        }
        state.save_in_flight.set(true);
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            match client
                .save_note_async(note.id, note.revision, source.to_string())
                .await
            {
                Ok(saved) => {
                    state.current_note.replace(Some(saved));
                }
                Err(error) => {
                    toast_overlay
                        .add_toast(adw::Toast::new(&format!("Could not save note: {error}")));
                }
            }
            state.save_in_flight.set(false);
            if state.autosave_generation.get() != generation {
                schedule_autosave(&state, &rich_buffer, &source_buffer, &toast_overlay);
            }
        });
    });
}
