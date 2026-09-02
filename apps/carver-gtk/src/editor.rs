//! Rich/source note editor, formatting, image paste, and autosave.

use std::{rc::Rc, time::Duration as StdDuration};

use carver_richtext::{ListMarker, RichBlock, RichInline, parse_carve};
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
    formatting::apply_theme_colors(&rich, &rich_buffer);
    connect_theme_colors(&rich, &rich_buffer);
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
pub(crate) fn render_rich_markup(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    source: &str,
    state: Option<&AppState>,
) {
    formatting::install_tags(buffer);
    buffer.set_text("");
    let Ok(document) = parse_carve(source) else {
        return;
    };
    for (index, block) in document.blocks.iter().enumerate() {
        if index > 0 && !block_ends_with_image(&document.blocks[index - 1]) {
            buffer.insert_at_cursor("\n");
        }
        render_block(view, buffer, block, state, block_ends_with_image(block));
    }
    formatting::apply_theme_colors(view, buffer);
}

fn connect_theme_colors(view: &gtk::TextView, buffer: &gtk::TextBuffer) {
    let view = view.clone();
    let buffer = buffer.clone();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        formatting::apply_theme_colors(&view, &buffer);
    });
}

fn block_ends_with_image(block: &RichBlock) -> bool {
    matches!(
        block,
        RichBlock::Paragraph(content) | RichBlock::Quote(content)
            if matches!(content.last(), Some(RichInline::Image { .. }))
    )
}

fn render_block(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    block: &RichBlock,
    state: Option<&AppState>,
    finish_image_line: bool,
) {
    let start_offset = buffer.end_iter().offset();
    match block {
        RichBlock::Heading { level, content } => {
            render_inlines(view, buffer, content, state, false);
            let end = buffer.end_iter();
            let start = buffer.iter_at_offset(start_offset);
            let tag = match level {
                1 => "rich-heading-1",
                2 => "rich-heading-2",
                _ => "rich-heading-3",
            };
            buffer.apply_tag_by_name(tag, &start, &end);
        }
        RichBlock::ListItem { marker, content } => {
            let (prefix, tag, prefix_width) = match marker {
                ListMarker::Bullet => ("• ", "rich-list-bullet", 2),
                ListMarker::Ordered => ("1. ", "rich-list-ordered", 3),
                ListMarker::TaskUnchecked => ("☐ ", "rich-list-task", 2),
                ListMarker::TaskChecked => ("☑ ", "rich-list-task", 2),
            };
            let mut marker_end = buffer.iter_at_offset(start_offset);
            buffer.insert(&mut marker_end, prefix);
            let start = buffer.iter_at_offset(start_offset);
            let marker_end = buffer.iter_at_offset(start_offset + prefix_width);
            buffer.apply_tag_by_name("rich-structural", &start, &marker_end);
            render_inlines(view, buffer, content, state, false);
            let end = buffer.end_iter();
            let start = buffer.iter_at_offset(start_offset);
            buffer.apply_tag_by_name(tag, &start, &end);
            if matches!(marker, ListMarker::TaskChecked) {
                buffer.apply_tag_by_name("rich-list-task-checked", &start, &end);
            }
        }
        RichBlock::CodeBlock { language, content } => {
            buffer.insert_at_cursor(content);
            let end = buffer.end_iter();
            let start = buffer.iter_at_offset(start_offset);
            formatting::apply_code_block_tag(buffer, &start, &end, language.as_deref());
        }
        RichBlock::Paragraph(content) => {
            render_inlines(view, buffer, content, state, finish_image_line);
        }
        RichBlock::Quote(content) => {
            render_inlines(view, buffer, content, state, finish_image_line);
            let end = buffer.end_iter();
            let start = buffer.iter_at_offset(start_offset);
            buffer.apply_tag_by_name("rich-quote", &start, &end);
        }
        RichBlock::Rule => {
            buffer.insert_at_cursor("────────────────");
            let end = buffer.end_iter();
            let start = buffer.iter_at_offset(start_offset);
            buffer.apply_tag_by_name("rich-structural", &start, &end);
            buffer.apply_tag_by_name("rich-rule", &start, &end);
        }
    }
}

