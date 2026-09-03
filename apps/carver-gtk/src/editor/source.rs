//! Literal source-buffer helpers.

use gtk::prelude::*;

/// Returns the canonical Carve source without interpretation.
pub(crate) fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
}
