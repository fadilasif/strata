// SPDX-License-Identifier: MIT

use super::{DateFormat, calendar_day_difference, format_full_timestamp, modified_date_at};

fn date_in_timezone(
    timezone: &glib::TimeZone,
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
) -> glib::DateTime {
    glib::DateTime::new(timezone, year, month, day, hour, minute, 0.0).expect("valid test date")
}

fn utc_date(year: i32, month: i32, day: i32, hour: i32, minute: i32) -> glib::DateTime {
    date_in_timezone(&glib::TimeZone::utc(), year, month, day, hour, minute)
}

/// Renders the UTC offset (`+0530` style) to prove a constructed test date
/// really sits on the claimed side of a transition.
fn offset_of(value: &glib::DateTime) -> String {
    value
        .format("%z")
        .map(|offset| offset.to_string())
        .expect("offset")
}

#[test]
fn future_modified_dates_use_an_absolute_timestamp() {
    let now = utc_date(2026, 9, 3, 12, 0);
    let modified = utc_date(2026, 9, 3, 13, 0);

    assert_eq!(
        modified_date_at(&modified, &now, DateFormat::Relative),
        "2026-09-03 13:00"
    );
}

#[test]
fn slight_future_timestamps_are_clock_skew_not_future_files() {
    let now = utc_date(2026, 9, 3, 12, 0);
    let skew = utc_date(2026, 9, 3, 12, 0).add_seconds(30.0).expect("skew");
    let just_past = utc_date(2026, 9, 3, 12, 1)
        .add_seconds(-1.0)
        .expect("minute");
    let minute_future = utc_date(2026, 9, 3, 12, 1)
        .add_seconds(1.0)
        .expect("future");

    assert_eq!(
        modified_date_at(&skew, &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(&just_past, &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(&minute_future, &now, DateFormat::Relative),
        "2026-09-03 12:01"
    );
    assert_eq!(
        modified_date_at(&skew, &now, DateFormat::Iso8601),
        "2026-09-03 12:00"
    );
}

#[test]
fn date_format_parsing_tolerates_hand_edited_values() {
    for (value, expected) in [
        ("relative", DateFormat::Relative),
        ("iso", DateFormat::Iso8601),
        ("iso8601", DateFormat::Iso8601),
        ("ISO-8601", DateFormat::Iso8601),
        (" Iso ", DateFormat::Iso8601),
        ("long", DateFormat::Long),
        ("", DateFormat::Relative),
        ("garbage", DateFormat::Relative),
    ] {
        assert_eq!(DateFormat::parse(value), expected, "{value:?}");
    }
}

#[test]
fn recent_past_modified_dates_remain_relative() {
    let now = utc_date(2026, 9, 3, 12, 0);
    let modified = utc_date(2026, 9, 3, 11, 45);

    assert_eq!(
        modified_date_at(&modified, &now, DateFormat::Relative),
        "15m ago"
    );
}

#[test]
fn recent_files_stay_relative_across_midnight() {
    let now = utc_date(2026, 9, 8, 0, 0);

    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 7, 23, 59), &now, DateFormat::Relative),
        "1m ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 7, 23, 45), &now, DateFormat::Relative),
        "15m ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 7, 23, 0), &now, DateFormat::Relative),
        "1h ago"
    );
}

#[test]
fn saved_formats_always_render_absolute() {
    let now = utc_date(2026, 9, 8, 0, 0);
    let recent = utc_date(2026, 9, 7, 23, 59);
    let future = utc_date(2026, 9, 8, 0, 30);

    assert_eq!(
        modified_date_at(&recent, &now, DateFormat::Iso8601),
        "2026-09-07 23:59"
    );
    assert_eq!(
        modified_date_at(&future, &now, DateFormat::Iso8601),
        "2026-09-08 00:30"
    );
    assert_eq!(
        modified_date_at(&recent, &now, DateFormat::Long),
        "September 7, 2026, 23:59"
    );
    assert_eq!(
        modified_date_at(&future, &now, DateFormat::Long),
        "September 8, 2026, 00:30"
    );
}