fn render_inlines(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    content: &[RichInline],
    state: Option<&AppState>,
    finish_image_line: bool,
) {
    for (index, inline) in content.iter().enumerate() {
        let start_offset = buffer.end_iter().offset();
        match inline {
            RichInline::Text(text) => buffer.insert_at_cursor(text),
            RichInline::Bold(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-bold", &start, &buffer.end_iter());
            }
            RichInline::Italic(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-italic", &start, &buffer.end_iter());
            }
            RichInline::Strike(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-strike", &start, &buffer.end_iter());
            }
            RichInline::Underline(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-underline", &start, &buffer.end_iter());
            }
            RichInline::Highlight(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-highlight", &start, &buffer.end_iter());
            }
            RichInline::Inserted(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-inserted", &start, &buffer.end_iter());
            }
            RichInline::Deleted(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-deleted", &start, &buffer.end_iter());
            }
            RichInline::Code(text) => {
                buffer.insert_at_cursor(text);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-code", &start, &buffer.end_iter());
            }
            RichInline::Link { text, destination } => {
                buffer.insert_at_cursor(text);
                let start = buffer.iter_at_offset(start_offset);
                formatting::apply_link_tag(buffer, &start, &buffer.end_iter(), destination);
            }
            RichInline::Attribute { text, attributes } => {
                buffer.insert_at_cursor(text);
                let start = buffer.iter_at_offset(start_offset);
                formatting::apply_attribute_tag(buffer, &start, &buffer.end_iter(), attributes);
            }
            RichInline::Image { alt, path } => render_image(
                view,
                buffer,
                alt,
                path,
                state,
                finish_image_line && index + 1 == content.len(),
            ),
        }
    }
}

fn render_image(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    alt: &str,
    path: &str,
    state: Option<&AppState>,
    finish_line: bool,
) {
    let Some(state) = state else {
        buffer.insert_at_cursor(alt);
        return;
    };
    let note_id = state.current_note.borrow().as_ref().map(|note| note.id);
    let Some(note_id) = note_id else {
        buffer.insert_at_cursor(alt);
        return;
    };
    let Ok(Some(bytes)) = state.client.note_asset_bytes(note_id, path) else {
        buffer.insert_at_cursor(alt);
        return;
    };
    let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) else {
        buffer.insert_at_cursor(alt);
        return;
    };
    let start_offset = buffer.end_iter().offset();
    let mut cursor = buffer.iter_at_offset(start_offset);
    let anchor = buffer.create_child_anchor(&mut cursor);
    let picture = gtk::Picture::for_paintable(&texture);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(480, 320);
    view.add_child_at_anchor(&picture, &anchor);
    let mut cursor = buffer.iter_at_offset(start_offset + 1);
    buffer.insert(&mut cursor, &format!("![{alt}]({path})"));
    let source_start = buffer.iter_at_offset(start_offset + 1);
    buffer.apply_tag_by_name("rich-hidden-source", &source_start, &cursor);
    if finish_line {
        append_image_break(buffer, &mut cursor);
    }
}

fn append_image_break(buffer: &gtk::TextBuffer, cursor: &mut gtk::TextIter) {
    let break_start = cursor.offset();
    buffer.insert(cursor, "\n");
    let break_end = buffer.iter_at_offset(cursor.offset());
    let break_start = buffer.iter_at_offset(break_start);
    buffer.apply_tag_by_name("rich-image-break", &break_start, &break_end);
}

