//! Rich/source note editor, formatting, image paste, and autosave.

use std::{cell::Cell, rc::Rc, time::Duration as StdDuration};

use carver_config::EditorMode;
use carver_richtext::{EditorProjection, editor_projection};
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::BreakpointBinExt;
use webkit6::prelude::*;

use crate::{
    browser::refresh_browser, controller::AppState, formatting, sidebar::refresh_sidebar,
    sidebar::sidebar_toggle_button, trash::refresh_trash,
};

mod preview;
mod render;
mod source;
pub(crate) mod source_commands;

use preview::{build_preview, load_preview};
use render::connect_theme_colors;
pub(crate) use render::{install_image_paste, render_rich_markup};
pub(crate) use source::buffer_text;
use source::has_tag;

/// Builds the note editor and connects its user-facing actions.
#[expect(
    clippy::too_many_lines,
    reason = "widget construction stays together so ownership and lifecycle are explicit"
)]
pub(crate) fn build_editor(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
    split_view: &adw::NavigationSplitView,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let toggle_sidebar = sidebar_toggle_button(split_view, "editor-toggle-categories-button");
    header.pack_start(&toggle_sidebar);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-to-notes-button");
    back.set_tooltip_text(Some("Back to notes"));
    header.pack_start(&back);
    let mode_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_group.add_css_class("linked");
    mode_group.set_widget_name("editor-mode-group");
    let rich_mode = editor_mode_button(
        "editor-mode-rich",
        "document-edit-symbolic",
        "Edit",
        "Edit with rich text",
    );
    rich_mode.set_active(true);
    let source_mode = editor_mode_button(
        "editor-mode-source",
        "text-x-generic-symbolic",
        "Source",
        "Edit Carve markup",
    );
    source_mode.set_group(Some(&rich_mode));
    let rendered_mode = editor_mode_button(
        "editor-mode-rendered",
        "view-reveal-symbolic",
        "Preview",
        "Read-only preview",
    );
    rendered_mode.set_group(Some(&rich_mode));
    mode_group.append(&rich_mode);
    mode_group.append(&source_mode);
    mode_group.append(&rendered_mode);
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name("delete-note-button");
    trash.set_tooltip_text(Some("Move Note to Trash"));
    trash.add_css_class("flat");
    header.pack_end(&trash);
    view.add_top_bar(&header);

    let split_toggle = gtk::ToggleButton::new();
    split_toggle.set_icon_name("view-dual-symbolic");
    split_toggle.set_widget_name("source-split-toggle");
    split_toggle.set_tooltip_text(Some("Show rendered preview"));
    split_toggle.set_sensitive(false);
    let mode_controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    mode_controls.set_widget_name("editor-mode-switcher");
    mode_controls.add_css_class("editor-mode-switcher");
    mode_controls.set_halign(gtk::Align::Center);
    mode_controls.append(&mode_group);
    mode_controls.append(&split_toggle);
    header.set_title_widget(Some(&mode_controls));

    let format_stack = gtk::Stack::new();
    format_stack.set_widget_name("formatting-toolbar");
    let rich_format_bar = formatting_bar();
    let source_format_bar = formatting_bar();

    let editor_stack = gtk::Stack::new();
    let rich_buffer = gtk::TextBuffer::new(None);
    let rich = text_view(&rich_buffer, "rich-editor", false);
    let source_buffer = gtk::TextBuffer::new(None);
    let source = text_view(&source_buffer, "source-editor", true);
    let split_preview = build_preview(state.assets_dir.as_deref());
    let rendered_preview = build_preview(state.assets_dir.as_deref());
    formatting::append_controls(&rich_format_bar, &rich_buffer);
    formatting::append_source_controls(&source_format_bar, &source_buffer);
    format_stack.add_named(&rich_format_bar, Some("rich"));
    format_stack.add_named(&source_format_bar, Some("source"));
    formatting::apply_theme_colors(&rich_buffer);
    connect_theme_colors(&rich_buffer);
    install_list_continuation(&rich, &rich_buffer);
    install_editor_shortcuts(&rich, &rich_buffer);
    install_source_shortcuts(&source, &source_buffer);
    let pages = add_editor_pages(
        &editor_stack,
        &rich,
        &source,
        &split_preview,
        &rendered_preview,
        &split_toggle,
        &source_mode,
    );
    view.add_bottom_bar(&format_stack);
    view.set_content(Some(&editor_stack));

    connect_mode_buttons(
        state,
        &rich_mode,
        &source_mode,
        &rendered_mode,
        &editor_stack,
        &format_stack,
        &rich,
        &rich_buffer,
        &source_buffer,
        &split_preview,
        &rendered_preview,
        &split_toggle,
        &pages.split_supported,
    );
    connect_split_toggle(state, &split_toggle, &editor_stack, &source, &split_preview);
    connect_split_availability(
        &rich_mode,
        &source_mode,
        &rendered_mode,
        &split_toggle,
        &pages.split_supported,
    );
    connect_source_scroll_sync(&pages.source_scroll, &split_preview);
    connect_preview_theme_refresh(state, &source_buffer, &split_preview, &rendered_preview);
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
    connect_source_preview(state, &source_buffer, &split_preview, &rendered_preview);
    let _rich_image_paste = install_image_paste(&rich, &rich_buffer, state, toast_overlay);
    let _source_image_paste = install_image_paste(&source, &source_buffer, state, toast_overlay);
    connect_note_loading(
        state,
        stack,
        &rich_mode,
        &source_mode,
        &rendered_mode,
        &format_stack,
        &editor_stack,
        &rich,
        &rich_buffer,
        &source_buffer,
        &split_preview,
        &rendered_preview,
    );
    view.upcast()
}

