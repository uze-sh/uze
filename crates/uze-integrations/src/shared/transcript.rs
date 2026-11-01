//! Reading a harness's own transcript: one JSON record per line, written by
//! a vendor that promises nothing about its shape.
//!
//! Everything here is lenient by design. A line that does not parse is
//! skipped rather than ending the read, because a transcript is appended to
//! while it is read and its last line can be half-written; a file that
//! cannot be opened is no transcript. What a vendor's records *mean* is the
//! integration's to say; this only gets them off the disk.

use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

use serde_json::Value;
use uze_core::usage::{Resume, Stamp, UsageRecord};

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

/// The records of `from` on whose line holds one of `markers`, and the
/// offset just past the last whole line, where the next reading resumes. A
/// line still being written is left for that reading. Lines without a
/// marker are never parsed, which is what keeps a reading of hundreds of
/// megabytes cheap.
pub(crate) fn records_from(
    path: &Path,
    from: Resume,
    markers: &[&str],
) -> Option<(Vec<Value>, u64)> {
    let mut file = File::open(path).ok()?;
    let start = match from {
        Resume::Start => 0,
        Resume::At(offset) => offset,
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = Vec::new();
    let mut offset = start;
    let mut found = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line).ok()?;
        if read == 0 || line.last() != Some(&b'\n') {
            break;
        }
        offset += read as u64;
        let Ok(text) = std::str::from_utf8(&line) else {
            continue;
        };
        if markers.iter().any(|marker| text.contains(marker))
            && let Ok(value) = serde_json::from_str(text)
        {
            found.push(value);
        }
    }
    Some((found, offset))
}

/// Every `.jsonl` file under `root`, at most `depth` directories down,
/// changed since `since`, as a usage record named by its path. Read from
/// metadata alone.
pub(crate) fn jsonl_records(root: &Path, depth: usize, since: SystemTime) -> Vec<UsageRecord> {
    let mut found = Vec::new();
    walk(root, depth, since, &mut found);
    found
}

fn walk(dir: &Path, depth: usize, since: SystemTime, found: &mut Vec<UsageRecord>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if depth > 0 {
                walk(&path, depth - 1, since, found);
            }
        } else if kind.is_file()
            && path.extension().is_some_and(|ext| ext == "jsonl")
            && let Ok(meta) = entry.metadata()
            && let Ok(changed) = meta.modified()
            && changed >= since
        {
            found.push(UsageRecord {
                id: path.to_string_lossy().into_owned(),
                stamp: Stamp::of(meta.len(), changed),
            });
        }
    }
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
    fn a_reading_resumes_after_the_last_whole_line_and_parses_only_marked_ones() {
        let root = uze_testkit::temp::TempDir::new("transcript-records-from");
        let path = root.join("t.jsonl");
        fs::write(&path, "{\"a\":1}\n{\"mark\":2}\n{\"mark\":3").unwrap();
        let (found, offset) = records_from(&path, Resume::Start, &["mark"]).unwrap();
        assert_eq!(found, vec![serde_json::json!({"mark": 2})]);
        assert_eq!(offset, 19);
        fs::write(&path, "{\"a\":1}\n{\"mark\":2}\n{\"mark\":3}\n").unwrap();
        let (found, offset) = records_from(&path, Resume::At(offset), &["mark"]).unwrap();
        assert_eq!(found, vec![serde_json::json!({"mark": 3})]);
        assert_eq!(offset, 30);
    }

    #[test]
    fn records_are_listed_by_depth_and_time_from_metadata() {
        let root = uze_testkit::temp::TempDir::new("transcript-jsonl-records");
        fs::create_dir_all(root.join("p/sub/deeper")).unwrap();
        for file in [
            "p/a.jsonl",
            "p/sub/b.jsonl",
            "p/sub/deeper/c.jsonl",
            "p/d.txt",
        ] {
            fs::write(root.join(file), "{}\n").unwrap();
        }
        let found = jsonl_records(&root.join("p"), 1, SystemTime::UNIX_EPOCH);
        let mut names: Vec<_> = found
            .iter()
            .map(|record| {
                Path::new(&record.id)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(names, ["a.jsonl", "b.jsonl"]);
        assert!(
            jsonl_records(
                &root.join("p"),
                1,
                SystemTime::now() + std::time::Duration::from_secs(60)
            )
            .is_empty()
        );
    }

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
