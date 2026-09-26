//! Configuration and XDG path handling for Carver.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use carver_domain::{
    FrontmatterField, FrontmatterFormat, FrontmatterValue, PropertyKind,
    frontmatter_source_with_format, is_reserved_key,
};
use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

const APPLICATION_QUALIFIER: &str = "io";
const APPLICATION_ORGANIZATION: &str = "github.josbeir";
const APPLICATION_NAME: &str = "carver";

/// File locations used by the application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    /// Config directory, normally `~/.config/carver`.
    pub config_dir: PathBuf,
    /// Data directory, normally `~/.local/share/carver`.
    pub data_dir: PathBuf,
    /// Cache directory, normally `~/.cache/carver`.
    pub cache_dir: PathBuf,
}

impl AppPaths {
    /// Resolves platform-standard user paths.
    #[must_use]
    pub fn discover() -> Self {
        if let Some(directories) = ProjectDirs::from(
            APPLICATION_QUALIFIER,
            APPLICATION_ORGANIZATION,
            APPLICATION_NAME,
        ) {
            return Self {
                config_dir: directories.config_dir().to_owned(),
                data_dir: directories.data_dir().to_owned(),
                cache_dir: directories.cache_dir().to_owned(),
            };
        }

        let fallback = PathBuf::from("carver");
        Self {
            config_dir: fallback.join("config"),
            data_dir: fallback.join("data"),
            cache_dir: fallback.join("cache"),
        }
    }

    /// Returns the TOML settings path.
    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Returns the `SQLite` database path.
    #[must_use]
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("library.sqlite3")
    }

    /// Returns the managed image directory.
    #[must_use]
    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    /// Returns the disposable remote-image cache directory.
    #[must_use]
    pub fn remote_image_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("remote-images")
    }

    /// Creates all directories required for a first launch.
    ///
    /// # Errors
    ///
    /// Returns an error when a required directory cannot be created.
    pub fn ensure_exists(&self) -> Result<(), ConfigError> {
        for path in [
            &self.config_dir,
            &self.data_dir,
            &self.assets_dir(),
            &self.remote_image_cache_dir(),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

/// Persisted user configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// On-disk schema version.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Editor settings.
    #[serde(default)]
    pub editor: EditorConfig,
    /// Image and network preferences.
    #[serde(default)]
    pub images: ImageConfig,
    /// Search preferences.
    #[serde(default)]
    pub search: SearchConfig,
    /// Window and navigation state.
    #[serde(default)]
    pub window: WindowConfig,
    /// Default document properties.
    #[serde(default)]
    pub document_properties: DocumentPropertiesConfig,
}

/// Editor preferences.
// CONTEXT: These flat fields preserve the established `[editor]` TOML schema;
// splitting them would require a user-facing configuration migration without
// improving the settings' two-state semantics.
#[expect(
    clippy::struct_excessive_bools,
    reason = "editor preferences persist independent two-state options"
)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Enable author-placed contents, disclosures, and automatic web links in HTML.
    #[serde(default = "default_true")]
    pub enhanced_carve_rendering: bool,
    /// The editor surface selected most recently by the user.
    ///
    /// `default_mode` was written before this became a session preference.
    /// It remains accepted on read so existing configurations migrate when next
    /// saved, which writes the clearer `last_mode` key.
    #[serde(default, alias = "default_mode")]
    pub last_mode: EditorMode,
    /// Milliseconds without edits before persisting a note.
    #[serde(default = "default_autosave_delay")]
    pub autosave_delay_ms: u64,
    /// Whether source mode restores its rendered split preview.
    #[serde(default)]
    pub source_split_view: bool,
    /// Whether the editor restores its document sidebar.
    #[serde(default, rename = "show_media_sidebar")]
    pub show_document_sidebar: bool,
    /// Whether the source editor shows a line-number gutter.
    #[serde(default)]
    pub source_line_numbers: bool,
    /// Whether the source editor highlights the line containing the cursor.
    #[serde(default)]
    pub source_highlight_current_line: bool,
    /// Visual density of Carve syntax highlighting in the source editor.
    ///
    /// The legacy `source_syntax_highlighting` boolean remains accepted on
    /// read and is migrated to this style when the configuration is saved.
    #[serde(
        default,
        alias = "source_syntax_highlighting",
        deserialize_with = "deserialize_source_syntax_style"
    )]
    pub source_syntax_style: SourceSyntaxStyle,
    /// Whether the editor shows the shared formatting toolbar.
    #[serde(default = "default_true")]
    pub show_formatting_toolbar: bool,
    /// Custom Pango font description for the source editor.
    ///
    /// When absent, the GTK frontend follows the desktop monospace-font
    /// preference instead of persisting a platform-specific default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_font: Option<String>,
    /// Custom Pango font description for formatted editing and previews.
    ///
    /// When absent, the GTK frontend follows the desktop document-font
    /// preference instead of persisting a platform-specific default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_font: Option<String>,
    /// Line height for formatted editing and previews, as a percentage.
    #[serde(
        default = "default_document_line_height_percent",
        deserialize_with = "deserialize_document_line_height_percent"
    )]
    pub document_line_height_percent: u16,
    /// Maximum readable measure for formatted editing and previews.
    #[serde(default)]
    pub document_width: DocumentWidth,
}