#[test]
fn relative_time_uses_calendar_days_beyond_an_hour() {
    let now = utc_date(2026, 9, 8, 23, 0);

    // Same calendar day stays in hours even late at night.
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 8, 0, 30), &now, DateFormat::Relative),
        "22h ago"
    );
    // Under 24 hours of elapsed time stays in hours even across midnight;
    // only once a full day has passed do calendar days take over.
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 7, 23, 30), &now, DateFormat::Relative),
        "23h ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 7, 0, 30), &now, DateFormat::Relative),
        "Mon"
    );
    // 2-6 calendar days show the weekday abbreviation.
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 6, 23, 30), &now, DateFormat::Relative),
        "Sun"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 2, 12, 0), &now, DateFormat::Relative),
        "Wed"
    );
    // 7-30 days show whole weeks.
    assert_eq!(
        modified_date_at(&utc_date(2026, 9, 1, 12, 0), &now, DateFormat::Relative),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 8, 25, 12, 0), &now, DateFormat::Relative),
        "2w ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 8, 18, 12, 0), &now, DateFormat::Relative),
        "3w ago"
    );
    assert_eq!(
        modified_date_at(&utc_date(2026, 8, 9, 12, 0), &now, DateFormat::Relative),
        "4w ago"
    );
    // Beyond 30 days the existing absolute format takes over.
    assert_eq!(
        modified_date_at(&utc_date(2026, 8, 8, 12, 0), &now, DateFormat::Relative),
        "Aug 8, 12:00"
    );
    assert_eq!(
        modified_date_at(&utc_date(2025, 8, 12, 14, 30), &now, DateFormat::Relative),
        "Aug 12, 2025"
    );
}

#[test]
fn month_and_year_boundaries_follow_the_calendar() {
    // Month boundary: Sep 30 is two days before Oct 2.
    let october = utc_date(2026, 10, 2, 12, 0);
    assert_eq!(
        modified_date_at(
            &utc_date(2026, 10, 1, 12, 0),
            &october,
            DateFormat::Relative
        ),
        "Thu"
    );
    assert_eq!(
        modified_date_at(
            &utc_date(2026, 9, 30, 12, 0),
            &october,
            DateFormat::Relative
        ),
        "Wed"
    );
    // Year boundary: Dec 31 shows its weekday relative to Jan 1.
    let january = utc_date(2027, 1, 1, 12, 0);
    assert_eq!(
        modified_date_at(
            &utc_date(2026, 12, 31, 12, 0),
            &january,
            DateFormat::Relative
        ),
        "Thu"
    );
}

