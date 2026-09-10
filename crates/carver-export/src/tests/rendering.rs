use super::*;

const SOURCE: &str = "::: toc\n:::\n\n# Heading\n\n::: details \"More\"\nhttps://example.com\n:::";

#[test]
fn html_export_should_apply_the_selected_profile() {
    let artifact = prepare_export_with_profile(
        SOURCE,
        "note",
        ExportFormat::Html,
        false,
        &[],
        HtmlProfile::Enhanced,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let html = String::from_utf8(artifact.bytes).unwrap_or_else(|error| panic!("{error}"));
    assert!(html.contains("<nav class=\"toc\""));
    assert!(html.contains("<details>"));
    assert!(html.contains("href=\"https://example.com\""));
    assert!(!html.contains("data-carver-heading"));
}

#[test]
fn legacy_html_export_should_keep_core_behavior() {
    let artifact = prepare_export(SOURCE, "note", ExportFormat::Html, false, &[])
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        !String::from_utf8(artifact.bytes)
            .unwrap_or_else(|error| panic!("{error}"))
            .contains("<nav")
    );
}

#[test]
fn source_exports_should_ignore_the_html_profile() {
    for format in [ExportFormat::Carve, ExportFormat::Markdown] {
        let core = prepare_export(SOURCE, "note", format, false, &[])
            .unwrap_or_else(|error| panic!("{error}"));
        let enhanced =
            prepare_export_with_profile(SOURCE, "note", format, false, &[], HtmlProfile::Enhanced)
                .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(core, enhanced);
    }
}

#[test]
fn portable_html_export_should_match_direct_export() {
    use std::io::Read;
    let direct = prepare_export_with_profile(
        SOURCE,
        "note",
        ExportFormat::Html,
        false,
        &[],
        HtmlProfile::Enhanced,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let archive = prepare_export_with_profile(
        SOURCE,
        "note",
        ExportFormat::Html,
        true,
        &[],
        HtmlProfile::Enhanced,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let mut zip =
        zip::ZipArchive::new(Cursor::new(archive.bytes)).unwrap_or_else(|error| panic!("{error}"));
    let mut html = Vec::new();
    zip.by_name("note.html")
        .unwrap_or_else(|error| panic!("{error}"))
        .read_to_end(&mut html)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(html, direct.bytes);
}
