//! When an `Elapsed` started counting, read from what the data says.
//!
//! By hand rather than with a date crate: the engine needs one instant
//! from two spellings (epoch milliseconds, or an RFC 3339 timestamp), and a
//! dependency for that is more surface than the arithmetic.

use serde_json::Value;

/// Milliseconds of the Unix epoch, from a number of them, a string of
/// digits or an RFC 3339 timestamp. `None` for anything else, and for an
/// instant before the epoch.
pub(super) fn since_ms(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64().or_else(|| {
            number
                .as_f64()
                .filter(|ms| ms.is_finite() && *ms >= 0.0)
                .map(|ms| ms as u64)
        }),
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
                text.parse().ok()
            } else {
                rfc3339_ms(text)
            }
        }
        _ => None,
    }
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`.
fn rfc3339_ms(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let separators = [(4, b'-'), (7, b'-'), (13, b':'), (16, b':')];
    if bytes.len() < 20
        || separators.iter().any(|(at, ch)| bytes[*at] != *ch)
        || !matches!(bytes[10], b'T' | b't' | b' ')
    {
        return None;
    }
    let year = digits(text, 0, 4)?;
    let month = digits(text, 5, 7)?;
    let day = digits(text, 8, 10)?;
    let hour = digits(text, 11, 13)?;
    let minute = digits(text, 14, 16)?;
    let second = digits(text, 17, 19)?;
    let valid = (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    if !valid {
        return None;
    }
    let (millis, offset) = fraction_and_offset(&text[19..])?;
    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - offset;
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1_000)?
        .checked_add(millis)
}

/// The milliseconds of an optional fraction, then the offset in seconds.
fn fraction_and_offset(rest: &str) -> Option<(u64, i64)> {
    let (millis, rest) = match rest.strip_prefix('.') {
        Some(fraction) => {
            let len = fraction.bytes().take_while(u8::is_ascii_digit).count();
            if len == 0 {
                return None;
            }
            let padded = format!("{:0<3}", &fraction[..len.min(3)]);
            (padded.parse().ok()?, &fraction[len..])
        }
        None => (0, rest),
    };
    let offset = match rest.as_bytes() {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), _, _, b':', _, _] => {
            let magnitude = digits(rest, 1, 3)? * 3_600 + digits(rest, 4, 6)? * 60;
            if *sign == b'-' { -magnitude } else { magnitude }
        }
        _ => return None,
    };
    Some((millis, offset))
}

fn digits(text: &str, from: usize, to: usize) -> Option<i64> {
    let part = text.get(from..to)?;
    part.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| part.parse().ok())
        .flatten()
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_instant_is_read_from_epoch_milliseconds_or_rfc_3339() {
        assert_eq!(
            since_ms(&json!(1_700_000_000_000_u64)),
            Some(1_700_000_000_000)
        );
        assert_eq!(since_ms(&json!("1700000000000")), Some(1_700_000_000_000));
        assert_eq!(since_ms(&json!("1970-01-01T00:00:00Z")), Some(0));
        assert_eq!(
            since_ms(&json!("2023-11-14T22:13:20.5Z")),
            Some(1_700_000_000_500)
        );
        assert_eq!(
            since_ms(&json!("2023-11-14T19:13:20-03:00")),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            since_ms(&json!("2024-02-29T00:00:00Z")),
            Some(1_709_164_800_000)
        );
    }

    #[test]
    fn anything_else_is_no_instant() {
        for bad in [
            json!("yesterday"),
            json!("2023-02-29T00:00:00Z"),
            json!("2023-11-14T25:00:00Z"),
            json!("2023-11-14T22:13:20"),
            json!("1969-12-31T23:59:59Z"),
            json!(-5),
            json!(true),
            json!(null),
        ] {
            assert_eq!(since_ms(&bad), None, "{bad}");
        }
    }
}