fn editor_mode_button(
    name: &str,
    icon_name: &str,
    label: &str,
    tooltip: &str,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::new();
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    content.append(&gtk::Image::from_icon_name(icon_name));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button
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

fn formatting_bar() -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    bar.add_css_class("toolbar");
    bar.set_margin_start(12);
    bar.set_margin_end(12);
    bar.set_margin_top(6);
    bar.set_margin_bottom(6);
    bar
}

struct EditorPages {
    source_scroll: gtk::ScrolledWindow,
    split_supported: Rc<Cell<bool>>,
}

fn add_editor_pages(
    editor_stack: &gtk::Stack,
    rich: &gtk::TextView,
    source: &gtk::TextView,
    split_preview: &webkit6::WebView,
    rendered_preview: &webkit6::WebView,
    split_toggle: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
) -> EditorPages {
    let rich_scroll = gtk::ScrolledWindow::new();
    rich_scroll.set_child(Some(rich));
    let source_scroll = gtk::ScrolledWindow::new();
    source_scroll.set_child(Some(source));
    let split_scroll = gtk::ScrolledWindow::new();
    split_scroll.set_child(Some(split_preview));
    let source_split = gtk::Paned::new(gtk::Orientation::Horizontal);
    source_split.set_widget_name("source-split-view");
    source_split.set_start_child(Some(&source_scroll));
    source_split.set_end_child(Some(&split_scroll));
    source_split.set_resize_start_child(true);
    source_split.set_resize_end_child(true);
    source_split.set_shrink_start_child(false);
    source_split.set_shrink_end_child(false);
    split_scroll.set_visible(false);
    let source_container = adw::BreakpointBin::new();
    // `BreakpointBin` requires an explicit minimum allocation to evaluate its
    // conditions without emitting a warning while the editor becomes visible.
    source_container.set_size_request(360, 240);
    source_container.set_child(Some(&source_split));
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    let split_supported = Rc::new(Cell::new(true));
    let split_supported_for_apply = Rc::clone(&split_supported);
    let split_scroll_for_apply = split_scroll.clone();
    let split_toggle_for_apply = split_toggle.clone();
    breakpoint.connect_apply(move |_| {
        split_supported_for_apply.set(false);
        split_scroll_for_apply.set_visible(false);
        split_toggle_for_apply.set_sensitive(false);
    });
    let split_supported_for_unapply = Rc::clone(&split_supported);
    let split_scroll_for_unapply = split_scroll.clone();
    let split_toggle_for_unapply = split_toggle.clone();
    let source_mode_for_unapply = source_mode.clone();
    breakpoint.connect_unapply(move |_| {
        split_supported_for_unapply.set(true);
        let source_active = source_mode_for_unapply.is_active();
        split_toggle_for_unapply.set_sensitive(source_active);
        split_scroll_for_unapply.set_visible(source_active && split_toggle_for_unapply.is_active());
    });
    source_container.add_breakpoint(breakpoint);
    let rendered_scroll = gtk::ScrolledWindow::new();
    rendered_scroll.set_child(Some(rendered_preview));
    editor_stack.add_named(&rich_scroll, Some("rich"));
    editor_stack.add_named(&source_container, Some("source"));
    editor_stack.add_named(&rendered_scroll, Some("rendered"));
    editor_stack.set_visible_child_name("rich");
    EditorPages {
        source_scroll,
        split_supported,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one place wires the three mode controls to their shared editor surfaces"
)]
fn connect_mode_buttons(
    state: &Rc<AppState>,
    rich_mode: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
    rendered_mode: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    format_stack: &gtk::Stack,
    rich_view: &gtk::TextView,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
    split_preview: &webkit6::WebView,
    rendered_preview: &webkit6::WebView,
    split_toggle: &gtk::ToggleButton,
    split_supported: &Rc<Cell<bool>>,
) {
    let connect = |button: &gtk::ToggleButton, surface: EditorMode| {
        let state = Rc::clone(state);
        let stack = editor_stack.clone();
        let formats = format_stack.clone();
        let rich_view = rich_view.clone();
        let rich = rich_buffer.clone();
        let source = source_buffer.clone();
        let split_preview = split_preview.clone();
        let rendered_preview = rendered_preview.clone();
        let rendered_mode = rendered_mode.clone();
        let split_toggle = split_toggle.clone();
        let split_supported = Rc::clone(split_supported);
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            let persist_selection = !state.synchronizing_editor.get();
            state.synchronizing_editor.set(true);
            match surface {
                EditorMode::Source => {
                    if !state.source_mode.get() && !state.rendered_mode.get() {
                        source.set_text(&buffer_text(&rich));
                    }
                    state.source_mode.set(true);
                    state.rendered_mode.set(false);
                    stack.set_visible_child_name("source");
                    formats.set_sensitive(true);
                    formats.set_visible_child_name("source");
                    split_toggle.set_active(state.config.borrow().editor.source_split_view);
                    split_toggle.set_sensitive(split_supported.get());
                }
                EditorMode::Rendered => {
                    if !state.source_mode.get() {
                        source.set_text(&buffer_text(&rich));
                    }
                    let source_text = source.text(&source.start_iter(), &source.end_iter(), false);
                    let remote = state.config.borrow().images.load_remote_automatically;
                    load_preview(&rendered_preview, &source_text, remote);
                    state.source_mode.set(false);
                    state.rendered_mode.set(true);
                    stack.set_visible_child_name("rendered");
                    formats.set_sensitive(false);
                    split_toggle.set_active(false);
                    split_toggle.set_sensitive(false);
                }
                EditorMode::Rich => {
                    let source_text = source.text(&source.start_iter(), &source.end_iter(), false);
                    if matches!(editor_projection(&source_text), EditorProjection::Native(_)) {
                        render_rich_markup(&rich_view, &rich, &source_text, Some(&state));
                        state.source_mode.set(false);
                        state.rendered_mode.set(false);
                        stack.set_visible_child_name("rich");
                        formats.set_sensitive(true);
                        formats.set_visible_child_name("rich");
                        split_toggle.set_active(false);
                        split_toggle.set_sensitive(false);
                    } else {
                        let remote = state.config.borrow().images.load_remote_automatically;
                        load_preview(&rendered_preview, &source_text, remote);
                        state.source_mode.set(false);
                        state.rendered_mode.set(true);
                        stack.set_visible_child_name("rendered");
                        formats.set_sensitive(false);
                        split_toggle.set_active(false);
                        split_toggle.set_sensitive(false);
                        // Keep the control honest about the effective surface,
                        // without overwriting the user's saved Edit preference.
                        rendered_mode.set_active(true);
                    }
                }
            }
            if surface == EditorMode::Source {
                let source_text = source.text(&source.start_iter(), &source.end_iter(), false);
                let remote = state.config.borrow().images.load_remote_automatically;
                load_preview(&split_preview, &source_text, remote);
            }
            state.synchronizing_editor.set(false);
            if persist_selection {
                let _ = state.set_last_editor_mode(surface);
            }
        });
    };
    connect(rich_mode, EditorMode::Rich);
    connect(source_mode, EditorMode::Source);
    connect(rendered_mode, EditorMode::Rendered);
}

