//! The record saying UZE made a checkout.
//!
//! It lives in the worktree's own Git administrative directory
//! (`<git-common-dir>/worktrees/<admin>/`), which Git creates with the
//! worktree, keeps across `git worktree move`, and deletes on `git worktree
//! remove` and on `prune` once the directory is gone. So the record lives
//! exactly as long as the checkout it describes, survives the loss of UZE's
//! own state, and cannot be found by a checkout UZE did not make.
//!
//! It says *that* UZE made the checkout, never who holds it now: that stays
//! the task store's answer, and two answers would disagree the first time an
//! older build reused a slot without rewriting this one.

use std::path::{Path, PathBuf};
use uze_core::path::Canonical as _;

use serde::{Deserialize, Serialize};

use crate::{task::AgentId, worktree::CHECKOUT_RECORD_FILE};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CheckoutRecord {
    /// Where the checkout was when it was recorded. A record read from a
    /// different checkout — a copied directory `git worktree repair`
    /// pointed at the same administrative directory — is not that
    /// checkout's.
    pub path: PathBuf,
    /// For a subagent's checkout, the agent it was split from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<AgentId>,
    /// For a subagent's checkout, the commit it was split at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_at: Option<String>,
    /// Why the checkout's work could not be shelved when its agent ended,
    /// so it is kept, and shown with the reason, until somebody deals with
    /// it. Additive: an older build that drops it reads the checkout from
    /// its other facts, which still hold it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<String>,
}

impl CheckoutRecord {
    pub fn made_at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            parent: None,
            split_at: None,
            pinned: None,
        }
    }
}

/// A record: it is the one thing that says UZE may recycle this directory,
/// and nothing else knows it.
impl uze_document::Shaped for CheckoutRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "checkout record";
}

/// What a checkout's administrative directory says about who made it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recorded {
    Ours(CheckoutRecord),
    /// A record is there, but this build cannot read it: unreadable, or
    /// written by a newer build. Neither reused nor removed.
    Unreadable,
    Absent,
}

/// What `checkout`'s administrative directory, as `primary` registers it,
/// says about who made it. A checkout whose `.git` no longer names that
/// directory carries no record: whatever its own `.git` leads to is
/// whoever rewrote it, not UZE.
pub fn read(primary: &Path, checkout: &Path) -> Recorded {
    let Some(file) = record_file(primary, checkout) else {
        return Recorded::Absent;
    };
    match uze_document::read::<CheckoutRecord>(&file) {
        Ok(carried) => match carried.record() {
            Some(record) if same_place(&record.path, checkout) => Recorded::Ours(record),
            Some(_) | None => Recorded::Absent,
        },
        Err(_) => Recorded::Unreadable,
    }
}

/// Writes `record` for `checkout`, unless a record this build cannot read
/// is already there: that one is a newer build's, and never written over.
pub fn write(primary: &Path, checkout: &Path, record: &CheckoutRecord) -> Result<(), String> {
    let file = super::anchor::admin_dir(primary, checkout)?.join(CHECKOUT_RECORD_FILE);
    if read(primary, checkout) == Recorded::Unreadable {
        return Err(format!(
            "{} carries a checkout record this build cannot read",
            file.display()
        ));
    }
    let payload = serde_json::to_vec_pretty(record).expect("a checkout record serializes");
    crate::persistence::write_atomic(&file, &payload).map_err(|error| error.to_string())
}

/// The record's file in `checkout`'s administrative directory, found from
/// `primary` ([`super::anchor`]). The primary checkout has none: it is the
/// operator's, never a slot.
fn record_file(primary: &Path, checkout: &Path) -> Option<PathBuf> {
    super::anchor::admin_dir(primary, checkout)
        .ok()
        .map(|admin| admin.join(CHECKOUT_RECORD_FILE))
}

fn same_place(recorded: &Path, checkout: &Path) -> bool {
    let canonical = |path: &Path| path.canonical().unwrap_or_else(|_| path.to_path_buf());
    canonical(recorded) == canonical(checkout)
}