#[test]
fn relative_time_boundaries_never_show_seconds() {
    let now = utc_date(2026, 9, 3, 12, 0);
    let ago = |seconds: f64| now.add_seconds(-seconds).expect("offset");

    assert_eq!(
        modified_date_at(&ago(0.0), &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(&ago(20.0), &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(&ago(59.0), &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(&ago(60.0), &now, DateFormat::Relative),
        "1m ago"
    );
    assert_eq!(
        modified_date_at(&ago(61.0), &now, DateFormat::Relative),
        "1m ago"
    );
    assert_eq!(
        modified_date_at(&ago(5.0 * 60.0), &now, DateFormat::Relative),
        "5m ago"
    );
    assert_eq!(
        modified_date_at(&ago(59.0 * 60.0 + 59.0), &now, DateFormat::Relative),
        "59m ago"
    );
    assert_eq!(
        modified_date_at(&ago(60.0 * 60.0), &now, DateFormat::Relative),
        "1h ago"
    );
    assert_eq!(
        modified_date_at(&ago(61.0 * 60.0), &now, DateFormat::Relative),
        "1h ago"
    );
    assert_eq!(
        modified_date_at(&ago(5.0 * 3600.0), &now, DateFormat::Relative),
        "5h ago"
    );
    // Late at night, almost a full day can elapse within the same calendar day.
    let late = utc_date(2026, 9, 3, 23, 59);
    let late_ago = |seconds: f64| late.add_seconds(-seconds).expect("offset");
    assert_eq!(
        modified_date_at(
            &late_ago(23.0 * 3600.0 + 59.0 * 60.0),
            &late,
            DateFormat::Relative
        ),
        "23h ago"
    );
    // A full day earlier is already the previous calendar day.
    assert_eq!(
        modified_date_at(&ago(24.0 * 3600.0), &now, DateFormat::Relative),
        "Wed"
    );
    // Just under 24 hours stays in hours even across midnight.
    assert_eq!(
        modified_date_at(&ago(24.0 * 3600.0 - 1.0), &now, DateFormat::Relative),
        "23h ago"
    );
}

#[test]
fn previous_local_date_shows_hours_before_24_hours_elapsed() {
    let timezone = glib::TimeZone::from_identifier(Some("America/New_York"))
        .expect("America/New_York timezone");
    let modified = date_in_timezone(&timezone, 2026, 9, 7, 23, 30);
    let now = date_in_timezone(&timezone, 2026, 9, 8, 0, 30);

    assert_eq!(
        modified_date_at(&modified, &now, DateFormat::Relative),
        "1h ago"
    );
}

#[test]
fn calendar_days_survive_daylight_saving_transitions() {
    let timezone = glib::TimeZone::from_identifier(Some("America/New_York"))
        .expect("America/New_York timezone");
    let spring_modified = date_in_timezone(&timezone, 2026, 3, 8, 23, 30);
    let spring_now = date_in_timezone(&timezone, 2026, 3, 9, 23, 0);
    let fall_modified = date_in_timezone(&timezone, 2026, 11, 1, 23, 30);
    let fall_now = date_in_timezone(&timezone, 2026, 11, 2, 12, 0);
    let two_dates_before_fall_now = date_in_timezone(&timezone, 2026, 10, 31, 23, 30);

    assert_eq!(
        calendar_day_difference(&spring_modified, &spring_now),
        Some(1)
    );
    assert_eq!(calendar_day_difference(&fall_modified, &fall_now), Some(1));
    assert_eq!(
        calendar_day_difference(&two_dates_before_fall_now, &fall_now),
        Some(2)
    );
}

#[test]
fn full_timestamp_always_shows_date_time_and_year() {
    let utc = glib::TimeZone::utc();
    assert_eq!(
        format_full_timestamp(&date_in_timezone(&utc, 2026, 9, 24, 22, 42)),
        "Sep 24, 2026, 10:42 PM"
    );
    assert_eq!(
        format_full_timestamp(&date_in_timezone(&utc, 2026, 1, 5, 9, 5)),
        "Jan 5, 2026, 9:05 AM"
    );
    assert_eq!(
        format_full_timestamp(&date_in_timezone(&utc, 2025, 12, 31, 0, 0)),
        "Dec 31, 2025, 12:00 AM"
    );
}

#[test]
fn half_hour_offsets_use_elapsed_time_and_local_midnights() {
    let kolkata = glib::TimeZone::from_identifier(Some("Asia/Kolkata")).expect("Asia/Kolkata");
    let now = date_in_timezone(&kolkata, 2026, 9, 8, 0, 10);
    assert_eq!(offset_of(&now), "+0530");

    // Sub-minute recency never shows seconds, even across a half-hour midnight.
    let just_now = now.add_seconds(-30.0).expect("offset");
    assert_eq!(
        modified_date_at(&just_now, &now, DateFormat::Relative),
        "Just now"
    );
    // A 20-minute-old file from "yesterday" is minutes, not a weekday.
    let evening = date_in_timezone(&kolkata, 2026, 9, 7, 23, 50);
    assert_eq!(offset_of(&evening), "+0530");
    assert_eq!(
        modified_date_at(&evening, &now, DateFormat::Relative),
        "20m ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kolkata, 2026, 9, 7, 23, 0),
            &now,
            DateFormat::Relative
        ),
        "1h ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kolkata, 2026, 9, 5, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "Sat"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kolkata, 2026, 9, 1, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kolkata, 2026, 7, 1, 8, 5),
            &now,
            DateFormat::Relative
        ),
        "Jul 1, 08:05"
    );
}

#[test]
fn quarter_hour_offsets_use_elapsed_time_and_local_midnights() {
    let kathmandu =
        glib::TimeZone::from_identifier(Some("Asia/Kathmandu")).expect("Asia/Kathmandu");
    let now = date_in_timezone(&kathmandu, 2026, 9, 8, 0, 10);
    assert_eq!(offset_of(&now), "+0545");

    let just_now = now.add_seconds(-45.0).expect("offset");
    assert_eq!(
        modified_date_at(&just_now, &now, DateFormat::Relative),
        "Just now"
    );
    let evening = date_in_timezone(&kathmandu, 2026, 9, 7, 23, 50);
    assert_eq!(offset_of(&evening), "+0545");
    assert_eq!(
        modified_date_at(&evening, &now, DateFormat::Relative),
        "20m ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kathmandu, 2026, 9, 7, 22, 30),
            &now,
            DateFormat::Relative
        ),
        "1h ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kathmandu, 2026, 9, 6, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "Sun"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kathmandu, 2026, 8, 25, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "2w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&kathmandu, 2026, 6, 15, 9, 0),
            &now,
            DateFormat::Relative
        ),
        "Jun 15, 09:00"
    );
}

