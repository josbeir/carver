//! Display-backed editor preferences dialog persistence coverage.
use super::*;

pub(super) fn document_appearance_should_persist(fixture: &WindowFixture) -> TestResult {
    let application = fixture.application.clone();
    let client = fixture.client.clone();
    let config = fixture.config.clone();
    let config_path = fixture.config_path.clone();
    let temporary_directory = &fixture.directory;
    let preferences_dialog = fixture.preferences_dialog.clone();
    let preferences_runtime = fixture.preferences_runtime.clone();
    let formatting_toolbar_setting = widget_as::<adw::SwitchRow>(
        preferences_dialog.upcast_ref(),
        "formatting-toolbar-setting",
    )
    .ok_or("formatting toolbar setting")?;
    assert!(formatting_toolbar_setting.is_active());
    let enhancements = widget_as::<adw::SwitchRow>(
        preferences_dialog.upcast_ref(),
        "enhanced-carve-rendering-setting",
    )
    .ok_or("enhancements")?;
    assert!(enhancements.is_active());
    enhancements.set_active(false);
    assert!(run_main_context_until(|| carver_config::load(&config_path)
        .is_ok_and(|config| !config.editor.enhanced_carve_rendering)));
    assert_eq!(
        preferences_runtime.model().preferences.html_profile,
        carver_domain::rendering::HtmlProfile::Core
    );
    assert_eq!(
        formatting_toolbar_setting.subtitle(),
        Some("Show formatting controls at the bottom of the editor.".into())
    );
    assert_eq!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "document-font-setting")
            .map(|row| row.title()),
        Some("Document font".into())
    );
    assert!(
        widget_as::<gtk::Label>(preferences_dialog.upcast_ref(), "document-font-value").is_some()
    );
    let document_line_height = widget_as::<adw::SpinRow>(
        preferences_dialog.upcast_ref(),
        "document-line-height-setting",
    )
    .ok_or("document line spacing setting")?;
    assert!((document_line_height.value() - 1.55).abs() < f64::EPSILON);
    assert_eq!(
        widget_as::<adw::ComboRow>(preferences_dialog.upcast_ref(), "document-width-setting")
            .map(|row| row.selected()),
        Some(1)
    );
    document_line_height.set_value(1.75);
    let document_width =
        widget_as::<adw::ComboRow>(preferences_dialog.upcast_ref(), "document-width-setting")
            .ok_or("document width setting")?;
    document_width.set_selected(2);
    assert!(run_main_context_until(|| {
        carver_config::load(&config_path).is_ok_and(|persisted| {
            persisted.editor.document_line_height_percent == 175
                && persisted.editor.document_width == carver_config::DocumentWidth::Wide
        })
    }));
    let document_appearance_reset = widget_as::<adw::ActionRow>(
        preferences_dialog.upcast_ref(),
        "document-appearance-reset-row",
    )
    .ok_or("document appearance reset")?;
    document_appearance_reset.emit_by_name::<()>("activated", &[]);
    assert!(run_main_context_until(|| {
        carver_config::load(&config_path).is_ok_and(|persisted| {
            persisted.editor.document_font.is_none()
                && persisted.editor.document_line_height_percent == 155
                && persisted.editor.document_width == carver_config::DocumentWidth::Comfortable
        })
    }));
    let mut purist_config = config.clone();
    purist_config.editor.show_formatting_toolbar = false;
    let purist_window = crate::app::build_window_for_test(
        &application,
        client.clone(),
        &purist_config,
        &temporary_directory.path().join("purist-config.toml"),
    )?;
    let purist_root = purist_window.child().ok_or("purist window content")?;
    assert!(
        widget_as::<gtk::Box>(&purist_root, "formatting-toolbar-bar")
            .is_some_and(|toolbar_bar| !toolbar_bar.is_visible())
    );
    purist_window.close();
    Ok(())
}

