//! What a script may ask of the machine, narrowed to the host's answer.
//!
//! The engine has no file, process or network access of its own. Each
//! function below is registered only when the extension's grants cover it,
//! and the host answers it: resolving the path beneath the checkout root,
//! following no link out of it, bounding the read. This crate refuses what
//! no host should ever see (an absolute path, a `..`) before asking.

use std::time::Duration;

pub use uze_contract::Grant;

use crate::fetch::{FetchReply, FetchRequest};

/// The grants an extension was consented to.
pub type Grants = std::collections::BTreeSet<Grant>;

/// A path relative to the checkout root, as a script asked for it: no
/// root, no drive, no `..`, `/`-separated. Whether it stays beneath the
/// root once links are considered is the host's to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutPath(String);

impl CheckoutPath {
    /// The path, refusing an absolute one or one that climbs. `""` and `.`
    /// are the root itself.
    pub fn parse(requested: &str) -> Result<Self, ReachError> {
        let refuse = |why: &str| Err(ReachError(format!("`{requested}` {why}")));
        if requested.contains('\0') {
            return refuse("contains a NUL byte");
        }
        if requested.contains('\\') {
            return refuse("uses `\\`; checkout paths are separated by `/`");
        }
        if requested.starts_with('/') || has_drive(requested) {
            return refuse("is absolute; checkout paths are relative to its root");
        }
        let mut parts = Vec::new();
        for part in requested.split('/') {
            match part {
                "" | "." => {}
                ".." => return refuse("climbs out with `..`"),
                part => parts.push(part),
            }
        }
        Ok(Self(parts.join("/")))
    }

    /// The path, `/`-separated, `""` for the root.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// One entry of a listed directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

/// What an entry is, without following it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Link,
    Other,
}

impl EntryKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "dir",
            Self::Link => "link",
            Self::Other => "other",
        }
    }
}

/// Why the host would not answer; the script sees it as a thrown error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ReachError(pub String);

/// The host's narrow interface for the functions a grant registers.
pub trait Reach: Send + Sync {
    /// A file beneath the checkout root, as text.
    fn read_checkout(&self, path: &CheckoutPath) -> Result<String, ReachError>;

    /// A directory beneath the checkout root.
    fn list_checkout(&self, path: &CheckoutPath) -> Result<Vec<Entry>, ReachError>;

    /// Time since the Unix epoch, which a test may fix.
    fn clock(&self) -> Duration {
        uze_platform::clock::since_epoch()
    }

    /// What the extension's store holds now, as JSON.
    fn store(&self) -> Result<serde_json::Value, ReachError> {
        Err(ReachError("this extension keeps no store".to_owned()))
    }

    /// Runs the program `name` an `exec:` grant covers with `args`, under
    /// the same pinning, bounds and confinement as a `run` source.
    fn run_program(&self, name: &str, args: &[String]) -> Result<ProgramReply, ReachError> {
        let _ = args;
        Err(ReachError(format!(
            "`{name}` is not a program this extension may run"
        )))
    }

    /// Calls `tool` of the MCP server `server` with `arguments`, already
    /// checked against the extension's `mcp:` grants, and answers its
    /// result as data (`extension-mcp`).
    fn mcp(
        &self,
        server: &str,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, ReachError> {
        let _ = arguments;
        Err(ReachError(format!(
            "mcp: `{server}/{tool}` is not reached here"
        )))
    }

    /// Makes `request`, already checked against the extension's `net:`
    /// grants, under the bounds and confinement of a program run.
    fn fetch(&self, request: &FetchRequest) -> Result<FetchReply, ReachError> {
        Err(ReachError(format!(
            "fetch: `{}` is not reached here",
            request.host
        )))
    }
}

/// How a program `run_program` started ended.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProgramReply {
    /// Its exit code, absent when it was ended by a signal or a bound.
    pub code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

/// The host functions, and the grant each needs (`None`: always there).
/// `run_program`'s grant is the `exec:` of the program it is called with,
/// so it is named apart, in [`RUN_PROGRAM`]. `fetch` needs a `net:` grant
/// of any host, which [`ANY_HOST`] stands for.
pub(crate) const HOST_FUNCTIONS: &[(&str, Option<Grant>)] = &[
    ("fetch", Some(ANY_HOST)),
    ("read_checkout", Some(Grant::ReadCheckout)),
    ("list_checkout", Some(Grant::ReadCheckout)),
    ("store", Some(Grant::Store)),
    ("timer", Some(Grant::Resident)),
    ("mcp", Some(ANY_TOOL)),
    ("clock", None),
];

/// The host function that runs a granted program.
pub(crate) const RUN_PROGRAM: &str = "run_program";

/// The grant `fetch` is registered under: a `net:` grant of any host.
pub const ANY_HOST: Grant = Grant::Net(String::new());

/// The grant `mcp` is registered under: an `mcp:` grant of any tool.
pub const ANY_TOOL: Grant = Grant::Mcp {
    server: String::new(),
    tool: String::new(),
};

/// Whether `grants` hold what `grant` asks: `store:global` is a store too,
/// [`ANY_HOST`] is held by any `net:` grant and [`ANY_TOOL`] by any
/// `mcp:` grant.
pub(crate) fn holds(grants: &Grants, grant: &Grant) -> bool {
    if *grant == ANY_HOST {
        return grants.iter().any(|held| held.host().is_some());
    }
    if *grant == ANY_TOOL {
        return grants.iter().any(|held| held.mcp_tool().is_some());
    }
    grants.contains(grant) || (*grant == Grant::Store && grants.contains(&Grant::StoreGlobal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_kept_normalised() {
        assert_eq!(CheckoutPath::parse("./a//b/").unwrap().as_str(), "a/b");
        assert_eq!(CheckoutPath::parse("").unwrap().as_str(), "");
    }

    #[test]
    fn a_path_that_leaves_the_root_is_refused_before_the_host_sees_it() {
        for refused in ["/etc/passwd", "a/../../b", "..", "C:/x", "a\\b", "a\0b"] {
            assert!(CheckoutPath::parse(refused).is_err(), "{refused}");
        }
    }
}
