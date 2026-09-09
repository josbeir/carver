//! GtkSourceView-backed canonical Carve source editing.

use std::{
    cell::RefCell,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

use atomic_write_file::AtomicWriteFile;
use carver_config::SourceSyntaxStyle;
use cssparser::ToCss;
use gtk::gio::prelude::*;
use gtk::prelude::*;
use sourceview5::prelude::*;
use thiserror::Error;

const CARVE_LANGUAGE: &str = include_str!("../../../resources/source-syntax/carve.lang");
const CARVE_LIGHT_STYLE: &str = include_str!("../../../resources/source-syntax/carve-light.xml");
const CARVE_DARK_STYLE: &str = include_str!("../../../resources/source-syntax/carve-dark.xml");
const CARVE_WRITING_FOCUS_LIGHT_STYLE: &str =
    include_str!("../../../resources/source-syntax/carve-writing-focus-light.xml");
const CARVE_WRITING_FOCUS_DARK_STYLE: &str =
    include_str!("../../../resources/source-syntax/carve-writing-focus-dark.xml");
const SYSTEM_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const SYSTEM_DOCUMENT_FONT_KEY: &str = "document-font-name";
const SYSTEM_MONOSPACE_FONT_KEY: &str = "monospace-font-name";
const FALLBACK_DOCUMENT_FONT: &str = "Sans 12";
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
        (
            "carve-writing-focus-light.xml",
            CARVE_WRITING_FOCUS_LIGHT_STYLE,
        ),
        (
            "carve-writing-focus-dark.xml",
            CARVE_WRITING_FOCUS_DARK_STYLE,
        ),
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
    let mut file = AtomicWriteFile::open(&destination)?;
    file.write_all(contents.as_bytes())?;
    file.commit()?;
    Ok(())
}

/// The source editor projection and its GtkSourceView-only configuration.
#[derive(Clone)]
pub(crate) struct SourceEditor {
    buffer: sourceview5::Buffer,
    view: sourceview5::View,
    light_style: sourceview5::StyleScheme,
    dark_style: sourceview5::StyleScheme,
    writing_focus_light_style: sourceview5::StyleScheme,
    writing_focus_dark_style: sourceview5::StyleScheme,
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
            load_style_scheme(&style_manager, "carve-light", "light Carve style scheme")?;
        let dark_style =
            load_style_scheme(&style_manager, "carve-dark", "dark Carve style scheme")?;
        let writing_focus_light_style = load_style_scheme(
            &style_manager,
            "carve-writing-focus-light",
            "light writing-focus Carve style scheme",
        )?;
        let writing_focus_dark_style = load_style_scheme(
            &style_manager,
            "carve-writing-focus-dark",
            "dark writing-focus Carve style scheme",
        )?;
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
        let system_font_settings = desktop_font_settings(SYSTEM_MONOSPACE_FONT_KEY);
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
            writing_focus_light_style,
            writing_focus_dark_style,
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

