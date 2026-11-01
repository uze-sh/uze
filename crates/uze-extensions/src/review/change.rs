//! What the surface does to the review it keeps: each gesture as a change,
//! applied at once to the copy on screen and again, under the record's
//! lock, to the record the host keeps, so an answer the agent wrote in
//! between is kept.

use super::record::{Comment, CommentStatus, ReviewRecord, Row};

/// One edit the operator made to the review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    /// A new draft, under the id it was given where it was written: an
    /// edit or a delete made before the record is read again names it so.
    Draft {
        id: u64,
        path: String,
        rows: Vec<Row>,
        excerpt: String,
        text: String,
    },
    /// A draft's text, rewritten.
    Edit { id: u64, text: String },
    /// A draft, deleted.
    Delete(u64),
    /// A sent comment answered and read, put away.
    Resolve(u64),
    /// Every one of `ids` handed to `agent` in one send.
    Sent { ids: Vec<u64>, agent: String },
    /// `path` marked viewed on the diff with this fingerprint, or unmarked.
    Viewed {
        path: String,
        fingerprint: Option<u64>,
    },
}

impl Change {
    /// Applies the change. Touches only what the operator owns: the
    /// answers an agent wrote are never rewritten here.
    pub(crate) fn apply(&self, record: &mut ReviewRecord) {
        match self {
            Change::Draft {
                id,
                path,
                rows,
                excerpt,
                text,
            } => {
                // Laid again over a record that already holds it, it is a
                // repeat; under an id another client took first, it steps
                // past it.
                let id = match record.comments.iter().find(|comment| comment.id == *id) {
                    Some(kept) if kept.text == *text && kept.path == *path => return,
                    Some(_) => record.free_id(),
                    None => *id,
                };
                record.next_comment = record.next_comment.max(id + 1);
                record.comments.push(Comment {
                    id,
                    path: path.clone(),
                    rows: rows.clone(),
                    excerpt: excerpt.clone(),
                    text: text.clone(),
                    ..Comment::default()
                });
            }
            Change::Edit { id, text } => {
                if let Some(comment) = draft(record, *id) {
                    comment.text = text.clone();
                }
            }
            Change::Delete(id) => record
                .comments
                .retain(|comment| comment.id != *id || comment.status != CommentStatus::Draft),
            Change::Resolve(id) => {
                if let Some(comment) = record
                    .comments
                    .iter_mut()
                    .find(|comment| comment.id == *id && comment.status != CommentStatus::Draft)
                {
                    comment.status = CommentStatus::Resolved;
                }
            }
            Change::Sent { ids, agent } => {
                record.next_send = record.next_send.max(1);
                let send = record.next_send;
                let mut number = 0;
                for comment in record.comments.iter_mut().filter(|comment| {
                    ids.contains(&comment.id) && comment.status == CommentStatus::Draft
                }) {
                    number += 1;
                    comment.status = CommentStatus::Sent;
                    comment.sent = Some((send, number));
                    comment.agent = Some(agent.clone());
                }
                if number > 0 {
                    record.next_send += 1;
                }
            }
            Change::Viewed { path, fingerprint } => match fingerprint {
                Some(fingerprint) => {
                    record.viewed.insert(path.clone(), *fingerprint);
                }
                None => {
                    record.viewed.remove(path);
                }
            },
        }
    }
}

fn draft(record: &mut ReviewRecord, id: u64) -> Option<&mut Comment> {
    record
        .comments
        .iter_mut()
        .find(|comment| comment.id == id && comment.status == CommentStatus::Draft)
}

/// The one line typed into the agent for a send: how many comments wait,
/// which send they are, and the commands that read and answer them. One
/// line of bounded text, as the bridge types and nothing else, never
/// leading with a harness's command character.
pub(crate) fn send_line(count: usize, send: u32) -> String {
    let comments = match count {
        1 => "1 review comment waits".to_owned(),
        many => format!("{many} review comments wait"),
    };
    format!(
        "{comments} for you (send {send}): run `uze agent review show` to read them, \
         address each, then answer each with `uze agent review reply {send}/<n> \"<answer>\"`."
    )
}

/// What a send does, as the operator reads it before allowing it: what the
/// agent is told and where its answers come back, never the commands it is
/// told to run, which are the agent's.
pub(crate) fn send_summary(count: usize) -> String {
    let waits = match count {
        1 => "your comment waits".to_owned(),
        many => format!("your {many} comments wait"),
    };
    format!(
        "The agent is told {waits} for it. It reads them, makes the changes, \
         and answers each one here, under the comment, where you resolve it."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drafted(record: &mut ReviewRecord, text: &str) -> u64 {
        Change::Draft {
            id: record.free_id(),
            path: "a.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: text.to_owned(),
        }
        .apply(record);
        record.comments.last().unwrap().id
    }

    #[test]
    fn a_send_numbers_its_drafts_and_the_next_send_starts_again() {
        let mut record = ReviewRecord::default();
        let first = drafted(&mut record, "one");
        let second = drafted(&mut record, "two");
        Change::Sent {
            ids: vec![first, second],
            agent: "a1".to_owned(),
        }
        .apply(&mut record);
        let sent: Vec<_> = record.comments.iter().map(|c| c.reference()).collect();
        assert_eq!(sent, [Some("1/1".to_owned()), Some("1/2".to_owned())]);
        let third = drafted(&mut record, "three");
        Change::Sent {
            ids: vec![third],
            agent: "a1".to_owned(),
        }
        .apply(&mut record);
        assert_eq!(record.comments[2].reference().as_deref(), Some("2/1"));
    }

    #[test]
    fn a_sent_comment_is_neither_edited_nor_deleted_as_a_draft_is() {
        let mut record = ReviewRecord::default();
        let id = drafted(&mut record, "one");
        Change::Sent {
            ids: vec![id],
            agent: "a1".to_owned(),
        }
        .apply(&mut record);
        Change::Edit {
            id,
            text: "changed".to_owned(),
        }
        .apply(&mut record);
        Change::Delete(id).apply(&mut record);
        assert_eq!(record.comments[0].text, "one");
    }

    #[test]
    fn the_send_line_is_one_line_the_bridge_takes() {
        let line = send_line(2, 3);
        assert!(!line.contains('\n'));
        assert!(line.len() < 2048);
        assert!(!line.starts_with(['!', '/', '#', '@', '$']));
        assert!(line.contains("uze agent review show"));
        assert!(line.contains("3/<n>"));
    }
}
