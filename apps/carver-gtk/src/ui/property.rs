//! Shared presentation helpers for typed frontmatter properties.
//!
//! The configured-defaults dialog, the per-note properties dialog, and the Bases grid all label
//! the same [`PropertyType`] values, so the translation lives here rather than in each surface.

use carver_domain::PropertyType;
use gettextrs::gettext;

/// Returns the user-visible label for a property type.
#[must_use]
pub(crate) fn property_type_label(field_type: PropertyType) -> String {
    match field_type {
        PropertyType::Text => gettext("Text"),
        PropertyType::LongText => gettext("Long text"),
        PropertyType::Number => gettext("Number"),
        PropertyType::Boolean => gettext("Boolean"),
        PropertyType::List => gettext("List"),
        PropertyType::Date => gettext("Date"),
        PropertyType::DateTime => gettext("Date & time"),
    }
}