const fn default_document_line_height_percent() -> u16 {
    155
}

fn deserialize_document_line_height_percent<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    u16::try_from(i64::deserialize(deserializer)?.clamp(100, 250)).map_err(serde::de::Error::custom)
}

/// An editor surface a user can select for a note.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorMode {
    /// Present the formatted editor.
    #[default]
    Rich,
    /// Present canonical Carve source.
    Source,
    /// Present the rendered, read-only result.
    Rendered,
}

/// Maximum readable measure for the formatted editor and previews.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentWidth {
    /// A compact 60-character reading column.
    Narrow,
    /// An 80-character reading column.
    #[default]
    Comfortable,
    /// A 100-character reading column.
    Wide,
    /// Use all available horizontal space.
    Full,
}

/// Visual density of Carve syntax highlighting in the source editor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceSyntaxStyle {
    /// Apply the complete Carve syntax palette.
    #[default]
    Detailed,
    /// Keep document hierarchy and links visible while muting markup structure.
    WritingFocus,
    /// Disable source syntax highlighting.
    None,
}

/// Remote-image preferences.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Whether HTTP(S) images load when notes open.
    #[serde(default = "default_true")]
    pub load_remote_automatically: bool,
}

/// Search preferences.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Whether a new search begins in the current category or all categories.
    #[serde(default)]
    pub default_scope: SearchScope,
}

/// The initial category scope for a new search.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchScope {
    /// Search every active category.
    #[default]
    AllCategories,
    /// Search only the selected category when there is one.
    CurrentCategory,
}

/// Persisted window state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Requested initial width.
    #[serde(default = "default_width")]
    pub width: i32,
    /// Requested initial height.
    #[serde(default = "default_height")]
    pub height: i32,
    /// Whether the window was maximized.
    #[serde(default)]
    pub maximized: bool,
    /// Whether the sidebar was manually collapsed.
    #[serde(default)]
    pub sidebar_collapsed: bool,
}

/// Default document properties applied to new notes and offered by the properties dialog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentPropertiesConfig {
    /// Whether new notes are seeded with the configured properties.
    #[serde(default)]
    pub enabled: bool,
    /// Whether the editor shows the floating properties button.
    #[serde(default = "default_true")]
    pub floating_button: bool,
    /// Frontmatter format used when the dialog creates a block or seeds a new note.
    #[serde(default = "default_frontmatter_format")]
    pub format: FrontmatterFormat,
    /// Ordered default properties.
    #[serde(default)]
    pub entries: Vec<DocumentProperty>,
}

impl Default for DocumentPropertiesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            floating_button: true,
            format: FrontmatterFormat::Yaml,
            entries: Vec::new(),
        }
    }
}

const fn default_frontmatter_format() -> FrontmatterFormat {
    FrontmatterFormat::Yaml
}

/// A user-selectable document property field type.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentPropertyType {
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
    /// An ISO 8601 calendar date.
    Date,
    /// An ISO 8601 date and time.
    DateTime,
}

