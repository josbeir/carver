//! Ordered frontmatter documents for the native properties dialog.
//!
//! Carve keeps a document's metadata block as opaque text. The properties dialog needs a
//! structured, order-preserving view of it so users can edit typed fields without losing the
//! authored formatting of the fields they did not touch.

use carve::{Options, parse_with_options};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

use crate::PropertyKind;

/// A supported frontmatter serialization format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontmatterFormat {
    /// YAML, Carve's default `---` fence.
    Yaml,
    /// JSON, spelled `---json`.
    Json,
    /// TOML, spelled `---toml`.
    Toml,
}

impl FrontmatterFormat {
    /// Returns the canonical format token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }

    /// Parses Carve's normalized format token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "yaml" | "yml" => Some(Self::Yaml),
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }

    /// Returns the opening fence Carver writes for a new block in this format.
    ///
    /// A bare `---` fence is canonical for YAML; JSON and TOML carry their token.
    #[must_use]
    pub const fn opener(self) -> &'static str {
        match self {
            Self::Yaml => "---",
            Self::Json => "---json",
            Self::Toml => "---toml",
        }
    }
}

/// One frontmatter value with its original kind.
#[derive(Clone, Debug, PartialEq)]
pub enum FrontmatterValue {
    /// A text scalar.
    Text(String),
    /// A numeric scalar.
    Number(serde_json::Number),
    /// A boolean scalar.
    Boolean(bool),
    /// An ordered list.
    List(Vec<FrontmatterValue>),
    /// An ordered nested object.
    Object(Vec<(String, FrontmatterValue)>),
    /// An explicit null.
    Null,
}

impl FrontmatterValue {
    /// Returns the observed kind for Bases interoperability.
    #[must_use]
    pub fn kind(&self) -> PropertyKind {
        match self {
            Self::Text(_) => PropertyKind::Text,
            Self::Number(_) => PropertyKind::Number,
            Self::Boolean(_) => PropertyKind::Boolean,
            Self::List(_) => PropertyKind::List,
            Self::Null => PropertyKind::Null,
            Self::Object(_) => PropertyKind::Mixed,
        }
    }

    /// Converts a JSON value, preserving object key order.
    #[must_use]
    pub fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(flag) => Self::Boolean(*flag),
            Value::Number(number) => Self::Number(number.clone()),
            Value::String(text) => Self::Text(text.clone()),
            Value::Array(items) => Self::List(items.iter().map(Self::from_json).collect()),
            Value::Object(map) => Self::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), Self::from_json(value)))
                    .collect(),
            ),
        }
    }

    /// Converts to a JSON value, preserving object key order.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Text(text) => Value::String(text.clone()),
            Self::Number(number) => Value::Number(number.clone()),
            Self::Boolean(flag) => Value::Bool(*flag),
            Self::Null => Value::Null,
            Self::List(items) => Value::Array(items.iter().map(Self::to_json).collect()),
            Self::Object(entries) => Value::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_json()))
                    .collect(),
            ),
        }
    }
}

