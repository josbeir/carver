use std::fs;

use super::{
    FALLBACK_DOCUMENT_FONT, FALLBACK_MONOSPACE_FONT, document_font_css, document_font_weight,
    escape_css_string, install_syntax_assets, normalize_document_font_description,
    normalize_source_font_description, source_font_css, system_document_font_from_settings,
    system_monospace_font_from_settings,
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
fn document_font_description_should_preserve_the_selected_face_and_size() {
    assert_eq!(
        normalize_document_font_description("Cantarell Semi-Bold Italic 14"),
        Some("Cantarell Semi-Bold Italic 14".to_owned())
    );
    assert_eq!(normalize_document_font_description("Sans 12px"), None);
}

#[test]
fn document_font_css_should_keep_the_selected_face() {
    assert_eq!(
        document_font_css("Cantarell Bold Italic 14"),
        "--document-font-family: \"Cantarell\"; --document-font-size: 14pt; --document-font-style: italic; --document-font-weight: 700;"
    );
}

#[test]
fn document_font_css_should_fall_back_to_a_stable_normal_face() {
    assert_eq!(
        document_font_css("Cantarell Oblique 12"),
        "--document-font-family: \"Cantarell\"; --document-font-size: 12pt; --document-font-style: oblique; --document-font-weight: 400;"
    );
    assert_eq!(
        document_font_css("Invalid font"),
        "--document-font-family: \"Sans\"; --document-font-size: 12pt; --document-font-style: normal; --document-font-weight: 400;"
    );
}

#[test]
fn document_font_weight_should_map_supported_pango_weights_to_css_values() {
    for (weight, expected) in [
        (gtk::pango::Weight::Thin, 100),
        (gtk::pango::Weight::Ultralight, 200),
        (gtk::pango::Weight::Light, 300),
        (gtk::pango::Weight::Semilight, 350),
        (gtk::pango::Weight::Book, 380),
        (gtk::pango::Weight::Medium, 500),
        (gtk::pango::Weight::Semibold, 600),
        (gtk::pango::Weight::Bold, 700),
        (gtk::pango::Weight::Ultrabold, 800),
        (gtk::pango::Weight::Heavy, 900),
        (gtk::pango::Weight::Ultraheavy, 1000),
    ] {
        assert_eq!(document_font_weight(weight), expected);
    }
}

#[test]
fn css_string_escaping_should_preserve_special_characters_inside_font_names() {
    let mut expected = String::from("A");
    expected.push('\\');
    expected.push('\\');
    expected.push('\\');
    expected.push('"');
    expected.push('\\');
    expected.push('a');
    expected.push(' ');
    expected.push('\\');
    expected.push('d');
    expected.push(' ');
    expected.push('\\');
    expected.push('c');
    expected.push(' ');

    assert_eq!(escape_css_string("A\\\"\n\r\u{c}"), expected);
}

#[test]
fn unavailable_desktop_font_setting_should_use_a_stable_monospace_fallback() {
    assert_eq!(
        system_monospace_font_from_settings(None),
        FALLBACK_MONOSPACE_FONT
    );
}

#[test]
fn unavailable_desktop_document_font_setting_should_use_a_stable_fallback() {
    assert_eq!(
        system_document_font_from_settings(None),
        FALLBACK_DOCUMENT_FONT
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
fn syntax_grammar_should_define_each_heading_level() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let syntax_dir = install_syntax_assets(directory.path())?;
    let grammar = fs::read_to_string(syntax_dir.join("carve.lang"))?;

    for (level, scale) in [
        (1, "def:heading1"),
        (2, "def:heading2"),
        (3, "def:heading3"),
        (4, "def:heading4"),
        (5, "def:heading5"),
        (6, "def:heading6"),
    ] {
        assert!(grammar.contains(&format!(
            "id=\"heading-{level}\" name=\"Heading {level}\" map-to=\"{scale}\""
        )));
        assert!(grammar.contains(&format!(
            "style-ref=\"heading-{level}\" class=\"carve-heading carve-heading-{level}\""
        )));
    }
    Ok(())
}

#[test]
fn syntax_style_schemes_should_scale_each_heading_level() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let syntax_dir = install_syntax_assets(directory.path())?;

    for style_name in [
        "carve-light.xml",
        "carve-dark.xml",
        "carve-writing-focus-light.xml",
        "carve-writing-focus-dark.xml",
    ] {
        let style_scheme = fs::read_to_string(syntax_dir.join(style_name))?;
        for (level, scale) in [
            (1, "1.45"),
            (2, "1.30"),
            (3, "1.18"),
            (4, "1.10"),
            (5, "1.04"),
            (6, "1.00"),
        ] {
            assert!(style_scheme.lines().any(|line| {
                line.contains(&format!("name=\"carve:heading-{level}\""))
                    && line.contains("bold=\"true\"")
                    && line.contains(&format!("scale=\"{scale}\""))
            }));
        }
    }
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
    assert!(writing_focus_light.contains("name=\"carve:heading-1\" foreground=\"#2b6f9e\""));
    let writing_focus_dark = fs::read_to_string(syntax_dir.join("carve-writing-focus-dark.xml"))?;
    assert!(writing_focus_dark.contains("parent-scheme=\"Adwaita-dark\""));
    assert!(writing_focus_dark.contains("name=\"carve:link-text\" foreground=\"#8ebddd\""));
    Ok(())
}
