//! What Codex holds back until the person trusts it: the hooks of its review.
//!
//! Codex runs a hook from the user's `hooks.json` only when its
//! `config.toml` records a `trusted_hash` for that handler equal to the
//! handler's current identity; with none it is untrusted, with another it
//! was modified, and either way it never runs — a TUI session asks, a
//! `codex exec` skips it in silence. The key and the identity are Codex's
//! (codex-rs `hooks/src/lib.rs` and `hooks/src/engine/discovery.rs`, tag
//! `rust-v0.160.1`), and the Lab reproduced every hash a real review
//! recorded (`experiments/codex/trust-store`).
//!
//! UZE reads that record and never writes it: answering the review for
//! the person is the bypass this exists to report, not to perform.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uze_core::hook::HookEvent;

use crate::hooks::HookEntry;

/// Where a delivered hook stands in Codex's review.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Review {
    Trusted,
    /// Never trusted: no hash is recorded for it.
    Untrusted,
    /// Trusted once, then changed: the recorded hash is another one.
    Modified,
}

impl Review {
    /// What the person does in Codex for the hook to run, or `None` when it
    /// already does.
    pub(crate) fn action(&self) -> Option<&'static str> {
        match self {
            Self::Trusted => None,
            Self::Untrusted => Some(
                "Codex runs it only once you trust it: open Codex and choose \
                 \"Trust all and continue\" on its hook review",
            ),
            Self::Modified => Some(
                "it changed since you trusted it, and Codex holds it until you \
                 review it again: open Codex and trust it on its hook review",
            ),
        }
    }
}

/// The review state of the delivered group `entry` records, from the
/// `hooks.json` it sits in and the `config.toml` beside it. `None` when the
/// group is not where its receipt says, which the receipt's own inspection
/// reports.
pub(crate) fn review(config_toml: &Path, entry: &HookEntry) -> Option<Review> {
    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(entry.config_file).ok()?).ok()?;
    let expected: Value = serde_json::from_str(entry.expected).ok()?;
    let groups = hooks
        .get("hooks")?
        .get(crate::hooks::hook_event_name(entry.event))?
        .as_array()?;
    let group_index = groups.iter().position(|group| group == &expected)?;
    let handlers = expected.get("hooks")?.as_array()?;
    let recorded = read_config(config_toml);
    let mut review = Review::Trusted;
    for (handler_index, handler) in handlers.iter().enumerate() {
        let key = format!(
            "{}:{}:{group_index}:{handler_index}",
            entry.config_file.display(),
            entry.event.abi_name()
        );
        let current = identity_hash(entry.event, expected.get("matcher"), handler);
        let stored = recorded.as_ref().and_then(|document| {
            document
                .get("hooks")?
                .get("state")?
                .get(&key)?
                .get("trusted_hash")?
                .as_str()
                .map(str::to_owned)
        });
        match stored {
            None => return Some(Review::Untrusted),
            Some(stored) if stored != current => review = Review::Modified,
            Some(_) => {}
        }
    }
    Some(review)
}

/// Whether the person marked `project_root` untrusted in Codex — the one
/// trust level that makes Codex skip the project's `AGENTS.md`.
pub(crate) fn project_untrusted(
    config: Option<&toml_edit::DocumentMut>,
    project_root: &Path,
) -> bool {
    config
        .and_then(|document| trust_level(document, project_root))
        .as_deref()
        == Some("untrusted")
}

/// Whether the person trusted, in Codex, the project a launch from `cwd`
/// sits in: Codex asks about the directory itself, then about the primary
/// checkout of the repository it belongs to (`primary_checkout`, asked only
/// when the directory has no level of its own), so a linked worktree takes
/// its repository's trust (codex-rs `get_active_project`). Only an explicit
/// `trusted` counts, since an unset level is every project nobody was ever
/// asked about.
pub(crate) fn project_trusted(
    config_toml: &Path,
    cwd: &Path,
    primary_checkout: impl FnOnce() -> Option<std::path::PathBuf>,
) -> bool {
    let Some(document) = read_config(config_toml) else {
        return false;
    };
    trust_level(&document, cwd)
        .or_else(|| trust_level(&document, &primary_checkout()?))
        .as_deref()
        == Some("trusted")
}

/// Codex's `config.toml`, when it exists and parses.
pub(crate) fn read_config(config_toml: &Path) -> Option<toml_edit::DocumentMut> {
    std::fs::read_to_string(config_toml).ok()?.parse().ok()
}

/// The level `config.toml` records for `root`, spelled as given or
/// canonical.
fn trust_level(document: &toml_edit::DocumentMut, root: &Path) -> Option<String> {
    let canonical = uze_platform::path::canonical(root).unwrap_or_else(|_| root.to_path_buf());
    [root, canonical.as_path()].iter().find_map(|root| {
        document
            .get("projects")?
            .get(root.display().to_string())?
            .get("trust_level")?
            .as_str()
            .map(str::to_owned)
    })
}

