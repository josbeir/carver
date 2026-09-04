//! Rich/source note editor, formatting, image paste, and autosave.

use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::Duration as StdDuration,
};

use carver_config::{Config, EditorMode};
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::BreakpointBinExt;
use webkit6::prelude::*;

use crate::{
    mvu::{AppDispatcher, AppModel, AppMsg, EditorMsg, EditorSessionId, PreferencesMsg},
    sidebar::sidebar_toggle_button,
};

const MOUSE_BACK_BUTTON: u32 = 8;

mod preview;
mod render;
mod source;
pub(crate) mod source_commands;
mod source_context;
mod toolbar;
mod web;

use preview::{build_preview, load_preview};
use source::SourceEditor;
pub(crate) use source::{SourceSyntaxError, buffer_text, install_syntax_assets};
use source_context::SourceContextCache;
use toolbar::Toolbar;
use web::RichEditor;

/// GTK/WebKit references that project the active editor document from the MVU model.
pub(crate) struct EditorViewRefs {
    rich_mode: gtk::ToggleButton,
    source_mode: gtk::ToggleButton,
    rendered_mode: gtk::ToggleButton,
    toolbar: Toolbar,
    editor_stack: gtk::Stack,
    split_toggle: gtk::ToggleButton,
    rich: RichEditor,
    source_buffer: gtk::TextBuffer,
    source_editor: SourceEditor,
    split_preview: webkit6::WebView,
    rendered_preview: webkit6::WebView,
    rendering: Rc<Cell<bool>>,
    remote_images: Rc<Cell<bool>>,
    preview_source: Rc<RefCell<Option<(EditorSessionId, String)>>>,
    rendered_theme_revision: RefCell<Option<u64>>,
    loaded_session: RefCell<Option<EditorSessionId>>,
}

impl EditorViewRefs {
    /// Applies an immutable editor-document snapshot to its GTK/WebKit projections.
    pub(crate) fn render(&self, model: &AppModel) {
        let Some(document) = model.editor.as_ref() else {
            self.preview_source.replace(None);
            return;
        };
        let new_document = self.loaded_session.borrow().as_ref() != Some(&document.session);
        let remote_images_changed = self
            .remote_images
            .replace(model.preferences.load_remote_images)
            != model.preferences.load_remote_images;
        let theme_changed = self
            .rendered_theme_revision
            .replace(Some(model.editor_theme_revision))
            != Some(model.editor_theme_revision);
        let source_changed = buffer_text(&self.source_buffer) != document.source;
        let preview = model
            .editor_preview
            .as_ref()
            .filter(|preview| preview.session == document.session)
            .cloned();
        let preview_changed = preview.as_ref().is_some_and(|preview| {
            self.preview_source
                .borrow()
                .as_ref()
                .is_none_or(|(session, source)| {
                    *session != preview.session || source != &preview.source
                })
        });
        self.rendering.set(true);
        if source_changed {
            self.source_buffer.set_text(&document.source);
        }
        if (preview_changed || remote_images_changed || theme_changed)
            && let Some(preview) = preview.as_ref()
        {
            load_preview(
                &self.split_preview,
                &preview.source,
                model.preferences.load_remote_images,
            );
            load_preview(
                &self.rendered_preview,
                &preview.source,
                model.preferences.load_remote_images,
            );
            self.preview_source
                .replace(Some((preview.session, preview.source.clone())));
        }
        if new_document || remote_images_changed || source_changed {
            if remote_images_changed {
                self.rich.reload_with_remote_images(
                    &document.source,
                    model.preferences.load_remote_images,
                );
            } else {
                self.rich.load_source(&document.source);
            }
        }
        if theme_changed {
            refresh_rich_theme(&self.rich);
        }
        self.source_editor.render_preferences(
            &model.preferences.source_editor,
            adw::StyleManager::default().is_dark(),
        );
        match document.mode {
            EditorMode::Source => {
                self.source_mode.set_active(true);
                self.editor_stack.set_visible_child_name("source");
                self.toolbar.set_mode(EditorMode::Source);
                self.split_toggle
                    .set_active(model.preferences.source_split_view);
            }
            EditorMode::Rendered => {
                self.rendered_mode.set_active(true);
                self.editor_stack.set_visible_child_name("rendered");
                self.toolbar.set_mode(EditorMode::Rendered);
                self.split_toggle.set_active(false);
            }
            EditorMode::Rich => {
                self.rich_mode.set_active(true);
                self.editor_stack.set_visible_child_name("rich");
                self.toolbar.set_mode(EditorMode::Rich);
                self.split_toggle.set_active(false);
            }
        }
        self.rendering.set(false);
        if new_document {
            self.loaded_session.replace(Some(document.session));
        }
    }
}

