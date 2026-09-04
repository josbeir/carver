//! WebKit-backed Carve editing surface and its native host bridge.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use carver_editor_protocol::{EditorCommand, EditorEvent, SelectionState};
use gtk::prelude::*;
use libadwaita::prelude::*;
use webkit6::prelude::*;

use crate::{
    controller::AppState,
    formatting,
    mvu::{AppDispatcher, AppMsg, EditorMsg},
};

const EDITOR_JAVASCRIPT: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/editor.js"));
const EDITOR_STYLESHEET: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/editor.css"));

type UnsupportedHandler = Rc<RefCell<Option<Box<dyn Fn()>>>>;
type SelectionHandler = Rc<RefCell<Option<Box<dyn Fn(SelectionState)>>>>;

/// A `WebKit` rich-text editor whose canonical state is kept in the source buffer.
#[derive(Clone)]
pub(crate) struct RichEditor {
    view: webkit6::WebView,
    session: Rc<Cell<u64>>,
    ready: Rc<Cell<bool>>,
    pending_source: Rc<RefCell<Option<(u64, String)>>>,
    unsupported_handler: UnsupportedHandler,
    selection_handler: SelectionHandler,
}

impl RichEditor {
    /// Builds an editor backed by the locally bundled, sandboxed Tiptap application.
    pub(crate) fn new(
        assets_dir: Option<std::path::PathBuf>,
        allow_remote_images: bool,
        dispatcher: &AppDispatcher,
        source_buffer: &gtk::TextBuffer,
        toast_overlay: &libadwaita::ToastOverlay,
    ) -> Self {
        let context = webkit6::WebContext::new();
        super::preview::install_editor_asset_scheme(&context, assets_dir);
        let manager = webkit6::UserContentManager::new();
        manager.register_script_message_handler("carver", None);
        // Use WebKit's privileged user-script channel instead of an inline
        // `<script>` tag. This is not affected by page markup policy and keeps
        // JavaScript disabled for arbitrary document markup.
        manager.add_script(&webkit6::UserScript::new(
            EDITOR_JAVASCRIPT,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::End,
            // The view owns an isolated context and only loads the document
            // below, so an empty allow-list deliberately means every frame in
            // this one view. Custom URI schemes are not portable allow-list
            // patterns across WebKit builds.
            &[],
            &[],
        ));
        let settings = webkit6::Settings::new();
        settings.set_enable_javascript(true);
        settings.set_enable_javascript_markup(false);
        settings.set_enable_media(false);
        settings.set_enable_html5_database(false);
        settings.set_enable_html5_local_storage(false);
        settings.set_auto_load_images(true);
        let view = webkit6::WebView::builder()
            .web_context(&context)
            .user_content_manager(&manager)
            .settings(&settings)
            .build();
        view.set_widget_name("rich-editor");

        let editor = Self {
            view,
            session: Rc::new(Cell::new(0)),
            ready: Rc::new(Cell::new(false)),
            pending_source: Rc::new(RefCell::new(None)),
            unsupported_handler: Rc::new(RefCell::new(None)),
            selection_handler: Rc::new(RefCell::new(None)),
        };
        editor.connect_messages(&manager, dispatcher, source_buffer, toast_overlay);
        editor.connect_load_lifecycle();
        editor.view.load_html(
            &editor_document(allow_remote_images),
            Some("carver-asset:///"),
        );
        editor
    }

    /// Returns the GTK widget to add to layout containers.
    pub(crate) fn view(&self) -> &webkit6::WebView {
        &self.view
    }

    /// Loads a new document into the rich editor without marking it dirty.
    pub(crate) fn load_source(&self, source: &str) {
        let next_session = self.session.get().saturating_add(1);
        self.session.set(next_session);
        self.pending_source
            .replace(Some((next_session, source.to_owned())));
        self.flush_pending_source();
    }

