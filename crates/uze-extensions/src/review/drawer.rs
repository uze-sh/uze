//! The review where the operator is, without opening it: an item on the
//! bar under the agent area that counts what waits, and the drawer it
//! opens, the comments by file with the one way to send the drafts to the
//! agent. Both are trees the host draws, built from the record alone.

use std::collections::BTreeMap;

use super::record::{Comment, CommentStatus, ReviewRecord};
use crate::view::{Align, Content, IconGlyph, Layout, Node, NodeKind, Role, Span, View};

/// The send button's node.
pub(crate) const SEND: usize = 1;
/// The first file's list; the next file's is one on.
pub(crate) const FIRST_LIST: usize = 1_000;
/// Where the nodes nothing answers a press on are numbered from.
const QUIET: usize = 1_000_000;

/// The comments not resolved, by file, each file's by the lines they are
/// on: the drawer's order, and the one a press on a row is read back in.
pub(crate) fn groups(record: &ReviewRecord) -> Vec<(String, Vec<&Comment>)> {
    let mut by_file: BTreeMap<&str, Vec<&Comment>> = BTreeMap::new();
    for comment in record
        .comments
        .iter()
        .filter(|comment| comment.status != CommentStatus::Resolved)
    {
        by_file
            .entry(comment.path.as_str())
            .or_default()
            .push(comment);
    }
    by_file
        .into_iter()
        .map(|(path, mut comments)| {
            comments.sort_by_key(|comment| (first_line(comment), comment.id));
            (path.to_owned(), comments)
        })
        .collect()
}

/// The comment a press on row `row` of list `node` is about.
pub(crate) fn comment_at(record: &ReviewRecord, node: usize, row: usize) -> Option<u64> {
    let groups = groups(record);
    let (_, comments) = groups.get(node.checked_sub(FIRST_LIST)?)?;
    comments.get(row).map(|comment| comment.id)
}

/// The bar's item: the comment mark and what waits on the operator, the
/// drafts in gold, else the answers in green, else how many are open and
/// waiting on the agent, held back. Nothing while nothing is open.
pub(crate) fn item(record: &ReviewRecord) -> Option<View> {
    let count = |status| {
        record
            .comments
            .iter()
            .filter(|comment| comment.status == status)
            .count()
    };
    let (drafts, answered, sent) = (
        count(CommentStatus::Draft),
        count(CommentStatus::Answered),
        count(CommentStatus::Sent),
    );
    let (number, role) = match (drafts, answered, sent) {
        (0, 0, 0) => return None,
        (0, 0, sent) => (sent, Role::Muted),
        (0, answered, _) => (answered, Role::Success),
        (drafts, _, _) => (drafts, Role::Warning),
    };
    let mut row = Node::new(
        0,
        NodeKind::Row {
            gap: 1,
            align: Align::Start,
        },
    );
    row.children = vec![
        Node::new(
            QUIET,
            NodeKind::Icon {
                glyph: IconGlyph::Host("comment".to_owned()),
                role,
            },
        ),
        Node::new(
            QUIET + 1,
            NodeKind::Text {
                spans: vec![Span::new(number.to_string(), role)],
                truncate: false,
                still: true,
            },
        ),
    ];
    Some(tree(String::new(), row))
}

/// The drawer: each file's open comments, its state and its lines over
/// what it says, and under them the send of the drafts, or why there is
/// none. A press on a comment opens the review on it.
pub(crate) fn drawer(record: &ReviewRecord, agent: bool, notice: Option<&Span>) -> View {
    let mut quiet = QUIET;
    let mut next = || {
        quiet += 1;
        quiet
    };
    let mut root = Node::new(0, NodeKind::Stack { gap: 1 });
    let groups = groups(record);
    if groups.is_empty() {
        root.children.push(Node::new(
            next(),
            NodeKind::Notice {
                text: "No comments on this branch yet. Mark lines in the review and press c."
                    .to_owned(),
                role: Role::Muted,
            },
        ));
    }
    for (index, (path, comments)) in groups.iter().enumerate() {
        root.children.push(Node::new(
            next(),
            NodeKind::Rule {
                label: Some(path.clone()),
                role: Role::Muted,
            },
        ));
        let mut list = Node::new(
            FIRST_LIST + index,
            NodeKind::List {
                selected: None,
                empty: None,
            },
        );
        for comment in comments {
            let (state, role) = state(comment);
            let mut head = Node::new(
                next(),
                NodeKind::Row {
                    gap: 1,
                    align: Align::Start,
                },
            );
            head.children = vec![Node::new(
                next(),
                NodeKind::Text {
                    spans: vec![
                        Span::new(lines(comment), Role::Muted),
                        Span::new(" · ", Role::Faint),
                        Span::new(state, role),
                    ],
                    truncate: true,
                    still: false,
                },
            )];
            let mut row = Node::new(next(), NodeKind::Stack { gap: 0 });
            row.children = vec![
                head,
                Node::new(
                    next(),
                    NodeKind::Markdown {
                        text: comment.text.clone(),
                    },
                ),
            ];
            list.children.push(row);
        }
        root.children.push(list);
    }
    let drafts = record
        .comments
        .iter()
        .filter(|comment| comment.status == CommentStatus::Draft)
        .count();
    if let Some(notice) = notice {
        root.children.push(Node::new(
            next(),
            NodeKind::Notice {
                text: notice.text.clone(),
                role: notice.role,
            },
        ));
    }
    if drafts > 0 {
        match agent {
            true => root.children.push(Node::new(
                SEND,
                NodeKind::Button {
                    label: match drafts {
                        1 => "send 1 draft to the agent".to_owned(),
                        many => format!("send {many} drafts to the agent"),
                    },
                    primary: true,
                },
            )),
            false => root.children.push(Node::new(
                next(),
                NodeKind::Notice {
                    text: "No agent UZE launched works in this checkout alone, so the drafts \
                           cannot be sent."
                        .to_owned(),
                    role: Role::Warning,
                },
            )),
        }
    }
    tree("Comments".to_owned(), root)
}

fn tree(heading: String, root: Node) -> View {
    View {
        tabs: Vec::new(),
        title: Vec::new(),
        caption: Vec::new(),
        navigator: None,
        content: Content::Tree {
            heading,
            scroll: 0,
            root,
        },
        footer: Vec::new(),
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: Layout::default(),
        trail: Vec::new(),
        label: String::new(),
    }
}

fn first_line(comment: &Comment) -> u32 {
    comment.rows.iter().map(|row| row.line).min().unwrap_or(0)
}

fn lines(comment: &Comment) -> String {
    let lines: Vec<u32> = comment.rows.iter().map(|row| row.line).collect();
    match (lines.iter().min(), lines.iter().max()) {
        (Some(from), Some(to)) if from == to => format!("L{from}"),
        (Some(from), Some(to)) => format!("L{from}–{to}"),
        _ => "file".to_owned(),
    }
}

fn state(comment: &Comment) -> (String, Role) {
    match comment.status {
        CommentStatus::Draft => ("draft".to_owned(), Role::Warning),
        CommentStatus::Sent => (
            format!("sent {}", comment.reference().unwrap_or_default()),
            Role::Info,
        ),
        CommentStatus::Answered => (
            format!("answered {}", comment.reference().unwrap_or_default()),
            Role::Success,
        ),
        CommentStatus::Resolved => ("resolved".to_owned(), Role::Dim),
    }
}