/// The complete editor surface and its immutable-model renderer.
pub(crate) struct EditorSurface {
    widget: gtk::Widget,
    refs: EditorViewRefs,
}

impl EditorSurface {
    pub(crate) fn into_parts(self) -> (gtk::Widget, EditorViewRefs) {
        (self.widget, self.refs)
    }
}

/// Builds the note editor and connects its user-facing actions.
#[expect(
    clippy::too_many_lines,
    reason = "widget construction stays together so ownership and lifecycle are explicit"
)]
pub(crate) fn build_editor(
    dispatcher: &AppDispatcher,
    config: &Config,
    assets_dir: Option<&Path>,
    source_syntax_dir: &Path,
    toast_overlay: &adw::ToastOverlay,
    split_view: &adw::NavigationSplitView,
) -> Result<EditorSurface, SourceSyntaxError> {
    let allow_remote_images = config.images.load_remote_automatically;
    let view = adw::ToolbarView::new();
    view.set_widget_name("editor-surface");
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

    let editor_stack = gtk::Stack::new();
    let rendering = Rc::new(Cell::new(false));
    let source_editor = SourceEditor::new(source_syntax_dir)?;
    let source_buffer = source_editor.buffer().clone();
    let source = source_editor.view().clone();
    let rich = RichEditor::new(
        assets_dir.map(Path::to_path_buf),
        allow_remote_images,
        dispatcher,
        &source_buffer,
        toast_overlay,
    );
    let remote_images = Rc::new(Cell::new(allow_remote_images));
    let preview_source = Rc::new(RefCell::new(None));
    refresh_rich_theme(&rich);
    let split_preview = build_preview(assets_dir);
    split_preview.set_widget_name("source-split-preview");
    let rendered_preview = build_preview(assets_dir);
    let toolbar = Toolbar::new(&source_buffer, &rich, dispatcher, toast_overlay);
    let source_context = SourceContextCache::new(&source_buffer);
    connect_source_context(&source_buffer, &source_context, &toolbar);
    let toolbar_for_selection = toolbar.clone();
    rich.connect_selection_changed(move |selection| {
        toolbar_for_selection.set_rich_selection(&selection);
    });
    install_source_shortcuts(source.upcast_ref(), &toolbar);
    let pages = add_editor_pages(
        &editor_stack,
        rich.view(),
        source.upcast_ref(),
        &split_preview,
        &rendered_preview,
        &split_toggle,
        &source_mode,
    );
    view.add_bottom_bar(toolbar.widget());
    view.set_content(Some(&editor_stack));

    connect_mode_buttons(
        dispatcher,
        &rich_mode,
        &source_mode,
        &rendered_mode,
        &editor_stack,
        &toolbar,
        &rich,
        &source_buffer,
        &split_toggle,
        &pages.split_supported,
        &rendering,
    );
    connect_rich_fallback(dispatcher, &rich);
    connect_split_toggle(
        dispatcher,
        &split_toggle,
        &editor_stack,
        source.upcast_ref(),
        &split_preview,
        &rendering,
    );
    connect_split_availability(
        &rich_mode,
        &source_mode,
        &rendered_mode,
        &split_toggle,
        &pages.split_supported,
    );
    connect_source_scroll_sync(&pages.source_scroll, &split_preview, &split_toggle);
    connect_theme_changes(dispatcher);
    connect_trash_action(dispatcher, &trash);
    connect_back_action(dispatcher, &back);
    connect_mouse_back_action(dispatcher, &view);
    connect_source_preview(dispatcher, &source_buffer, &rendering);
    let _source_image_paste = render::install_image_paste(source.upcast_ref(), dispatcher);
    let _source_image_drop = render::install_image_drop(&source, dispatcher, toast_overlay);
    let _rich_image_drop = render::install_image_drop(rich.view(), dispatcher, toast_overlay);
    let refs = EditorViewRefs {
        rich_mode,
        source_mode,
        rendered_mode,
        toolbar,
        editor_stack,
        split_toggle,
        rich,
        source_buffer,
        source_editor,
        split_preview,
        rendered_preview,
        rendering,
        remote_images,
        preview_source,
        rendered_theme_revision: RefCell::new(Some(0)),
        loaded_session: RefCell::new(None),
    };
    Ok(EditorSurface {
        widget: view.upcast(),
        refs,
    })
}

