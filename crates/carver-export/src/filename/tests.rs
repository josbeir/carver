use super::sanitized_component;

#[test]
fn component_should_replace_portability_restricted_characters() {
    assert_eq!(
        sanitized_component("a/b\\c:d?e*f|g<h>i\"j", 255),
        "a-b-c-d-e-f-g-h-i-j"
    );
}

#[test]
fn component_should_replace_reserved_device_names_on_unix_too() {
    assert_eq!(sanitized_component("CON.txt", 255), "-");
}

#[test]
fn component_should_leave_an_empty_name_for_the_callers_fallback() {
    assert_eq!(sanitized_component(". \n\t", 255), "");
}

#[test]
fn component_should_fit_multibyte_text_within_the_byte_budget() {
    assert_eq!(sanitized_component("測試文件", 8), "測試");
}

#[test]
fn component_should_sanitize_reserved_names_exposed_by_truncation() {
    assert_eq!(sanitized_component("CONnection", 3), "-");
}

#[test]
fn component_should_remove_trailing_dots_exposed_by_truncation() {
    assert_eq!(sanitized_component("abc.def", 4), "abc");
}

#[test]
fn component_should_be_empty_when_no_bytes_are_available() {
    assert_eq!(sanitized_component("abc", 0), "");
}

#[test]
fn export_stem_should_reserve_space_for_the_longest_extension()
-> Result<(), Box<dyn std::error::Error>> {
    let stem = crate::sanitized_filename_stem(&"測".repeat(120));
    let name = format!("{stem}.html");
    assert!(name.len() <= 255);
    let artifact = crate::prepare_export("# Note", &stem, crate::ExportFormat::Html, true, &[])?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(artifact.bytes))?;
    assert_eq!(archive.by_index(0)?.name(), name);
    Ok(())
}

#[test]
fn export_stem_should_trim_whitespace_before_applying_the_length_limit() {
    assert_eq!(
        crate::sanitized_filename_stem(&format!("{}Title", " ".repeat(120))),
        "Title"
    );
}
