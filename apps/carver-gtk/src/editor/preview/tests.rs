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
fn rendered_document_uses_the_active_dark_palette() {
    let html = rendered_document_for_theme("# Heading", false, true);
    assert!(html.contains("background:#1e1e1e"));
    assert!(html.contains("color-scheme:dark"));
}
