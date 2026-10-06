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
    let recorded = std::fs::read_to_string(config_toml)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok());
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
pub(crate) fn project_untrusted(config_toml: &Path, project_root: &Path) -> bool {
    let Some(document) = std::fs::read_to_string(config_toml)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
    else {
        return false;
    };
    let canonical =
        std::fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    [project_root, canonical.as_path()].iter().any(|root| {
        document
            .get("projects")
            .and_then(|projects| projects.get(root.display().to_string()))
            .and_then(|project| project.get("trust_level"))
            .and_then(|level| level.as_str())
            == Some("untrusted")
    })
}

/// How much of a project's `AGENTS.md` Codex reads: `project_doc_max_bytes`
/// from `config.toml`, or Codex's default.
pub(crate) fn project_doc_max_bytes(config_toml: &Path) -> u64 {
    std::fs::read_to_string(config_toml)
        .ok()
        .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok())
        .and_then(|document| document.get("project_doc_max_bytes")?.as_integer())
        .and_then(|limit| u64::try_from(limit).ok())
        .unwrap_or(DEFAULT_PROJECT_DOC_MAX_BYTES)
}

/// Codex's default `project_doc_max_bytes` (codex-rs `core/src/config`).
const DEFAULT_PROJECT_DOC_MAX_BYTES: u64 = 32 * 1024;

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
            project_untrusted(&config, &project)
        };
        assert!(level("untrusted"));
        assert!(!level("trusted"));
        std::fs::write(&config, "").unwrap();
        assert!(
            !project_untrusted(&config, &project),
            "an unset level still reads it"
        );
    }

    #[test]
    fn the_project_doc_limit_is_codexs_default_unless_configured() {
        let root = uze_testkit::temp::scratch("project-doc-limit");
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        assert_eq!(project_doc_max_bytes(&config), 32 * 1024);
        std::fs::write(&config, "project_doc_max_bytes = 65536\n").unwrap();
        assert_eq!(project_doc_max_bytes(&config), 65536);
    }

    const CENSUS_HASH: &str =
        "sha256:580e559b2527966febf5d9dc4433194f434d08c835b527fe3bdfa25d6993a5e8";
}
