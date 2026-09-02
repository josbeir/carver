//! Rich Carve rendering and image integration for the GTK editor.

use std::rc::Rc;

use carver_richtext::{ListMarker, RichBlock, RichInline, parse_carve};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{controller::AppState, formatting};

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
    formatting::apply_theme_colors(buffer);
}

pub(super) fn connect_theme_colors(buffer: &gtk::TextBuffer) {
    let manager = adw::StyleManager::default();
    let buffer_for_scheme = buffer.clone();
    manager.connect_dark_notify(move |_| {
        formatting::apply_theme_colors(&buffer_for_scheme);
    });
    let buffer_for_accent = buffer.clone();
    manager.connect_accent_color_rgba_notify(move |_| {
        formatting::apply_theme_colors(&buffer_for_accent);
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
                3 => "rich-heading-3",
                4 => "rich-heading-4",
                5 => "rich-heading-5",
                _ => "rich-heading-6",
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
            RichInline::Superscript(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-superscript", &start, &buffer.end_iter());
            }
            RichInline::Subscript(nodes) => {
                render_inlines(view, buffer, nodes, state, false);
                let start = buffer.iter_at_offset(start_offset);
                buffer.apply_tag_by_name("rich-subscript", &start, &buffer.end_iter());
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
    let start_offset = buffer.end_iter().offset();
    let mut cursor = buffer.iter_at_offset(start_offset);
    let anchor = buffer.create_child_anchor(&mut cursor);
    let client = state.client.clone();
    let view = view.clone();
    let path_for_load = path.to_owned();
    glib::spawn_future_local(async move {
        let Ok(Some(bytes)) = client.note_asset_bytes_async(note_id, path_for_load).await else {
            return;
        };
        let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) else {
            return;
        };
        let picture = gtk::Picture::for_paintable(&texture);
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_size_request(480, 320);
        view.add_child_at_anchor(&picture, &anchor);
    });
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
            let Some(note) = state.current_note.borrow().clone() else {
                return;
            };
            let client = state.client.clone();
            let bytes = bytes.as_ref().to_vec();
            glib::spawn_future_local(async move {
                match client
                    .store_asset_async(note.id, "png".to_owned(), bytes)
                    .await
                {
                    Ok(path) if view.widget_name() == "rich-editor" => {
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
                    Ok(path) => buffer.insert_at_cursor(&format!("\n![Pasted image]({path})\n")),
                    Err(error) => toast_overlay.add_toast(adw::Toast::new(&format!(
                        "Could not store pasted image: {error}"
                    ))),
                }
            });
        });
        glib::Propagation::Proceed
    });
    view.add_controller(controller.clone());
    controller
}
