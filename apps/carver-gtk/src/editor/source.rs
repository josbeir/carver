//! GtkSourceView-backed canonical Carve source editing.

use std::{
    cell::RefCell,
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::gio::prelude::*;
use gtk::prelude::*;
use sourceview5::prelude::*;
use thiserror::Error;

const CARVE_LANGUAGE: &str = include_str!("../../resources/source-syntax/carve.lang");
const CARVE_LIGHT_STYLE: &str = include_str!("../../resources/source-syntax/carve-light.xml");
const CARVE_DARK_STYLE: &str = include_str!("../../resources/source-syntax/carve-dark.xml");
const SYSTEM_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const SYSTEM_MONOSPACE_FONT_KEY: &str = "monospace-font-name";
const FALLBACK_MONOSPACE_FONT: &str = "Monospace 11";

/// Error returned while installing or loading Carver's bundled source syntax assets.
#[derive(Debug, Error)]
pub(crate) enum SourceSyntaxError {
    /// A bundled syntax asset could not be written to XDG data.
    #[error("could not install Carve source syntax assets: {0}")]
    Io(#[from] io::Error),
    /// `GtkSourceView` could not load a bundled asset after it was installed.
    #[error("GtkSourceView could not load the bundled {asset} asset")]
    MissingAsset {
        /// User-visible name of the unavailable asset.
        asset: &'static str,
    },
    /// `GtkSourceView` only accepts UTF-8 paths for syntax search directories.
    #[error("source syntax directory is not valid UTF-8: {0}")]
    NonUtf8Directory(PathBuf),
}

/// Installs the syntax assets embedded in the application into Carver-managed XDG data.
///
/// Existing files with identical content are left untouched, so the operation is
/// safe to call during every application startup.
pub(crate) fn install_syntax_assets(data_dir: &Path) -> Result<PathBuf, SourceSyntaxError> {
    let directory = data_dir.join("source-syntax");
    fs::create_dir_all(&directory)?;
    for (name, contents) in [
        ("carve.lang", CARVE_LANGUAGE),
        ("carve-light.xml", CARVE_LIGHT_STYLE),
        ("carve-dark.xml", CARVE_DARK_STYLE),
    ] {
        write_asset(&directory, name, contents)?;
    }
    Ok(directory)
}

fn write_asset(directory: &Path, name: &str, contents: &str) -> Result<(), io::Error> {
    let destination = directory.join(name);
    if fs::read_to_string(&destination).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    let temporary = destination.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, destination)?;
    Ok(())
}

/// The source editor projection and its GtkSourceView-only configuration.
#[derive(Clone)]
pub(crate) struct SourceEditor {
    buffer: sourceview5::Buffer,
    view: sourceview5::View,
    light_style: sourceview5::StyleScheme,
    dark_style: sourceview5::StyleScheme,
    font_provider: gtk::CssProvider,
    custom_font: Rc<RefCell<Option<String>>>,
    system_font: Rc<RefCell<String>>,
    // The settings object owns the changed-signal registration for this editor.
    _system_font_settings: Option<gtk::gio::Settings>,
}

impl SourceEditor {
    /// Creates a Carve-configured source view from installed syntax assets.
    pub(crate) fn new(syntax_dir: &Path) -> Result<Self, SourceSyntaxError> {
        let syntax_dir = syntax_dir
            .to_str()
            .ok_or_else(|| SourceSyntaxError::NonUtf8Directory(syntax_dir.to_owned()))?;
        let language_manager = sourceview5::LanguageManager::new();
        let default_language_paths = language_manager.search_path();
        let mut language_paths = Vec::with_capacity(default_language_paths.len() + 1);
        language_paths.push(syntax_dir);
        language_paths.extend(default_language_paths.iter().map(glib::GString::as_str));
        language_manager.set_search_path(&language_paths);
        let language =
            language_manager
                .language("carve")
                .ok_or(SourceSyntaxError::MissingAsset {
                    asset: "Carve grammar",
                })?;
        let style_manager = sourceview5::StyleSchemeManager::new();
        style_manager.prepend_search_path(syntax_dir);
        let light_style =
            style_manager
                .scheme("carve-light")
                .ok_or(SourceSyntaxError::MissingAsset {
                    asset: "light Carve style scheme",
                })?;
        let dark_style =
            style_manager
                .scheme("carve-dark")
                .ok_or(SourceSyntaxError::MissingAsset {
                    asset: "dark Carve style scheme",
                })?;
        let buffer = sourceview5::Buffer::builder()
            .language(&language)
            .style_scheme(&light_style)
            .highlight_syntax(true)
            .build();
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_widget_name("source-editor");
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        // Keep relaxed source spacing without CSS line-height changing Pango glyph metrics.
        view.set_pixels_above_lines(3);
        view.set_pixels_below_lines(3);
        view.set_top_margin(24);
        view.set_bottom_margin(24);
        view.set_left_margin(24);
        view.set_right_margin(24);
        view.set_show_line_numbers(false);
        view.set_highlight_current_line(false);
        let font_provider = gtk::CssProvider::new();
        install_source_font_provider(&view, &font_provider);
        let custom_font = Rc::new(RefCell::new(None));
        let system_font = Rc::new(RefCell::new(system_monospace_font_description()));
        font_provider.load_from_string(&source_font_css(&system_font.borrow()));
        let system_font_settings = desktop_font_settings();
        if let Some(settings) = system_font_settings.as_ref() {
            let font_provider = font_provider.clone();
            let custom_font = Rc::clone(&custom_font);
            let system_font = Rc::clone(&system_font);
            settings.connect_changed(Some(SYSTEM_MONOSPACE_FONT_KEY), move |settings, _| {
                let font = system_monospace_font_from_settings(Some(settings));
                system_font.replace(font.clone());
                if custom_font.borrow().is_none() {
                    font_provider.load_from_string(&source_font_css(&font));
                }
            });
        }
        Ok(Self {
            buffer,
            view,
            light_style,
            dark_style,
            font_provider,
            custom_font,
            system_font,
            _system_font_settings: system_font_settings,
        })
    }

    /// Returns the GTK buffer shared with the canonical editor projections.
    pub(crate) fn buffer(&self) -> &gtk::TextBuffer {
        self.buffer.upcast_ref()
    }

    /// Returns the source-editor widget.
    pub(crate) fn view(&self) -> &sourceview5::View {
        &self.view
    }

    /// Applies source-editor preferences from the immutable MVU model snapshot.
    pub(crate) fn render_preferences(
        &self,
        preferences: &crate::mvu::SourceEditorPreferences,
        dark: bool,
    ) {
        self.view
            .set_show_line_numbers(preferences.show_line_numbers);
        self.view
            .set_highlight_current_line(preferences.highlight_current_line);
        self.buffer
            .set_highlight_syntax(preferences.syntax_highlighting);
        self.buffer.set_style_scheme(Some(if dark {
            &self.dark_style
        } else {
            &self.light_style
        }));
        let custom_font = preferences.font.clone();
        if self.custom_font.borrow().as_ref() != custom_font.as_ref() {
            self.custom_font.replace(custom_font);
            let font = self
                .custom_font
                .borrow()
                .clone()
                .unwrap_or_else(|| self.system_font.borrow().clone());
            self.font_provider.load_from_string(&source_font_css(&font));
        }
    }
}

/// Returns the desktop source-font preference, with a stable cross-desktop fallback.
pub(crate) fn system_monospace_font_description() -> String {
    system_monospace_font_from_settings(desktop_font_settings().as_ref())
}

/// Canonicalizes a Pango font description accepted by the source font chooser.
///
/// Only the family and point size are preserved: source syntax tags remain responsible for
/// inline bold and italic styling.
pub(crate) fn normalize_source_font_description(description: &str) -> Option<String> {
    let description = gtk::pango::FontDescription::from_string(description);
    let family = description.family()?;
    let points = description.size();
    if family.is_empty() || points <= 0 || description.is_size_absolute() {
        return None;
    }
    let points = f64::from(points) / f64::from(gtk::pango::SCALE);
    Some(format!("{family} {points}"))
}

fn desktop_font_settings() -> Option<gtk::gio::Settings> {
    let schema = gtk::gio::SettingsSchemaSource::default()?
        .lookup(SYSTEM_INTERFACE_SCHEMA, true)
        .filter(|schema| schema.has_key(SYSTEM_MONOSPACE_FONT_KEY))?;
    Some(gtk::gio::Settings::new_full(
        &schema,
        None::<&gtk::gio::SettingsBackend>,
        None,
    ))
}

fn system_monospace_font_from_settings(settings: Option<&gtk::gio::Settings>) -> String {
    settings
        .map(|settings| settings.string(SYSTEM_MONOSPACE_FONT_KEY).to_string())
        .and_then(|font| normalize_source_font_description(&font))
        .unwrap_or_else(|| FALLBACK_MONOSPACE_FONT.to_owned())
}

fn source_font_css(description: &str) -> String {
    let description = normalize_source_font_description(description)
        .unwrap_or_else(|| FALLBACK_MONOSPACE_FONT.to_owned());
    let description = gtk::pango::FontDescription::from_string(&description);
    let family = description.family().unwrap_or_else(|| "Monospace".into());
    let points = f64::from(description.size()) / f64::from(gtk::pango::SCALE);
    format!(
        "#source-editor {{ font-family: \"{}\"; font-size: {points}pt; }}",
        escape_css_string(&family)
    )
}

fn escape_css_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn install_source_font_provider(view: &sourceview5::View, provider: &gtk::CssProvider) {
    // CONTEXT: GtkSourceView font CSS must be isolated to this editor instance;
    // the modern display-wide provider would affect every source editor window.
    #[expect(
        deprecated,
        reason = "GTK exposes no non-global CSS-provider replacement for a single widget"
    )]
    view.style_context()
        .add_provider(provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
}

/// Returns the canonical Carve source without interpretation.
pub(crate) fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
}

#[cfg(test)]
mod tests;