impl Serialize for FrontmatterValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::Number(number) => number.serialize(serializer),
            Self::Boolean(flag) => serializer.serialize_bool(*flag),
            Self::Null => serializer.serialize_none(),
            Self::List(items) => {
                let mut sequence = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    sequence.serialize_element(item)?;
                }
                sequence.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for FrontmatterValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = FrontmatterValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a frontmatter scalar, list, or object")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Boolean(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Number(serde_json::Number::from(value)))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Number(serde_json::Number::from(value)))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(FrontmatterValue::Number)
                    .ok_or_else(|| E::custom("frontmatter number is not finite"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Text(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Text(value))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(FrontmatterValue::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                FrontmatterValue::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut items = Vec::new();
                while let Some(item) = sequence.next_element()? {
                    items.push(item);
                }
                Ok(FrontmatterValue::List(items))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    entries.push((key, map.next_value()?));
                }
                Ok(FrontmatterValue::Object(entries))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

/// One top-level frontmatter property.
#[derive(Clone, Debug, PartialEq)]
pub struct FrontmatterField {
    /// Property name.
    pub key: String,
    /// Property value.
    pub value: FrontmatterValue,
}

impl FrontmatterField {
    /// Creates a field from its parts.
    #[must_use]
    pub fn new(key: impl Into<String>, value: FrontmatterValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    /// Returns the observed value kind.
    #[must_use]
    pub fn kind(&self) -> PropertyKind {
        self.value.kind()
    }
}

/// An ordered view of a document's frontmatter block.
#[derive(Clone, Debug, PartialEq)]
pub struct FrontmatterDocument {
    /// Format of the block.
    pub format: FrontmatterFormat,
    /// Top-level fields in document order.
    pub fields: Vec<FrontmatterField>,
    /// Parse failure for a malformed or non-object block; `Some` means raw-only editing.
    pub error: Option<String>,
}

/// Errors produced while rendering a frontmatter document.
#[derive(Debug, Error)]
pub enum FrontmatterError {
    /// The requested format could not serialize the fields.
    #[error("frontmatter could not be serialized: {0}")]
    Serialize(String),
}

const RESERVED_KEYS: [&str; 1] = ["title"];

/// Returns whether a key is reserved and cannot be configured as a default.
#[must_use]
pub fn is_reserved_key(key: &str) -> bool {
    RESERVED_KEYS.contains(&key)
}

/// Parses a document's frontmatter block, or `None` when it has none.
///
/// A malformed or non-object block yields a document with [`FrontmatterDocument::error`] set and
/// no fields; callers then offer raw editing.
#[must_use]
pub fn parse_frontmatter_document(source: &str) -> Option<FrontmatterDocument> {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    let raw = document.frontmatter_raw.as_ref()?;
    let Some(format) = FrontmatterFormat::from_token(&raw.format) else {
        return Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: Vec::new(),
            error: Some(format!("unsupported frontmatter format: {}", raw.format)),
        });
    };
    match parse_fields(format, &raw.content) {
        Ok(fields) => Some(FrontmatterDocument {
            format,
            fields,
            error: None,
        }),
        Err(error) => Some(FrontmatterDocument {
            format,
            fields: Vec::new(),
            error: Some(error),
        }),
    }
}

fn parse_fields(format: FrontmatterFormat, content: &str) -> Result<Vec<FrontmatterField>, String> {
    let value = match format {
        FrontmatterFormat::Yaml => {
            yaml_serde::from_str::<FrontmatterValue>(content).map_err(|error| error.to_string())
        }
        FrontmatterFormat::Json => {
            serde_json::from_str::<FrontmatterValue>(content).map_err(|error| error.to_string())
        }
        FrontmatterFormat::Toml => {
            toml::from_str::<FrontmatterValue>(content).map_err(|error| error.to_string())
        }
    }?;
    match value {
        FrontmatterValue::Object(entries) => Ok(entries
            .into_iter()
            .map(|(key, value)| FrontmatterField { key, value })
            .collect()),
        _ => Err(String::from("frontmatter root must be an object")),
    }
}

/// Renders a complete frontmatter block, fences included.
///
/// # Errors
///
/// Returns [`FrontmatterError::Serialize`] when the selected format rejects a value.
pub fn render_frontmatter_document(
    document: &FrontmatterDocument,
) -> Result<String, FrontmatterError> {
    let fields = if document.format == FrontmatterFormat::Toml {
        document
            .fields
            .iter()
            .filter(|field| field.value != FrontmatterValue::Null)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        document.fields.clone()
    };
    let body = render_body(document.format, &fields)?;
    Ok(format!("{}\n{}\n---", document.format.opener(), body))
}

fn render_body(
    format: FrontmatterFormat,
    fields: &[FrontmatterField],
) -> Result<String, FrontmatterError> {
    let object = FrontmatterValue::Object(
        fields
            .iter()
            .map(|field| (field.key.clone(), field.value.clone()))
            .collect(),
    );
    let rendered = match format {
        FrontmatterFormat::Yaml => yaml_serde::to_string(&object)
            .map_err(|error| FrontmatterError::Serialize(error.to_string()))?,
        FrontmatterFormat::Json => serde_json::to_string_pretty(&object)
            .map_err(|error| FrontmatterError::Serialize(error.to_string()))?,
        FrontmatterFormat::Toml => toml::to_string(&object)
            .map_err(|error| FrontmatterError::Serialize(error.to_string()))?,
    };
    Ok(rendered.trim_end_matches('\n').to_owned())
}

struct ScannedBlock {
    opener: String,
    content: String,
    /// Byte index just past the closing `---`.
    block_end: usize,
}

fn scan_frontmatter(source: &str) -> Option<ScannedBlock> {
    if !source.starts_with("---") {
        return None;
    }
    let opener_end = source.find('\n')?;
    let opener = source[..opener_end].to_owned();
    let rest_start = opener_end + 1;
    let rest = &source[rest_start..];
    let mut line_start = 0usize;
    loop {
        let (line, line_end) = match rest[line_start..].find('\n') {
            Some(offset) => (
                &rest[line_start..line_start + offset],
                Some(line_start + offset),
            ),
            None => (&rest[line_start..], None),
        };
        if line == "---" {
            let content = rest[..line_start]
                .strip_suffix('\n')
                .unwrap_or(&rest[..line_start]);
            return Some(ScannedBlock {
                opener,
                content: content.to_owned(),
                block_end: rest_start + line_start + 3,
            });
        }
        let end = line_end?;
        line_start = end + 1;
    }
}

/// Replaces a document's frontmatter block with `desired`, preserving untouched formatting.
///
/// Passing `None` removes the block. Scalar edits, additions, and removals are applied line by
/// line for YAML and TOML so that untouched entries keep their exact spelling; anything else is
/// re-rendered from the ordered model.
///
/// # Errors
///
/// Returns [`FrontmatterError::Serialize`] when a re-render is required and the format rejects a
/// value.
pub fn replace_frontmatter(
    source: &str,
    desired: Option<&FrontmatterDocument>,
) -> Result<String, FrontmatterError> {
    let scanned = scan_frontmatter(source);
    match (scanned, desired) {
        (None, None) => Ok(source.to_owned()),
        (Some(block), None) => {
            let mut rest = &source[block.block_end..];
            if let Some(stripped) = rest.strip_prefix('\n') {
                rest = stripped;
            }
            Ok(rest.to_owned())
        }
        (None, Some(document)) => {
            let rendered = render_frontmatter_document(document)?;
            if source.is_empty() {
                Ok(format!("{rendered}\n"))
            } else {
                Ok(format!("{rendered}\n{source}"))
            }
        }
        (Some(block), Some(document)) => {
            let current_format =
                FrontmatterFormat::from_token(block.opener.trim_start_matches('-').trim())
                    .unwrap_or(FrontmatterFormat::Yaml);
            let current_fields = parse_fields(current_format, &block.content).unwrap_or_default();
            let body = if current_format == document.format {
                apply_fields(
                    &block.content,
                    current_format,
                    &current_fields,
                    &document.fields,
                )
                .map_or_else(|| render_body(document.format, &document.fields), Ok)?
            } else {
                render_body(document.format, &document.fields)?
            };
            let opener = if current_format == document.format {
                block.opener
            } else {
                document.format.opener().to_owned()
            };
            let rendered = format!("{opener}\n{body}\n---");
            Ok(format!("{rendered}{}", &source[block.block_end..]))
        }
    }
}

/// Renders YAML source seeding a new note, or an empty string when there are no fields.
///
/// Reserved keys such as `title` are dropped. The block ends with a newline.
#[must_use]
pub fn frontmatter_source(fields: &[FrontmatterField]) -> String {
    let fields: Vec<FrontmatterField> = fields
        .iter()
        .filter(|field| !field.key.trim().is_empty() && !is_reserved_key(&field.key))
        .cloned()
        .collect();
    if fields.is_empty() {
        return String::new();
    }
    let document = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields,
        error: None,
    };
    render_frontmatter_document(&document)
        .map_or_else(|_| String::new(), |block| format!("{block}\n"))
}