    /// Rebuilds the sandbox shell so its CSP reflects the remote-image policy,
    /// then restores the canonical Carve source when the new editor is ready.
    pub(crate) fn reload_with_remote_images(&self, source: &str, allow_remote_images: bool) {
        self.ready.set(false);
        self.view.load_html(
            &editor_document(allow_remote_images),
            Some("carver-asset:///"),
        );
        self.load_source(source);
    }

    /// Sends a native formatting action to the focused editor selection.
    pub(crate) fn command(&self, command: &EditorCommand) {
        let (name, argument) = match command {
            EditorCommand::Named(name) => (name.as_str(), String::from("null")),
            EditorCommand::Heading(level) => ("heading", level.to_string()),
            EditorCommand::InsertTable {
                rows,
                columns,
                header,
            } => (
                "insert-table",
                format!("{{rows:{rows},columns:{columns},header:{header}}}"),
            ),
            EditorCommand::ImageWidth(width) => (
                "image-width",
                width.map_or_else(|| String::from("0"), |value| value.to_string()),
            ),
            EditorCommand::InsertLink { text, destination } => (
                "insert-link",
                format!("{{text:{},destination:{}}}", json(text), json(destination)),
            ),
        };
        self.evaluate(&format!(
            "window.carverEditor.command({}, {});",
            json(name),
            argument
        ));
    }

    /// Applies GNOME's color scheme and system accent without reloading the document.
    pub(crate) fn set_theme(&self, dark: bool, accent: &gtk::gdk::RGBA) {
        let selection = selection_theme(dark, accent);
        self.evaluate(&format!(
            "window.carverEditor.setTheme({dark}, {}, {}, {});",
            json(&selection.accent),
            json(&selection.background),
            json(selection.foreground)
        ));
    }

    /// Invokes `handler` when a source document cannot be edited without loss.
    pub(crate) fn connect_unsupported(&self, handler: impl Fn() + 'static) {
        self.unsupported_handler.replace(Some(Box::new(handler)));
    }

    /// Invokes `handler` whenever the focused selection's formatting changes.
    pub(crate) fn connect_selection_changed(&self, handler: impl Fn(SelectionState) + 'static) {
        self.selection_handler.replace(Some(Box::new(handler)));
    }

    fn connect_load_lifecycle(&self) {
        let ready = Rc::clone(&self.ready);
        self.view.connect_load_changed(move |_view, event| {
            // A finished page load does not guarantee that the editor bundle has
            // initialized yet. Only its explicit `ready` bridge message permits
            // source delivery; otherwise an initial note can be silently lost.
            if event == webkit6::LoadEvent::Started {
                ready.set(false);
            }
        });
    }

