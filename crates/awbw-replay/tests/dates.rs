//! Parse replay timestamps and verify that they round-trip in AWBW's format.

use awbrn_types::{AwbwDate, AwbwDateTime};
use awbw_replay::ReplayParser;
use jiff::fmt::strtime;

const AWBW_DATE_TIME: &str = "%Y-%m-%d %H:%M:%S";
const AWBW_DATE: &str = "%Y-%m-%d";

/// Formats a parsed timestamp using AWBW's wire format.
fn rendered(value: AwbwDateTime) -> String {
    strtime::format(
        AWBW_DATE_TIME,
        &value.timestamp().to_zoned(jiff::tz::TimeZone::UTC),
    )
    .unwrap()
}

fn replays() -> Vec<(String, Vec<u8>)> {
    let mut found: Vec<_> = std::fs::read_dir("../../assets/replays")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "zip"))
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).unwrap())
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!found.is_empty(), "no replay fixtures found");
    found
}

#[test]
fn every_replay_timestamp_round_trips_through_awbw_format() {
    let parser = ReplayParser::new();
    let mut checked = 0usize;

    for (name, data) in replays() {
        let replay = parser
            .parse(&data)
            .unwrap_or_else(|error| panic!("{name} failed to parse: {error}"));

        for game in &replay.games {
            for value in [game.start_date, game.activity_date, game.aet_date] {
                assert_eq!(
                    rendered(value).len(),
                    19,
                    "{name}: {value} does not render as an AWBW date and time"
                );
                checked += 1;
            }

            if let Some(end) = game.end_date {
                assert_eq!(rendered(end).len(), 19, "{name}: bad end_date");
                checked += 1;
            }

            // Ordering checks catch transposed dates that still match the format.
            assert!(
                game.start_date <= game.activity_date,
                "{name}: activity {} precedes start {}",
                game.activity_date,
                game.start_date
            );
            assert!(
                game.start_date <= game.aet_date,
                "{name}: aet {} precedes start {}",
                game.aet_date,
                game.start_date
            );
        }
    }

    // Use a floor so new fixtures do not require test updates.
    assert!(checked >= 1_965, "only {checked} timestamps were checked");
}

/// Keep AWBW's date-only game-end field date-only.
#[test]
fn game_end_dates_are_calendar_days() {
    let cases = [
        ("2025-03-21", true),
        ("2026-08-21", true),
        // Reject timestamps instead of inventing a calendar day.
        ("2025-03-21 15:29:14", false),
        ("2025-02-30", false),
        ("", false),
    ];

    for (raw, expected) in cases {
        let parsed = AwbwDate::parse(raw);
        assert_eq!(parsed.is_some(), expected, "unexpected result for {raw:?}");

        if let Some(date) = parsed {
            assert_eq!(strtime::format(AWBW_DATE, date.date()).unwrap(), raw);
        }
    }
}