/// Installs Ctrl+V image paste support on one editor view.
pub(crate) fn install_image_paste(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::EventControllerKey {
    formatting::install_tags(buffer);
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
                    if !cursor.starts_line() {
                        buffer.insert(&mut cursor, "\n");
                    }
                    let anchor = buffer.create_child_anchor(&mut cursor);
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(true);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_size_request(480, 320);
                    view.add_child_at_anchor(&picture, &anchor);
                    let source_start_offset = cursor.offset();
                    let source = format!("![Pasted image]({path})");
                    buffer.insert(&mut cursor, &source);
                    let source_start = buffer.iter_at_offset(source_start_offset);
                    buffer.apply_tag_by_name("rich-hidden-source", &source_start, &cursor);
                    append_image_break(&buffer, &mut cursor);
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

/// Serializes the rich text buffer back into Carve source.
pub(crate) fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    let mut output = Vec::new();
    let mut line_number = 0;
    while line_number < buffer.line_count() {
        let Some(line) = buffer.iter_at_line(line_number) else {
            break;
        };
        if line.char() == '\n' {
            output.push(String::new());
            line_number += 1;
            continue;
        }
        let mut end = line;
        end.forward_to_line_end();
        if let Some(language) = formatting::code_block_language(&line) {
            let mut content = Vec::new();
            while let Some(code_line) = buffer.iter_at_line(line_number) {
                let mut code_end = code_line;
                code_end.forward_to_line_end();
                content.push(buffer.text(&code_line, &code_end, false).to_string());
                line_number += 1;
                if line_number >= buffer.line_count() {
                    break;
                }
                let Some(next_line) = buffer.iter_at_line(line_number) else {
                    break;
                };
                if formatting::code_block_language(&next_line).as_deref() != Some(&language) {
                    break;
                }
            }
            output.push(format!("```{language}\n{}\n```", content.join("\n")));
            continue;
        }
        output.push(serialize_line(&line, &end));
        line_number += 1;
    }
    if output.last().is_some_and(String::is_empty) {
        let mut trailing_break = buffer.end_iter();
        if trailing_break.backward_char() && has_tag(&trailing_break, "rich-image-break") {
            output.pop();
        }
    }
    output.join("\n").into()
}

fn serialize_line(start: &gtk::TextIter, end: &gtk::TextIter) -> String {
    if has_line_tag(start, "rich-rule") {
        return "---".to_owned();
    }
    let (prefix, marker_width) = if has_line_tag(start, "rich-heading-1") {
        ("# ", 0)
    } else if has_line_tag(start, "rich-heading-2") {
        ("## ", 0)
    } else if has_line_tag(start, "rich-heading-3") {
        ("### ", 0)
    } else if has_line_tag(start, "rich-quote") {
        ("> ", 0)
    } else if let Some((prefix, width)) = list_marker(start) {
        (prefix, width)
    } else {
        ("", 0)
    };
    let mut output = prefix.to_owned();
    let mut current = *start;
    let mut active = [false; 8];
    let delimiters = [
        ("rich-bold", "*"),
        ("rich-italic", "/"),
        ("rich-strike", "~"),
        ("rich-underline", "_"),
        ("rich-highlight", "="),
        ("rich-inserted", "{+"),
        ("rich-deleted", "{-"),
        ("rich-code", "`"),
    ];
    let closing_delimiters = ["*", "/", "~", "_", "=", "+}", "-}", "`"];
    while current.offset() < end.offset() {
        let mut next = current;
        next.forward_char();
        let tags = [
            has_tag(&current, "rich-bold"),
            has_tag(&current, "rich-italic"),
            has_tag(&current, "rich-strike"),
            has_tag(&current, "rich-underline"),
            has_tag(&current, "rich-highlight"),
            has_tag(&current, "rich-inserted"),
            has_tag(&current, "rich-deleted"),
            has_tag(&current, "rich-code"),
        ];
        for index in (0..active.len()).rev() {
            if active[index] && !tags[index] {
                output.push_str(closing_delimiters[index]);
                active[index] = false;
            }
        }
        for index in 0..active.len() {
            if !active[index] && tags[index] {
                output.push_str(delimiters[index].1);
                active[index] = true;
            }
        }
        let is_marker = current.offset() < start.offset() + marker_width;
        if !is_marker && !has_tag(&current, "rich-structural") && current.char() != '\u{fffc}' {
            if let Some((markup, metadata_end)) = serialize_metadata(&current, end) {
                output.push_str(&markup);
                current = metadata_end;
                continue;
            }
            output.push(current.char());
        }
        current = next;
    }
    for index in (0..active.len()).rev() {
        if active[index] {
            output.push_str(closing_delimiters[index]);
        }
    }
    output
}

fn serialize_metadata(
    start: &gtk::TextIter,
    end: &gtk::TextIter,
) -> Option<(String, gtk::TextIter)> {
    if let Some(destination) = formatting::link_destination(start) {
        let link_end = metadata_end(start, end, formatting::link_destination, &destination);
        return Some((
            format!("[{}]({destination})", iter_text(start, &link_end)),
            link_end,
        ));
    }
    let attributes = formatting::attribute_spec(start)?;
    let attribute_end = metadata_end(start, end, formatting::attribute_spec, &attributes);
    Some((
        format!("[{}]{{{attributes}}}", iter_text(start, &attribute_end)),
        attribute_end,
    ))
}

fn metadata_end(
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    metadata: impl Fn(&gtk::TextIter) -> Option<String>,
    expected: &str,
) -> gtk::TextIter {
    let mut current = *start;
    while current.offset() < end.offset() && metadata(&current).as_deref() == Some(expected) {
        current.forward_char();
    }
    current
}

fn iter_text(start: &gtk::TextIter, end: &gtk::TextIter) -> String {
    let mut output = String::new();
    let mut current = *start;
    while current.offset() < end.offset() {
        output.push(current.char());
        current.forward_char();
    }
    output
}

fn list_marker(start: &gtk::TextIter) -> Option<(&'static str, i32)> {
    if has_line_tag(start, "rich-list-task-checked") || starts_with(start, "☑ ") {
        Some(("- [x] ", 2))
    } else if starts_with(start, "☐ ") {
        Some(("- [ ] ", 2))
    } else if has_line_tag(start, "rich-list-bullet") || starts_with(start, "• ") {
        Some(("- ", 2))
    } else if has_line_tag(start, "rich-list-ordered") || starts_with(start, "1. ") {
        Some(("1. ", 3))
    } else if has_line_tag(start, "rich-list-task") {
        Some(("- [ ] ", 2))
    } else {
        None
    }
}

fn starts_with(start: &gtk::TextIter, text: &str) -> bool {
    let mut current = *start;
    text.chars().all(|character| {
        let matches = current.char() == character;
        if matches {
            current.forward_char();
        }
        matches
    })
}

fn has_tag(iter: &gtk::TextIter, name: &str) -> bool {
    iter.tags()
        .iter()
        .any(|tag| tag.name().as_deref() == Some(name))
}

fn has_line_tag(iter: &gtk::TextIter, name: &str) -> bool {
    let mut current = *iter;
    loop {
        if has_tag(&current, name) {
            return true;
        }
        if !current.forward_char() || current.line() != iter.line() {
            return false;
        }
    }
}
