//! Headless tests for editor serialization semantics.

use super::*;

#[test]
fn preview_scroll_script_uses_a_bounded_relative_position() {
    assert_eq!(
        preview_scroll_script(0.5),
        "window.scrollTo(0, (document.documentElement.scrollHeight - window.innerHeight) * 0.5);"
    );
}

#[test]
fn adjustment_fraction_clamps_and_handles_non_scrollable_content() {
    assert!((adjustment_fraction(60.0, 160.0, 40.0) - 0.5).abs() < f64::EPSILON);
    assert!((adjustment_fraction(240.0, 160.0, 40.0) - 1.0).abs() < f64::EPSILON);
    assert!(adjustment_fraction(0.0, 40.0, 40.0).abs() < f64::EPSILON);
}

#[test]
fn presentation_change_should_invalidate_both_preview_caches() {
    let split_preview_source = RefCell::new(Some((EditorSessionId(1), String::from("Split"))));
    let rendered_preview_source =
        RefCell::new(Some((EditorSessionId(1), String::from("Rendered"))));

    invalidate_preview_sources(&split_preview_source, &rendered_preview_source);

    assert!(split_preview_source.borrow().is_none());
    assert!(rendered_preview_source.borrow().is_none());
}

#[test]
fn split_preview_should_refresh_after_a_hidden_source_change() {
    let loaded = Some((EditorSessionId(1), String::from("Before")));

    assert!(preview_source_needs_load(
        loaded.as_ref(),
        EditorSessionId(1),
        "After"
    ));
}

#[test]
fn split_preview_should_not_refresh_when_the_visible_source_is_current() {
    let loaded = Some((EditorSessionId(1), String::from("Current")));

    assert!(!preview_source_needs_load(
        loaded.as_ref(),
        EditorSessionId(1),
        "Current"
    ));
}

#[test]
fn split_preview_should_not_render_while_the_breakpoint_hides_it() {
    assert!(!split_preview_is_visible(EditorMode::Source, true, false));
}

#[test]
fn default_document_appearance_should_use_the_comfortable_reading_measure() {
    let appearance = default_document_appearance();
    let style = web::appearance_style(&appearance);

    assert!(style.contains("--document-line-height: 1.55"));
    assert!(style.contains("--document-content-width: 80ch"));
}
