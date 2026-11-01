//! The review surface: what an agent's branch adds against its target,
//! read file by file, said back to the agent line by line.
//!
//! # Why apart from the code surface
//!
//! The changes used to be the code surface's first half, on the argument
//! that "I see it in the diff, let me fix that line" must not cross two
//! surfaces. Two things moved that argument. A surface can be opened on a
//! path and a line from outside, so the crossing is one gesture
//! ([`crate::contract::Outcome::OpenPath`]). And a review is not another
//! way of reading the same tree: it keeps what the operator wrote across
//! sessions, it speaks to the agent, and it diffs against the target
//! rather than against the last commit. Kept in the code surface, every
//! one of those would be a branch on the mode deciding what the state
//! means, which that surface's own rule forbids.

pub mod record;

pub use record::{Comment, CommentStatus, ReviewRecord, Row, RowKind};