/// How much of a project's `AGENTS.md` Codex reads: `project_doc_max_bytes`
/// from `config.toml`, or Codex's default.
pub(crate) fn project_doc_max_bytes(config: Option<&toml_edit::DocumentMut>) -> u64 {
    config
        .and_then(|document| document.get("project_doc_max_bytes")?.as_integer())
        .and_then(|limit| u64::try_from(limit).ok())
        .unwrap_or(DEFAULT_PROJECT_DOC_MAX_BYTES)
}

/// Codex's default `project_doc_max_bytes` (codex-rs `core/src/config`).
const DEFAULT_PROJECT_DOC_MAX_BYTES: u64 = 32 * 1024;

/// The trust a launch gives one handler it declares on its own `-c` layer:
/// the key Codex files a session-flags handler under, and the hash of that
/// exact handler. Answering the review for a handler UZE wrote, for the one
/// process it was given to, is the single exception to reading the record
/// and never writing it — nothing is written into the person's
/// configuration, and `--dangerously-bypass-hook-trust` would trust every
/// hook of the invocation, theirs included.
pub(crate) fn session_layer_trust(
    event: HookEvent,
    command: &str,
    timeout: u64,
) -> (String, String) {
    let handler = serde_json::json!({ "type": "command", "command": command, "timeout": timeout });
    (
        format!("/<session-flags>/config.toml:{}:0:0", event.abi_name()),
        identity_hash(event, None, &handler),
    )
}

/// Codex's identity of one handler of a group: the event, the group's
/// matcher (none for an event that takes none), and the handler with its
/// defaults filled in, as JSON with sorted keys and no whitespace, hashed.
fn identity_hash(event: HookEvent, matcher: Option<&Value>, handler: &Value) -> String {
    let mut normalized = Map::new();
    normalized.insert("type".into(), Value::from("command"));
    normalized.insert(
        "command".into(),
        handler.get("command").cloned().unwrap_or(Value::from("")),
    );
    normalized.insert(
        "timeout".into(),
        handler
            .get("timeout")
            .cloned()
            .unwrap_or(Value::from(DEFAULT_TIMEOUT_SECONDS)),
    );
    normalized.insert(
        "async".into(),
        handler.get("async").cloned().unwrap_or(Value::Bool(false)),
    );
    if let Some(message) = handler.get("statusMessage") {
        normalized.insert("statusMessage".into(), message.clone());
    }
    let mut identity = Map::new();
    identity.insert("event_name".into(), Value::from(event.abi_name()));
    identity.insert(
        "hooks".into(),
        Value::Array(vec![Value::Object(normalized)]),
    );
    if let Some(matcher) = matcher.filter(|_| event != HookEvent::Stop) {
        identity.insert("matcher".into(), matcher.clone());
    }
    let mut canonical = String::new();
    write_sorted(&Value::Object(identity), &mut canonical);
    // Spelled byte by byte: sha2 0.11 offers no `LowerHex` on a digest.
    let digest = Sha256::digest(canonical.as_bytes());
    let mut spelled = String::from("sha256:");
    for byte in digest {
        let _ = write!(spelled, "{byte:02x}");
    }
    spelled
}

/// Codex's default handler timeout, filled in before hashing.
const DEFAULT_TIMEOUT_SECONDS: u64 = 600;