impl DocumentPropertyType {
    /// Returns the domain value kind this field type is stored as.
    #[must_use]
    pub const fn domain_kind(self) -> PropertyKind {
        match self {
            Self::Text | Self::LongText | Self::Date | Self::DateTime => PropertyKind::Text,
            Self::Number => PropertyKind::Number,
            Self::Boolean => PropertyKind::Boolean,
            Self::List => PropertyKind::List,
        }
    }

    /// Returns whether the field type is a date or a date and time.
    #[must_use]
    pub const fn is_date(self) -> bool {
        matches!(self, Self::Date | Self::DateTime)
    }
}

/// One user-configured document property.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentProperty {
    /// Property name.
    pub key: String,
    /// Field type offered by the properties dialog.
    #[serde(default)]
    pub field_type: DocumentPropertyType,
    /// Whether a list property allows selecting more than one option.
    #[serde(default)]
    pub multiple: bool,
    /// The default value, or the option list (`List`).
    #[serde(default)]
    pub value: serde_json::Value,
}

impl DocumentProperty {
    /// Returns the option list for a list property.
    #[must_use]
    pub fn options(&self) -> Vec<String> {
        self.value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the field seeded into a new note at `now`.
    ///
    /// A date or date-time seeds the current date/time. A list seeds its first option: a scalar
    /// when single-select, or a one-item list when multiple; a list with no options seeds nothing.
    #[must_use]
    pub fn default_field_at(&self, now: time::OffsetDateTime) -> Option<FrontmatterField> {
        let value = match self.field_type {
            DocumentPropertyType::List => {
                let first = self.options().into_iter().next()?;
                if self.multiple {
                    FrontmatterValue::List(vec![FrontmatterValue::Text(first)])
                } else {
                    FrontmatterValue::Text(first)
                }
            }
            DocumentPropertyType::Date => FrontmatterValue::Text(now.date().to_string()),
            DocumentPropertyType::DateTime => FrontmatterValue::Text(
                now.replace_nanosecond(0)
                    .unwrap_or(now)
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            ),
            DocumentPropertyType::Text
            | DocumentPropertyType::LongText
            | DocumentPropertyType::Number
            | DocumentPropertyType::Boolean => FrontmatterValue::from_json(&self.value),
        };
        Some(FrontmatterField::new(self.key.clone(), value))
    }

    /// Returns the field seeded into a new note using the current local time.
    #[must_use]
    pub fn default_field(&self) -> Option<FrontmatterField> {
        self.default_field_at(now_local())
    }
}

/// The current local time, falling back to UTC when the offset is indeterminate.
fn now_local() -> time::OffsetDateTime {
    time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc())
}

/// Returns whether a seeded default carries a value worth writing.
///
/// A blank text or null default is dropped so new notes are not seeded with an empty key, which
/// matches how the properties dialog treats unfilled values.
fn is_seedable_field(field: &FrontmatterField) -> bool {
    match &field.value {
        FrontmatterValue::Null => false,
        FrontmatterValue::Text(text) => !text.trim().is_empty(),
        FrontmatterValue::Number(_)
        | FrontmatterValue::Boolean(_)
        | FrontmatterValue::List(_)
        | FrontmatterValue::Object(_) => true,
    }
}

impl DocumentPropertiesConfig {
    /// Returns canonical Carve source seeding a new note, or an empty string.
    #[must_use]
    pub fn default_source(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        let now = now_local();
        let fields: Vec<FrontmatterField> = self
            .entries
            .iter()
            .filter_map(|entry| entry.default_field_at(now))
            .filter(is_seedable_field)
            .collect();
        frontmatter_source_with_format(&fields, self.format)
    }