    fn connect_messages(
        &self,
        manager: &webkit6::UserContentManager,
        dispatcher: &AppDispatcher,
        source_buffer: &gtk::TextBuffer,
        toast_overlay: &libadwaita::ToastOverlay,
    ) {
        let editor = self.clone();
        let dispatcher = dispatcher.clone();
        let source_buffer = source_buffer.clone();
        let toast_overlay = toast_overlay.clone();
        let unsupported_handler = Rc::clone(&self.unsupported_handler);
        let selection_handler = Rc::clone(&self.selection_handler);
        manager.connect_script_message_received(Some("carver"), move |_manager, value| {
            let Some(bytes) = value.to_string_as_bytes() else {
                return;
            };
            let Ok(message) = serde_json::from_slice::<EditorEvent>(bytes.as_ref()) else {
                return;
            };
            match message {
                EditorEvent::Ready => {
                    editor.ready.set(true);
                    editor.flush_pending_source();
                }
                EditorEvent::Changed {
                    session, source, ..
                } if session == editor.session.get() => {
                    if source_buffer.text(
                        &source_buffer.start_iter(),
                        &source_buffer.end_iter(),
                        false,
                    ) != source
                    {
                        source_buffer.set_text(&source);
                    }
                }
                EditorEvent::Unsupported {
                    session,
                    unsupported,
                    degraded,
                } if session == editor.session.get() => {
                    let names = unsupported.into_iter().chain(degraded).collect::<Vec<_>>();
                    let detail = if names.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", names.join(", "))
                    };
                    toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                        "This note cannot be edited safely{detail}. Showing Preview instead."
                    )));
                    if let Some(handler) = unsupported_handler.borrow().as_ref() {
                        handler();
                    }
                }
                EditorEvent::PasteImage {
                    session,
                    mime_type,
                    data,
                } if session == editor.session.get() => {
                    let Ok(bytes) = STANDARD.decode(data) else {
                        toast_overlay
                            .add_toast(libadwaita::Toast::new("Could not read pasted image"));
                        return;
                    };
                    let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::PasteImage {
                        extension: image_extension(&mime_type).to_owned(),
                        bytes,
                    }));
                }
                EditorEvent::Selection {
                    session,
                    state: selection,
                } if session == editor.session.get() => {
                    if let Some(handler) = selection_handler.borrow().as_ref() {
                        handler(selection);
                    }
                }
                EditorEvent::Selection { .. }
                | EditorEvent::Changed { .. }
                | EditorEvent::Unsupported { .. }
                | EditorEvent::PasteImage { .. } => {}
            }
        });
    }

    fn flush_pending_source(&self) {
        if !self.ready.get() {
            return;
        }
        let Some((session, source)) = self.pending_source.borrow_mut().take() else {
            return;
        };
        self.evaluate(&format!(
            "window.carverEditor.load({}, {session});",
            json(&source)
        ));
    }

    /// Inserts a managed asset that was supplied by a native image source.
    pub(crate) fn insert_image_with_alt(&self, path: &str, alt: &str) {
        self.evaluate(&format!(
            "window.carverEditor.insertImage({}, {});",
            json(path),
            json(alt)
        ));
    }

    fn evaluate(&self, script: &str) {
        self.view.evaluate_javascript(
            script,
            None,
            Some("carver-editor:///bridge"),
            None::<&gtk::gio::Cancellable>,
            |_| {},
        );
    }
}

/// Appends native controls that dispatch their actions to the web editing surface.
#[expect(
    clippy::too_many_lines,
    reason = "the ordered toolbar declaration keeps command parity visibly auditable"
)]
pub(crate) fn append_controls(
    toolbar: &gtk::Box,
    editor: &RichEditor,
    state: &Rc<AppState>,
    toast_overlay: &libadwaita::ToastOverlay,
) -> RichToolbar {
    let mut toggle_buttons = Vec::new();
    for (name, icon, tooltip, command) in [
        (
            "format-bold-button",
            "format-text-bold-symbolic",
            "Bold (Ctrl+B)",
            "bold",
        ),
        (
            "format-italic-button",
            "format-text-italic-symbolic",
            "Italic (Ctrl+I)",
            "italic",
        ),
        (
            "format-strike-button",
            "format-text-strikethrough-symbolic",
            "Strikethrough",
            "strike",
        ),
        (
            "format-underline-button",
            "format-text-underline-symbolic",
            "Underline (Ctrl+U)",
            "underline",
        ),
        (
            "format-highlight-button",
            "format-text-highlight-symbolic",
            "Highlight",
            "highlight",
        ),
        (
            "format-superscript-button",
            "format-text-superscript-symbolic",
            "Superscript",
            "superscript",
        ),
        (
            "format-subscript-button",
            "format-text-subscript-symbolic",
            "Subscript",
            "subscript",
        ),
        (
            "format-code-button",
            "text-editor-symbolic",
            "Inline code",
            "inline-code",
        ),
        (
            "format-code-block-button",
            "utilities-terminal-symbolic",
            "Code block",
            "code-block",
        ),
        (
            "format-bullet-button",
            "view-list-bullet-symbolic",
            "Bulleted list",
            "bullet-list",
        ),
        (
            "format-ordered-button",
            "view-list-ordered-symbolic",
            "Numbered list",
            "ordered-list",
        ),
        (
            "format-task-button",
            "object-select-symbolic",
            "Task list",
            "task-list",
        ),
    ] {
        let active_name = command;
        let button = gtk::ToggleButton::new();
        set_toolbar_icon(&button, icon);
        button.set_widget_name(name);
        button.set_tooltip_text(Some(tooltip));
        button.add_css_class("flat");
        let editor = editor.clone();
        let command = EditorCommand::Named(command.to_owned());
        button.connect_clicked(move |_| editor.command(&command));
        toolbar.append(&button);
        toggle_buttons.push((active_name, button));
    }
    let link = gtk::ToggleButton::new();
    link.set_icon_name("insert-link-symbolic");
    link.set_widget_name("format-link-button");
    link.set_tooltip_text(Some("Insert link"));
    link.add_css_class("flat");
    let editor_for_link = editor.clone();
    link.connect_clicked(move |button| show_rich_link_dialog(button, &editor_for_link));
    toolbar.append(&link);
    toggle_buttons.push(("link", link));
    let (heading, heading_choices) = append_heading_menu(toolbar, editor);
    let table = append_table_menu(toolbar, editor);
    let (image, image_width_choices) = append_image_menu(toolbar, editor, state, toast_overlay);
    RichToolbar {
        toggle_buttons,
        heading,
        heading_choices,
        table,
        image,
        image_width_choices,
    }
}

