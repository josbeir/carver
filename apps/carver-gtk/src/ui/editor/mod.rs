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
use libadwaita::prelude::{
    ActionRowExt, AdwDialogExt, AlertDialogExt, AlertDialogExtManual, BreakpointBinExt,
    ComboRowExt, PreferencesGroupExt, PreferencesRowExt,
};
use webkit6::prelude::*;

use super::{
    dialogs::{EXPORT_NOTE_ACTION, PRINT_NOTE_ACTION, TOGGLE_FAVORITE_ACTION, TRASH_NOTE_ACTION},
    sidebar::{CompactNavigation, sidebar_toggle_button},
};
use crate::mvu::{
    AppDispatcher, AppModel, AppMsg, EditorCopyRequest, EditorExportDialogRequest,
    EditorExportFormat, EditorExportWarningRequest, EditorMsg, EditorPdfExportRequest,
    EditorSessionId, PreferencesMsg,
};

mod clipboard;
mod find;
pub(crate) mod focus;
mod media_preview;
#[cfg(test)]
pub(crate) use media_preview::tests::preview_service_should_receive_a_copy_and_support_portal_export;
mod document_navigation;
mod document_sidebar;
mod preview;
mod render;
mod source;
pub(crate) mod source_commands;
mod source_context;
mod toolbar;
mod web;

#[cfg(test)]
pub(crate) use clipboard::CARVER_CLIPBOARD_MIME;

const COMPACT_EDITOR_WIDTH: f64 = 700.0;
use clipboard::publish_note;
use find::FindController;
use preview::{build_preview, load_preview};
use source::SourceEditor;
pub(crate) use source::{
    SourceSyntaxError, buffer_text, install_syntax_assets, normalize_source_font_description,
    system_monospace_font_description,
};
use source_context::SourceContextCache;
use toolbar::Toolbar;
use web::RichEditor;

type PreviewSource = (EditorSessionId, String);
type PreviewSourceCache = Rc<RefCell<Option<PreviewSource>>>;

/// GTK/WebKit references that project the active editor document from the MVU model.
pub(crate) struct EditorViewRefs {
    favorite: gtk::ToggleButton,
    compact_options: gtk::gio::Menu,
    sidebar: document_sidebar::DocumentSidebar,
    rich_mode: gtk::ToggleButton,
    source_mode: gtk::ToggleButton,
    rendered_mode: gtk::ToggleButton,
    find: FindController,
    toolbar: Toolbar,
    toolbar_bar: gtk::Box,
    editor_stack: gtk::Stack,
    split_toggle: gtk::ToggleButton,
    rich: RichEditor,
    source_buffer: gtk::TextBuffer,
    source_editor: SourceEditor,
    split_preview: webkit6::WebView,
    split_navigation: document_navigation::PreviewNavigation,
    rendered_navigation: document_navigation::PreviewNavigation,
    rendered_preview: webkit6::WebView,
    rendering: Rc<Cell<bool>>,
    remote_images: Rc<Cell<bool>>,
    split_supported: Rc<Cell<bool>>,
    split_preview_source: PreviewSourceCache,
    latest_split_preview_source: PreviewSourceCache,
    rendered_preview_source: RefCell<Option<PreviewSource>>,
    rendered_theme_revision: RefCell<Option<u64>>,
    loaded_session: RefCell<Option<EditorSessionId>>,
    source_generation: Cell<u64>,
    dispatcher: AppDispatcher,
    assets_dir: Option<std::path::PathBuf>,
}

