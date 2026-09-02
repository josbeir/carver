//! Configuration and XDG path handling for Carver.

#![forbid(unsafe_code)]

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
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

    /// Returns the SQLite database path.
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
}

/// Editor preferences.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Rich or source mode selected for new sessions.
    #[serde(default)]
    pub default_mode: EditorMode,
    /// Milliseconds without edits before persisting a note.
    #[serde(default = "default_autosave_delay")]
    pub autosave_delay_ms: u64,
    /// Whether source mode restores its rendered split preview.
    #[serde(default)]
    pub source_split_view: bool,
}

/// The editor representation selected when a session begins.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorMode {
    /// Present the formatted editor.
    #[default]
    Rich,
    /// Present canonical Carve source.
    Source,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            editor: EditorConfig::default(),
            images: ImageConfig::default(),
            search: SearchConfig::default(),
            window: WindowConfig::default(),
        }
    }
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            default_mode: EditorMode::default(),
            autosave_delay_ms: default_autosave_delay(),
            source_split_view: false,
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
    toml::from_str(&source).map_err(|error| ConfigError::InvalidToml(error.to_string()))
}

/// Saves typed configuration atomically.
///
/// # Errors
///
/// Returns an error when the parent directory or configuration file cannot be written.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let document = toml::to_string_pretty(config)
        .map_err(|error| ConfigError::InvalidToml(error.to_string()))?;
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, document)?;
    fs::rename(temporary, path)?;
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
const fn default_width() -> i32 {
    1120
}
const fn default_height() -> i32 {
    760
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_remote_images() {
        assert!(Config::default().images.load_remote_automatically);
    }

    #[test]
    fn missing_config_uses_complete_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;

        assert_eq!(
            load(&directory.path().join("missing.toml"))?,
            Config::default()
        );
        Ok(())
    }

    #[test]
    fn partial_config_keeps_defaults_for_unset_sections() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "[editor]\ndefault_mode = 'source'\n\n[window]\nmaximized = true\n",
        )?;

        let config = load(&path)?;

        assert_eq!(config.editor.default_mode, EditorMode::Source);
        assert_eq!(config.editor.autosave_delay_ms, 500);
        assert!(!config.editor.source_split_view);
        assert!(config.images.load_remote_automatically);
        assert_eq!(config.window.width, 1120);
        assert_eq!(config.window.height, 760);
        assert!(config.window.maximized);
        Ok(())
    }

    #[test]
    fn app_paths_create_and_resolve_every_required_location()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths {
            config_dir: directory.path().join("config"),
            data_dir: directory.path().join("data"),
            cache_dir: directory.path().join("cache"),
        };

        paths.ensure_exists()?;

        assert_eq!(paths.config_file(), paths.config_dir.join("config.toml"));
        assert_eq!(
            paths.database_file(),
            paths.data_dir.join("library.sqlite3")
        );
        assert_eq!(paths.assets_dir(), paths.data_dir.join("assets"));
        assert_eq!(
            paths.remote_image_cache_dir(),
            paths.cache_dir.join("remote-images")
        );
        assert!(paths.config_dir.is_dir());
        assert!(paths.assets_dir().is_dir());
        assert!(paths.remote_image_cache_dir().is_dir());
        Ok(())
    }

    #[test]
    fn saved_config_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let mut config = Config::default();
        config.window.sidebar_collapsed = true;
        config.editor.source_split_view = true;
        save(&path, &config)?;
        assert_eq!(load(&path)?, config);
        Ok(())
    }

    #[test]
    fn saved_config_uses_readable_table_sections() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");

        save(&path, &Config::default())?;

        let source = fs::read_to_string(path)?;
        assert!(source.contains("[editor]"));
        assert!(source.contains("[images]"));
        assert!(source.contains("[search]"));
        assert!(source.contains("[window]"));
        Ok(())
    }

    #[test]
    fn saving_legacy_config_removes_the_obsolete_onboarding_section()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        fs::write(&path, "[onboarding]\neditor_format_chosen = true\n")?;

        save(&path, &load(&path)?)?;

        assert!(!fs::read_to_string(path)?.contains("[onboarding]"));
        Ok(())
    }

    #[test]
    fn load_rejects_unknown_editor_mode() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        fs::write(&path, "[editor]\ndefault_mode = 'preview'\n")?;
        let result = load(&path);
        assert!(matches!(result, Err(ConfigError::InvalidToml(_))));
        Ok(())
    }
}
