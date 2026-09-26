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
    assert!(!config.editor.show_document_sidebar);
    assert!(!config.editor.source_line_numbers);
    assert!(!config.editor.source_highlight_current_line);
    assert_eq!(
        config.editor.source_syntax_style,
        SourceSyntaxStyle::Detailed
    );
    assert!(config.editor.show_formatting_toolbar);
    assert_eq!(config.editor.source_font, None);
    assert_eq!(config.editor.document_font, None);
    assert_eq!(config.editor.document_line_height_percent, 155);
    assert_eq!(config.editor.document_width, DocumentWidth::Comfortable);
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
    config.editor.show_document_sidebar = true;
    config.editor.source_line_numbers = true;
    config.editor.source_highlight_current_line = true;
    config.editor.source_syntax_style = SourceSyntaxStyle::WritingFocus;
    config.editor.show_formatting_toolbar = false;
    config.editor.source_font = Some("Adwaita Mono 13".to_owned());
    config.editor.document_font = Some("Cantarell Semi-Bold Italic 14".to_owned());
    config.editor.document_line_height_percent = 175;
    config.editor.document_width = DocumentWidth::Wide;
    save(&path, &config)?;
    assert_eq!(load(&path)?, config);
    Ok(())
}

#[test]
fn legacy_syntax_highlighting_boolean_migrates_to_a_syntax_style()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "[editor]\nsource_syntax_highlighting = false\n")?;

    assert_eq!(
        load(&path)?.editor.source_syntax_style,
        SourceSyntaxStyle::None
    );

    save(&path, &load(&path)?)?;
    let source = fs::read_to_string(path)?;
    assert!(source.contains("source_syntax_style = \"none\""));
    assert!(!source.contains("source_syntax_highlighting"));
    Ok(())
}

#[test]
fn enabled_legacy_syntax_highlighting_migrates_to_detailed()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "[editor]\nsource_syntax_highlighting = true\n")?;

    assert_eq!(
        load(&path)?.editor.source_syntax_style,
        SourceSyntaxStyle::Detailed
    );
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
    assert!(!source.contains("source_font"));
    assert!(!source.contains("document_font"));
    Ok(())
}

#[test]
fn document_line_height_should_clamp_legacy_out_of_range_values()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(
        &path,
        "[editor]\ndocument_line_height_percent = 999\ndocument_width = 'narrow'\n",
    )?;

    let config = load(&path)?;
    assert_eq!(config.editor.document_line_height_percent, 250);
    assert_eq!(config.editor.document_width, DocumentWidth::Narrow);

    fs::write(&path, "[editor]\ndocument_line_height_percent = -1\n")?;
    assert_eq!(load(&path)?.editor.document_line_height_percent, 100);
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

#[test]
fn document_sidebar_preference_should_keep_the_legacy_toml_key()
-> Result<(), Box<dyn std::error::Error>> {
    let config: Config = toml::from_str("[editor]\nshow_media_sidebar = true\n")?;
    assert!(config.editor.show_document_sidebar);
    let serialized = toml::to_string(&config)?;
    assert!(serialized.contains("show_media_sidebar = true"));
    assert!(!serialized.contains("show_document_sidebar"));
    Ok(())
}
#[test]
fn enhanced_rendering_should_default_on_for_existing_configuration() {
    let config: super::Config = toml::from_str("[editor]\nsource_split_view = true\n")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(config.editor.enhanced_carve_rendering);
    assert!(super::Config::default().editor.enhanced_carve_rendering);
}

#[test]
fn enhanced_rendering_should_persist_disabled_preference() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = directory.path().join("config.toml");
    let mut config = super::Config::default();
    config.editor.enhanced_carve_rendering = false;
    super::save(&path, &config).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        !super::load(&path)
            .unwrap_or_else(|error| panic!("{error}"))
            .editor
            .enhanced_carve_rendering
    );
    assert!(
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{error}"))
            .contains("enhanced_carve_rendering = false")
    );
}

#[test]
fn document_properties_should_default_to_disabled_with_the_floating_button() {
    let config = Config::default();
    assert!(!config.document_properties.enabled);
    assert!(config.document_properties.floating_button);
    assert!(config.document_properties.entries.is_empty());
}

#[test]
fn partial_config_should_keep_document_property_defaults() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "[document_properties]\nenabled = true\n")?;

    let config = load(&path)?;
    assert!(config.document_properties.enabled);
    assert!(config.document_properties.floating_button);
    Ok(())
}

#[test]
fn document_properties_should_round_trip_typed_entries() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    let mut config = Config::default();
    config.document_properties.enabled = true;
    config.document_properties.entries = vec![
        DocumentProperty {
            key: String::from("author"),
            field_type: DocumentPropertyType::Text,
            multiple: false,
            value: serde_json::Value::String(String::from("Jane")),
        },
        DocumentProperty {
            key: String::from("tags"),
            field_type: DocumentPropertyType::List,
            multiple: false,
            value: serde_json::json!(["rust", "gtk"]),
        },
        DocumentProperty {
            key: String::from("summary"),
            field_type: DocumentPropertyType::LongText,
            multiple: false,
            value: serde_json::Value::String(String::new()),
        },
    ];
    save(&path, &config)?;

    let source = fs::read_to_string(&path)?;
    assert!(source.contains("[document_properties]"));
    assert!(source.contains("field_type = \"text\""));
    assert!(source.contains("field_type = \"long-text\""));

    assert_eq!(load(&path)?, config);
    Ok(())
}