impl EditorViewRefs {
    /// Applies an immutable editor-document snapshot to its GTK/WebKit projections.
    // CONTEXT: Rendering all projections together prevents GTK widgets from becoming a second
    // document state store.
    #[expect(clippy::too_many_lines)]
    pub(crate) fn render(&self, model: &AppModel) {
        self.toolbar_bar
            .set_visible(model.preferences.show_formatting_toolbar);
        let Some(document) = model.editor.as_ref() else {
            self.split_preview_source.replace(None);
            self.latest_split_preview_source.replace(None);
            self.rendered_preview_source.replace(None);
            return;
        };
        self.rendering.set(true);
        self.source_generation.set(document.source_generation);
        self.favorite.set_active(document.is_favorite);
        self.compact_options.remove(2);
        self.compact_options.insert(
            2,
            Some(if document.is_favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            }),
            Some("editor.toggle-favorite"),
        );
        self.sidebar.render(document, &self.dispatcher);
        self.favorite
            .set_tooltip_text(Some(if document.is_favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            }));
        let new_document = self.loaded_session.borrow().as_ref() != Some(&document.session);
        let remote_images_changed = self
            .remote_images
            .replace(model.preferences.load_remote_images)
            != model.preferences.load_remote_images;
        let theme_changed = self
            .rendered_theme_revision
            .replace(Some(model.editor_theme_revision))
            != Some(model.editor_theme_revision);
        let presentation_changed = remote_images_changed || theme_changed;
        if presentation_changed {
            invalidate_preview_sources(&self.split_preview_source, &self.rendered_preview_source);
        }
        let source_changed = buffer_text(&self.source_buffer) != document.source;
        let preview = model
            .editor_preview
            .as_ref()
            .filter(|preview| preview.session == document.session)
            .cloned();
        if new_document {
            self.rich.set_document_session(document.session);
            self.find.reset();
        }
        if source_changed {
            if new_document {
                self.source_buffer.set_text(&document.source);
            } else {
                source_commands::replace_source_buffer(&self.source_buffer, &document.source);
            }
        }
        let theme = editor_theme();
        if let Some(preview) = preview.as_ref() {
            self.render_visible_preview(
                document.mode,
                model.preferences.source_split_view,
                preview,
                model.preferences.load_remote_images,
                &theme,
                presentation_changed,
            );
        }
        if new_document || remote_images_changed {
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
            self.rich.set_theme(&theme);
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
        self.find.set_mode(document.mode);
        self.rendering.set(false);
        if new_document {
            self.loaded_session.replace(Some(document.session));
        }
    }

    fn render_visible_preview(
        &self,
        mode: EditorMode,
        source_split_view: bool,
        preview: &crate::mvu::EditorPreview,
        allow_remote_images: bool,
        theme: &web::EditorTheme,
        presentation_changed: bool,
    ) {
        let (view, rendered) = match mode {
            EditorMode::Source if source_split_view => {
                remember_preview_source(&self.latest_split_preview_source, preview);
                if !split_preview_is_visible(mode, source_split_view, self.split_supported.get()) {
                    return;
                }
                (&self.split_preview, false)
            }
            EditorMode::Rendered => (&self.rendered_preview, true),
            EditorMode::Source | EditorMode::Rich => return,
        };
        let rendered_source = if rendered {
            &self.rendered_preview_source
        } else {
            &self.split_preview_source
        };
        let source_changed = preview_source_needs_load(
            rendered_source.borrow().as_ref(),
            preview.session,
            &preview.source,
        );
        if source_changed || presentation_changed {
            let navigation = if rendered {
                &self.rendered_navigation
            } else {
                &self.split_navigation
            };
            navigation.set_document(preview.session, &preview.source);
            preview::load_preview_with_theme(view, &preview.source, allow_remote_images, theme);
            rendered_source.replace(Some((preview.session, preview.source.clone())));
        }
    }

    /// Applies a reducer-approved command to the isolated rich-text projection.
    pub(crate) fn apply_rich_command(&self, command: &carver_editor_protocol::EditorCommand) {
        self.rich.command(command);
    }

    /// Reloads the rich projection after an asynchronous reducer-owned source mutation.
    pub(crate) fn reload_rich_source(&self, session: EditorSessionId, source: &str) {
        if self.loaded_session.borrow().as_ref() == Some(&session) {
            self.rich.load_source(source);
        }
    }

    /// Restores the selection calculated by a pure source edit after its snapshot renders.
    pub(crate) fn select_source_range(
        &self,
        session: EditorSessionId,
        selection: std::ops::Range<usize>,
    ) {
        if self.loaded_session.borrow().as_ref() != Some(&session) {
            return;
        }
        let start = self
            .source_buffer
            .iter_at_offset(i32::try_from(selection.start).unwrap_or(i32::MAX));
        let end = self
            .source_buffer
            .iter_at_offset(i32::try_from(selection.end).unwrap_or(i32::MAX));
        if selection.is_empty() {
            self.source_buffer.place_cursor(&start);
        } else {
            self.source_buffer.select_range(&start, &end);
        }
    }

    /// Focuses a validated occurrence while retaining the current editor mode.
    pub(crate) fn focus_document_target(
        &self,
        session: EditorSessionId,
        generation: u64,
        selection: std::ops::Range<usize>,
        target: &carver_editor_protocol::DocumentTarget,
    ) {
        if self.loaded_session.borrow().as_ref() != Some(&session)
            || self.source_generation.get() != generation
        {
            return;
        }
        if self.source_mode.is_active() {
            let mut start = self
                .source_buffer
                .iter_at_offset(i32::try_from(selection.start).unwrap_or(i32::MAX));
            let end = self
                .source_buffer
                .iter_at_offset(i32::try_from(selection.end).unwrap_or(i32::MAX));
            if selection.is_empty() {
                self.source_buffer.place_cursor(&start);
            } else {
                self.source_buffer.select_range(&start, &end);
            }
            self.source_editor.view().grab_focus();
            if let Some(root) = self.source_editor.view().root() {
                root.set_focus(Some(self.source_editor.view()));
            }
            self.source_editor
                .view()
                .scroll_to_iter(&mut start, 0.2, false, 0.0, 0.0);
            if self.split_toggle.is_active() {
                self.split_navigation
                    .focus(target, &buffer_text(&self.source_buffer), false);
            }
        } else if self.rich_mode.is_active() {
            self.rich.focus_document_target(target);
        } else {
            self.rendered_navigation
                .focus(target, &buffer_text(&self.source_buffer), true);
        }
    }

    /// Launches a prepared file copy after the reducer admits the preview.
    pub(crate) fn preview_media(&self, session: EditorSessionId, path: &Path) {
        if self.loaded_session.borrow().as_ref() != Some(&session) {
            return;
        }
        let parent = self.sidebar.root.root().and_downcast::<gtk::Window>();
        media_preview::launch(path, parent.as_ref(), session, &self.dispatcher);
    }

    /// Executes a clipboard effect after the reducer has admitted its immutable request.
    pub(crate) fn copy_document(&self, request: &EditorCopyRequest) {
        let dispatcher = self.dispatcher.clone();
        let source = request.source.clone();
        let assets_dir = self.assets_dir.clone();
        let clipboard = self.source_editor.view().display().clipboard();
        let request_id = request.request_id;
        glib::idle_add_local_once(move || {
            let message = match publish_note(&clipboard, &source, assets_dir.as_deref()) {
                Ok(document) => AppMsg::Editor(EditorMsg::CopyCompleted {
                    request_id,
                    omitted_images: document.omitted_images,
                }),
                Err(_) => AppMsg::Editor(EditorMsg::CopyFailed { request_id }),
            };
            let _ = dispatcher.dispatch(message);
        });
    }

    /// Executes a native export-options effect after the reducer captures its snapshot.
    pub(crate) fn show_export_dialog(&self, request: EditorExportDialogRequest) {
        let parent = self
            .source_editor
            .view()
            .root()
            .and_downcast::<gtk::Window>();
        show_export_options_dialog(request, parent.as_ref(), self.dispatcher.clone());
    }

    /// Executes a native export-warning effect after preparation reports warnings.
    pub(crate) fn show_export_warning(&self, request: &EditorExportWarningRequest) {
        let parent = self
            .source_editor
            .view()
            .root()
            .and_downcast::<gtk::Window>();
        show_export_warning_dialog(request, parent.as_ref(), self.dispatcher.clone());
    }

    /// Executes a native PDF rendering effect after the reducer captures its snapshot.
    pub(crate) fn export_pdf(&self, request: &EditorPdfExportRequest) {
        let parent = self
            .source_editor
            .view()
            .root()
            .and_downcast::<gtk::Window>();
        export_rendered_snapshot(
            &request.source,
            self.remote_images.get(),
            request.print_dialog,
            &request.target_uri,
            parent.as_ref(),
            self.assets_dir.as_deref(),
            self.dispatcher.clone(),
            request.request_id,
        );
    }
}