fn connect_split_availability(
    rich_mode: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
    rendered_mode: &gtk::ToggleButton,
    split_toggle: &gtk::ToggleButton,
    split_supported: &Rc<Cell<bool>>,
) {
    let split = split_toggle.clone();
    let split_supported = Rc::clone(split_supported);
    source_mode.connect_toggled(move |button| {
        split.set_sensitive(button.is_active() && split_supported.get());
    });
    for mode in [rich_mode, rendered_mode] {
        let split = split_toggle.clone();
        mode.connect_toggled(move |button| {
            if button.is_active() {
                split.set_active(false);
                split.set_sensitive(false);
            }
        });
    }
}

fn connect_split_toggle(
    state: &Rc<AppState>,
    toggle: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    source: &gtk::TextView,
    preview: &webkit6::WebView,
) {
    let state = Rc::clone(state);
    let stack = editor_stack.clone();
    let source = source.clone();
    let preview = preview.clone();
    toggle.connect_toggled(move |toggle| {
        let Some(split) = stack.child_by_name("source") else {
            return;
        };
        let Some(container) = split.downcast_ref::<adw::BreakpointBin>() else {
            return;
        };
        let Some(paned) = container.child().and_downcast::<gtk::Paned>() else {
            return;
        };
        let Some(preview_scroll) = paned.end_child() else {
            return;
        };
        preview_scroll.set_visible(toggle.is_active());
        if !state.synchronizing_editor.get() {
            let _ = state.set_source_split_view(toggle.is_active());
        }
        if toggle.is_active() {
            preview.grab_focus();
            source.grab_focus();
        }
    });
}

