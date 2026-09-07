use super::*;

#[test]
fn source_line_number_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceLineNumbers(true)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_line_numbers)
    );
    assert!(model.preferences.source_editor.show_line_numbers);
}

#[test]
fn formatting_toolbar_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetFormattingToolbarVisible(false)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if !config.editor.show_formatting_toolbar)
    );
    assert!(!model.preferences.show_formatting_toolbar);
}

#[test]
fn current_line_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceHighlightCurrentLine(true)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_highlight_current_line)
    );
    assert!(model.preferences.source_editor.highlight_current_line);
}

#[test]
fn writing_focus_syntax_style_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceSyntaxStyle(
            carver_config::SourceSyntaxStyle::WritingFocus,
        )),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_syntax_style == carver_config::SourceSyntaxStyle::WritingFocus)
    );
    assert_eq!(
        model.preferences.source_editor.syntax_style,
        carver_config::SourceSyntaxStyle::WritingFocus
    );
}

#[test]
fn source_font_preference_should_persist_the_selected_font() {
    let mut model = AppModel::new(&Config::default());
    let font = "Adwaita Mono 13".to_owned();

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceFont(Some(font.clone()))),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_font.as_deref() == Some("Adwaita Mono 13"))
    );
    assert_eq!(model.preferences.source_editor.font, Some(font));
}

#[test]
fn source_font_reset_should_restore_the_system_font_setting() {
    let mut config = Config::default();
    config.editor.source_font = Some("Adwaita Mono 13".to_owned());
    let mut model = AppModel::new(&config);

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceFont(None)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_font.is_none())
    );
    assert_eq!(model.preferences.source_editor.font, None);
}

#[test]
fn close_geometry_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Window(super::WindowMsg::SaveGeometry {
            width: 900,
            height: 640,
            maximized: true,
        }),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.window.width == 900 && config.window.height == 640 && config.window.maximized)
    );
}
