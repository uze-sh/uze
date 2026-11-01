//! The POSIX `sh` rules engine: what Claude Code and Codex run as a command
//! hook, and what OpenCode's generated plugin asks through Bun.
//!
//! One file, byte-identical wherever it is delivered: the sessions
//! directory arrives as its only argument, so nothing in the template
//! depends on the machine and nothing in it names the packager (ADR-040
//! D8). It reads the checkout the session is in on every call (design D2),
//! so a branch or a worktree with different rules is answered with its own.

/// Where the engine sits inside the bundled `uze` package, and so inside its
/// build on every harness: one path a reviewer can look for everywhere. It
/// is a real file of `plugins/uze`, rendered into each build like any
/// package file; this module only reads it.
pub(crate) const RELATIVE_PATH: &str = "engine/rules";

/// How every generated engine opens, whichever build wrote it: what tells
/// an engine an earlier template produced from a file somebody else wrote.
pub(crate) const HEADER: &str = "#!/bin/sh\n# rules/engine — generated";

/// The engine itself (design D5). Three small programs do the work, each
/// kept in a shell variable so the file stays one artifact:
///
/// - `CANDIDATES` (jq) lists what a tool call may have touched: each
///   one-line string field, each patch header, and each line of a shell
///   command, marked by how much it is to be trusted;
/// - `NORMALIZE` (awk) makes each one a path relative to the checkout root
///   and drops anything outside it; a shell token, or a field that only
///   looks like a path, must also exist to count;
/// - `MANIFEST` (jq) reads `.agents/rules.json` as `uze-core`'s rule model
///   reads it (`uze_core::rules::manifest`): each entry's path relative to
///   the checkout root, its `include_only` and `exclude` relative to the
///   entry, and nothing at all from a manifest it cannot parse;
/// - `RULES` (awk) parses each rule's frontmatter, translates its globs to
///   EREs and composes the answer, recording what it handed per session.
pub(crate) const SOURCE: &str = include_str!("../../../../plugins/uze/engine/rules");
