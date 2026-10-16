//! Reading a harness's own transcript: one JSON record per line, written by
//! a vendor that promises nothing about its shape.
//!
//! Everything here is lenient by design. A line that does not parse is
//! skipped rather than ending the read, because a transcript is appended to
//! while it is read and its last line can be half-written; a file that
//! cannot be opened is no transcript. What a vendor's records *mean* is the
//! integration's to say; this only gets them off the disk.

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use serde_json::Value;

/// Every record of the transcript at `path`, oldest first.
pub(crate) fn records(path: &Path) -> Option<impl Iterator<Item = Value>> {
    let file = File::open(path).ok()?;
    Some(
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str(&line).ok()),
    )
}

/// An RFC 3339 UTC timestamp (`2026-10-06T00:15:51.744Z`) as Unix seconds.
/// Only the UTC form, which is the one both transcripts read here write;
/// anything else is no timestamp rather than a guessed one.
pub(crate) fn unix_seconds(timestamp: &str) -> Option<u64> {
    let (date, time) = timestamp.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let mut time = time.splitn(3, ':');
    let hours: i64 = time.next()?.parse().ok()?;
    let minutes: i64 = time.next()?.parse().ok()?;
    let seconds: i64 = time.next()?.split('.').next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    u64::try_from(days * 86_400 + hours * 3_600 + minutes * 60 + seconds).ok()
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_utc_timestamp_reads_as_unix_seconds() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            unix_seconds("2026-10-06T00:15:51.744Z"),
            Some(1_791_245_751)
        );
        assert_eq!(unix_seconds("2024-02-29T12:00:00Z"), Some(1_709_208_000));
    }

    #[test]
    fn anything_but_the_utc_form_is_no_timestamp() {
        assert_eq!(unix_seconds("2026-10-06T00:15:51+02:00"), None);
        assert_eq!(unix_seconds("yesterday"), None);
        assert_eq!(unix_seconds("2026-13-06T00:15:51Z"), None);
    }

    #[test]
    fn a_half_written_line_is_skipped_not_fatal() {
        let path = uze_testkit::temp::scratch("transcript-lenient").join("t.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{\"a\":1}\nnot json\n{\"a\":2}\n{\"a\":").unwrap();
        let read: Vec<Value> = records(&path).unwrap().collect();
        assert_eq!(
            read,
            [serde_json::json!({"a":1}), serde_json::json!({"a":2})]
        );
    }
}
