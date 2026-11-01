//! What a review keeps for one agent, as the host stores and returns it:
//! the operator's comments, the agent's answers to them, and which files
//! were viewed on which diff.
//!
//! Plain data with no I/O. The host reads and rewrites it under the
//! record's lock ([`crate::Host::update_review_record`]), so a change the
//! surface makes and an answer the agent writes from its own process never
//! write over each other.

use std::collections::BTreeMap;

/// One agent's review.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReviewRecord {
    pub comments: Vec<Comment>,
    /// Each viewed file, by its path in the checkout, with the fingerprint
    /// of the diff it was viewed on: a mark holds for that diff only.
    pub viewed: BTreeMap<String, u64>,
    /// The number the next send takes.
    pub next_send: u32,
    /// The id the next comment takes.
    pub next_comment: u64,
}

/// One comment, from the moment it is drafted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Comment {
    pub id: u64,
    /// The file, as a path in the checkout.
    pub path: String,
    /// The lines it is about, as they read when it was written: what
    /// places it in a diff that has moved since.
    pub rows: Vec<Row>,
    /// The text the operator selected within those lines, when they
    /// selected rather than marked whole lines.
    pub excerpt: String,
    /// What the operator wrote.
    pub text: String,
    pub status: CommentStatus,
    /// The send it went out in, and its number there: `3/1`.
    pub sent: Option<(u32, u32)>,
    /// The agent it was sent to, who alone may answer it.
    pub agent: Option<String>,
    /// What the agent answered, oldest first.
    pub answers: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CommentStatus {
    #[default]
    Draft,
    Sent,
    Answered,
    Resolved,
}

/// One line of a diff as a comment remembers it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Row {
    pub kind: RowKind,
    /// Its number on its side: the old file's for a removal, the new
    /// file's otherwise.
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RowKind {
    #[default]
    Context,
    Added,
    Removed,
}

impl ReviewRecord {
    /// An id no comment holds yet.
    pub fn free_id(&self) -> u64 {
        let highest = self
            .comments
            .iter()
            .map(|comment| comment.id)
            .max()
            .unwrap_or(0);
        self.next_comment.max(highest + 1).max(1)
    }
}

impl Comment {
    /// `send/n`, once it was sent.
    pub fn reference(&self) -> Option<String> {
        self.sent.map(|(send, n)| format!("{send}/{n}"))
    }
}