    /// Validates the configured default properties.
    ///
    /// # Errors
    ///
    /// Returns a message when a key is blank, reserved, or duplicated, or when a field type or
    /// value is unsupported.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            let key = entry.key.trim();
            if key.is_empty() {
                return Err(String::from("property keys must not be empty"));
            }
            if is_reserved_key(key) {
                return Err(format!(
                    "'{key}' is reserved and cannot be a default property"
                ));
            }
            if entry.multiple && entry.field_type != DocumentPropertyType::List {
                return Err(format!(
                    "property '{key}' can only allow multiple values when it is a list"
                ));
            }
            if !value_matches_type(entry.field_type, &entry.value) {
                return Err(format!("property '{key}' value does not match its type"));
            }
            if !seen.insert(key.to_owned()) {
                return Err(format!("property '{key}' is configured more than once"));
            }
        }
        Ok(())
    }
}

fn value_matches_type(field_type: DocumentPropertyType, value: &serde_json::Value) -> bool {
    if value.is_null() {
        return true;
    }
    match field_type {
        DocumentPropertyType::Text
        | DocumentPropertyType::LongText
        | DocumentPropertyType::Date
        | DocumentPropertyType::DateTime => value.is_string(),
        DocumentPropertyType::Number => value.is_number(),
        DocumentPropertyType::Boolean => value.is_boolean(),
        // A list property's value is its option list, so every entry is a string.
        DocumentPropertyType::List => value
            .as_array()
            .is_some_and(|items| items.iter().all(serde_json::Value::is_string)),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            editor: EditorConfig::default(),
            images: ImageConfig::default(),
            search: SearchConfig::default(),
            window: WindowConfig::default(),
            document_properties: DocumentPropertiesConfig::default(),
        }
    }
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            enhanced_carve_rendering: true,
            last_mode: EditorMode::default(),
            autosave_delay_ms: default_autosave_delay(),
            source_split_view: false,
            show_document_sidebar: false,
            source_line_numbers: false,
            source_highlight_current_line: false,
            source_syntax_style: SourceSyntaxStyle::default(),
            show_formatting_toolbar: true,
            source_font: None,
            document_font: None,
            document_line_height_percent: default_document_line_height_percent(),
            document_width: DocumentWidth::default(),
        }
    }
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            load_remote_automatically: true,
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            maximized: false,
            sidebar_collapsed: false,
        }
    }
}

/// Configuration read/write errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// An I/O operation failed.
    #[error("configuration I/O failed: {0}")]
    Io(#[from] io::Error),
    /// TOML could not be parsed or decoded.
    #[error("configuration is invalid: {0}")]
    InvalidToml(String),
    /// Document-property preferences are invalid.
    #[error("document properties are invalid: {0}")]
    InvalidDocumentProperties(String),
}

/// Reads the existing config, returning defaults when it has not been created.
///
/// # Errors
///
/// Returns an error when the file cannot be read or contains invalid TOML.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let source = fs::read_to_string(path)?;
    let config: Config =
        toml::from_str(&source).map_err(|error| ConfigError::InvalidToml(error.to_string()))?;
    config
        .document_properties
        .validate()
        .map_err(ConfigError::InvalidDocumentProperties)?;
    Ok(config)
}

/// Saves typed configuration atomically.
///
/// # Errors
///
/// Returns an error when the parent directory or configuration file cannot be written.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    config
        .document_properties
        .validate()
        .map_err(ConfigError::InvalidDocumentProperties)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let document = toml::to_string_pretty(config)
        .map_err(|error| ConfigError::InvalidToml(error.to_string()))?;
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(document.as_bytes())?;
    file.commit()?;
    Ok(())
}

const fn default_schema_version() -> u32 {
    1
}
const fn default_autosave_delay() -> u64 {
    500
}
const fn default_true() -> bool {
    true
}

fn deserialize_source_syntax_style<'de, D>(deserializer: D) -> Result<SourceSyntaxStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SyntaxStyleValue {
        Style(SourceSyntaxStyle),
        LegacyBoolean(bool),
    }

    match SyntaxStyleValue::deserialize(deserializer)? {
        SyntaxStyleValue::Style(style) => Ok(style),
        SyntaxStyleValue::LegacyBoolean(true) => Ok(SourceSyntaxStyle::Detailed),
        SyntaxStyleValue::LegacyBoolean(false) => Ok(SourceSyntaxStyle::None),
    }
}
const fn default_width() -> i32 {
    1120
}
const fn default_height() -> i32 {
    760
}

#[cfg(test)]
mod tests;
