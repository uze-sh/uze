//! What an agent UZE launched reports about itself (`extension-agent-bridge`): a
//! bounded journal per agent, appended by `uze agent report` from inside
//! the agent's pane and read by the workspace client from where it last
//! stopped.
//!
//! A report is a claim: any process in the pane inherits the launch key
//! that proves it. So it moves only what is drawn and which sources are
//! asked again, never what is written anywhere.

use std::io::{Read, Seek, SeekFrom, Write};

use serde::{Deserialize, Serialize};

use crate::{Result, UzeError, home::UzeHome};

/// The most lines a journal keeps.
pub const REPORTS_MAX: usize = 256;
/// The most bytes a journal holds; past either bound the oldest half goes.
pub const REPORTS_BYTES: u64 = 64 << 10;
/// The most of one field a report keeps.
const FIELD_MAX: usize = 256;

/// The events an agent may report, as `uze agent report` spells them.
pub const EVENTS: &[&str] = &[
    "session.started",
    "turn.started",
    "turn.finished",
    "tool.started",
    "tool.finished",
    "approval.asked",
    "approval.resolved",
];

/// One report.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Report {
    /// One of [`EVENTS`].
    pub event: String,
    /// Milliseconds since the epoch, when it was reported.
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// As the harness gave it; the client keeps only a path inside the
    /// checkout, relative to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The harness's id for a tool call, pairing its start and end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

impl Report {
    /// `self` with every field cut to its bound and stripped of control
    /// characters: what reaches the journal.
    fn bounded(mut self) -> Self {
        let cut = |field: &mut Option<String>| {
            if let Some(text) = field {
                *text = text
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(FIELD_MAX)
                    .collect();
            }
        };
        cut(&mut self.tool);
        cut(&mut self.path);
        cut(&mut self.call);
        cut(&mut self.outcome);
        self
    }
}

/// Appends `report` to the journal of `agent`, keeping it inside its
/// bounds. Refused for an event that is not one of [`EVENTS`].
pub fn append(home: &UzeHome, agent: &str, report: Report) -> Result<()> {
    if !EVENTS.contains(&report.event.as_str()) {
        return Err(UzeError::AgentWork(format!(
            "`{}` is not an event an agent reports; one of {}",
            report.event,
            EVENTS.join(", ")
        )));
    }
    let path = home.agent_reports_path(agent);
    let written = |source| UzeError::Write {
        path: path.clone(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(written)?;
    }
    let mut line = serde_json::to_vec(&report.bounded()).expect("a report serializes");
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(written)?;
    file.write_all(&line).map_err(written)?;
    let size = file.metadata().map_err(written)?.len();
    drop(file);
    if size > REPORTS_BYTES || lines_over(&path) {
        keep_newest_half(&path).map_err(written)?;
    }
    Ok(())
}

fn lines_over(path: &std::path::Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.iter().filter(|byte| **byte == b'\n').count() > REPORTS_MAX)
        .unwrap_or(false)
}

/// Rewrites the journal with its newest half, whole lines only. A reader
/// whose offset now lies past the end reads it again from the start, past
/// the last report it delivered.
fn keep_newest_half(path: &std::path::Path) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    let lines: Vec<&[u8]> = bytes.split_inclusive(|byte| *byte == b'\n').collect();
    let kept = lines[lines.len() / 2..].concat();
    crate::persistence::write_atomic(path, &kept).map_err(std::io::Error::other)
}

/// Where a reader of one journal stands: the byte it reads from next, and
/// the last report it delivered, which is what it resumes after when the
/// journal was cut down beneath it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Cursor {
    pub offset: u64,
    pub last: Option<Report>,
}