fn invalidate_preview_sources(
    split_preview_source: &RefCell<Option<PreviewSource>>,
    rendered_preview_source: &RefCell<Option<PreviewSource>>,
) {
    split_preview_source.replace(None);
    rendered_preview_source.replace(None);
}

fn remember_preview_source(cache: &PreviewSourceCache, preview: &crate::mvu::EditorPreview) {
    if preview_source_needs_load(cache.borrow().as_ref(), preview.session, &preview.source) {
        cache.replace(Some((preview.session, preview.source.clone())));
    }
}

fn preview_source_needs_load(
    cached: Option<&PreviewSource>,
    session: EditorSessionId,
    source: &str,
) -> bool {
    cached
        .as_ref()
        .is_none_or(|(cached_session, cached_source)| {
            *cached_session != session || cached_source != source
        })
}

fn split_preview_is_visible(
    mode: EditorMode,
    source_split_view: bool,
    split_supported: bool,
) -> bool {
    mode == EditorMode::Source && source_split_view && split_supported
}

fn refresh_split_preview(
    view: &webkit6::WebView,
    latest: &PreviewSourceCache,
    loaded: &PreviewSourceCache,
    allow_remote_images: bool,
    navigation: &document_navigation::PreviewNavigation,
) {
    let Some((session, source)) = latest.borrow().clone() else {
        return;
    };
    if !preview_source_needs_load(loaded.borrow().as_ref(), session, &source) {
        return;
    }
    navigation.set_document(session, &source);
    preview::load_preview_with_theme(view, &source, allow_remote_images, &editor_theme());
    loaded.replace(Some((session, source)));
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
    compact_navigation: &CompactNavigation,
) -> Result<EditorSurface, SourceSyntaxError> {
    let allow_remote_images = config.images.load_remote_automatically;
    let assets_dir = assets_dir.map(Path::to_path_buf);
    let view = adw::ToolbarView::new();
    view.set_widget_name("editor-surface");
    let header = adw::HeaderBar::new();
    let toggle_sidebar = sidebar_toggle_button(
        split_view,
        compact_navigation,
        "editor-toggle-categories-button",
    );
    header.pack_start(&toggle_sidebar);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-to-notes-button");
    back.set_tooltip_text(Some("Back to notes"));
    header.pack_start(&back);
    let mode_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_group.add_css_class("linked");
    mode_group.set_widget_name("editor-mode-group");
    let (rich_mode, rich_mode_label) = editor_mode_button(
        "editor-mode-rich",
        "document-edit-symbolic",
        "Edit",
        "Edit with rich text",
    );
    rich_mode.set_active(true);
    let (source_mode, source_mode_label) = editor_mode_button(
        "editor-mode-source",
        "text-x-generic-symbolic",
        "Source",
        "Edit Carve markup",
    );
    source_mode.set_group(Some(&rich_mode));
    let (rendered_mode, rendered_mode_label) = editor_mode_button(
        "editor-mode-rendered",
        "view-reveal-symbolic",
        "Preview",
        "Read-only preview",
    );
    rendered_mode.set_group(Some(&rich_mode));
    mode_group.append(&rich_mode);
    mode_group.append(&source_mode);
    mode_group.append(&rendered_mode);
    let favorite = gtk::ToggleButton::new();
    favorite.set_icon_name("starred-symbolic");
    favorite.set_widget_name("favorite-note-button");
    favorite.set_tooltip_text(Some("Add to Favorites"));
    favorite.add_css_class("flat");
    let document_sidebar_toggle = gtk::ToggleButton::new();
    document_sidebar_toggle.set_icon_name("sidebar-show-right-symbolic");
    document_sidebar_toggle.set_widget_name("editor-document-sidebar-toggle");
    document_sidebar_toggle.set_tooltip_text(Some("Show document sidebar"));
    document_sidebar_toggle.add_css_class("flat");
    let copy_note = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy_note.set_widget_name("copy-note-button");
    copy_note.set_tooltip_text(Some("Copy note"));
    copy_note.add_css_class("flat");
    let (options_menu, compact_options) = editor_options_menu();
    header.pack_end(&options_menu);
    header.pack_end(&document_sidebar_toggle);
    header.pack_end(&copy_note);
    header.pack_end(&favorite);
    view.add_top_bar(&header);

    let split_toggle = gtk::ToggleButton::new();
    split_toggle.set_icon_name("view-dual-symbolic");
    split_toggle.set_widget_name("source-split-toggle");
    split_toggle.set_tooltip_text(Some("Show rendered preview"));
    split_toggle.set_sensitive(false);
    let mode_controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    mode_controls.set_widget_name("editor-mode-switcher");
    mode_controls.add_css_class("editor-mode-switcher");
    mode_controls.set_size_request(0, -1);
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
        assets_dir.clone(),
        allow_remote_images,
        dispatcher,
        &source_buffer,
        toast_overlay,
    );
    let remote_images = Rc::new(Cell::new(allow_remote_images));
    refresh_rich_theme(&rich);
    let split_preview = build_preview(assets_dir.as_deref(), toast_overlay);
    split_preview.set_widget_name("source-split-preview");
    let rendered_preview = build_preview(assets_dir.as_deref(), toast_overlay);
    rendered_preview.set_widget_name("editor-rendered-preview");
    let rendered_navigation = document_navigation::PreviewNavigation::new(
        &rendered_preview,
        dispatcher,
        EditorMode::Rendered,
    );
    let split_navigation =
        document_navigation::PreviewNavigation::new(&split_preview, dispatcher, EditorMode::Source);
    let find = FindController::new(&source_editor, rich.view(), &view);
    view.add_top_bar(find.widget());
    install_editor_window_shortcuts(&view, dispatcher);
    let toolbar = Toolbar::new(source.upcast_ref(), &rich, dispatcher, toast_overlay);
    let source_context = SourceContextCache::new(&source_buffer);
    let selection_dispatcher = dispatcher.clone();
    let selection_rendering = Rc::clone(&rendering);
    source_buffer.connect_mark_set(move |buffer, _, mark| {
        if selection_rendering.get()
            || !matches!(mark.name().as_deref(), Some("insert" | "selection_bound"))
        {
            return;
        }
        let (start, end) = buffer.selection_bounds().unwrap_or_else(|| {
            let cursor = buffer.iter_at_mark(&buffer.get_insert());
            (cursor, cursor)
        });
        let _ = selection_dispatcher.dispatch(AppMsg::Editor(EditorMsg::SourceSelectionChanged {
            selection: usize::try_from(start.offset()).unwrap_or(0)
                ..usize::try_from(end.offset()).unwrap_or(0),
        }));
    });
    connect_source_context(&source_buffer, &source_context, &toolbar);
    let find_for_source_change = find.clone();
    source_buffer.connect_changed(move |_| find_for_source_change.refresh_after_document_change());
    let toolbar_for_selection = toolbar.clone();
    rich.connect_selection_changed(move |selection| {
        toolbar_for_selection.set_rich_selection(&selection);
    });
    install_source_shortcuts(source.upcast_ref(), &toolbar);
    let split_preview_state = SplitPreviewState::new();
    let pages = add_editor_pages(
        &editor_stack,
        &EditorPageViews {
            rich: rich.view(),
            source: source.upcast_ref(),
            split_preview: &split_preview,
            rendered_preview: &rendered_preview,
            split_navigation: &split_navigation,
        },
        &split_toggle,
        &source_mode,
        &split_preview_state,
        &remote_images,
    );
    let toolbar_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    toolbar_bar.set_widget_name("formatting-toolbar-bar");
    toolbar_bar.append(toolbar.widget());
    view.add_bottom_bar(&toolbar_bar);
    let sidebar =
        document_sidebar::DocumentSidebar::new(&editor_stack, document_sidebar_toggle, dispatcher);
    view.set_content(Some(&sidebar.container));
    install_compact_editor_actions(&view, dispatcher, &split_toggle);

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
        &split_preview_state.supported,
        &rendering,
        &find,
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
        &split_preview_state.supported,
    );
    connect_source_scroll_sync(&pages.source_scroll, &split_preview, &split_toggle);
    connect_theme_changes(dispatcher);
    connect_favorite_action(dispatcher, &favorite);
    connect_copy_action(dispatcher, &copy_note);
    connect_document_sidebar_toggle(dispatcher, &sidebar.toggle, &rendering);
    connect_add_files(
        dispatcher,
        &sidebar.add_files,
        &source_buffer,
        &source_mode,
        &rich,
    );
    connect_back_action(dispatcher, &back);
    connect_source_preview(dispatcher, &source_buffer, &rendering);
    let _source_image_paste = render::install_image_paste(source.upcast_ref(), dispatcher, &rich);
    let _source_image_drop = render::install_image_drop(&source, dispatcher, &rich);
    let _rich_image_drop = render::install_image_drop(rich.view(), dispatcher, &rich);
    let responsive_container = adw::BreakpointBin::new();
    responsive_container.set_widget_name("editor-responsive-container");
    responsive_container.set_size_request(360, 240);
    responsive_container.set_child(Some(&view));
    let compact_breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        COMPACT_EDITOR_WIDTH,
        adw::LengthUnit::Px,
    ));
    compact_breakpoint.add_setters(&[(&rich_mode_label, "visible", false)]);
    compact_breakpoint.add_setters(&[(&source_mode_label, "visible", false)]);
    compact_breakpoint.add_setters(&[(&rendered_mode_label, "visible", false)]);
    compact_breakpoint.add_setters(&[(&favorite, "visible", false)]);
    compact_breakpoint.add_setters(&[(&copy_note, "visible", false)]);
    compact_breakpoint.add_setters(&[(&back, "visible", false)]);
    compact_breakpoint.add_setters(&[(&split_toggle, "visible", false)]);
    compact_breakpoint.add_setters(&[(toolbar.desktop_widget(), "visible", false)]);
    compact_breakpoint.add_setters(&[(toolbar.compact_widget(), "visible", true)]);
    responsive_container.add_breakpoint(compact_breakpoint);
    let refs = EditorViewRefs {
        favorite,
        compact_options,
        sidebar,
        rich_mode,
        source_mode,
        rendered_mode,
        find,
        toolbar,
        toolbar_bar,
        editor_stack,
        split_toggle,
        rich,
        source_buffer,
        source_editor,
        split_preview,
        split_navigation,
        rendered_navigation,
        rendered_preview,
        rendering,
        remote_images,
        split_supported: split_preview_state.supported,
        split_preview_source: split_preview_state.loaded_source,
        latest_split_preview_source: split_preview_state.latest_source,
        rendered_preview_source: RefCell::new(None),
        rendered_theme_revision: RefCell::new(Some(0)),
        loaded_session: RefCell::new(None),
        source_generation: Cell::new(0),
        dispatcher: dispatcher.clone(),
        assets_dir,
    };
    Ok(EditorSurface {
        widget: responsive_container.upcast(),
        refs,
    })
}

