//! Rich/source note editor, formatting, image paste, and autosave.

use std::{rc::Rc, time::Duration as StdDuration};

use carver_richtext::parse_carve;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::refresh_browser,
    controller::{AppState, save_current_note, store_pasted_image, trash_current_note},
    formatting,
    sidebar::refresh_sidebar,
    trash::refresh_trash,
};

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
        match trash_current_note(&state_for_trash) {
            Ok(true) => {
                refresh_browser(&state_for_trash);
                refresh_sidebar(&state_for_trash);
                refresh_trash(&state_for_trash);
                stack_for_trash.set_visible_child_name("browser");
                let toast = adw::Toast::new("Moved note to Trash");
                toast.set_button_label(Some("Undo"));
                let state_for_undo = Rc::clone(&state_for_trash);
                toast.connect_button_clicked(move |_| {
                    if let Some(note) = note_for_undo.as_ref()
                        && state_for_undo.client.restore_note(note.id).is_ok()
                    {
                        refresh_browser(&state_for_undo);
                        refresh_sidebar(&state_for_undo);
                        refresh_trash(&state_for_undo);
                    }
                });
                toast_for_trash.add_toast(toast);
            }
            Ok(false) => {}
            Err(error) => toast_for_trash.add_toast(adw::Toast::new(&format!(
                "Could not move note to Trash: {error}"
            ))),
        }
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
        match save_current_note(&state_for_back, &source) {
            Ok(_) => {
                refresh_browser(&state_for_back);
                stack_for_back.set_visible_child_name("browser");
            }
            Err(error) => {
                toast_for_back.add_toast(adw::Toast::new(&format!("Could not save note: {error}")));
            }
        }
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
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
) {
    let state_for_visible = Rc::clone(state);
    let rich_for_visible = rich_buffer.clone();
    let source_for_visible = source_buffer.clone();
    let mode_for_visible = mode.clone();
    let editor_stack_for_visible = editor_stack.clone();
    stack.connect_visible_child_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("editor")
            && let Some(note) = state_for_visible.current_note.borrow().as_ref()
        {
            state_for_visible.synchronizing_editor.set(true);
            if parse_carve(&note.source).is_ok() {
                render_rich_markup_placeholder(&rich_for_visible, &note.source);
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

fn render_rich_markup(view: &gtk::TextView, buffer: &gtk::TextBuffer, source: &str) {
    let _ = view;
    render_rich_markup_placeholder(buffer, source);
}

fn render_rich_markup_placeholder(buffer: &gtk::TextBuffer, source: &str) {
    buffer.set_text("");
    let mut remaining = source;
    while let Some(start) = remaining.find('*') {
        let before = &remaining[..start];
        buffer.insert_at_cursor(before);
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('*') else {
            buffer.insert_at_cursor(&remaining[start..]);
            return;
        };
        let content = &after_start[..end];
        let mut tag_start = buffer.end_iter();
        buffer.insert(&mut tag_start, content);
        let tag_end = buffer.end_iter();
        buffer.apply_tag_by_name("rich-bold", &tag_start, &tag_end);
        remaining = &after_start[end + 1..];
    }
    buffer.insert_at_cursor(remaining);
}

/// Installs Ctrl+V image paste support on one editor view.
pub(crate) fn install_image_paste(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let state = Rc::clone(state);
    let buffer = buffer.clone();
    let view_for_handler = view.clone();
    let toast_overlay = toast_overlay.clone();
    let clipboard = view.display().clipboard();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if key != gtk::gdk::Key::v || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let view = view_for_handler.clone();
        let toast_overlay = toast_overlay.clone();
        clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
            let Ok(Some(texture)) = result else {
                return;
            };
            let bytes = texture.save_to_png_bytes();
            match store_pasted_image(&state, bytes.as_ref()) {
                Ok(Some(path)) if view.widget_name() == "rich-editor" => {
                    let insert = buffer.get_insert();
                    let mut cursor = buffer.iter_at_mark(&insert);
                    buffer.insert(&mut cursor, "\n");
                    let anchor = buffer.create_child_anchor(&mut cursor);
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_size_request(480, -1);
                    view.add_child_at_anchor(&picture, &anchor);
                    let source_start = cursor;
                    buffer.insert(&mut cursor, &format!("![Pasted image]({path})\n"));
                    buffer.apply_tag_by_name("rich-hidden-source", &source_start, &cursor);
                }
                Ok(Some(path)) => buffer.insert_at_cursor(&format!("\n![Pasted image]({path})\n")),
                Ok(None) => {}
                Err(error) => toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Could not store pasted image: {error}"
                ))),
            }
        });
        glib::Propagation::Proceed
    });
    view.add_controller(controller.clone());
    controller
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
        match state.client.save_note(note.id, note.revision, &source) {
            Ok(saved) => {
                state.current_note.replace(Some(saved));
            }
            Err(error) => {
                toast_overlay.add_toast(adw::Toast::new(&format!("Could not save note: {error}")));
            }
        }
    });
}

fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    let mut output = String::new();
    let mut current = buffer.start_iter();
    let mut bold = false;
    while !current.is_end() {
        let is_bold = current
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-bold"));
        if is_bold != bold {
            output.push('*');
            bold = is_bold;
        }
        if current.char() != '\u{fffc}' {
            output.push(current.char());
        }
        current.forward_char();
    }
    if bold {
        output.push('*');
    }
    output.into()
}
