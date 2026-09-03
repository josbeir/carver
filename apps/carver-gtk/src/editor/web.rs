//! WebKit-backed Carve editing surface and its native host bridge.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use carver_editor_protocol::{EditorCommand, EditorEvent, SelectionState};
use gtk::prelude::*;
use webkit6::prelude::*;

use crate::controller::AppState;

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
        state: &Rc<AppState>,
        source_buffer: &gtk::TextBuffer,
        toast_overlay: &libadwaita::ToastOverlay,
    ) -> Self {
        let context = webkit6::WebContext::new();
        super::preview::install_editor_asset_scheme(&context, state.assets_dir.clone());
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
        editor.connect_messages(&manager, state, source_buffer, toast_overlay);
        editor.connect_load_lifecycle();
        editor
            .view
            .load_html(&editor_document(), Some("carver-asset:///"));
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
        };
        self.evaluate(&format!(
            "window.carverEditor.command({}, {});",
            json(name),
            argument
        ));
    }

    /// Applies GNOME's color scheme and system accent without reloading the document.
    pub(crate) fn set_theme(&self, dark: bool, accent: &gtk::gdk::RGBA) {
        let accent = rgba_css(accent);
        let foreground = selection_foreground(accent.as_str());
        self.evaluate(&format!(
            "window.carverEditor.setTheme({dark}, {}, {});",
            json(&accent),
            json(foreground)
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
        state: &Rc<AppState>,
        source_buffer: &gtk::TextBuffer,
        toast_overlay: &libadwaita::ToastOverlay,
    ) {
        let editor = self.clone();
        let state = Rc::clone(state);
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
                    let Some(note) = state.current_note.borrow().clone() else {
                        return;
                    };
                    let Ok(bytes) = STANDARD.decode(data) else {
                        toast_overlay
                            .add_toast(libadwaita::Toast::new("Could not read pasted image"));
                        return;
                    };
                    let extension = image_extension(&mime_type).to_owned();
                    let client = state.client.clone();
                    let editor = editor.clone();
                    let toast_overlay = toast_overlay.clone();
                    glib::spawn_future_local(async move {
                        match client.store_asset_async(note.id, extension, bytes).await {
                            Ok(path) => editor.insert_image(&path),
                            Err(error) => toast_overlay.add_toast(libadwaita::Toast::new(
                                &format!("Could not store pasted image: {error}"),
                            )),
                        }
                    });
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

    fn insert_image(&self, path: &str) {
        self.evaluate(&format!("window.carverEditor.insertImage({});", json(path)));
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
pub(crate) fn append_controls(toolbar: &gtk::Box, editor: &RichEditor) -> RichToolbar {
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
        button.set_icon_name(icon);
        button.set_widget_name(name);
        button.set_tooltip_text(Some(tooltip));
        button.add_css_class("flat");
        let editor = editor.clone();
        let command = EditorCommand::Named(command.to_owned());
        button.connect_clicked(move |_| editor.command(&command));
        toolbar.append(&button);
        toggle_buttons.push((active_name, button));
    }
    let (heading, heading_choices) = append_heading_menu(toolbar, editor);
    let table = append_table_menu(toolbar, editor);
    let (image, image_width_choices) = append_image_menu(toolbar, editor);
    RichToolbar {
        toggle_buttons,
        heading,
        heading_choices,
        table,
        image,
        image_width_choices,
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
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-table-button");
    menu.set_icon_name("view-grid-symbolic");
    menu.set_tooltip_text(Some("Table"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let commands = [
        (
            "Insert 3 × 3 table",
            EditorCommand::InsertTable {
                rows: 3,
                columns: 3,
                header: true,
            },
        ),
        (
            "Add row above",
            EditorCommand::Named(String::from("add-row-before")),
        ),
        (
            "Add row below",
            EditorCommand::Named(String::from("add-row-after")),
        ),
        (
            "Delete row",
            EditorCommand::Named(String::from("delete-row")),
        ),
        (
            "Add column before",
            EditorCommand::Named(String::from("add-column-before")),
        ),
        (
            "Add column after",
            EditorCommand::Named(String::from("add-column-after")),
        ),
        (
            "Delete column",
            EditorCommand::Named(String::from("delete-column")),
        ),
        (
            "Delete table",
            EditorCommand::Named(String::from("delete-table")),
        ),
    ];
    for (label, command) in commands {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let editor = editor.clone();
        choice.connect_clicked(move |_| editor.command(&command));
        choices.append(&choice);
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    menu
}

fn append_image_menu(
    toolbar: &gtk::Box,
    editor: &RichEditor,
) -> (gtk::MenuButton, Vec<(Option<u8>, gtk::ToggleButton)>) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-image-size-button");
    menu.set_icon_name("image-x-generic-symbolic");
    menu.set_tooltip_text(Some("Image size"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
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

fn image_extension(mime_type: &str) -> &str {
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
    let component = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        component(rgba.red()),
        component(rgba.green()),
        component(rgba.blue())
    )
}

fn selection_foreground(accent: &str) -> &'static str {
    let Some((red, green, blue)) = parse_hex_rgb(accent) else {
        return "#ffffff";
    };
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
    let black_contrast = (luminance + 0.05) / 0.05;
    let white_contrast = 1.05 / (luminance + 0.05);
    if black_contrast >= white_contrast {
        "#000000"
    } else {
        "#ffffff"
    }
}

fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.strip_prefix('#')?;
    (value.len() == 6).then_some(()).and_then(|_| {
        Some((
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
        ))
    })
}

fn editor_document() -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; img-src data: carver-asset: blob:; connect-src blob:; media-src 'none'; frame-src 'none'\"><style>{EDITOR_STYLESHEET}</style></head><body><div id=\"editor\"></div></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::selection_foreground;

    #[test]
    fn selection_foreground_uses_the_highest_contrast_text_color() {
        assert_eq!(selection_foreground("#1a5fb4"), "#ffffff");
        assert_eq!(selection_foreground("#358e45"), "#000000");
        assert_eq!(selection_foreground("#f6d32d"), "#000000");
    }
}