#[test]
fn document_properties_default_source_should_be_gated_by_enabled() {
    let mut config = DocumentPropertiesConfig {
        enabled: false,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![DocumentProperty {
            key: String::from("author"),
            field_type: DocumentPropertyType::Text,
            multiple: false,
            value: serde_json::Value::String(String::from("Jane")),
        }],
    };
    assert_eq!(config.default_source(), "");

    config.enabled = true;
    assert_eq!(config.default_source(), "---\nauthor: Jane\n---\n");
}

#[test]
fn document_properties_should_reject_reserved_and_mismatched_entries() {
    let property = |key: &str, field_type: DocumentPropertyType| DocumentProperty {
        key: key.to_owned(),
        field_type,
        multiple: false,
        value: serde_json::Value::Null,
    };

    let reserved = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![property("title", DocumentPropertyType::Text)],
    };
    assert!(reserved.validate().is_err());

    let mismatched = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![DocumentProperty {
            key: String::from("count"),
            field_type: DocumentPropertyType::Number,
            multiple: false,
            value: serde_json::Value::String(String::from("three")),
        }],
    };
    assert!(mismatched.validate().is_err());

    let duplicated = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![
            property("author", DocumentPropertyType::Text),
            property("author", DocumentPropertyType::Text),
        ],
    };
    assert!(duplicated.validate().is_err());
}

#[test]
fn loading_should_reject_a_reserved_default_property() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(
        &path,
        "[[document_properties.entries]]\nkey = \"title\"\nfield_type = \"text\"\nvalue = \"X\"\n",
    )?;

    assert!(matches!(
        load(&path),
        Err(ConfigError::InvalidDocumentProperties(_))
    ));
    Ok(())
}

#[test]
fn document_properties_list_defaults_should_seed_the_first_option() {
    let list = |multiple: bool| DocumentProperty {
        key: String::from("status"),
        field_type: DocumentPropertyType::List,
        multiple,
        value: serde_json::json!(["active", "archived"]),
    };

    let single = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![list(false)],
    };
    assert_eq!(single.default_source(), "---\nstatus: active\n---\n");

    let multi = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![list(true)],
    };
    assert_eq!(multi.default_source(), "---\nstatus:\n- active\n---\n");
}

#[test]
fn document_properties_list_without_options_should_seed_nothing() {
    let config = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![DocumentProperty {
            key: String::from("status"),
            field_type: DocumentPropertyType::List,
            multiple: false,
            value: serde_json::json!([]),
        }],
    };
    assert_eq!(config.default_source(), "");
}

#[test]
fn document_properties_should_reject_invalid_list_options() {
    let non_string = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![DocumentProperty {
            key: String::from("status"),
            field_type: DocumentPropertyType::List,
            multiple: false,
            value: serde_json::json!(["ok", 3]),
        }],
    };
    assert!(non_string.validate().is_err());

    let multiple_on_text = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Yaml,
        entries: vec![DocumentProperty {
            key: String::from("author"),
            field_type: DocumentPropertyType::Text,
            multiple: true,
            value: serde_json::Value::String(String::from("Jane")),
        }],
    };
    assert!(multiple_on_text.validate().is_err());
}

#[test]
fn document_properties_default_source_should_use_the_configured_format() {
    let mut config = DocumentPropertiesConfig {
        enabled: true,
        floating_button: true,
        format: FrontmatterFormat::Json,
        entries: vec![DocumentProperty {
            key: String::from("author"),
            field_type: DocumentPropertyType::Text,
            multiple: false,
            value: serde_json::Value::String(String::from("Jane")),
        }],
    };
    let json = config.default_source();
    assert!(json.starts_with("---json\n"), "{json}");
    assert!(json.contains("\"author\": \"Jane\""), "{json}");

    config.format = FrontmatterFormat::Yaml;
    assert_eq!(config.default_source(), "---\nauthor: Jane\n---\n");
}

#[test]
fn document_properties_date_defaults_should_seed_the_configured_moment() {
    let now = time::macros::datetime!(2023-11-14 22:13:20 UTC);
    let date = DocumentProperty {
        key: String::from("due"),
        field_type: DocumentPropertyType::Date,
        multiple: false,
        value: serde_json::Value::Null,
    };
    let date_time = DocumentProperty {
        key: String::from("at"),
        field_type: DocumentPropertyType::DateTime,
        multiple: false,
        value: serde_json::Value::Null,
    };

    assert_eq!(
        date.default_field_at(now).map(|field| field.value),
        Some(FrontmatterValue::Text(String::from("2023-11-14")))
    );
    assert_eq!(
        date_time.default_field_at(now).map(|field| field.value),
        Some(FrontmatterValue::Text(String::from("2023-11-14T22:13:20Z")))
    );
}

#[test]
fn document_properties_date_time_defaults_should_drop_subsecond_precision() {
    let now = time::macros::datetime!(2023-11-14 22:13:20.123456789 UTC);
    let property = DocumentProperty {
        key: String::from("at"),
        field_type: DocumentPropertyType::DateTime,
        multiple: false,
        value: serde_json::Value::Null,
    };
    assert_eq!(
        property.default_field_at(now).map(|field| field.value),
        Some(FrontmatterValue::Text(String::from("2023-11-14T22:13:20Z")))
    );
}
