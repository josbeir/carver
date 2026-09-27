//! `GtkSourceView`-backed raw frontmatter editing for the document-properties dialog.
//!
//! The raw view highlights the note's frontmatter with the grammar matching its declared format
//! (YAML, TOML, or JSON). The grammars are bundled with Carver and installed alongside the Carve
//! grammar, so highlighting does not depend on the host's `GtkSourceView` data.

use std::path::Path;

use carver_domain::FrontmatterFormat;

/// Returns the `GtkSourceView` language id highlighting `format`.
///
/// The frontmatter format tokens (`yaml`, `toml`, `json`) are also the upstream grammar ids.
#[must_use]
pub(crate) fn language_id(format: FrontmatterFormat) -> &'static str {
    format.as_str()
}

/// Builds a syntax-highlighting buffer for a frontmatter format.
///
/// `syntax_dir` is Carver's installed syntax directory; the default `GtkSourceView` search paths
/// stay available as a fallback. A missing grammar degrades to an unhighlighted buffer.
#[must_use]
pub(crate) fn source_buffer(
    format: FrontmatterFormat,
    syntax_dir: Option<&Path>,
) -> sourceview5::Buffer {
    let manager = sourceview5::LanguageManager::new();
    if let Some(directory) = syntax_dir.and_then(Path::to_str) {
        let defaults = manager.search_path();
        let mut paths = Vec::with_capacity(defaults.len() + 1);
        paths.push(directory);
        paths.extend(defaults.iter().map(glib::GString::as_str));
        manager.set_search_path(&paths);
    }
    let mut builder = sourceview5::Buffer::builder().highlight_syntax(true);
    if let Some(language) = manager.language(language_id(format)) {
        builder = builder.language(&language);
    }
    builder.build()
}

#[cfg(test)]
mod tests;