/// Falls back to the lossless read-only renderer when the web adapter reports
/// a construct it cannot faithfully edit.
fn connect_rich_fallback(dispatcher: &AppDispatcher, rich: &RichEditor) {
    let dispatcher = dispatcher.clone();
    rich.connect_unsupported(move || {
        let _ = dispatcher.dispatch(AppMsg::Preferences(PreferencesMsg::SetEditorMode(
            EditorMode::Rendered,
        )));
    });
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

fn connect_source_context(
    source: &gtk::TextBuffer,
    context_cache: &SourceContextCache,
    toolbar: &Toolbar,
) {
    toolbar.set_source_context(context_cache.context());
    let context_for_change = context_cache.clone();
    let toolbar_for_change = toolbar.clone();
    source.connect_changed(move |_| {
        context_for_change.refresh();
        toolbar_for_change.set_source_context(context_for_change.context());
    });
    let context_for_mark = context_cache.clone();
    let toolbar_for_mark = toolbar.clone();
    source.connect_mark_set(move |_, _, _| {
        toolbar_for_mark.set_source_context(context_for_mark.context());
    });
}

struct EditorPages {
    source_scroll: gtk::ScrolledWindow,
    split_supported: Rc<Cell<bool>>,
}

fn add_editor_pages(
    editor_stack: &gtk::Stack,
    rich: &webkit6::WebView,
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
    dispatcher: &AppDispatcher,
    rich_mode: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
    rendered_mode: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    toolbar: &Toolbar,
    rich: &RichEditor,
    source_buffer: &gtk::TextBuffer,
    split_toggle: &gtk::ToggleButton,
    split_supported: &Rc<Cell<bool>>,
    rendering: &Rc<Cell<bool>>,
) {
    let connect = |button: &gtk::ToggleButton, surface: EditorMode| {
        let dispatcher = dispatcher.clone();
        let stack = editor_stack.clone();
        let toolbar = toolbar.clone();
        let rich = rich.clone();
        let source = source_buffer.clone();
        let split_toggle = split_toggle.clone();
        let split_supported = Rc::clone(split_supported);
        let rendering = Rc::clone(rendering);
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            let persist_selection = !rendering.get();
            let was_rendering = rendering.replace(true);
            match surface {
                EditorMode::Source => {
                    stack.set_visible_child_name("source");
                    toolbar.set_mode(EditorMode::Source);
                    split_toggle.set_sensitive(split_supported.get());
                }
                EditorMode::Rendered => {
                    stack.set_visible_child_name("rendered");
                    toolbar.set_mode(EditorMode::Rendered);
                    split_toggle.set_active(false);
                    split_toggle.set_sensitive(false);
                }
                EditorMode::Rich => {
                    let source_text = source.text(&source.start_iter(), &source.end_iter(), false);
                    rich.load_source(&source_text);
                    stack.set_visible_child_name("rich");
                    toolbar.set_mode(EditorMode::Rich);
                    split_toggle.set_active(false);
                    split_toggle.set_sensitive(false);
                }
            }
            rendering.set(was_rendering);
            if persist_selection {
                let _ = dispatcher
                    .dispatch(AppMsg::Preferences(PreferencesMsg::SetEditorMode(surface)));
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
    dispatcher: &AppDispatcher,
    toggle: &gtk::ToggleButton,
    editor_stack: &gtk::Stack,
    source: &gtk::TextView,
    preview: &webkit6::WebView,
    rendering: &Rc<Cell<bool>>,
) {
    let dispatcher = dispatcher.clone();
    let stack = editor_stack.clone();
    let source = source.clone();
    let preview = preview.clone();
    let rendering = Rc::clone(rendering);
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
        if !rendering.get() {
            let _ = dispatcher.dispatch(AppMsg::Preferences(PreferencesMsg::SetSourceSplitView(
                toggle.is_active(),
            )));
        }
        if toggle.is_active() {
            preview.grab_focus();
            source.grab_focus();
        }
    });
}

/// Keeps the rendered split preview aligned to the source editor's scroll position.
///
/// GTK reports every pixel-level adjustment while the user scrolls. Coalesce
/// those signals and allow only one `WebKit` evaluation at a time: otherwise
/// slow preview script processing creates a growing queue and makes scrolling
/// feel delayed. The bridge is dormant unless the split preview is visible.
fn connect_source_scroll_sync(
    source_scroll: &gtk::ScrolledWindow,
    preview: &webkit6::WebView,
    split_toggle: &gtk::ToggleButton,
) {
    let sync = PreviewScrollSync::new(preview, split_toggle);
    let sync_for_adjustment = sync.clone();
    source_scroll
        .vadjustment()
        .connect_value_changed(move |source_adjustment| {
            sync_for_adjustment.request(scroll_fraction(source_adjustment));
        });

    // A source change reloads the preview document. Apply the latest source
    // position after that replacement document exists, rather than sending a
    // scroll command to the outgoing one.
    let sync_for_preview_load = sync.clone();
    preview.connect_load_changed(move |_preview, event| {
        if event == webkit6::LoadEvent::Finished {
            sync_for_preview_load.request_current();
        }
    });

    let adjustment = source_scroll.vadjustment();
    split_toggle.connect_toggled(move |toggle| {
        if toggle.is_active() {
            sync.request(scroll_fraction(&adjustment));
        }
    });
}

#[derive(Clone)]
struct PreviewScrollSync {
    preview: webkit6::WebView,
    split_toggle: gtk::ToggleButton,
    fraction: Rc<Cell<f64>>,
    dirty: Rc<Cell<bool>>,
    in_flight: Rc<Cell<bool>>,
    dispatch_scheduled: Rc<Cell<bool>>,
}

impl PreviewScrollSync {
    fn new(preview: &webkit6::WebView, split_toggle: &gtk::ToggleButton) -> Self {
        Self {
            preview: preview.clone(),
            split_toggle: split_toggle.clone(),
            fraction: Rc::new(Cell::new(0.0)),
            dirty: Rc::new(Cell::new(false)),
            in_flight: Rc::new(Cell::new(false)),
            dispatch_scheduled: Rc::new(Cell::new(false)),
        }
    }

    fn request_current(&self) {
        self.request(self.fraction.get());
    }

    fn request(&self, fraction: f64) {
        self.fraction.set(fraction);
        self.dirty.set(true);
        self.schedule_dispatch();
    }

    fn schedule_dispatch(&self) {
        if !self.split_toggle.is_active()
            || self.in_flight.get()
            || self.dispatch_scheduled.replace(true)
        {
            return;
        }
        let sync = self.clone();
        glib::timeout_add_local_once(StdDuration::from_millis(16), move || {
            sync.dispatch_scheduled.set(false);
            sync.flush();
        });
    }

    fn flush(&self) {
        if !self.split_toggle.is_active() || self.in_flight.get() || !self.dirty.replace(false) {
            return;
        }
        self.in_flight.set(true);
        let script = preview_scroll_script(self.fraction.get());
        let sync = self.clone();
        self.preview.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |_| {
                sync.in_flight.set(false);
                if sync.dirty.get() {
                    sync.schedule_dispatch();
                }
            },
        );
    }
}

