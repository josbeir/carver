//! WebKit-backed Carve editing surface and its native host bridge.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use carver_config::DocumentWidth;
use carver_editor_protocol::{EditorCommand, EditorEvent, SelectionState};
use gtk::prelude::*;
use libadwaita::prelude::*;
use webkit6::prelude::*;

use crate::mvu::{AppDispatcher, AppMsg, DocumentPreferences, EditorMsg};

use super::focus::EditorFocusRestorer;

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
    document_session: Rc<Cell<Option<crate::mvu::EditorSessionId>>>,
    ready: Rc<Cell<bool>>,
    revision: Rc<Cell<u64>>,
    navigation_epoch: Rc<Cell<u64>>,
    canonical_source: Rc<RefCell<Rc<str>>>,
    pending_source: Rc<RefCell<Option<(u64, String)>>>,
    current_theme: Rc<RefCell<Option<EditorTheme>>>,
    current_appearance: Rc<RefCell<Option<DocumentAppearance>>>,
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
        settings.set_enable_developer_extras(cfg!(debug_assertions));
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
            document_session: Rc::new(Cell::new(None)),
            ready: Rc::new(Cell::new(false)),
            revision: Rc::new(Cell::new(0)),
            navigation_epoch: Rc::new(Cell::new(0)),
            canonical_source: Rc::new(RefCell::new(Rc::from(""))),
            pending_source: Rc::new(RefCell::new(None)),
            current_theme: Rc::new(RefCell::new(None)),
            current_appearance: Rc::new(RefCell::new(None)),
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

    /// Returns the document identity rendered by this editor.
    pub(crate) fn document_session(&self) -> Option<crate::mvu::EditorSessionId> {
        self.document_session.get()
    }

    /// Associates projection events with the current MVU document session.
    pub(crate) fn set_document_session(&self, session: crate::mvu::EditorSessionId) {
        self.document_session.set(Some(session));
    }

    /// Loads a new document into the rich editor without marking it dirty.
    pub(crate) fn load_source(&self, source: &str) {
        let next_session = self.session.get().saturating_add(1);
        self.session.set(next_session);
        self.revision.set(0);
        self.navigation_epoch.set(0);
        self.canonical_source.replace(Rc::from(source));
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

    /// Focuses a document occurrence without changing its source.
    pub(crate) fn focus_document_target(&self, target: &carver_editor_protocol::DocumentTarget) {
        if !self.ready.get() || self.pending_source.borrow().is_some() {
            return;
        }
        let Ok(target) = serde_json::to_string(target) else {
            return;
        };
        let epoch = self.navigation_epoch.get().wrapping_add(1);
        self.navigation_epoch.set(epoch);
        let session = self.session.get();
        let revision = self.revision.get();
        let editor = self.clone();
        // Restore native focus only after the projection has selected its target. Focusing
        // WebKit first can report the old caret while this asynchronous command is queued.
        self.view.evaluate_javascript(
            &format!(
                "window.carverEditor.focusDocumentTarget({target}, {session}, {revision}, {epoch});"
            ),
            None,
            Some("carver-editor:///bridge"),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                if editor.session.get() != session
                    || editor.revision.get() != revision
                    || editor.navigation_epoch.get() != epoch
                    || !editor.view.is_mapped()
                {
                    return;
                }
                if result.is_ok_and(|value| value.to_boolean()) {
                    editor.view.grab_focus();
                    if let Some(root) = editor.view.root() {
                        root.set_focus(Some(&editor.view));
                    }
                }
            },
        );
    }

    /// Opens the Rich editor's contextual link dialog from the shared toolbar.
    pub(crate) fn show_link_dialog(
        &self,
        anchor: &gtk::Widget,
        dispatcher: &AppDispatcher,
        focus: &EditorFocusRestorer,
    ) {
        show_rich_link_dialog(anchor, self, dispatcher, focus);
    }

    /// Applies GNOME's resolved editor colors without reloading the document.
    pub(super) fn set_theme(&self, theme: &EditorTheme) {
        self.current_theme.replace(Some(theme.clone()));
        self.apply_theme();
    }

    /// Applies formatted-document typography and measure without reloading the document.
    pub(super) fn set_appearance(&self, appearance: &DocumentAppearance) {
        self.current_appearance.replace(Some(appearance.clone()));
        self.apply_appearance();
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

    fn accepts_selection(&self, session: u64, selection: &SelectionState) -> bool {
        session == self.session.get()
            && selection.revision == self.revision.get()
            && selection.navigation_epoch == self.navigation_epoch.get()
    }

    fn connect_messages(
        &self,
        manager: &webkit6::UserContentManager,
        dispatcher: &AppDispatcher,
        _source_buffer: &gtk::TextBuffer,
        toast_overlay: &libadwaita::ToastOverlay,
    ) {
        let editor = self.clone();
        let dispatcher = dispatcher.clone();
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
                    editor.apply_theme();
                    editor.apply_appearance();
                }
                EditorEvent::Changed {
                    session,
                    source,
                    revision,
                } if session == editor.session.get() => {
                    editor.revision.set(revision);
                    editor.canonical_source.replace(Rc::from(source.as_str()));
                    for message in rich_source_change_messages(source) {
                        let _ = dispatcher.dispatch(message);
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
                EditorEvent::CopySelection { session, source }
                    if session == editor.session.get() =>
                {
                    if let Some(session) = editor.document_session.get() {
                        let _ = dispatcher.dispatch(AppMsg::Editor(
                            EditorMsg::CopySelectionRequested { session, source },
                        ));
                    }
                }
                EditorEvent::Selection {
                    session,
                    state: selection,
                } if editor.accepts_selection(session, &selection) => {
                    if let Some(session) = editor.document_session.get() {
                        let source = Rc::clone(&editor.canonical_source.borrow());
                        let _ = dispatcher.dispatch(AppMsg::Editor(
                            EditorMsg::DocumentSelectionChanged {
                                session,
                                mode: carver_config::EditorMode::Rich,
                                media: selection.media.clone(),
                                heading: selection.heading_occurrence,
                                source,
                            },
                        ));
                    }
                    if let Some(handler) = selection_handler.borrow().as_ref() {
                        handler(selection);
                    }
                }
                EditorEvent::Selection { .. }
                | EditorEvent::Changed { .. }
                | EditorEvent::Unsupported { .. }
                | EditorEvent::CopySelection { .. }
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

    fn apply_theme(&self) {
        if !self.ready.get() {
            return;
        }
        let Some(theme) = self.current_theme.borrow().clone() else {
            return;
        };
        self.evaluate(&theme_javascript(&theme));
    }

    fn apply_appearance(&self) {
        if !self.ready.get() {
            return;
        }
        let Some(appearance) = self.current_appearance.borrow().clone() else {
            return;
        };
        self.evaluate(&appearance_javascript(&appearance));
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

fn show_rich_link_dialog(
    button: &impl IsA<gtk::Widget>,
    editor: &RichEditor,
    dispatcher: &AppDispatcher,
    focus: &EditorFocusRestorer,
) {
    let parent = button.root().and_downcast::<gtk::Window>();
    let dispatcher = dispatcher.clone();
    let focus_for_dialog = focus.clone();
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
            present_rich_link_dialog(parent.as_ref(), &dispatcher, &context, &focus_for_dialog);
        },
    );
}

fn present_rich_link_dialog(
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    context: &LinkContext,
    focus: &EditorFocusRestorer,
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
    let dispatcher = dispatcher.clone();
    let focus = focus.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response == "insert" {
            let text = text.text();
            let destination = url.text();
            if !text.trim().is_empty() && !destination.trim().is_empty() {
                let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ApplyRichCommand(
                    EditorCommand::InsertLink {
                        text: text.to_string(),
                        destination: destination.to_string(),
                    },
                )));
            }
        }
        focus.restore_later();
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

#[derive(Clone)]
pub(super) struct SelectionTheme {
    pub(super) accent: String,
    pub(super) background: String,
    pub(super) foreground: String,
}

/// Matches GTK text views: a translucent accent keeps selected text readable
/// while the document foreground remains unchanged in both color schemes.
#[cfg(test)]
pub(super) fn selection_theme(dark: bool, accent: &gtk::gdk::RGBA) -> SelectionTheme {
    selection_theme_with_foreground(accent, default_document_foreground(dark))
}

fn selection_theme_with_foreground(accent: &gtk::gdk::RGBA, foreground: &str) -> SelectionTheme {
    let (red, green, blue) = rgba_components(accent);
    SelectionTheme {
        accent: rgba_css(accent),
        background: format!("rgb({red} {green} {blue} / 25%)"),
        foreground: foreground.to_owned(),
    }
}

/// Color-scheme and selection colors transferred from Adwaita into `WebKit`.
#[derive(Clone)]
pub(super) struct EditorTheme {
    pub(super) dark: bool,
    pub(super) selection: SelectionTheme,
}

/// Typography and readable measure shared by formatted editor projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DocumentAppearance {
    css: String,
}