fn connect_document_sidebar_toggle(
    dispatcher: &AppDispatcher,
    toggle: &gtk::ToggleButton,
    rendering: &Rc<Cell<bool>>,
) {
    let rendering = Rc::clone(rendering);
    let dispatcher = dispatcher.clone();
    toggle.connect_toggled(move |_| {
        if rendering.get() {
            return;
        }
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ToggleDocumentSidebar));
    });
}

fn connect_add_files(
    dispatcher: &AppDispatcher,
    button: &gtk::Button,
    source_buffer: &gtk::TextBuffer,
    source_mode: &gtk::ToggleButton,
    rich: &RichEditor,
) {
    let dispatcher = dispatcher.clone();
    let source_buffer = source_buffer.clone();
    let source_mode = source_mode.clone();
    let rich = rich.clone();
    button.connect_clicked(move |button| {
        if !button.is_sensitive() {
            return;
        }
        let Some(session) = rich.document_session() else {
            return;
        };
        let source = source_mode
            .is_active()
            .then(|| source_commands::image_target_from_buffer(&source_buffer));
        super::formatting::choose_managed_files(
            button,
            &dispatcher,
            crate::mvu::ImportTarget { session, source },
        );
    });
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
) -> (gtk::ToggleButton, gtk::Label) {
    let button = gtk::ToggleButton::new();
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    content.append(&gtk::Image::from_icon_name(icon_name));
    let label = gtk::Label::new(Some(label));
    label.set_widget_name(&format!("{name}-label"));
    content.append(&label);
    button.set_child(Some(&content));
    (button, label)
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
}