fn adjustment_fraction(value: f64, upper: f64, page_size: f64) -> f64 {
    let denominator = upper - page_size;
    if denominator > 0.0 {
        (value / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn scroll_fraction(adjustment: &gtk::Adjustment) -> f64 {
    adjustment_fraction(
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size(),
    )
}

fn preview_scroll_script(fraction: f64) -> String {
    format!(
        "window.scrollTo(0, (document.documentElement.scrollHeight - window.innerHeight) * {fraction});"
    )
}

fn connect_trash_action(dispatcher: &AppDispatcher, trash: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    trash.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::TrashRequested));
    });
}

fn connect_back_action(dispatcher: &AppDispatcher, back: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    back.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
    });
}

/// Routes the conventional pointer Back button through the editor's close transition.
fn connect_mouse_back_action(dispatcher: &AppDispatcher, editor: &impl IsA<gtk::Widget>) {
    let back = gtk::GestureClick::new();
    back.set_name(Some("editor-mouse-back-controller"));
    back.set_button(MOUSE_BACK_BUTTON);
    // The rich text surface is an embedded child that may otherwise consume pointer events.
    back.set_propagation_phase(gtk::PropagationPhase::Capture);
    let dispatcher = dispatcher.clone();
    back.connect_released(move |_, _, _, _| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
    });
    editor.add_controller(back);
}

