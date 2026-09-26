//! Unit coverage for sidebar helpers that do not require a display.

use super::badge_text;

#[test]
fn badge_text_should_cap_counts_above_ninety_nine() {
    assert_eq!(badge_text(0), "0");
    assert_eq!(badge_text(9), "9");
    assert_eq!(badge_text(99), "99");
    assert_eq!(badge_text(100), "99+");
    assert_eq!(badge_text(1842), "99+");
}