/// Resolves document preferences into a safe stylesheet declaration list.
pub(super) fn document_appearance(preferences: &DocumentPreferences) -> DocumentAppearance {
    let font = preferences
        .font
        .as_deref()
        .and_then(super::normalize_document_font_description)
        .unwrap_or_else(super::system_document_font_description);
    let (width, outer_width) = match preferences.width {
        DocumentWidth::Narrow => ("60ch", "calc(60ch + 48px)"),
        DocumentWidth::Comfortable => ("80ch", "calc(80ch + 48px)"),
        DocumentWidth::Wide => ("100ch", "calc(100ch + 48px)"),
        DocumentWidth::Full => ("none", "100%"),
    };
    DocumentAppearance {
        css: format!(
            "{} --document-line-height: {}; --document-content-width: {width}; --document-content-outer-width: {outer_width};",
            super::document_font_css(&font),
            f64::from(preferences.line_height_percent.clamp(100, 250)) / 100.0,
        ),
    }
}

/// Returns CSS declarations for preview document roots.
pub(super) fn appearance_style(appearance: &DocumentAppearance) -> &str {
    &appearance.css
}

/// Builds the `WebKit` palette from Adwaita's selected color scheme.
///
/// The bundled stylesheet owns the canonical Adwaita document surface for the
/// active scheme. Passing a native background across the `WebKit` boundary is
/// unreliable because GTK can update it after the style-manager notification.
pub(super) fn editor_theme(dark: bool, accent: &gtk::gdk::RGBA) -> EditorTheme {
    EditorTheme {
        dark,
        selection: selection_theme_with_foreground(accent, default_document_foreground(dark)),
    }
}

