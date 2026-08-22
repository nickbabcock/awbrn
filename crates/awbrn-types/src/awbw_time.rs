//! AWBW date and timestamp wrappers.
//!
//! AWBW omits an offset, so timestamps are parsed as UTC.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use jiff::civil;
use jiff::tz::Offset;
use jiff::{Timestamp, civil::Date};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const DATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

const DATE_FORMAT: &str = "%Y-%m-%d";

/// An AWBW timestamp, parsed as UTC.
/// Serializes back into AWBW's `YYYY-MM-DD HH:MM:SS`, so it round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AwbwDateTime(Timestamp);

impl AwbwDateTime {
    /// Parses AWBW's `YYYY-MM-DD HH:MM:SS` format.
    pub fn parse(raw: &str) -> Option<Self> {
        let civil = civil::DateTime::strptime(DATE_TIME_FORMAT, raw).ok()?;
        Offset::UTC.to_timestamp(civil).ok().map(Self)
    }

    pub const fn timestamp(self) -> Timestamp {
        self.0
    }
}

impl From<AwbwDateTime> for Timestamp {
    fn from(value: AwbwDateTime) -> Self {
        value.0
    }
}

impl fmt::Display for AwbwDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AwbwDateTime {
    type Err = AwbwTimeError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw).ok_or_else(|| AwbwTimeError::new(DATE_TIME_FORMAT, raw))
    }
}

/// An AWBW calendar date without a time of day.
/// Kept as a date to preserve AWBW's precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AwbwDate(Date);

impl AwbwDate {
    /// Reads AWBW's `YYYY-MM-DD`, returning `None` for anything else.
    pub fn parse(raw: &str) -> Option<Self> {
        Date::strptime(DATE_FORMAT, raw).ok().map(Self)
    }

    pub const fn date(self) -> Date {
        self.0
    }
}

impl From<AwbwDate> for Date {
    fn from(value: AwbwDate) -> Self {
        value.0
    }
}

impl fmt::Display for AwbwDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AwbwDate {
    type Err = AwbwTimeError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw).ok_or_else(|| AwbwTimeError::new(DATE_FORMAT, raw))
    }
}

/// Parse error naming the expected AWBW format and received value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwbwTimeError {
    expected: &'static str,
    found: String,
}

impl AwbwTimeError {
    fn new(expected: &'static str, found: &str) -> Self {
        Self {
            expected,
            found: found.to_string(),
        }
    }
}

impl fmt::Display for AwbwTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected an AWBW time of the form `{}`, found `{}`",
            self.expected, self.found
        )
    }
}

impl std::error::Error for AwbwTimeError {}

/// Deserializes an AWBW time from its wire format via `FromStr`.
///
/// A visitor rather than `<&str>::deserialize`, which refuses any deserializer
/// that cannot hand out a borrow, such as `serde_json::from_reader`.
fn deserialize_awbw_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr<Err = AwbwTimeError>,
    D: Deserializer<'de>,
{
    struct AwbwStr<T>(PhantomData<T>);

    impl<T: FromStr<Err = AwbwTimeError>> serde::de::Visitor<'_> for AwbwStr<T> {
        type Value = T;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an AWBW date or timestamp")
        }

        fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<T, E> {
            raw.parse().map_err(E::custom)
        }
    }

    deserializer.deserialize_str(AwbwStr(PhantomData))
}

impl Serialize for AwbwDateTime {
    /// Writes AWBW's format rather than the inner `Timestamp`'s RFC 3339, which
    /// [`AwbwDateTime::parse`] would refuse to read back.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0.strftime(DATE_TIME_FORMAT))
    }
}

impl<'de> Deserialize<'de> for AwbwDateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_awbw_str(deserializer)
    }
}

impl Serialize for AwbwDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AwbwDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_awbw_str(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awbw_date_times_parse_as_utc() {
        let parsed = AwbwDateTime::parse("2024-07-31 15:29:14").unwrap();
        assert_eq!(parsed.to_string(), "2024-07-31T15:29:14Z");
    }

    #[test]
    fn out_of_range_values_are_refused() {
        for raw in [
            "2024-13-01 00:00:00",
            "2024-02-31 00:00:00",
            "2024-07-31 99:99:99",
            "2024-07-31 15:60:00",
            "2024-07-31 24:00:00",
        ] {
            assert_eq!(AwbwDateTime::parse(raw), None, "accepted {raw:?}");
        }
    }

    /// Document jiff's leap-second clamping behavior.
    #[test]
    fn a_leap_second_clamps_to_fifty_nine() {
        let parsed = AwbwDateTime::parse("2016-12-31 23:59:60").unwrap();
        assert_eq!(parsed.to_string(), "2016-12-31T23:59:59Z");
    }

    #[test]
    fn the_two_formats_do_not_overlap() {
        assert_eq!(AwbwDateTime::parse("2024-07-31"), None);
        assert_eq!(AwbwDate::parse("2024-07-31 15:29:14"), None);
        assert!(AwbwDateTime::parse("2024-07-31 15:29:14").is_some());
        assert!(AwbwDate::parse("2024-07-31").is_some());
    }

    #[test]
    fn unreadable_values_are_refused() {
        for raw in ["", "not a date", "2024-07-31T15:29:14Z", "1722439754"] {
            assert_eq!(AwbwDateTime::parse(raw), None, "accepted {raw:?}");
            assert_eq!(AwbwDate::parse(raw), None, "accepted {raw:?}");
        }
    }

    #[test]
    fn serde_round_trips_in_awbws_format() {
        let time = AwbwDateTime::parse("2024-07-31 15:29:14").unwrap();
        let json = serde_json::to_string(&time).unwrap();
        assert_eq!(json, r#""2024-07-31 15:29:14""#);
        assert_eq!(serde_json::from_str::<AwbwDateTime>(&json).unwrap(), time);

        let date = AwbwDate::parse("2024-07-31").unwrap();
        let json = serde_json::to_string(&date).unwrap();
        assert_eq!(json, r#""2024-07-31""#);
        assert_eq!(serde_json::from_str::<AwbwDate>(&json).unwrap(), date);
    }

    /// A deserializer that cannot lend out a borrow still works.
    #[test]
    fn values_deserialize_without_borrowing() {
        let raw = br#""2024-07-31 15:29:14""#;
        assert_eq!(
            serde_json::from_reader::<_, AwbwDateTime>(&raw[..]).unwrap(),
            AwbwDateTime::parse("2024-07-31 15:29:14").unwrap()
        );

        let raw = br#""2024-07-31""#;
        assert_eq!(
            serde_json::from_reader::<_, AwbwDate>(&raw[..]).unwrap(),
            AwbwDate::parse("2024-07-31").unwrap()
        );
    }

    #[test]
    fn deserializing_reports_the_format_it_wanted() {
        let error = serde_json::from_str::<AwbwDateTime>(r#""2024-07-31T15:29:14Z""#).unwrap_err();
        assert!(error.to_string().contains("%Y-%m-%d %H:%M:%S"), "{error}");
    }

    #[test]
    fn the_error_names_the_format_it_wanted() {
        let error = "nope".parse::<AwbwDateTime>().unwrap_err();
        assert!(error.to_string().contains("%Y-%m-%d %H:%M:%S"), "{error}");
    }
}