    /// Returns the native buffer required by `GtkSourceView`'s search context.
    pub(crate) fn native_buffer(&self) -> &sourceview5::Buffer {
        &self.buffer
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
        let (highlight_syntax, style_scheme) = match preferences.syntax_style {
            SourceSyntaxStyle::Detailed => (
                true,
                if dark {
                    &self.dark_style
                } else {
                    &self.light_style
                },
            ),
            SourceSyntaxStyle::WritingFocus => (
                true,
                if dark {
                    &self.writing_focus_dark_style
                } else {
                    &self.writing_focus_light_style
                },
            ),
            SourceSyntaxStyle::None => (
                false,
                if dark {
                    &self.dark_style
                } else {
                    &self.light_style
                },
            ),
        };
        self.buffer.set_highlight_syntax(highlight_syntax);
        self.buffer.set_style_scheme(Some(style_scheme));
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

fn load_style_scheme(
    manager: &sourceview5::StyleSchemeManager,
    id: &str,
    asset: &'static str,
) -> Result<sourceview5::StyleScheme, SourceSyntaxError> {
    manager
        .scheme(id)
        .ok_or(SourceSyntaxError::MissingAsset { asset })
}

/// Returns the desktop source-font preference, with a stable cross-desktop fallback.
pub(crate) fn system_monospace_font_description() -> String {
    system_monospace_font_from_settings(desktop_font_settings(SYSTEM_MONOSPACE_FONT_KEY).as_ref())
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

/// Returns the desktop document-font preference with a cross-desktop fallback.
pub(crate) fn system_document_font_description() -> String {
    system_document_font_from_settings(desktop_font_settings(SYSTEM_DOCUMENT_FONT_KEY).as_ref())
}

/// Canonicalizes a Pango font description for formatted editing and previews.
///
/// Unlike source mode, the selected face is retained because document markup
/// can still layer semantic emphasis on top of the user's base font choice.
pub(crate) fn normalize_document_font_description(description: &str) -> Option<String> {
    let description = gtk::pango::FontDescription::from_string(description);
    let family = description.family()?;
    if family.is_empty() || description.size() <= 0 || description.is_size_absolute() {
        return None;
    }
    Some(description.to_string())
}

pub(crate) fn document_font_css(description: &str) -> String {
    let description = normalize_document_font_description(description)
        .unwrap_or_else(|| FALLBACK_DOCUMENT_FONT.to_owned());
    let description = gtk::pango::FontDescription::from_string(&description);
    let family = description.family().unwrap_or_else(|| "Sans".into());
    let points = f64::from(description.size()) / f64::from(gtk::pango::SCALE);
    let variations = description.variations();
    format!(
        "--document-font-family: {}; --document-font-size: {points}pt; --document-font-style: {}; --document-font-weight: {}; --document-font-stretch: {}; --document-font-variant: {}; --document-font-variation-settings: {};",
        css_string(&family),
        document_font_style_with_variations(description.style(), variations.as_deref()),
        document_font_weight_with_variations(description.weight(), variations.as_deref()),
        document_font_stretch_with_variations(description.stretch(), variations.as_deref()),
        document_font_variant(description.variant()),
        document_font_variations(variations.as_deref()),
    )
}

fn document_font_style_with_variations(
    style: gtk::pango::Style,
    variations: Option<&str>,
) -> String {
    if variation_axis_value(variations, "ital").is_some_and(|italic| italic != 0.0) {
        return "italic".to_owned();
    }
    if let Some(slant) = variation_axis_value(variations, "slnt").filter(|slant| *slant != 0.0) {
        // OpenType slant angles use the opposite sign from CSS oblique angles.
        return format!("oblique {}deg", -slant);
    }
    if variation_axis_value(variations, "ital").is_some() {
        return "normal".to_owned();
    }
    document_font_style(style).to_owned()
}

fn document_font_weight_with_variations(
    weight: gtk::pango::Weight,
    variations: Option<&str>,
) -> String {
    variation_axis_value(variations, "wght")
        .unwrap_or_else(|| f64::from(document_font_weight(weight)))
        .to_string()
}

fn document_font_stretch_with_variations(
    stretch: gtk::pango::Stretch,
    variations: Option<&str>,
) -> String {
    variation_axis_value(variations, "wdth").map_or_else(
        || document_font_stretch(stretch).to_owned(),
        |width| format!("{width}%"),
    )
}

const fn document_font_style(style: gtk::pango::Style) -> &'static str {
    match style {
        gtk::pango::Style::Oblique => "oblique",
        gtk::pango::Style::Italic => "italic",
        _ => "normal",
    }
}

const fn document_font_weight(weight: gtk::pango::Weight) -> i32 {
    match weight {
        gtk::pango::Weight::Thin => 100,
        gtk::pango::Weight::Ultralight => 200,
        gtk::pango::Weight::Light => 300,
        gtk::pango::Weight::Semilight => 350,
        gtk::pango::Weight::Book => 380,
        gtk::pango::Weight::Medium => 500,
        gtk::pango::Weight::Semibold => 600,
        gtk::pango::Weight::Bold => 700,
        gtk::pango::Weight::Ultrabold => 800,
        gtk::pango::Weight::Heavy => 900,
        gtk::pango::Weight::Ultraheavy => 1000,
        gtk::pango::Weight::__Unknown(weight) => weight,
        _ => 400,
    }
}

const fn document_font_stretch(stretch: gtk::pango::Stretch) -> &'static str {
    match stretch {
        gtk::pango::Stretch::UltraCondensed => "ultra-condensed",
        gtk::pango::Stretch::ExtraCondensed => "extra-condensed",
        gtk::pango::Stretch::Condensed => "condensed",
        gtk::pango::Stretch::SemiCondensed => "semi-condensed",
        gtk::pango::Stretch::SemiExpanded => "semi-expanded",
        gtk::pango::Stretch::Expanded => "expanded",
        gtk::pango::Stretch::ExtraExpanded => "extra-expanded",
        gtk::pango::Stretch::UltraExpanded => "ultra-expanded",
        _ => "normal",
    }
}

const fn document_font_variant(variant: gtk::pango::Variant) -> &'static str {
    match variant {
        gtk::pango::Variant::SmallCaps => "small-caps",
        gtk::pango::Variant::AllSmallCaps => "all-small-caps",
        gtk::pango::Variant::PetiteCaps => "petite-caps",
        gtk::pango::Variant::AllPetiteCaps => "all-petite-caps",
        gtk::pango::Variant::Unicase => "unicase",
        gtk::pango::Variant::TitleCaps => "titling-caps",
        _ => "normal",
    }
}

