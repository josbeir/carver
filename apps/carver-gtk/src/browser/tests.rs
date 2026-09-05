use time::macros::{date, datetime};

use super::{
    NoteDateGroup, TouchpadBackGesture, compact_note_excerpt, note_date_group_for_days,
    relative_update_time,
};

#[test]
fn compact_note_excerpt_should_collapse_whitespace_and_omit_a_repeated_title() {
    assert_eq!(
        compact_note_excerpt("Heading 1", "Heading 1\n\n  Relevant   body text"),
        "Relevant body text"
    );
}

#[test]
fn relative_update_time_should_report_seconds_before_a_minute() {
    assert_eq!(
        relative_update_time(
            datetime!(2026-09-03 12:00:00 UTC),
            datetime!(2026-09-03 12:00:30 UTC)
        ),
        "30 seconds ago"
    );
}

#[test]
fn relative_update_time_should_use_yesterday_for_the_previous_day() {
    assert_eq!(
        relative_update_time(
            datetime!(2026-09-02 08:00:00 UTC),
            datetime!(2026-09-03 12:00:00 UTC)
        ),
        "Yesterday"
    );
}

#[test]
fn note_date_group_should_use_today_for_the_current_day() {
    assert_eq!(
        note_date_group_for_days(date!(2026 - 09 - 09), date!(2026 - 09 - 09)),
        NoteDateGroup::Today
    );
}

#[test]
fn note_date_group_should_use_yesterday_for_the_previous_day() {
    assert_eq!(
        note_date_group_for_days(date!(2026 - 09 - 08), date!(2026 - 09 - 09)),
        NoteDateGroup::Yesterday
    );
}

#[test]
fn note_date_group_should_use_this_week_after_monday() {
    assert_eq!(
        note_date_group_for_days(date!(2026 - 09 - 07), date!(2026 - 09 - 09)),
        NoteDateGroup::ThisWeek
    );
}

#[test]
fn note_date_group_should_use_this_month_before_the_current_week() {
    assert_eq!(
        note_date_group_for_days(date!(2026 - 09 - 01), date!(2026 - 09 - 09)),
        NoteDateGroup::ThisMonth
    );
}

#[test]
fn note_date_group_should_use_earlier_this_year_for_a_previous_month() {
    assert_eq!(
        note_date_group_for_days(date!(2026 - 08 - 31), date!(2026 - 09 - 09)),
        NoteDateGroup::EarlierThisYear
    );
}

#[test]
fn note_date_group_should_use_a_year_for_previous_years() {
    assert_eq!(
        note_date_group_for_days(date!(2025 - 12 - 31), date!(2026 - 09 - 09)),
        NoteDateGroup::Year(2025)
    );
}

#[test]
fn touchpad_back_gesture_should_ignore_vertical_and_leftward_scrolls() {
    assert_eq!(
        TouchpadBackGesture::Idle.advance(8.0, 20.0),
        TouchpadBackGesture::Idle
    );
    assert_eq!(
        TouchpadBackGesture::Idle.advance(-20.0, 1.0),
        TouchpadBackGesture::Idle
    );
}

#[test]
fn touchpad_back_gesture_should_request_back_after_a_rightward_scroll() {
    let gesture = TouchpadBackGesture::Idle.advance(30.0, 1.0);

    assert_eq!(gesture.advance(50.0, 2.0), TouchpadBackGesture::Triggered);
}

#[test]
fn touchpad_back_gesture_should_trigger_only_once() {
    assert_eq!(
        TouchpadBackGesture::Triggered.advance(100.0, 0.0),
        TouchpadBackGesture::Triggered
    );
}
