use super::*;

#[test]
fn rendered_document_blocks_network_images_when_disabled() {
    let html = rendered_document("![remote](https://example.test/image.png)", false);
    assert!(html.contains("img-src data: carver-asset:"));
    assert!(html.contains("script-src 'none'"));
}

#[test]
fn rendered_document_keeps_full_carve_table_output() {
    let html = rendered_document("|= Name |= Value |\n| One | Two |", false);
    assert!(html.contains("<table"));
    assert!(!html.contains("<link"));
    assert!(PREVIEW_STYLESHEET.contains("border-collapse: collapse;\n  width: 100%"));
}

#[test]
fn rendered_document_matches_the_editor_block_presentation() {
    let html = rendered_document("# Heading\n\n- [x] Complete", false);
    assert!(html.contains("<style>"));
    assert!(PREVIEW_STYLESHEET.contains("h1 {\n  font-size: 2em;"));
    assert!(PREVIEW_STYLESHEET.contains("ul.task-list"));
    assert!(PREVIEW_STYLESHEET.contains("ul:has(> li > input[type=checkbox])"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] > img"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] th"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview]::selection"));
    assert!(PREVIEW_STYLESHEET.contains("--document-content-width"));
}

#[test]
fn shared_document_inset_should_live_on_the_body() {
    assert!(PREVIEW_STYLESHEET.contains("body {\n  box-sizing: border-box;"));
    assert!(PREVIEW_STYLESHEET.contains("padding: 24px;"));
    assert!(PREVIEW_STYLESHEET.contains("min-height: calc(100vh - 48px);"));
}

#[test]
fn rendered_document_should_constrain_preview_on_the_body_content_box() {
    let html = rendered_document("![image](assets/example.png){width=\"50%\"}", false);

    assert!(
        html.contains("<body data-preview data-carver-heading-token=\"") && html.contains("><img")
    );
    assert!(!html.contains("preview-content"));
    assert!(html.contains("width=\"50%\""));
    assert!(
        PREVIEW_STYLESHEET
            .contains("body[data-preview] {\n  max-width: var(--document-content-outer-width);")
    );
    assert!(PREVIEW_STYLESHEET.contains("margin-inline: auto;"));
}

#[test]
fn rendered_document_should_include_the_shared_document_appearance() {
    let html = rendered_document("Preview", false);

    assert!(html.contains("--document-font-family"));
    assert!(html.contains("--document-line-height: 1.55"));
    assert!(html.contains("--document-content-width: 80ch"));
    assert!(html.contains("--document-content-outer-width: calc(80ch + 48px)"));
}

#[test]
fn rendered_document_should_keep_preview_selection_colors_with_custom_appearance() {
    let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
    let theme = super::super::web::editor_theme(false, &accent);
    let appearance = super::super::web::document_appearance(&crate::mvu::DocumentPreferences {
        font: Some("Cantarell Bold Italic 14".to_owned()),
        line_height_percent: 175,
        width: carver_config::DocumentWidth::Wide,
    });

    let html = rendered_document_with_theme("Preview", false, &theme, &appearance);

    assert!(html.contains("--preview-accent-color: #358e45"));
    assert!(html.contains("--preview-selection-background: rgb(53 142 69 / 25%)"));
    assert!(html.contains("--preview-selection-foreground: #333334"));
    assert!(html.contains("--document-font-family: \"Cantarell\""));
    assert!(html.contains("--document-line-height: 1.75"));
    assert!(html.contains("--document-content-width: 100ch"));
    assert!(html.contains("--document-content-outer-width: calc(100ch + 48px)"));
    assert!(html.contains("data-carver-heading-token=\""));
    assert!(html.contains("<style>"));
    let body_tag = html
        .split("<body")
        .nth(1)
        .and_then(|body| body.split('>').next())
        .unwrap_or_default();
    assert!(!body_tag.contains("style="));
}

