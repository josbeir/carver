use super::*;

#[test]
fn preview_scroll_script_uses_a_bounded_relative_position() {
    assert_eq!(
        preview_scroll_script(0.5),
        "window.scrollTo(0, (document.documentElement.scrollHeight - window.innerHeight) * 0.5);"
    );
}
