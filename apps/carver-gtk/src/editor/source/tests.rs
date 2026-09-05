use std::fs;

use super::{
    FALLBACK_MONOSPACE_FONT, install_syntax_assets, normalize_source_font_description,
    source_font_css, system_monospace_font_from_settings,
};

#[test]
fn source_font_description_should_keep_only_family_and_point_size() {
    assert_eq!(
        normalize_source_font_description("Adwaita Mono Semi-Bold 13"),
        Some("Adwaita Mono 13".to_owned())
    );
}

#[test]
fn source_font_description_should_reject_missing_or_absolute_sizes() {
    assert_eq!(normalize_source_font_description("Monospace"), None);
    assert_eq!(normalize_source_font_description("Monospace 12px"), None);
}

#[test]
fn source_font_css_should_scope_the_selected_family_and_size_to_source_mode() {
    assert_eq!(
        source_font_css("JetBrains Mono 12"),
        "#source-editor { font-family: \"JetBrains Mono\"; font-size: 12pt; }"
    );
}

#[test]
fn unavailable_desktop_font_setting_should_use_a_stable_monospace_fallback() {
    assert_eq!(
        system_monospace_font_from_settings(None),
        FALLBACK_MONOSPACE_FONT
    );
}

#[test]
fn syntax_assets_should_replace_stale_embedded_grammar() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let syntax_dir = install_syntax_assets(directory.path())?;
    fs::write(syntax_dir.join("carve.lang"), "stale")?;

    install_syntax_assets(directory.path())?;

    assert!(fs::read_to_string(syntax_dir.join("carve.lang"))?.contains("id=\"carve\""));
    Ok(())
}

#[test]
fn syntax_style_schemes_should_inherit_gnome_adwaita_variants()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let syntax_dir = install_syntax_assets(directory.path())?;

    assert!(
        fs::read_to_string(syntax_dir.join("carve-light.xml"))?
            .contains("parent-scheme=\"Adwaita\"")
    );
    assert!(
        fs::read_to_string(syntax_dir.join("carve-dark.xml"))?
            .contains("parent-scheme=\"Adwaita-dark\"")
    );
    let writing_focus_light = fs::read_to_string(syntax_dir.join("carve-writing-focus-light.xml"))?;
    assert!(writing_focus_light.contains("parent-scheme=\"Adwaita\""));
    assert!(writing_focus_light.contains("name=\"carve:heading\" foreground=\"#2b6f9e\""));
    let writing_focus_dark = fs::read_to_string(syntax_dir.join("carve-writing-focus-dark.xml"))?;
    assert!(writing_focus_dark.contains("parent-scheme=\"Adwaita-dark\""));
    assert!(writing_focus_dark.contains("name=\"carve:link-text\" foreground=\"#8ebddd\""));
    Ok(())
}