#[test]
fn zone_without_daylight_saving_stays_on_all_bands() {
    let tokyo = glib::TimeZone::from_identifier(Some("Asia/Tokyo")).expect("Asia/Tokyo");
    let now = date_in_timezone(&tokyo, 2026, 9, 8, 12, 0);
    assert_eq!(offset_of(&now), "+0900");

    let just_now = now.add_seconds(-30.0).expect("offset");
    assert_eq!(
        modified_date_at(&just_now, &now, DateFormat::Relative),
        "Just now"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&tokyo, 2026, 9, 8, 11, 55),
            &now,
            DateFormat::Relative
        ),
        "5m ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&tokyo, 2026, 9, 8, 7, 0),
            &now,
            DateFormat::Relative
        ),
        "5h ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&tokyo, 2026, 9, 5, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "Sat"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&tokyo, 2026, 8, 29, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&tokyo, 2026, 7, 1, 12, 0),
            &now,
            DateFormat::Relative
        ),
        "Jul 1, 12:00"
    );
}

#[test]
fn newfoundland_half_hour_daylight_saving_uses_elapsed_time() {
    let st_johns =
        glib::TimeZone::from_identifier(Some("America/St_Johns")).expect("America/St_Johns");
    // Spring forward on 2026-03-08: the wall clock jumps 80 minutes while
    // only 20 minutes elapse.
    let spring_before = date_in_timezone(&st_johns, 2026, 3, 8, 1, 50);
    let spring_after = date_in_timezone(&st_johns, 2026, 3, 8, 3, 10);
    assert_eq!(offset_of(&spring_before), "-0330");
    assert_eq!(offset_of(&spring_after), "-0230");
    assert_eq!(
        modified_date_at(&spring_before, &spring_after, DateFormat::Relative),
        "20m ago"
    );
    // Fall back on 2026-11-01: the wall clock advances 140 minutes while
    // 200 minutes elapse.
    let fall_before = date_in_timezone(&st_johns, 2026, 10, 31, 23, 50);
    let fall_after = date_in_timezone(&st_johns, 2026, 11, 1, 2, 10);
    assert_eq!(offset_of(&fall_before), "-0230");
    assert_eq!(offset_of(&fall_after), "-0330");
    assert_eq!(
        modified_date_at(&fall_before, &fall_after, DateFormat::Relative),
        "3h ago"
    );
    // A 25-hour fall-back day still counts as one calendar day.
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&st_johns, 2026, 11, 1, 0, 30),
            &date_in_timezone(&st_johns, 2026, 11, 2, 0, 30),
            DateFormat::Relative
        ),
        "Sun"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&st_johns, 2026, 10, 25, 12, 0),
            &date_in_timezone(&st_johns, 2026, 11, 2, 12, 0),
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&st_johns, 2026, 9, 1, 12, 0),
            &date_in_timezone(&st_johns, 2026, 11, 2, 12, 0),
            DateFormat::Relative
        ),
        "Sep 1, 12:00"
    );
    let just_now = date_in_timezone(&st_johns, 2026, 11, 2, 12, 0)
        .add_seconds(-30.0)
        .expect("offset");
    assert_eq!(
        modified_date_at(
            &just_now,
            &date_in_timezone(&st_johns, 2026, 11, 2, 12, 0),
            DateFormat::Relative
        ),
        "Just now"
    );
}