fn apply_fields(
    content: &str,
    format: FrontmatterFormat,
    current: &[FrontmatterField],
    desired: &[FrontmatterField],
) -> Option<String> {
    if matches!(format, FrontmatterFormat::Json) {
        return None;
    }
    let entries = scan_entries(content, format);
    if entries.len() != current.len() {
        return None;
    }
    // A reorder is only safe to preserve when the common keys keep their existing order.
    let existing_keys: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
    let desired_keys: Vec<&str> = desired.iter().map(|field| field.key.as_str()).collect();
    let ordered: Vec<&str> = desired_keys
        .iter()
        .copied()
        .filter(|key| existing_keys.contains(key))
        .collect();
    let existing_common: Vec<&str> = existing_keys
        .iter()
        .copied()
        .filter(|key| desired_keys.contains(key))
        .collect();
    if existing_common != ordered {
        return None;
    }

    let mut output: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    let lines: Vec<&str> = content.lines().collect();
    for entry in &entries {
        while cursor < entry.start {
            output.push(lines[cursor].to_owned());
            cursor += 1;
        }
        cursor = entry.end;
        let Some(field) = desired.iter().find(|field| field.key == entry.key) else {
            continue;
        };
        let unchanged = current
            .iter()
            .find(|current| current.key == entry.key)
            .is_some_and(|existing| existing.value == field.value);
        if unchanged {
            for line in &lines[entry.start..entry.end] {
                output.push((*line).to_owned());
            }
        } else {
            output.push(render_field_line(format, field)?);
        }
    }
    while cursor < lines.len() {
        output.push(lines[cursor].to_owned());
        cursor += 1;
    }
    for field in desired {
        if existing_keys.contains(&field.key.as_str()) {
            continue;
        }
        output.push(render_field_line(format, field)?);
    }
    Some(output.join("\n"))
}