struct EditorPageViews<'a> {
    rich: &'a webkit6::WebView,
    source: &'a gtk::TextView,
    split_preview: &'a webkit6::WebView,
    rendered_preview: &'a webkit6::WebView,
    split_navigation: &'a document_navigation::PreviewNavigation,
}

struct SplitPreviewState {
    supported: Rc<Cell<bool>>,
    loaded_source: PreviewSourceCache,
    latest_source: PreviewSourceCache,
}

impl SplitPreviewState {
    fn new() -> Self {
        Self {
            supported: Rc::new(Cell::new(true)),
            loaded_source: Rc::new(RefCell::new(None)),
            latest_source: Rc::new(RefCell::new(None)),
        }
    }
}

fn add_editor_pages(
    editor_stack: &gtk::Stack,
    views: &EditorPageViews<'_>,
    split_toggle: &gtk::ToggleButton,
    source_mode: &gtk::ToggleButton,
    split_preview_state: &SplitPreviewState,
    remote_images: &Rc<Cell<bool>>,
) -> EditorPages {
    let rich_scroll = gtk::ScrolledWindow::new();
    rich_scroll.set_child(Some(views.rich));
    let source_scroll = gtk::ScrolledWindow::new();
    source_scroll.set_child(Some(views.source));
    let split_scroll = gtk::ScrolledWindow::new();
    split_scroll.set_child(Some(views.split_preview));
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
    let split_supported_for_apply = Rc::clone(&split_preview_state.supported);
    let split_scroll_for_apply = split_scroll.clone();
    let split_toggle_for_apply = split_toggle.clone();
    breakpoint.connect_apply(move |_| {
        split_supported_for_apply.set(false);
        split_scroll_for_apply.set_visible(false);
        split_toggle_for_apply.set_sensitive(false);
    });
    let split_supported_for_unapply = Rc::clone(&split_preview_state.supported);
    let split_scroll_for_unapply = split_scroll.clone();
    let split_toggle_for_unapply = split_toggle.clone();
    let source_mode_for_unapply = source_mode.clone();
    let split_preview_for_unapply = views.split_preview.clone();
    let split_preview_source_for_unapply = Rc::clone(&split_preview_state.loaded_source);
    let latest_split_preview_source_for_unapply = Rc::clone(&split_preview_state.latest_source);
    let remote_images_for_unapply = Rc::clone(remote_images);
    let navigation = views.split_navigation.clone();
    breakpoint.connect_unapply(move |_| {
        split_supported_for_unapply.set(true);
        let source_active = source_mode_for_unapply.is_active();
        split_toggle_for_unapply.set_sensitive(source_active);
        let split_visible = source_active && split_toggle_for_unapply.is_active();
        split_scroll_for_unapply.set_visible(split_visible);
        if split_visible {
            refresh_split_preview(
                &split_preview_for_unapply,
                &latest_split_preview_source_for_unapply,
                &split_preview_source_for_unapply,
                remote_images_for_unapply.get(),
                &navigation,
            );
        }
    });
    source_container.add_breakpoint(breakpoint);
    let rendered_scroll = gtk::ScrolledWindow::new();
    rendered_scroll.set_child(Some(views.rendered_preview));
    editor_stack.add_named(&rich_scroll, Some("rich"));
    editor_stack.add_named(&source_container, Some("source"));
    editor_stack.add_named(&rendered_scroll, Some("rendered"));
    editor_stack.set_visible_child_name("rich");
    EditorPages { source_scroll }
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
    find: &FindController,
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
        let find = find.clone();
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
            find.set_mode(surface);
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

fn connect_favorite_action(dispatcher: &AppDispatcher, favorite: &gtk::ToggleButton) {
    let dispatcher = dispatcher.clone();
    favorite.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ToggleFavorite));
    });
}