pub(super) fn source_preferences_should_toggle_gutter_and_font(
    fixture: &WindowFixture,
) -> TestResult {
    let window = fixture.window.clone();
    let config = fixture.config.clone();
    let preferences_dialog = fixture.preferences_dialog.clone();
    assert_eq!(
        widget_as::<adw::SwitchRow>(
            preferences_dialog.upcast_ref(),
            "source-line-numbers-setting"
        )
        .map(|row| row.subtitle()),
        Some(Some(
            "Show source line positions in the editor gutter.".into()
        ))
    );
    assert_eq!(
        widget_as::<adw::SwitchRow>(
            preferences_dialog.upcast_ref(),
            "source-current-line-setting"
        )
        .map(|row| row.subtitle()),
        Some(Some(
            "Shade the line containing the cursor in Source mode.".into()
        ))
    );
    let syntax_style = widget_as::<adw::ComboRow>(
        preferences_dialog.upcast_ref(),
        "source-syntax-style-setting",
    )
    .ok_or("source syntax style setting")?;
    assert_eq!(syntax_style.title(), "Syntax style");
    assert_eq!(
        syntax_style.subtitle(),
        Some("Choose how much markup colour appears in Source mode.".into())
    );
    assert_eq!(syntax_style.selected(), 1);
    assert_eq!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-setting")
            .map(|row| row.title()),
        Some("Source font".into())
    );
    assert!(
        widget_as::<gtk::Label>(preferences_dialog.upcast_ref(), "source-font-value").is_some()
    );
    assert!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-reset-row")
            .is_some_and(|row| !row.is_visible())
    );
    let source_font =
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-setting")
            .ok_or("source font setting")?;
    source_font.emit_by_name::<()>("activated", &[]);
    let mut custom_font_config = config.clone();
    custom_font_config.editor.source_font = Some("Adwaita Mono 13".to_owned());
    let (custom_font_preferences, _) = crate::ui::dialogs::present_dialogs_for_test(
        &window,
        &custom_font_config,
        &crate::mvu::AppDispatcher::default(),
    );
    let reset_font = widget_as::<adw::ActionRow>(
        custom_font_preferences.upcast_ref(),
        "source-font-reset-row",
    )
    .ok_or("source font reset")?;
    assert!(reset_font.is_visible());
    reset_font.emit_by_name::<()>("activated", &[]);
    assert!(!reset_font.is_visible());
    Ok(())
}

pub(super) fn document_properties_preferences_should_persist(
    fixture: &WindowFixture,
) -> TestResult {
    let window = fixture.window.clone();
    let config_path = fixture.config_path.clone();
    let preferences_dialog = fixture.preferences_dialog.clone();
    let document_properties_setting = widget_as::<adw::SwitchRow>(
        preferences_dialog.upcast_ref(),
        "document-properties-setting",
    )
    .ok_or("document properties setting")?;
    assert!(!document_properties_setting.is_active());
    let document_properties_floating = widget_as::<adw::SwitchRow>(
        preferences_dialog.upcast_ref(),
        "document-properties-floating-button",
    )
    .ok_or("document properties floating button")?;
    assert!(document_properties_floating.is_active());
    document_properties_floating.set_active(false);
    assert!(run_main_context_until(|| carver_config::load(&config_path)
        .is_ok_and(|config| !config
            .document_properties
            .floating_button)));
    assert_eq!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "document-properties-row")
            .and_then(|row| row.subtitle()),
        Some("0 default properties".into())
    );
    document_properties_setting.set_active(true);
    assert!(run_main_context_until(
        || carver_config::load(&config_path).is_ok_and(|config| config.document_properties.enabled)
    ));
    let document_properties_format = widget_as::<adw::ComboRow>(
        preferences_dialog.upcast_ref(),
        "document-properties-format",
    )
    .ok_or("document properties format")?;
    document_properties_format.set_selected(1);
    assert!(run_main_context_until(|| carver_config::load(&config_path)
        .is_ok_and(
            |config| config.document_properties.format == carver_domain::FrontmatterFormat::Json
        )));
    assert_eq!(
        window.icon_name().as_deref(),
        Some(crate::app::APPLICATION_ICON)
    );
    Ok(())
}
