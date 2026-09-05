use time::macros::{date, datetime};

use super::{NoteDateGroup, compact_note_excerpt, note_date_group_for_days, relative_update_time};

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
