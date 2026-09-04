//! GtkSourceView-backed canonical Carve source editing.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use gtk::prelude::*;
use sourceview5::prelude::*;
use thiserror::Error;

const CARVE_LANGUAGE: &str = include_str!("../../resources/source-syntax/carve.lang");
const CARVE_LIGHT_STYLE: &str = include_str!("../../resources/source-syntax/carve-light.xml");
const CARVE_DARK_STYLE: &str = include_str!("../../resources/source-syntax/carve-dark.xml");

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
        view.set_monospace(true);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_top_margin(24);
        view.set_bottom_margin(24);
        view.set_left_margin(24);
        view.set_right_margin(24);
        view.set_show_line_numbers(false);
        view.set_highlight_current_line(false);
        Ok(Self {
            buffer,
            view,
            light_style,
            dark_style,
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
        show_line_numbers: bool,
        highlight_current_line: bool,
        dark: bool,
    ) {
        self.view.set_show_line_numbers(show_line_numbers);
        self.view.set_highlight_current_line(highlight_current_line);
        self.buffer.set_style_scheme(Some(if dark {
            &self.dark_style
        } else {
            &self.light_style
        }));
    }
}

/// Returns the canonical Carve source without interpretation.
pub(crate) fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
}

#[cfg(test)]
mod tests;