fn document_font_variations(variations: Option<&str>) -> String {
    let settings = variations
        .into_iter()
        .flat_map(|variations| variations.split(','))
        .filter_map(|variation| {
            let (axis, value) = variation.trim().split_once('=')?;
            let axis = axis.trim();
            let value = value.trim().parse::<f64>().ok()?;
            // CSS's high-level font properties must remain free to cascade to
            // semantic markup such as headings and strong text.
            (axis.len() == 4
                && axis.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && !matches!(axis, "wght" | "wdth" | "slnt" | "ital")
                && value.is_finite())
            .then(|| format!("\"{axis}\" {value}"))
        })
        .collect::<Vec<_>>();
    if settings.is_empty() {
        "normal".to_owned()
    } else {
        settings.join(", ")
    }
}

fn variation_axis_value(variations: Option<&str>, target_axis: &str) -> Option<f64> {
    variations
        .into_iter()
        .flat_map(|variations| variations.split(','))
        .filter_map(|variation| {
            let (axis, value) = variation.trim().split_once('=')?;
            (axis.trim() == target_axis)
                .then(|| value.trim().parse::<f64>().ok())
                .flatten()
                .filter(|value| value.is_finite())
        })
        .next_back()
}

fn desktop_font_settings(key: &str) -> Option<gtk::gio::Settings> {
    let schema = gtk::gio::SettingsSchemaSource::default()?
        .lookup(SYSTEM_INTERFACE_SCHEMA, true)
        .filter(|schema| schema.has_key(key))?;
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

fn system_document_font_from_settings(settings: Option<&gtk::gio::Settings>) -> String {
    settings
        .map(|settings| settings.string(SYSTEM_DOCUMENT_FONT_KEY).to_string())
        .and_then(|font| normalize_document_font_description(&font))
        .unwrap_or_else(|| FALLBACK_DOCUMENT_FONT.to_owned())
}

fn source_font_css(description: &str) -> String {
    let description = normalize_source_font_description(description)
        .unwrap_or_else(|| FALLBACK_MONOSPACE_FONT.to_owned());
    let description = gtk::pango::FontDescription::from_string(&description);
    let family = description.family().unwrap_or_else(|| "Monospace".into());
    let points = f64::from(description.size()) / f64::from(gtk::pango::SCALE);
    format!(
        "#source-editor {{ font-family: {}; font-size: {points}pt; }}",
        css_string(&family)
    )
}

fn css_string(value: &str) -> String {
    // CSS escaping alone leaves '<' intact, which could terminate an HTML style element.
    cssparser::Token::QuotedString(value.into())
        .to_css_string()
        .replace('<', "\\3c ")
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
