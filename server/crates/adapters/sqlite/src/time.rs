//! The stored timestamp format: fixed-width UTC RFC 3339 with
//! milliseconds, so text order is time order.

use chrono::{DateTime, Utc};

use chrono::SecondsFormat;

/// Formats `t` as stored: `2026-11-25T08:30:00.123Z`.
pub fn to_db(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Parses a stored timestamp; `None` for any other format.
pub fn from_db(s: &str) -> Option<DateTime<Utc>> {
    if s.len() != "2026-11-25T08:30:00.000Z".len() || !s.ends_with('Z') {
        return None;
    }
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sample() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, 8, 30, 0).unwrap() + chrono::Duration::milliseconds(123)
    }

    #[test]
    fn time_format_is_fixed_width() {
        assert_eq!(to_db(sample()), "2026-11-25T08:30:00.123Z");
        let whole = Utc.with_ymd_and_hms(2026, 11, 25, 8, 30, 0).unwrap();
        assert_eq!(to_db(whole), "2026-11-25T08:30:00.000Z");
    }

    #[test]
    fn time_round_trips() {
        assert_eq!(from_db(&to_db(sample())), Some(sample()));
    }

    #[test]
    fn time_rejects_other_formats() {
        assert_eq!(from_db("2026-11-25 08:30"), None);
    }
}
