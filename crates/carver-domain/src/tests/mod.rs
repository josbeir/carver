use super::*;

#[test]
fn derives_title_from_first_heading() {
    let content = derive_content("# A /useful/ note\n\nBody text");
    assert_eq!(content.title, "A useful note");
    assert!(content.plain_text.contains("Body text"));
}

#[test]
fn derives_title_from_first_text_when_heading_is_missing() {
    assert_eq!(
        derive_content("\n\nHello world\n\nMore").title,
        "Hello world"
    );
}

#[test]
fn gives_empty_notes_a_safe_title() {
    assert_eq!(derive_content(" ").title, "Untitled Note");
}