/// Compact JSON with every object's keys sorted, as Codex fingerprints it.
fn write_sorted(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_sorted(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash the Lab's `experiments/codex/agent-context` handed Codex on
    /// the session layer, which it accepted with no review: the same
    /// canonical JSON, computed independently.
    #[test]
    fn a_session_layer_handler_is_trusted_by_codexs_own_identity() {
        let (key, hash) = session_layer_trust(HookEvent::SessionStart, "cat /x/context.md", 10);
        assert_eq!(key, "/<session-flags>/config.toml:session_start:0:0");
        assert_eq!(
            hash,
            "sha256:c1acea534e87e0c6e5c0e9f23bfd4512f60eef563a730348aa6f891c54662192"
        );
    }

    /// A linked worktree has no level of its own and takes its primary
    /// checkout's, as Codex resolves it; a level on the directory itself
    /// decides first, and only `trusted` counts.
    #[test]
    fn a_project_is_trusted_by_its_own_level_else_by_its_primary_checkout() {
        let root = uze_testkit::temp::scratch("codex-project-trust");
        let config = root.join("config.toml");
        let (primary, slot, other) = (
            root.join("repo"),
            root.join("repo/.worktrees/a"),
            root.join("other"),
        );
        std::fs::write(
            &config,
            format!(
                "[projects.{:?}]\ntrust_level = \"trusted\"\n[projects.{:?}]\ntrust_level = \"untrusted\"\n",
                primary.display().to_string(),
                other.display().to_string()
            ),
        )
        .unwrap();
        assert!(project_trusted(&config, &primary, || None));
        assert!(project_trusted(&config, &slot, || Some(primary.clone())));
        assert!(!project_trusted(&config, &slot, || None));
        assert!(!project_trusted(&config, &other, || Some(primary.clone())));
        assert!(!project_trusted(
            &root.join("absent.toml"),
            &primary,
            || None
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A hash a real Codex 0.160.1 review recorded for a group UZE
    /// delivered (`experiments/codex/trust-store`, 2026-10-06).
    #[test]
    fn the_identity_hashes_as_codex_recorded_it() {
        let handler = serde_json::json!({
            "type": "command",
            "command": "'/work/home/.uze/runtime/attachments/codex/hooks/exec' '/work/home/.uze/runtime/packages/hook-rows@uze-lab' 'pre_tool_use' 'observe' '10:/work/home/.uze/runtime/packages/hook-rows@uze-lab/scripts/probe census allow'",
            "timeout": 12,
        });
        assert_eq!(
            identity_hash(HookEvent::PreToolUse, None, &handler),
            CENSUS_HASH,
        );
    }

    /// One delivered group in a `hooks.json`, a foreign one before it, and
    /// a `config.toml` holding `trusted`'s hash for it, or none.
    fn scene(
        label: &str,
        trusted: Option<&str>,
    ) -> (std::path::PathBuf, String, std::path::PathBuf) {
        let root = uze_testkit::temp::scratch(label);
        std::fs::create_dir_all(&root).unwrap();
        let hooks = root.join("hooks.json");
        let ours = serde_json::json!({
            "hooks": [{"type": "command", "command": "'/w/exec' 'pre_tool_use' 'observe'", "timeout": 12}]
        });
        let foreign = serde_json::json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "mine"}]});
        std::fs::write(
            &hooks,
            serde_json::json!({"hooks": {"PreToolUse": [foreign, ours.clone()]}}).to_string(),
        )
        .unwrap();
        let config = root.join("config.toml");
        let key = format!("{}:pre_tool_use:1:0", hooks.display());
        let state = trusted.map_or_else(String::new, |hash| {
            format!("[hooks.state.'{key}']\ntrusted_hash = \"{hash}\"\n")
        });
        std::fs::write(&config, state).unwrap();
        (hooks, ours.to_string(), config)
    }

    fn reviewed(label: &str, trusted: Option<&str>) -> Option<Review> {
        let (hooks, expected, config) = scene(label, trusted);
        let entry = HookEntry {
            config_file: &hooks,
            entry_name: "lab:group",
            event: HookEvent::PreToolUse,
            expected: &expected,
            wrapper: Path::new("/w/exec"),
        };
        review(&config, &entry)
    }

    #[test]
    fn a_group_with_no_recorded_hash_awaits_review() {
        assert_eq!(reviewed("trust-none", None), Some(Review::Untrusted));
    }

    #[test]
    fn a_group_whose_recorded_hash_is_its_own_is_trusted() {
        let handler = serde_json::json!({"type": "command", "command": "'/w/exec' 'pre_tool_use' 'observe'", "timeout": 12});
        let hash = identity_hash(HookEvent::PreToolUse, None, &handler);
        assert_eq!(reviewed("trust-same", Some(&hash)), Some(Review::Trusted));
    }

    #[test]
    fn a_group_changed_since_its_review_awaits_it_again() {
        assert_eq!(
            reviewed("trust-changed", Some("sha256:00")),
            Some(Review::Modified)
        );
    }

    #[test]
    fn only_a_project_marked_untrusted_goes_unread() {
        let root = uze_testkit::temp::scratch("project-trust");
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let level = |level: &str| {
            std::fs::write(
                &config,
                format!(
                    "[projects.'{}']\ntrust_level = \"{level}\"\n",
                    project.display()
                ),
            )
            .unwrap();
            project_untrusted(read_config(&config).as_ref(), &project)
        };
        assert!(level("untrusted"));
        assert!(!level("trusted"));
        std::fs::write(&config, "").unwrap();
        assert!(
            !project_untrusted(read_config(&config).as_ref(), &project),
            "an unset level still reads it"
        );
    }

    #[test]
    fn the_project_doc_limit_is_codexs_default_unless_configured() {
        let root = uze_testkit::temp::scratch("project-doc-limit");
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        assert_eq!(
            project_doc_max_bytes(read_config(&config).as_ref()),
            32 * 1024
        );
        std::fs::write(&config, "project_doc_max_bytes = 65536\n").unwrap();
        assert_eq!(project_doc_max_bytes(read_config(&config).as_ref()), 65536);
    }

    const CENSUS_HASH: &str =
        "sha256:580e559b2527966febf5d9dc4433194f434d08c835b527fe3bdfa25d6993a5e8";
}