/// Uses a compact text glyph only when the active icon theme lacks one of the
/// newer text-formatting icons. This prevents broken-image placeholders on
/// distributions whose Adwaita icon set predates those icon names.
fn set_toolbar_icon(button: &gtk::ToggleButton, icon_name: &str) {
    let has_icon = gtk::gdk::Display::default()
        .is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(icon_name));
    if has_icon {
        button.set_icon_name(icon_name);
    } else if let Some(glyph) = toolbar_fallback_glyph(icon_name) {
        let label = gtk::Label::new(Some(glyph));
        label.set_width_chars(2);
        label.set_max_width_chars(2);
        label.add_css_class("format-fallback-glyph");
        button.set_child(Some(&label));
        button.add_css_class("format-fallback-button");
        button.add_css_class("image-button");
    } else {
        button.set_icon_name(icon_name);
    }
}

fn toolbar_fallback_glyph(icon_name: &str) -> Option<&'static str> {
    match icon_name {
        "format-text-highlight-symbolic" => Some("H"),
        "format-text-superscript-symbolic" => Some("Aˣ"),
        "format-text-subscript-symbolic" => Some("Aₓ"),
        _ => None,
    }
}

/// Native controls whose appearance follows the web surface's selection state.
#[derive(Clone)]
pub(crate) struct RichToolbar {
    toggle_buttons: Vec<(&'static str, gtk::ToggleButton)>,
    heading: gtk::MenuButton,
    heading_choices: Vec<(u8, gtk::ToggleButton)>,
    table: gtk::MenuButton,
    image: gtk::MenuButton,
    image_width_choices: Vec<(Option<u8>, gtk::ToggleButton)>,
}

impl RichToolbar {
    /// Reflects active inline and block formatting without changing editor state.
    pub(crate) fn set_selection_state(&self, selection: &SelectionState) {
        for (name, button) in &self.toggle_buttons {
            button.set_active(selection.active.iter().any(|active| active == name));
        }
        set_context_active(&self.heading, selection.heading != 0);
        for (level, choice) in &self.heading_choices {
            choice.set_active(*level == selection.heading);
        }
        set_context_active(
            &self.table,
            selection.active.iter().any(|active| active == "table"),
        );
        set_context_active(
            &self.image,
            selection.active.iter().any(|active| active == "image"),
        );
        let active_image = selection.active.iter().any(|active| active == "image");
        let selected_width = selection
            .image_width
            .and_then(|width| (width != 0).then_some(width));
        for (width, choice) in &self.image_width_choices {
            choice.set_active(active_image && *width == selected_width);
        }
    }
}

fn set_context_active(menu: &gtk::MenuButton, active: bool) {
    if active {
        menu.add_css_class("context-active");
    } else {
        menu.remove_css_class("context-active");
    }
}

fn append_heading_menu(
    toolbar: &gtk::Box,
    editor: &RichEditor,
) -> (gtk::MenuButton, Vec<(u8, gtk::ToggleButton)>) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-heading-button");
    menu.set_icon_name("format-text-rich-symbolic");
    menu.set_tooltip_text(Some("Text style"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut active_choices = Vec::new();
    for (label, level) in [
        ("Normal text", 0),
        ("Heading 1", 1),
        ("Heading 2", 2),
        ("Heading 3", 3),
        ("Heading 4", 4),
        ("Heading 5", 5),
        ("Heading 6", 6),
    ] {
        let choice = gtk::ToggleButton::with_label(label);
        choice.add_css_class("flat");
        let editor = editor.clone();
        choice.connect_clicked(move |_| editor.command(&EditorCommand::Heading(level)));
        choices.append(&choice);
        active_choices.push((level, choice));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    (menu, active_choices)
}

fn append_table_menu(toolbar: &gtk::Box, editor: &RichEditor) -> gtk::MenuButton {
    let editor = editor.clone();
    formatting::append_table_picker(
        toolbar,
        "format-table-button",
        move |rows, columns, header| {
            // The web adapter inserts at an ordinary caret and resizes when
            // the selection is inside a table, so one grid owns both flows.
            editor.command(&EditorCommand::InsertTable {
                rows,
                columns,
                header,
            });
        },
    )
}

fn show_rich_link_dialog(button: &impl IsA<gtk::Widget>, editor: &RichEditor) {
    let parent = button.root().and_downcast::<gtk::Window>();
    let editor_for_dialog = editor.clone();
    editor.view().evaluate_javascript(
        "JSON.stringify(window.carverEditor.linkContext());",
        None,
        Some("carver-editor:///bridge"),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let context = result
                .ok()
                .map(|value| parse_link_context(&value.to_str()))
                .unwrap_or_default();
            present_rich_link_dialog(parent.as_ref(), &editor_for_dialog, &context);
        },
    );
}

fn present_rich_link_dialog(
    parent: Option<&gtk::Window>,
    editor: &RichEditor,
    context: &LinkContext,
) {
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let text = gtk::Entry::new();
    text.set_placeholder_text(Some("Link text"));
    text.set_text(&context.text);
    let url = gtk::Entry::new();
    url.set_placeholder_text(Some("https://example.com"));
    url.set_input_purpose(gtk::InputPurpose::Url);
    url.set_text(&context.destination);
    fields.append(&gtk::Label::new(Some("Text")));
    fields.append(&text);
    fields.append(&gtk::Label::new(Some("Address")));
    fields.append(&url);
    let dialog = libadwaita::AlertDialog::builder()
        .heading("Insert Link")
        .extra_child(&fields)
        .default_response("insert")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("insert", "Insert")]);
    let editor = editor.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response == "insert" {
            let text = text.text();
            let destination = url.text();
            if !text.trim().is_empty() && !destination.trim().is_empty() {
                editor.command(&EditorCommand::InsertLink {
                    text: text.to_string(),
                    destination: destination.to_string(),
                });
            }
        }
    });
    dialog.present(parent);
}