struct ScannedEntry {
    key: String,
    start: usize,
    end: usize,
}

fn scan_entries(content: &str, format: FrontmatterFormat) -> Vec<ScannedEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut entries: Vec<ScannedEntry> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with(char::is_whitespace) || line.trim().is_empty() {
            continue;
        }
        if let Some(key) = top_level_key(line, format) {
            if let Some(previous) = entries.last_mut() {
                previous.end = index;
            }
            entries.push(ScannedEntry {
                key,
                start: index,
                end: lines.len(),
            });
        }
    }
    entries
}

fn top_level_key(line: &str, format: FrontmatterFormat) -> Option<String> {
    let separator = match format {
        FrontmatterFormat::Toml => '=',
        _ => ':',
    };
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (raw_key, _) = trimmed.split_once(separator)?;
    let raw_key = raw_key.trim();
    if raw_key.is_empty() {
        return None;
    }
    Some(unquote_key(raw_key))
}

fn unquote_key(raw: &str) -> String {
    if raw.len() >= 2 {
        let (first, rest) = raw.split_at(1);
        if (first == "\"" || first == "'") && rest.ends_with(first) {
            return rest[..rest.len() - 1].to_owned();
        }
    }
    raw.to_owned()
}

fn needs_quoted_key(key: &str, format: FrontmatterFormat) -> bool {
    if key.is_empty() {
        return true;
    }
    match format {
        FrontmatterFormat::Toml => !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        _ => !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'),
    }
}

fn render_field_line(format: FrontmatterFormat, field: &FrontmatterField) -> Option<String> {
    let key = if needs_quoted_key(&field.key, format) {
        format!("\"{}\"", escape_double_quoted(&field.key))
    } else {
        field.key.clone()
    };
    let value = render_inline_value(format, &field.value)?;
    Some(match format {
        FrontmatterFormat::Toml => format!("{key} = {value}"),
        _ => format!("{key}: {value}"),
    })
}

fn render_inline_value(format: FrontmatterFormat, value: &FrontmatterValue) -> Option<String> {
    Some(match value {
        FrontmatterValue::Text(text) => format!("\"{}\"", escape_double_quoted(text)),
        FrontmatterValue::Number(number) => number.to_string(),
        FrontmatterValue::Boolean(flag) => flag.to_string(),
        FrontmatterValue::Null => match format {
            FrontmatterFormat::Toml => return None,
            _ => String::from("null"),
        },
        FrontmatterValue::List(items) => {
            let rendered: Option<Vec<String>> = items
                .iter()
                .map(|item| render_inline_value(format, item))
                .collect();
            format!("[{}]", rendered?.join(", "))
        }
        FrontmatterValue::Object(entries) => {
            let rendered: Option<Vec<String>> = entries
                .iter()
                .map(|(key, value)| {
                    let key = if needs_quoted_key(key, format) {
                        format!("\"{}\"", escape_double_quoted(key))
                    } else {
                        key.clone()
                    };
                    let value = render_inline_value(format, value)?;
                    Some(match format {
                        FrontmatterFormat::Toml => format!("{key} = {value}"),
                        _ => format!("{key}: {value}"),
                    })
                })
                .collect();
            let inner = rendered?.join(", ");
            match format {
                FrontmatterFormat::Toml => format!("{{ {inner} }}"),
                _ => format!("{{{inner}}}"),
            }
        }
    })
}

fn escape_double_quoted(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests;