#[test]
fn southern_hemisphere_daylight_saving_uses_elapsed_time() {
    let sydney =
        glib::TimeZone::from_identifier(Some("Australia/Sydney")).expect("Australia/Sydney");
    // Southern fall back on 2026-04-05: the wall clock advances 80 minutes
    // while 140 minutes elapse.
    let fall_before = date_in_timezone(&sydney, 2026, 4, 5, 1, 50);
    let fall_after = date_in_timezone(&sydney, 2026, 4, 5, 3, 10);
    assert_eq!(offset_of(&fall_before), "+1100");
    assert_eq!(offset_of(&fall_after), "+1000");
    assert_eq!(
        modified_date_at(&fall_before, &fall_after, DateFormat::Relative),
        "2h ago"
    );
    // Southern spring forward on 2026-10-04: the wall clock advances 80
    // minutes while 20 minutes elapse.
    let spring_before = date_in_timezone(&sydney, 2026, 10, 4, 1, 50);
    let spring_after = date_in_timezone(&sydney, 2026, 10, 4, 3, 10);
    assert_eq!(offset_of(&spring_before), "+1000");
    assert_eq!(offset_of(&spring_after), "+1100");
    assert_eq!(
        modified_date_at(&spring_before, &spring_after, DateFormat::Relative),
        "20m ago"
    );
    // A 25-hour southern fall-back day still counts as one calendar day.
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&sydney, 2026, 4, 4, 12, 0),
            &date_in_timezone(&sydney, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Sat"
    );
    let just_now = date_in_timezone(&sydney, 2026, 4, 5, 12, 0)
        .add_seconds(-30.0)
        .expect("offset");
    assert_eq!(
        modified_date_at(
            &just_now,
            &date_in_timezone(&sydney, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Just now"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&sydney, 2026, 3, 29, 12, 0),
            &date_in_timezone(&sydney, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&sydney, 2026, 2, 1, 12, 0),
            &date_in_timezone(&sydney, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Feb 1, 12:00"
    );
}

#[test]
fn half_hour_offset_daylight_saving_uses_elapsed_time() {
    let adelaide =
        glib::TimeZone::from_identifier(Some("Australia/Adelaide")).expect("Australia/Adelaide");
    // Fall back on 2026-04-05 with half-hour offsets on both sides.
    let fall_before = date_in_timezone(&adelaide, 2026, 4, 5, 1, 50);
    let fall_after = date_in_timezone(&adelaide, 2026, 4, 5, 3, 10);
    assert_eq!(offset_of(&fall_before), "+1030");
    assert_eq!(offset_of(&fall_after), "+0930");
    assert_eq!(
        modified_date_at(&fall_before, &fall_after, DateFormat::Relative),
        "2h ago"
    );
    // Spring forward on 2026-10-04.
    let spring_before = date_in_timezone(&adelaide, 2026, 10, 4, 1, 50);
    let spring_after = date_in_timezone(&adelaide, 2026, 10, 4, 3, 10);
    assert_eq!(offset_of(&spring_before), "+0930");
    assert_eq!(offset_of(&spring_after), "+1030");
    assert_eq!(
        modified_date_at(&spring_before, &spring_after, DateFormat::Relative),
        "20m ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&adelaide, 2026, 4, 4, 12, 0),
            &date_in_timezone(&adelaide, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Sat"
    );
    let just_now = date_in_timezone(&adelaide, 2026, 4, 5, 12, 0)
        .add_seconds(-30.0)
        .expect("offset");
    assert_eq!(
        modified_date_at(
            &just_now,
            &date_in_timezone(&adelaide, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Just now"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&adelaide, 2026, 3, 29, 12, 0),
            &date_in_timezone(&adelaide, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&adelaide, 2026, 1, 15, 12, 0),
            &date_in_timezone(&adelaide, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Jan 15, 12:00"
    );
}

#[test]
fn thirty_minute_daylight_saving_shift_uses_elapsed_time() {
    let lord_howe =
        glib::TimeZone::from_identifier(Some("Australia/Lord_Howe")).expect("Australia/Lord_Howe");
    // Fall back on 2026-04-05 shifts only 30 minutes: 02:00 +11:00 becomes
    // 01:30 +10:30. The wall clock advances 50 minutes while 80 elapse.
    let fall_before = date_in_timezone(&lord_howe, 2026, 4, 5, 1, 20);
    let fall_after = date_in_timezone(&lord_howe, 2026, 4, 5, 2, 10);
    assert_eq!(offset_of(&fall_before), "+1100");
    assert_eq!(offset_of(&fall_after), "+1030");
    assert_eq!(
        modified_date_at(&fall_before, &fall_after, DateFormat::Relative),
        "1h ago"
    );
    // Spring forward on 2026-10-04 shifts 02:00 +10:30 to 02:30 +11:00.
    let spring_before = date_in_timezone(&lord_howe, 2026, 10, 4, 1, 50);
    let spring_after = date_in_timezone(&lord_howe, 2026, 10, 4, 2, 40);
    assert_eq!(offset_of(&spring_before), "+1030");
    assert_eq!(offset_of(&spring_after), "+1100");
    assert_eq!(
        modified_date_at(&spring_before, &spring_after, DateFormat::Relative),
        "20m ago"
    );
    // A 24.5-hour fall-back day still counts as one calendar day.
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&lord_howe, 2026, 4, 4, 12, 0),
            &date_in_timezone(&lord_howe, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Sat"
    );
    let just_now = date_in_timezone(&lord_howe, 2026, 4, 5, 12, 0)
        .add_seconds(-30.0)
        .expect("offset");
    assert_eq!(
        modified_date_at(
            &just_now,
            &date_in_timezone(&lord_howe, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Just now"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&lord_howe, 2026, 3, 29, 12, 0),
            &date_in_timezone(&lord_howe, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&lord_howe, 2026, 2, 1, 12, 0),
            &date_in_timezone(&lord_howe, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Feb 1, 12:00"
    );
}

#[test]
fn quarter_hour_offset_daylight_saving_uses_elapsed_time() {
    let chatham =
        glib::TimeZone::from_identifier(Some("Pacific/Chatham")).expect("Pacific/Chatham");
    // Fall back on 2026-04-05: 03:45 +13:45 becomes 02:45 +12:45, so the
    // wall clock advances 100 minutes while 160 elapse. The :45 lives in
    // the base offset; the shift itself is one hour.
    let fall_before = date_in_timezone(&chatham, 2026, 4, 5, 2, 30);
    let fall_after = date_in_timezone(&chatham, 2026, 4, 5, 4, 10);
    assert_eq!(offset_of(&fall_before), "+1345");
    assert_eq!(offset_of(&fall_after), "+1245");
    assert_eq!(
        modified_date_at(&fall_before, &fall_after, DateFormat::Relative),
        "2h ago"
    );
    // Spring forward on 2026-09-27 jumps 02:45 +12:45 to 03:45 +13:45: the
    // wall clock advances 75 minutes while only 15 elapse.
    let spring_before = date_in_timezone(&chatham, 2026, 9, 27, 2, 30);
    let spring_after = date_in_timezone(&chatham, 2026, 9, 27, 3, 45);
    assert_eq!(offset_of(&spring_before), "+1245");
    assert_eq!(offset_of(&spring_after), "+1345");
    assert_eq!(
        modified_date_at(&spring_before, &spring_after, DateFormat::Relative),
        "15m ago"
    );
    let noon = date_in_timezone(&chatham, 2026, 4, 5, 12, 0);
    let just_now = noon.add_seconds(-30.0).expect("offset");
    assert_eq!(
        modified_date_at(&just_now, &noon, DateFormat::Relative),
        "Just now"
    );
    // A 25-hour fall-back day still counts as one calendar day.
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&chatham, 2026, 4, 4, 12, 0),
            &date_in_timezone(&chatham, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Sat"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&chatham, 2026, 3, 29, 12, 0),
            &date_in_timezone(&chatham, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "1w ago"
    );
    assert_eq!(
        modified_date_at(
            &date_in_timezone(&chatham, 2026, 2, 1, 12, 0),
            &date_in_timezone(&chatham, 2026, 4, 5, 12, 0),
            DateFormat::Relative
        ),
        "Feb 1, 12:00"
    );
}

#[test]
fn calendar_rendering_converts_entries_into_now_timezone() {
    // The entry sits on one UTC date while local time has already rolled to
    // the next day: only the converted local date may drive the weekday.
    let utc = glib::TimeZone::utc();
    let new_york =
        glib::TimeZone::from_identifier(Some("America/New_York")).expect("America/New_York");
    let modified = date_in_timezone(&utc, 2026, 9, 7, 3, 30);
    let now = date_in_timezone(&new_york, 2026, 9, 8, 3, 30);
    assert_eq!(offset_of(&now), "-0400");
    // 03:30 UTC is 23:30 EDT the previous local day; a naive UTC-midnight
    // calculation would report Monday instead of Sunday.
    assert_eq!(
        modified_date_at(&modified, &now, DateFormat::Relative),
        "Sun"
    );
}