/// Keeps the rendered split preview aligned to the source editor's scroll position.
fn connect_source_scroll_sync(source_scroll: &gtk::ScrolledWindow, preview: &webkit6::WebView) {
    let preview = preview.clone();
    source_scroll
        .vadjustment()
        .connect_value_changed(move |adjustment| {
            let denominator = adjustment.upper() - adjustment.page_size();
            let fraction = if denominator > 0.0 {
                (adjustment.value() / denominator).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let script = preview_scroll_script(fraction);
            preview.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |_| {},
            );
        });
}

fn preview_scroll_script(fraction: f64) -> String {
    format!(
        "window.scrollTo(0, (document.documentElement.scrollHeight - window.innerHeight) * {fraction});"
    )
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
        let source = if state_for_back.source_mode.get() || state_for_back.rendered_mode.get() {
            buffer_text(&source_for_back)
        } else {
            buffer_text(&rich_for_back)
        };
        let Some(note) = state_for_back.current_note.borrow().clone() else {
            return;
        };
        if source.as_str() == note.source {
            refresh_browser(&state_for_back);
            stack_for_back.set_visible_child_name("browser");
            return;
        }
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

#[expect(
    clippy::too_many_arguments,
    reason = "note loading must update the coordinated editor widgets atomically"
)]
fn connect_note_loading(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    rich_mode: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
    rendered_mode: &gtk::ToggleButton,
    format_stack: &gtk::Stack,
    editor_stack: &gtk::Stack,
    rich_view: &gtk::TextView,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
    split_preview: &webkit6::WebView,
    rendered_preview: &webkit6::WebView,
) {
    let state_for_visible = Rc::clone(state);
    let rich_for_visible = rich_buffer.clone();
    let rich_view_for_visible = rich_view.clone();
    let source_for_visible = source_buffer.clone();
    let rich_mode_for_visible = rich_mode.clone();
    let source_mode_for_visible = source_mode.clone();
    let rendered_mode_for_visible = rendered_mode.clone();
    let format_stack_for_visible = format_stack.clone();
    let editor_stack_for_visible = editor_stack.clone();
    let split_preview_for_visible = split_preview.clone();
    let rendered_preview_for_visible = rendered_preview.clone();
    stack.connect_visible_child_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("editor")
            && let Some(note) = state_for_visible.current_note.borrow().as_ref()
        {
            state_for_visible.synchronizing_editor.set(true);
            source_for_visible.set_text(&note.source);
            let remote = state_for_visible
                .config
                .borrow()
                .images
                .load_remote_automatically;
            load_preview(&split_preview_for_visible, &note.source, remote);
            load_preview(&rendered_preview_for_visible, &note.source, remote);
            if matches!(editor_projection(&note.source), EditorProjection::Native(_)) {
                render_rich_markup(
                    &rich_view_for_visible,
                    &rich_for_visible,
                    &note.source,
                    Some(&state_for_visible),
                );
                if state_for_visible.source_mode.get() {
                    source_mode_for_visible.set_active(true);
                    editor_stack_for_visible.set_visible_child_name("source");
                    format_stack_for_visible.set_sensitive(true);
                    format_stack_for_visible.set_visible_child_name("source");
                } else if state_for_visible.rendered_mode.get() {
                    rendered_mode_for_visible.set_active(true);
                    editor_stack_for_visible.set_visible_child_name("rendered");
                    format_stack_for_visible.set_sensitive(false);
                } else {
                    rich_mode_for_visible.set_active(true);
                    editor_stack_for_visible.set_visible_child_name("rich");
                    format_stack_for_visible.set_sensitive(true);
                    format_stack_for_visible.set_visible_child_name("rich");
                }
            } else {
                rendered_mode_for_visible.set_active(true);
                editor_stack_for_visible.set_visible_child_name("rendered");
                format_stack_for_visible.set_sensitive(false);
                state_for_visible.source_mode.set(false);
                state_for_visible.rendered_mode.set(true);
            }
            state_for_visible.synchronizing_editor.set(false);
        }
    });
}