fn connect_copy_action(dispatcher: &AppDispatcher, copy_note: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    copy_note.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::CopyRequested));
    });
}

fn editor_options_menu() -> (gtk::MenuButton, gtk::gio::Menu) {
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("Note options"));
    menu.add_css_class("flat");
    menu.set_widget_name("editor-options-menu");
    let actions = gtk::gio::Menu::new();
    actions.append(Some("Back to notes"), Some("editor.back"));
    actions.append(Some("Copy note"), Some("editor.copy-note"));
    actions.append(Some("Add to Favorites"), Some("editor.toggle-favorite"));
    actions.append(
        Some("Show rendered preview"),
        Some("editor.toggle-split-preview"),
    );
    actions.append(Some("Export note…"), Some(EXPORT_NOTE_ACTION));
    actions.append(Some("Print…"), Some(PRINT_NOTE_ACTION));
    actions.append(Some("Move to Trash"), Some(TRASH_NOTE_ACTION));
    menu.set_menu_model(Some(&actions));
    (menu, actions)
}

fn install_compact_editor_actions(
    view: &adw::ToolbarView,
    dispatcher: &AppDispatcher,
    split_toggle: &gtk::ToggleButton,
) {
    let actions = gtk::gio::SimpleActionGroup::new();
    let back = gtk::gio::SimpleAction::new("back", None);
    let back_dispatcher = dispatcher.clone();
    back.connect_activate(move |_, _| {
        let _ = back_dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
    });
    actions.add_action(&back);
    let copy_note = gtk::gio::SimpleAction::new("copy-note", None);
    let copy_dispatcher = dispatcher.clone();
    copy_note.connect_activate(move |_, _| {
        let _ = copy_dispatcher.dispatch(AppMsg::Editor(EditorMsg::CopyRequested));
    });
    actions.add_action(&copy_note);
    let favorite = gtk::gio::SimpleAction::new("toggle-favorite", None);
    let favorite_dispatcher = dispatcher.clone();
    favorite.connect_activate(move |_, _| {
        let _ = favorite_dispatcher.dispatch(AppMsg::Editor(EditorMsg::ToggleFavorite));
    });
    actions.add_action(&favorite);
    let split_preview = gtk::gio::SimpleAction::new("toggle-split-preview", None);
    split_preview.set_enabled(split_toggle.is_sensitive());
    let split_preview_for_sensitivity = split_preview.clone();
    split_toggle.connect_sensitive_notify(move |toggle| {
        split_preview_for_sensitivity.set_enabled(toggle.is_sensitive());
    });
    let split_toggle = split_toggle.clone();
    split_preview.connect_activate(move |_, _| {
        split_toggle.set_active(!split_toggle.is_active());
    });
    actions.add_action(&split_preview);
    view.insert_action_group("editor", Some(&actions));
}

/// Installs editor-wide actions before embedded rich-text widgets receive their key events.
fn install_editor_window_shortcuts(view: &adw::ToolbarView, dispatcher: &AppDispatcher) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("editor-window-shortcuts"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let action_host = view.clone().upcast::<gtk::Widget>();
    let action_host_for_callback = action_host.clone();
    let dispatcher = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk::gdk::Key::F9 {
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ToggleDocumentSidebar));
            return glib::Propagation::Stop;
        }
        if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let action = match (key, shift) {
            (gtk::gdk::Key::e, false) => EXPORT_NOTE_ACTION,
            (gtk::gdk::Key::p, false) => PRINT_NOTE_ACTION,
            (gtk::gdk::Key::d, false) => TRASH_NOTE_ACTION,
            (gtk::gdk::Key::f, true) => TOGGLE_FAVORITE_ACTION,
            _ => return glib::Propagation::Proceed,
        };
        let _ = action_host_for_callback.activate_action(action, None::<&glib::Variant>);
        glib::Propagation::Stop
    });
    action_host.add_controller(controller);
}

/// Presents the export-format chooser for the currently open note.
pub(crate) fn show_export_options_dialog(
    request: crate::mvu::EditorExportDialogRequest,
    parent: Option<&gtk::Window>,
    dispatcher: AppDispatcher,
) -> adw::AlertDialog {
    let format_options = gtk::StringList::new(&["Carve", "Markdown", "HTML", "PDF"]);
    let format_expression = gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    );
    let format = adw::ComboRow::new();
    format.set_widget_name("export-format-setting");
    format.set_title("Format");
    format.set_model(Some(&format_options));
    format.set_expression(Some(&format_expression));
    format.set_selected(0);
    let include_assets = adw::SwitchRow::new();
    include_assets.set_widget_name("export-assets-setting");
    include_assets.set_title("Include managed files");
    include_assets.set_subtitle("Create a portable ZIP archive with the document and its files.");
    let format_for_toggle = format.clone();
    let assets_for_toggle = include_assets.clone();
    format.connect_selected_notify(move |_| {
        let pdf_selected = format_for_toggle.selected() == 3;
        assets_for_toggle.set_sensitive(!pdf_selected);
        if pdf_selected {
            assets_for_toggle.set_active(false);
        }
    });
    let contents = adw::PreferencesGroup::new();
    contents.add(&format);
    contents.add(&include_assets);
    let dialog = adw::AlertDialog::builder()
        .heading("Export note")
        .body("Export the current note, including any unsaved edits.")
        .extra_child(&contents)
        .default_response("export")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("export", "Export…")]);
    let parent_for_response = parent.cloned();
    dialog.connect_response(None, move |_, response| {
        if response != "export" {
            return;
        }
        let format = match format.selected() {
            0 => EditorExportFormat::Carve,
            1 => EditorExportFormat::Markdown,
            2 => EditorExportFormat::Html,
            _ => EditorExportFormat::Pdf,
        };
        let include_assets =
            include_assets.is_active() && !matches!(format, EditorExportFormat::Pdf);
        show_export_file_dialog(
            request.request_id,
            &request.filename_stem,
            format,
            include_assets,
            parent_for_response.as_ref(),
            dispatcher.clone(),
        );
    });
    dialog.present(parent);
    dialog
}

