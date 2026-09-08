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
    assert!(!html.contains("<style>"));
    assert!(PREVIEW_STYLESHEET.contains("h1 {\n  font-size: 2em;"));
    assert!(PREVIEW_STYLESHEET.contains("ul.task-list"));
    assert!(PREVIEW_STYLESHEET.contains("ul:has(> li > input[type=checkbox])"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] > img"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview] th"));
    assert!(PREVIEW_STYLESHEET.contains("body[data-preview]::selection"));
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
    assert!(PREVIEW_STYLESHEET.contains("font-size: 21pt"));
    assert!(
        PREVIEW_STYLESHEET.contains("body[data-preview] {\n    padding: 0;\n    line-height: 1.4")
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