fn connect_source_preview(
    state: &Rc<AppState>,
    source_buffer: &gtk::TextBuffer,
    split_preview: &webkit6::WebView,
    rendered_preview: &webkit6::WebView,
) {
    let state = Rc::clone(state);
    let source = source_buffer.clone();
    let split_preview = split_preview.clone();
    let rendered_preview = rendered_preview.clone();
    source_buffer.connect_changed(move |_| {
        if state.synchronizing_editor.get() {
            return;
        }
        let source_text = source
            .text(&source.start_iter(), &source.end_iter(), false)
            .to_string();
        let remote = state.config.borrow().images.load_remote_automatically;
        let generation = state.preview_generation.get().saturating_add(1);
        state.preview_generation.set(generation);
        let state_for_timeout = Rc::clone(&state);
        let split_preview = split_preview.clone();
        let rendered_preview = rendered_preview.clone();
        glib::timeout_add_local_once(StdDuration::from_millis(120), move || {
            if state_for_timeout.preview_generation.get() != generation {
                return;
            }
            load_preview(&split_preview, &source_text, remote);
            load_preview(&rendered_preview, &source_text, remote);
        });
    });
}

/// Reloads `WebKit` previews when GNOME switches between light and dark palettes.
fn connect_preview_theme_refresh(
    state: &Rc<AppState>,
    source_buffer: &gtk::TextBuffer,
    split_preview: &webkit6::WebView,
    rendered_preview: &webkit6::WebView,
) {
    let state = Rc::clone(state);
    let source = source_buffer.clone();
    let split_preview = split_preview.clone();
    let rendered_preview = rendered_preview.clone();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        let source_text = buffer_text(&source);
        let remote = state.config.borrow().images.load_remote_automatically;
        load_preview(&split_preview, &source_text, remote);
        load_preview(&rendered_preview, &source_text, remote);
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
            gtk::gdk::Key::u if !shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-underline");
                true
            }
            gtk::gdk::Key::h if shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-highlight");
                true
            }
            gtk::gdk::Key::period if shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-superscript");
                true
            }
            gtk::gdk::Key::comma if shift => {
                formatting::toggle_tag_on_selection(&buffer, "rich-subscript");
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

/// Installs source-mode equivalents of the common Rich Text keyboard shortcuts.
pub(crate) fn install_source_shortcuts(
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
                source_commands::toggle_inline(&buffer, "*", "*");
                true
            }
            gtk::gdk::Key::i if !shift => {
                source_commands::toggle_inline(&buffer, "/", "/");
                true
            }
            gtk::gdk::Key::x if shift => {
                source_commands::toggle_inline(&buffer, "~", "~");
                true
            }
            gtk::gdk::Key::u if !shift => {
                source_commands::toggle_inline(&buffer, "_", "_");
                true
            }
            gtk::gdk::Key::h if shift => {
                source_commands::toggle_inline(&buffer, "=", "=");
                true
            }
            gtk::gdk::Key::period if shift => {
                source_commands::toggle_inline(&buffer, "{^", "^}");
                true
            }
            gtk::gdk::Key::comma if shift => {
                source_commands::toggle_inline(&buffer, "{,", ",}");
                true
            }
            gtk::gdk::Key::_8 if shift => {
                source_commands::toggle_list(&buffer, "- ");
                true
            }
            gtk::gdk::Key::_7 if shift => {
                source_commands::toggle_list(&buffer, "1. ");
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
        let source = if state.source_mode.get() || state.rendered_mode.get() {
            buffer_text(&source_buffer)
        } else {
            buffer_text(&rich_buffer)
        };
        if source.as_str() == note.source {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_scroll_script_uses_a_bounded_relative_position() {
        assert_eq!(
            preview_scroll_script(0.5),
            "window.scrollTo(0, (document.documentElement.scrollHeight - window.innerHeight) * 0.5);"
        );
    }
}