fn connect_source_preview(
    dispatcher: &AppDispatcher,
    source_buffer: &gtk::TextBuffer,
    rendering: &Rc<Cell<bool>>,
) {
    let dispatcher = dispatcher.clone();
    let source = source_buffer.clone();
    let rendering = Rc::clone(rendering);
    source_buffer.connect_changed(move |_| {
        if rendering.get() {
            return;
        }
        let source_text = source
            .text(&source.start_iter(), &source.end_iter(), false)
            .to_string();
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::SourceChanged(source_text)));
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::AutosaveRequested));
    });
}

/// Routes desktop theme notifications through the immutable MVU render pass.
fn connect_theme_changes(dispatcher: &AppDispatcher) {
    let dispatcher_for_dark_theme = dispatcher.clone();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        let _ = dispatcher_for_dark_theme.dispatch(AppMsg::Editor(EditorMsg::ThemeChanged));
    });
    let dispatcher_for_accent = dispatcher.clone();
    adw::StyleManager::default().connect_accent_color_notify(move |_| {
        let _ = dispatcher_for_accent.dispatch(AppMsg::Editor(EditorMsg::ThemeChanged));
    });
}

fn refresh_rich_theme(rich: &RichEditor) {
    let style_manager = adw::StyleManager::default();
    let dark = style_manager.is_dark();
    rich.set_theme(dark, &style_manager.accent_color().to_standalone_rgba(dark));
}

/// Installs source-mode equivalents of the common Rich Text keyboard shortcuts.
pub(crate) fn install_source_shortcuts(
    view: &gtk::TextView,
    toolbar: &Toolbar,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let toolbar = toolbar.clone();
    let anchor = view.clone().upcast::<gtk::Widget>();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let command = match key {
            gtk::gdk::Key::b if !shift => Some(toolbar::ToolbarCommand::Bold),
            gtk::gdk::Key::i if !shift => Some(toolbar::ToolbarCommand::Italic),
            gtk::gdk::Key::x if shift => Some(toolbar::ToolbarCommand::Strike),
            gtk::gdk::Key::u if !shift => Some(toolbar::ToolbarCommand::Underline),
            gtk::gdk::Key::h if shift => Some(toolbar::ToolbarCommand::Highlight),
            gtk::gdk::Key::period if shift => Some(toolbar::ToolbarCommand::Superscript),
            gtk::gdk::Key::comma if shift => Some(toolbar::ToolbarCommand::Subscript),
            gtk::gdk::Key::_8 if shift => Some(toolbar::ToolbarCommand::BulletList),
            gtk::gdk::Key::_7 if shift => Some(toolbar::ToolbarCommand::OrderedList),
            _ => None,
        };
        if let Some(command) = command {
            toolbar.execute_source_shortcut(command, &anchor);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.add_controller(controller.clone());
    controller
}

#[cfg(test)]
mod tests;