fn show_export_file_dialog(
    request_id: u64,
    filename_stem: &str,
    format: EditorExportFormat,
    include_assets: bool,
    parent: Option<&gtk::Window>,
    dispatcher: AppDispatcher,
) {
    let extension = format.extension(include_assets);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(match extension {
        "crv" => "Carve documents",
        "md" => "Markdown documents",
        "html" => "HTML documents",
        "pdf" => "PDF documents",
        _ => "Portable ZIP archives",
    }));
    filter.add_suffix(extension);
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder()
        .title("Export note")
        .accept_label("Export")
        .initial_name(format!("{filename_stem}.{extension}"))
        .build();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));
    dialog.save(parent, None::<&gtk::gio::Cancellable>, move |result| {
        let Ok(file) = result else {
            return;
        };
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ExportRequested {
            request_id,
            format,
            include_assets,
            target_uri: file.uri().to_string(),
        }));
    });
}

/// Presents a loss-warning confirmation before a lossy export is written.
pub(crate) fn show_export_warning_dialog(
    request: &EditorExportWarningRequest,
    parent: Option<&gtk::Window>,
    dispatcher: AppDispatcher,
) -> adw::AlertDialog {
    let details = request.warnings.join("\n");
    let dialog = adw::AlertDialog::builder()
        .heading("Export may lose content")
        .body(format!("{details}\n\nExport anyway?"))
        .default_response("export")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("export", "Export anyway")]);
    let request_id = request.request_id;
    dialog.connect_response(None, move |_, response| {
        let message = if response == "export" {
            EditorMsg::ExportConfirmed { request_id }
        } else {
            EditorMsg::ExportCancelled { request_id }
        };
        let _ = dispatcher.dispatch(AppMsg::Editor(message));
    });
    dialog.present(parent);
    dialog
}

#[expect(
    clippy::too_many_arguments,
    reason = "a PDF request is an immutable rendering snapshot supplied by the MVU view"
)]
/// Renders a canonical source snapshot to PDF or submits it to the native print dialog.
pub(crate) fn export_rendered_snapshot(
    source: &str,
    allow_remote_images: bool,
    print_dialog: bool,
    target_uri: &str,
    parent: Option<&gtk::Window>,
    assets_dir: Option<&Path>,
    dispatcher: AppDispatcher,
    request_id: u64,
) {
    let toast_overlay = adw::ToastOverlay::new();
    let preview = build_preview(assets_dir, &toast_overlay);
    let source = source.to_owned();
    let target_uri = target_uri.to_owned();
    let parent = parent.cloned();
    // WebKit can only print a realized view. Keep the implementation detail in a tiny,
    // transient window rather than disturbing the editor's live preview state.
    let print_window = gtk::Window::new();
    print_window.set_default_size(1, 1);
    print_window.set_decorated(false);
    print_window.set_resizable(false);
    if let Some(parent) = parent.as_ref() {
        print_window.set_transient_for(Some(parent));
    }
    print_window.set_child(Some(&preview));
    print_window.present();
    let print_window_weak = print_window.downgrade();
    let load_started = Rc::new(Cell::new(false));
    let load_started_for_callback = Rc::clone(&load_started);
    preview.connect_load_changed(move |preview, event| {
        if event != webkit6::LoadEvent::Finished || load_started_for_callback.replace(true) {
            return;
        }
        let Some(print_window) = print_window_weak.upgrade() else {
            return;
        };
        let operation = webkit6::PrintOperation::new(preview);
        let reported = Rc::new(Cell::new(false));
        if print_dialog {
            run_native_print_dialog(
                &operation,
                &print_window,
                &reported,
                &dispatcher,
                request_id,
            );
            return;
        }
        let settings = gtk::PrintSettings::new();
        settings.set(gtk::PRINT_SETTINGS_PRINTER, Some("Print to File"));
        settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(&target_uri));
        settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT, Some("pdf"));
        operation.set_print_settings(&settings);
        let page_setup = pdf_page_setup();
        operation.set_page_setup(&page_setup);
        // The printing operation is asynchronous. Retain it until it reports completion;
        // otherwise the Rust wrapper can be dropped before GTK writes the file.
        let retained_operation = Rc::new(RefCell::new(Some(operation.clone())));
        let retained_for_finished = Rc::clone(&retained_operation);
        let reported_for_finished = Rc::clone(&reported);
        let dispatcher_for_finished = dispatcher.clone();
        let print_window_for_finished = print_window.clone();
        operation.connect_finished(move |_| {
            let _ = retained_for_finished.borrow_mut().take();
            if reported_for_finished.replace(true) {
                return;
            }
            print_window_for_finished.destroy();
            let _ = dispatcher_for_finished
                .dispatch(AppMsg::Editor(EditorMsg::PdfExportCompleted { request_id }));
        });
        let retained_for_failed = Rc::clone(&retained_operation);
        let reported_for_failed = Rc::clone(&reported);
        let dispatcher_for_failed = dispatcher.clone();
        let print_window_for_failed = print_window.clone();
        operation.connect_failed(move |_, _| {
            let _ = retained_for_failed.borrow_mut().take();
            if reported_for_failed.replace(true) {
                return;
            }
            print_window_for_failed.destroy();
            let _ = dispatcher_for_failed
                .dispatch(AppMsg::Editor(EditorMsg::PdfExportFailed { request_id }));
        });
        operation.print();
    });
    load_preview(&preview, &source, allow_remote_images);
}