fn default_document_foreground(dark: bool) -> &'static str {
    if dark { "#ffffff" } else { "#333334" }
}

fn theme_javascript(theme: &EditorTheme) -> String {
    format!(
        "window.carverEditor.setTheme({}, {}, {}, {});",
        theme.dark,
        json(&theme.selection.accent),
        json(&theme.selection.background),
        json(&theme.selection.foreground),
    )
}

fn appearance_javascript(appearance: &DocumentAppearance) -> String {
    format!(
        "window.carverEditor.setAppearance({});",
        json(&appearance.css)
    )
}

/// Builds the sandboxed editor shell using the configured image source policy.
fn editor_document(allow_remote_images: bool) -> String {
    let image_sources = if allow_remote_images {
        "data: https: http: carver-asset: blob:"
    } else {
        "data: carver-asset: blob:"
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'none'; img-src {image_sources}; connect-src blob:; media-src 'none'; frame-src 'none'\"><style>{EDITOR_STYLESHEET}</style><style id=\"editor-runtime-styles\"></style></head><body><div id=\"editor\"></div></body></html>"
    )
}

/// Translates one rich-editor mutation into the same model notifications as a source edit.
fn rich_source_change_messages(source: String) -> [AppMsg; 2] {
    [
        AppMsg::Editor(EditorMsg::SourceChanged(source)),
        AppMsg::Editor(EditorMsg::AutosaveRequested),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        LinkContext, appearance_javascript, document_appearance, editor_document, editor_theme,
        parse_link_context, rich_source_change_messages, selection_theme, theme_javascript,
    };
    use crate::mvu::{AppMsg, DocumentPreferences, EditorMsg};

    #[test]
    fn editor_document_allows_remote_images_when_configured() {
        assert!(editor_document(true).contains("img-src data: https: http: carver-asset: blob:"));
    }

    #[test]
    fn editor_document_keeps_remote_images_blocked_when_disabled() {
        assert!(editor_document(false).contains("img-src data: carver-asset: blob:"));
    }

    #[test]
    fn editor_document_should_keep_runtime_styles_in_the_head() {
        let document = editor_document(false);

        assert!(document.contains("<style id=\"editor-runtime-styles\"></style>"));
        assert!(!document.contains("<html style="));
    }

    #[test]
    fn document_appearance_should_transfer_font_spacing_and_measure_to_webkit() {
        let appearance = document_appearance(&DocumentPreferences {
            font: Some("Cantarell Bold Italic 14".to_owned()),
            line_height_percent: 175,
            width: carver_config::DocumentWidth::Wide,
        });

        let script = appearance_javascript(&appearance);
        assert!(script.contains("--document-font-family: \\\"Cantarell\\\""));
        assert!(script.contains("--document-line-height: 1.75"));
        assert!(script.contains("--document-content-width: 100ch"));
        assert!(script.contains("--document-content-outer-width: calc(100ch + 48px)"));
    }

    #[test]
    fn document_appearance_should_support_every_reading_measure() {
        for (width, measure, outer_measure) in [
            (
                carver_config::DocumentWidth::Narrow,
                "60ch",
                "calc(60ch + 48px)",
            ),
            (
                carver_config::DocumentWidth::Comfortable,
                "80ch",
                "calc(80ch + 48px)",
            ),
            (
                carver_config::DocumentWidth::Wide,
                "100ch",
                "calc(100ch + 48px)",
            ),
            (carver_config::DocumentWidth::Full, "none", "100%"),
        ] {
            let appearance = document_appearance(&DocumentPreferences {
                font: None,
                line_height_percent: 155,
                width,
            });

            assert!(
                appearance_javascript(&appearance)
                    .contains(&format!("--document-content-width: {measure}"))
            );
            assert!(
                appearance_javascript(&appearance)
                    .contains(&format!("--document-content-outer-width: {outer_measure}"))
            );
        }
    }

    #[test]
    fn rich_source_change_should_schedule_an_autosave_notification() {
        let [source_changed, autosave] = rich_source_change_messages(String::from("Changed"));

        assert!(matches!(
            source_changed,
            AppMsg::Editor(EditorMsg::SourceChanged(source)) if source == "Changed"
        ));
        assert!(matches!(
            autosave,
            AppMsg::Editor(EditorMsg::AutosaveRequested)
        ));
    }

    #[test]
    fn selection_theme_preserves_dark_document_text() {
        let accent = gtk::gdk::RGBA::new(0.102, 0.373, 0.706, 1.0);
        let theme = selection_theme(true, &accent);
        assert_eq!(theme.foreground, "#ffffff");
    }

    #[test]
    fn selection_theme_uses_a_translucent_accent_background() {
        let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
        let theme = selection_theme(false, &accent);
        assert_eq!(theme.background, "rgb(53 142 69 / 25%)");
    }

    #[test]
    fn editor_theme_uses_the_adwaita_dark_selection_foreground() {
        let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
        let theme = editor_theme(true, &accent);
        assert_eq!(theme.selection.foreground, "#ffffff");
    }

    #[test]
    fn theme_javascript_applies_the_selected_adwaita_scheme_to_webkit() {
        let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
        let theme = editor_theme(true, &accent);
        assert!(theme_javascript(&theme).contains("setTheme(true"));
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