#[test]
fn preview_styles_should_include_document_appearance_in_the_document_head() {
    let accent = gtk::gdk::RGBA::new(0.208, 0.557, 0.271, 1.0);
    let theme = super::super::web::editor_theme(false, &accent);
    let appearance = super::super::web::document_appearance(&crate::mvu::DocumentPreferences {
        font: Some("Cantarell Bold Italic 14".to_owned()),
        line_height_percent: 175,
        width: carver_config::DocumentWidth::Wide,
    });
    let style = super::preview_document_style(&theme, &appearance);

    assert!(style.contains("--document-font-family: \"Cantarell\""));
    assert!(style.contains("--preview-selection-background: rgb(53 142 69 / 25%)"));
}

#[test]
fn rendered_document_routes_managed_assets_through_the_restricted_scheme() {
    let html = rendered_document("![image](assets/example.png)", false);
    assert!(html.contains("carver-asset:///assets/example.png"));
}

#[test]
fn asset_uri_rejects_parent_directory_paths() {
    assert_eq!(asset_filename("/assets/example.png"), Some("example.png"));
    assert_eq!(asset_filename("/assets/../library.sqlite3"), None);
}

#[test]
fn external_links_should_only_route_web_uris_to_the_desktop_browser() {
    assert!(super::is_external_link("https://example.com/path"));
    assert!(super::is_external_link("http://example.com"));
    assert!(!super::is_external_link("carver-preview://document/"));
    assert!(!super::is_external_link("carver-asset:///assets/image.png"));
    assert!(!super::is_external_link("file:///home/example/note.carve"));
}

#[test]
fn rendered_document_uses_the_active_dark_palette() {
    let html = rendered_document_for_theme("# Heading", false, true);
    assert!(html.contains("data-theme=\"dark\""));
    assert!(PREVIEW_STYLESHEET.contains("--document-background: #1d1d20"));
}

#[test]
fn rendered_document_uses_the_native_view_background_when_provided() {
    assert!(PREVIEW_STYLESHEET.contains("--document-background: #1d1d20"));
}

#[test]
fn preview_stylesheet_should_use_a_compact_document_print_layout() {
    assert!(PREVIEW_STYLESHEET.contains("@media print"));
    assert!(PREVIEW_STYLESHEET.contains("background: #ffffff !important"));
    assert!(PREVIEW_STYLESHEET.contains("font-size: 10.5pt"));
    assert!(PREVIEW_STYLESHEET.contains("font-stretch: normal"));
    assert!(PREVIEW_STYLESHEET.contains("font-variant: normal"));
    assert!(PREVIEW_STYLESHEET.contains("font-variation-settings: normal"));
    assert!(PREVIEW_STYLESHEET.contains("font-size: 21pt"));
    assert!(
        PREVIEW_STYLESHEET.contains("body[data-preview] {\n    padding: 0;\n    line-height: 1.4")
    );
    assert!(
        PREVIEW_STYLESHEET
            .contains("body[data-preview] {\n    max-width: none;\n    line-height: 1.4")
    );
    assert!(PREVIEW_STYLESHEET.contains("display: table-header-group"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] ul.task-list li"));
    assert!(PREVIEW_STYLESHEET.contains("white-space: pre-wrap"));
    assert!(PREVIEW_STYLESHEET.contains("padding: 4pt 5pt"));
}

#[test]
fn rendered_headings_should_carry_per_render_provenance() {
    let html = rendered_document("# First\n\n> ## Nested", false);
    let token = html
        .split("data-carver-heading-token=\"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .unwrap_or_default();
    assert!(!token.is_empty());
    assert_eq!(
        html.matches(&format!("data-carver-heading=\"{token}\""))
            .count(),
        2
    );
    assert!(!rendered_document("# First", false).contains(token));
}

#[test]
fn raw_html_headings_should_not_share_renderer_provenance() {
    let html = rendered_document(
        "```=html\n<h2 data-source-line=\"1\" data-carver-heading=\"forged\">Raw</h2>\n```\n\n# Authored",
        false,
    );
    assert!(html.contains("data-carver-heading=\"forged\">Raw</h2>"));
    assert_eq!(html.matches("data-carver-heading=\"").count(), 2);
    assert!(!html.contains("data-carver-heading-token=\"forged\""));
}

#[test]
fn raw_html_should_not_escape_preview_layout_rules() {
    let html = rendered_document("```=html\n</main>\n```\n\n# Authored", false);

    assert!(!html.contains("<main"));
    assert!(html.contains("</main>"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] {\n  max-width:"));
}