#[derive(Default, Debug, PartialEq, Eq)]
struct LinkContext {
    text: String,
    destination: String,
}

fn parse_link_context(value: &str) -> LinkContext {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(value) else {
        return LinkContext::default();
    };
    let field = |name| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    LinkContext {
        text: field("text"),
        destination: field("destination"),
    }
}

fn append_image_menu(
    toolbar: &gtk::Box,
    editor: &RichEditor,
    state: &Rc<AppState>,
    toast_overlay: &libadwaita::ToastOverlay,
) -> (gtk::MenuButton, Vec<(Option<u8>, gtk::ToggleButton)>) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-image-size-button");
    menu.set_icon_name("image-x-generic-symbolic");
    menu.set_tooltip_text(Some("Image size"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let insert = gtk::Button::with_label("Insert image…");
    insert.add_css_class("flat");
    let editor_for_insert = editor.clone();
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    insert.connect_clicked(move |button| {
        let editor = editor_for_insert.clone();
        formatting::choose_managed_image(button, &state, &toast_overlay, move |path, alt| {
            editor.insert_image_with_alt(&path, &alt);
        });
    });
    choices.append(&insert);
    choices.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let mut active_choices = Vec::new();
    for (label, width) in [
        ("Original size", None),
        ("25%", Some(25)),
        ("50%", Some(50)),
        ("75%", Some(75)),
        ("100%", Some(100)),
    ] {
        let choice = gtk::ToggleButton::with_label(label);
        choice.add_css_class("flat");
        let editor = editor.clone();
        choice.connect_clicked(move |_| editor.command(&EditorCommand::ImageWidth(width)));
        choices.append(&choice);
        active_choices.push((width, choice));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    (menu, active_choices)
}

pub(crate) fn image_extension(mime_type: &str) -> &str {
    match mime_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "png",
    }
}

fn json(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| String::from("\"\""))
}

