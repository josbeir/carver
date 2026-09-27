//! Typed frontmatter property definitions shared by configuration, Bases, and the UI.
//!
//! A document property's type is the single authority for how the configured defaults dialog,
//! the per-note properties dialog, and the Bases grid interpret and validate a value. Keeping it
//! in the domain lets every layer derive the same [`PropertyType`] from an authored
//! [`FrontmatterValue`] instead of each re-inventing the mapping.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Iso8601;

use crate::bases::PropertyKind;
use crate::frontmatter::FrontmatterValue;

/// A user-selectable frontmatter property type.
///
/// The type is a semantic layer over the stored JSON kind: a `Date` or `DateTime` is stored as
/// text, so [`PropertyType::kind`] reports [`PropertyKind::Text`] for them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum PropertyType {
    /// A single-line text value.
    #[default]
    Text,
    /// A multi-line text value.
    LongText,
    /// A numeric value.
    Number,
    /// A boolean value.
    Boolean,
    /// A list of configured options.
    List,
    /// An ISO 8601 calendar date stored as text.
    Date,
    /// An ISO 8601 date and time stored as text.
    DateTime,
}

impl PropertyType {
    /// Returns the stored JSON kind this type serializes to.
    #[must_use]
    pub const fn kind(self) -> PropertyKind {
        match self {
            Self::Text | Self::LongText | Self::Date | Self::DateTime => PropertyKind::Text,
            Self::Number => PropertyKind::Number,
            Self::Boolean => PropertyKind::Boolean,
            Self::List => PropertyKind::List,
        }
    }

    /// Returns whether the type is a date or a date and time.
    #[must_use]
    pub const fn is_date(self) -> bool {
        matches!(self, Self::Date | Self::DateTime)
    }

    /// Infers a field type from an existing frontmatter value.
    ///
    /// A date or date-time is stored as text, so its type is recovered from the ISO 8601 shape to
    /// round-trip an ad-hoc property across dialog opens. `LongText` is never inferred because the
    /// authored formatting is indistinguishable from single-line text once parsed.
    #[must_use]
    pub fn from_value(value: &FrontmatterValue) -> Self {
        match value {
            FrontmatterValue::Number(_) => Self::Number,
            FrontmatterValue::Boolean(_) => Self::Boolean,
            FrontmatterValue::List(_) => Self::List,
            FrontmatterValue::Text(text) => {
                if text.contains('T') && parse_iso_date_time(text).is_some() {
                    Self::DateTime
                } else if parse_iso_date(text).is_some() {
                    Self::Date
                } else {
                    Self::Text
                }
            }
            FrontmatterValue::Null | FrontmatterValue::Object(_) => Self::Text,
        }
    }

    /// Returns whether a parsed value can be edited as this type.
    ///
    /// A `Null` value is accepted by every scalar type so an explicit empty survives. The option
    /// set of a `List` is validated separately by the caller, which owns the configured options.
    #[must_use]
    pub fn accepts_value(self, value: &FrontmatterValue) -> bool {
        match self {
            Self::Number => matches!(value, FrontmatterValue::Number(_) | FrontmatterValue::Null),
            Self::Boolean => matches!(value, FrontmatterValue::Boolean(_) | FrontmatterValue::Null),
            Self::Text | Self::LongText => {
                matches!(value, FrontmatterValue::Text(_) | FrontmatterValue::Null)
            }
            Self::Date => {
                matches!(value, FrontmatterValue::Text(text) if parse_iso_date(text).is_some())
                    || matches!(value, FrontmatterValue::Null)
            }
            Self::DateTime => {
                matches!(value, FrontmatterValue::Text(text) if parse_iso_date_time(text).is_some())
                    || matches!(value, FrontmatterValue::Null)
            }
            // A list with no configured options is not an option set, so it stays free-form.
            Self::List => true,
        }
    }

    /// Returns whether a JSON default value is representable by this type.
    ///
    /// A `null` value is accepted by every type so an explicit empty default can be configured.
    #[must_use]
    pub fn accepts_json(self, value: &Value) -> bool {
        if value.is_null() {
            return true;
        }
        match self {
            Self::Text | Self::LongText | Self::Date | Self::DateTime => value.is_string(),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            // A list property's value is its option list, so every entry is a string.
            Self::List => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
        }
    }

    /// Coerces a JSON default into a value this type can hold.
    #[must_use]
    pub fn normalized_json(self, value: &Value) -> Value {
        if self.accepts_json(value) {
            return value.clone();
        }
        match self {
            Self::Number => Value::from(0),
            Self::Boolean => Value::Bool(false),
            Self::List => Value::Array(Vec::new()),
            Self::Text | Self::LongText | Self::Date | Self::DateTime => {
                Value::String(String::new())
            }
        }
    }
}

/// Parses an ISO 8601 calendar date, rejecting a date and time.
///
/// `time`'s well-known ISO 8601 parser accepts a date prefix and ignores a trailing time, so this
/// checks the exact `YYYY-MM-DD` shape first. That keeps a `Date` property from silently absorbing
/// a date-time value.
#[must_use]
pub fn parse_iso_date(value: &str) -> Option<time::Date> {
    let value = value.trim();
    let bytes = value.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    shaped
        .then(|| time::Date::parse(value, &Iso8601::DATE).ok())
        .flatten()
}

/// Parses an ISO 8601 date and time into an instant.
///
/// The well-known ISO 8601 parser tolerates a missing offset (assuming UTC) and a space separator,
/// matching how the date picker reads authored values.
#[must_use]
pub fn parse_iso_date_time(value: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(value, &Iso8601::DEFAULT).ok()
}

#[cfg(test)]
mod tests;
