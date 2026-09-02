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
fn partial_config_keeps_defaults_for_unset_sections() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(
        &path,
        "[editor]\ndefault_mode = 'source'\n\n[window]\nmaximized = true\n",
    )?;

    let config = load(&path)?;

    assert_eq!(config.editor.last_mode, EditorMode::Source);
    assert_eq!(config.editor.autosave_delay_ms, 500);
    assert!(!config.editor.source_split_view);
    assert!(config.images.load_remote_automatically);
    assert_eq!(config.window.width, 1120);
    assert_eq!(config.window.height, 760);
    assert!(config.window.maximized);
    Ok(())
}

#[test]
fn app_paths_create_and_resolve_every_required_location() -> Result<(), Box<dyn std::error::Error>>
{
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
fn saving_a_legacy_editor_mode_migrates_to_last_mode() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "[editor]\ndefault_mode = 'source'\n")?;

    save(&path, &load(&path)?)?;

    let source = fs::read_to_string(path)?;
    assert!(source.contains("last_mode = \"source\""));
    assert!(!source.contains("default_mode"));
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
    fs::write(&path, "[editor]\nlast_mode = 'not-a-mode'\n")?;
    let result = load(&path);
    assert!(matches!(result, Err(ConfigError::InvalidToml(_))));
    Ok(())
}