fn rgba_css(rgba: &gtk::gdk::RGBA) -> String {
    let (red, green, blue) = rgba_components(rgba);
    format!("#{red:02x}{green:02x}{blue:02x}")
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the explicitly clamped, rounded range is always an sRGB byte"
)]
fn rgba_components(rgba: &gtk::gdk::RGBA) -> (u8, u8, u8) {
    let component = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    (
        component(rgba.red()),
        component(rgba.green()),
        component(rgba.blue()),
    )
}

pub(super) struct SelectionTheme {
    pub(super) accent: String,
    pub(super) background: String,
    pub(super) foreground: &'static str,
}

/// Matches GTK text views: a translucent accent keeps selected text readable
/// while the document foreground remains unchanged in both color schemes.
pub(super) fn selection_theme(dark: bool, accent: &gtk::gdk::RGBA) -> SelectionTheme {
    let (red, green, blue) = rgba_components(accent);
    SelectionTheme {
        accent: rgba_css(accent),
        background: format!("rgb({red} {green} {blue} / 25%)"),
        foreground: if dark { "#f6f5f4" } else { "#242424" },
    }
}

/// Builds the sandboxed editor shell using the configured image source policy.
fn editor_document(allow_remote_images: bool) -> String {
    let image_sources = if allow_remote_images {
        "data: https: http: carver-asset: blob:"
    } else {
        "data: carver-asset: blob:"
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; img-src {image_sources}; connect-src blob:; media-src 'none'; frame-src 'none'\"><style>{EDITOR_STYLESHEET}</style></head><body><div id=\"editor\"></div></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        LinkContext, editor_document, parse_link_context, selection_theme, toolbar_fallback_glyph,
    };

    #[test]
    fn editor_document_allows_remote_images_when_configured() {
        assert!(editor_document(true).contains("img-src data: https: http: carver-asset: blob:"));
    }

    #[test]
    fn editor_document_keeps_remote_images_blocked_when_disabled() {
        assert!(editor_document(false).contains("img-src data: carver-asset: blob:"));
    }

    #[test]
    fn selection_theme_preserves_dark_document_text() {
        let accent = gtk::gdk::RGBA::new(0.102, 0.373, 0.706, 1.0);
        let theme = selection_theme(true, &accent);
        assert_eq!(theme.foreground, "#f6f5f4");
    }

    #[test]
    fn selection_theme_uses_a_translucent_accent_background() {
        let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
        let theme = selection_theme(false, &accent);
        assert_eq!(theme.background, "rgb(53 142 69 / 25%)");
    }

    #[test]
    fn toolbar_fallback_glyphs_cover_unavailable_formatting_icons() {
        assert_eq!(
            toolbar_fallback_glyph("format-text-superscript-symbolic"),
            Some("Aˣ")
        );
    }

    #[test]
    fn link_context_keeps_the_dialog_fields_from_the_editor() {
        assert_eq!(
            parse_link_context(r#"{"text":"Carve","destination":"https://markup-carve.dev"}"#),
            LinkContext {
                text: String::from("Carve"),
                destination: String::from("https://markup-carve.dev"),
            }
        );
    }
}