pub(crate) fn pdf_page_setup() -> gtk::PageSetup {
    let page_setup = gtk::PageSetup::new();
    page_setup.set_paper_size(&gtk::PaperSize::new(Some("iso_a4")));
    page_setup.set_orientation(gtk::PageOrientation::Portrait);
    page_setup.set_top_margin(18.0, gtk::Unit::Mm);
    page_setup.set_bottom_margin(18.0, gtk::Unit::Mm);
    page_setup.set_left_margin(18.0, gtk::Unit::Mm);
    page_setup.set_right_margin(18.0, gtk::Unit::Mm);
    page_setup
}

fn run_native_print_dialog(
    operation: &webkit6::PrintOperation,
    print_window: &gtk::Window,
    reported: &Rc<Cell<bool>>,
    dispatcher: &AppDispatcher,
    request_id: u64,
) {
    // WebKitGTK's GTK4 `run_dialog` path destroys a non-window object when the dialog is
    // cancelled. Use GTK's supported print dialog to collect the native settings, then let
    // WebKit render the accepted document with those settings.
    let native_dialog = gtk::PrintOperation::new();
    let page_setup = pdf_page_setup();
    native_dialog.set_default_page_setup(Some(&page_setup));
    native_dialog.set_embed_page_setup(true);
    native_dialog.set_n_pages(1);
    let accepted = Rc::new(Cell::new(false));
    let accepted_for_begin_print = Rc::clone(&accepted);
    native_dialog.connect_begin_print(move |native_dialog, _| {
        accepted_for_begin_print.set(true);
        // GTK has delivered the selected settings. It must not render a second, empty document;
        // the WebKit operation below owns rendering Carve's HTML snapshot.
        native_dialog.cancel();
    });
    let response = native_dialog.run(gtk::PrintOperationAction::PrintDialog, Some(print_window));
    if response.is_err() {
        complete_native_print_failure(print_window, dispatcher, request_id);
        return;
    }
    if !accepted.get() {
        if !reported.replace(true) {
            let _ =
                dispatcher.dispatch(AppMsg::Editor(EditorMsg::PdfExportCancelled { request_id }));
        }
        print_window.destroy();
        return;
    }

    if let Some(settings) = native_dialog.print_settings() {
        operation.set_print_settings(&settings);
    }
    operation.set_page_setup(&native_dialog.default_page_setup());

    // The native GTK operation was deliberately cancelled after its dialog supplied the
    // settings. Retain the WebKit operation and realized host until its own render completes.
    let retained_operation = Rc::new(RefCell::new(Some(operation.clone())));
    let retained_for_finished = Rc::clone(&retained_operation);
    let reported_for_finished = Rc::clone(reported);
    let dispatcher_for_finished = dispatcher.clone();
    let print_window_for_finished = print_window.clone();
    operation.connect_finished(move |_| {
        let _ = retained_for_finished.borrow_mut().take();
        if reported_for_finished.replace(true) {
            return;
        }
        print_window_for_finished.destroy();
        let _ = dispatcher_for_finished
            .dispatch(AppMsg::Editor(EditorMsg::PdfExportCompleted { request_id }));
    });
    let retained_for_failed = Rc::clone(&retained_operation);
    let reported_for_failed = Rc::clone(reported);
    let dispatcher_for_failed = dispatcher.clone();
    let print_window_for_failed = print_window.clone();
    operation.connect_failed(move |_, _| {
        let _ = retained_for_failed.borrow_mut().take();
        if reported_for_failed.replace(true) {
            return;
        }
        print_window_for_failed.destroy();
        let _ = dispatcher_for_failed
            .dispatch(AppMsg::Editor(EditorMsg::PdfExportFailed { request_id }));
    });
    operation.print();
}

fn complete_native_print_failure(
    print_window: &gtk::Window,
    dispatcher: &AppDispatcher,
    request_id: u64,
) {
    print_window.destroy();
    let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::PdfExportFailed { request_id }));
}

fn connect_back_action(dispatcher: &AppDispatcher, back: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    back.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
    });
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

fn editor_theme() -> web::EditorTheme {
    let style_manager = adw::StyleManager::default();
    let dark = style_manager.is_dark();
    web::editor_theme(dark, &style_manager.accent_color().to_standalone_rgba(dark))
}

fn refresh_rich_theme(rich: &RichEditor) {
    rich.set_theme(&editor_theme());
}

/// Installs source-mode equivalents of the common Rich Text keyboard shortcuts.
pub(crate) fn install_source_shortcuts(
    view: &gtk::TextView,
    toolbar: &Toolbar,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("source-format-shortcuts"));
    let toolbar = toolbar.clone();
    let anchor = view.clone().upcast::<gtk::Widget>();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        let control = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if shift && !control && matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) {
            toolbar.execute_source_command(crate::mvu::SourceCommand::InsertHardBreak);
            return glib::Propagation::Stop;
        }
        if !control {
            return glib::Propagation::Proceed;
        }
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