/// The reports `agent` appended since `cursor` (one this returned before,
/// or the default for a first read), and the cursor to read from next. A
/// journal shorter than the cursor's offset was cut down: it is read from
/// its start, and only what follows the last report delivered is new (all
/// of it, when that report was among the half cut away).
pub fn read_from(home: &UzeHome, agent: &str, cursor: &Cursor) -> (Vec<Report>, Cursor) {
    let path = home.agent_reports_path(agent);
    let Ok(mut file) = std::fs::File::open(&path) else {
        return (Vec::new(), Cursor::default());
    };
    let size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let cut = cursor.offset > size;
    let from = if cut { 0 } else { cursor.offset };
    let unread = |offset| Cursor {
        offset,
        last: cursor.last.clone(),
    };
    let mut tail = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_err()
        || file.take(REPORTS_BYTES).read_to_end(&mut tail).is_err()
    {
        return (Vec::new(), unread(from));
    }
    // A line still being written is read next time, whole.
    let complete = tail
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    let mut reports: Vec<Report> = tail[..complete]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect();
    if cut && let Some(last) = &cursor.last {
        let delivered = reports
            .iter()
            .rposition(|report| report == last)
            .map_or(0, |at| at + 1);
        reports.drain(..delivered);
    }
    let next = Cursor {
        offset: from + complete as u64,
        last: reports.last().cloned().or_else(|| cursor.last.clone()),
    };
    (reports, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(event: &str) -> Report {
        Report {
            event: event.to_owned(),
            at: 1,
            ..Report::default()
        }
    }

    #[test]
    fn reports_are_read_from_where_the_reader_stopped() {
        let root = uze_testkit::temp::scratch("agent-reports");
        let home = UzeHome::at(root.join("uze"));
        assert_eq!(
            read_from(&home, "a1", &Cursor::default()),
            (Vec::new(), Cursor::default())
        );
        append(&home, "a1", report("turn.started")).unwrap();
        append(
            &home,
            "a1",
            Report {
                tool: Some("Bash\u{1b}[31m".to_owned()),
                ..report("tool.started")
            },
        )
        .unwrap();
        let (first, cursor) = read_from(&home, "a1", &Cursor::default());
        assert_eq!(first.len(), 2);
        assert_eq!(
            first[1].tool.as_deref(),
            Some("Bash[31m"),
            "no control characters"
        );
        append(&home, "a1", report("turn.finished")).unwrap();
        let (next, _) = read_from(&home, "a1", &cursor);
        assert_eq!(next, [report("turn.finished")]);
        assert!(append(&home, "a1", report("rm -rf")).is_err());
    }

    #[test]
    fn a_flood_keeps_the_journal_bounded_and_its_newest() {
        let root = uze_testkit::temp::scratch("agent-reports-flood");
        let home = UzeHome::at(root.join("uze"));
        for at in 0..2_000 {
            append(
                &home,
                "a1",
                Report {
                    at,
                    ..report("tool.finished")
                },
            )
            .unwrap();
        }
        let (kept, _) = read_from(&home, "a1", &Cursor::default());
        assert!(kept.len() <= REPORTS_MAX, "{}", kept.len());
        assert_eq!(kept.last().unwrap().at, 1_999);
        let (after_cut, _) = read_from(
            &home,
            "a1",
            &Cursor {
                offset: u64::MAX,
                last: None,
            },
        );
        assert!(
            !after_cut.is_empty(),
            "an offset past the end reads from the start"
        );
    }

    #[test]
    fn a_reader_the_journal_was_cut_beneath_delivers_only_what_is_new() {
        let root = uze_testkit::temp::scratch("agent-reports-cut");
        let home = UzeHome::at(root.join("uze"));
        let at = |at| Report {
            at,
            ..report("tool.finished")
        };
        let mut delivered = Vec::new();
        let mut cursor = Cursor::default();
        for stamp in 0..600 {
            append(&home, "a1", at(stamp)).unwrap();
            if stamp % 7 == 0 {
                let (read, next) = read_from(&home, "a1", &cursor);
                delivered.extend(read.into_iter().map(|report| report.at));
                cursor = next;
            }
        }
        let (read, _) = read_from(&home, "a1", &cursor);
        delivered.extend(read.into_iter().map(|report| report.at));
        let mut sorted = delivered.clone();
        sorted.dedup();
        assert_eq!(sorted, delivered, "nothing was delivered twice");
        assert!(
            delivered.windows(2).all(|pair| pair[0] < pair[1]),
            "in the order reported"
        );
        assert_eq!(delivered.last(), Some(&599));
    }
}
