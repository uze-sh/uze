//! A project's rules on the harnesses that do not read `.agents/rules/`
//! themselves (project-rules, design D4 and D5): one `sh` engine, delivered
//! by machine setup into each harness's own configuration, which reads the
//! checkout a session is in and answers with the rules that apply.
//!
//! What differs between the harnesses is only *how the engine is started*,
//! which each vertical declares as a [`RulesTarget`] beside its hooks:
//! receipt-owned command-hook entries in an event-keyed config (Claude
//! Code's `settings.json`, Codex's `hooks.json`), or a generated plugin in
//! a global plugin directory (OpenCode). Nothing here branches on which
//! harness it is delivering to, and no `uze` is ever on the engine's
//! execution path.
//!
//! Every artifact is an `IntegrationOwned` receipt of [`KIND`], recorded
//! under `state::SETUP_OWNER`: the engine belongs to no package. Each
//! receipt carries the exact content it delivered, which is the identity
//! inspection compares against before anything is replaced or removed.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    home::UzeHome,
    hook::HookEvent,
    integration::{AttachmentInspection, AttachmentState, ManagedArtifact, RulesRoute},
};

use crate::hooks::{EntryShape, hook_event_name, merge_event_entry, remove_event_entry};
use crate::shared::{json_config, plan::blocked};

pub(crate) mod engine;
pub(crate) mod plugin;

/// The `IntegrationOwned` kind every rules-engine receipt carries.
pub(crate) const KIND: &str = "rules-engine";

/// The longest a harness waits for the engine. It reads a few small files
/// and runs `jq`, `git` and `awk` once or twice each; a checkout that keeps
/// it past this has something wrong with it, and the call goes on without
/// rules rather than waiting.
pub(crate) const ENGINE_TIMEOUT_SECONDS: u16 = 10;

/// The directory a package ships rules in. Rules reach a harness per
/// project, and every plugin route UZE has is machine-wide: a package's
/// `rules/` is therefore carried by no envelope (spec: "A package's rules
/// never reach a harness's machine-wide rules"). A project that declares
/// the plugin receives them as copies in its own checkout instead
/// (plugin-rules), which the project's rules route delivers.
pub(crate) const PACKAGE_RULES_DIR: &str = uze_core::rules::plugin::PACKAGE_RULES_DIR;

/// Why a package's `rules/` is not delivered, said wherever it is withheld,
/// naming the command that declares it in a project.
pub(crate) fn package_rules_withheld(package: &str) -> String {
    format!(
        "rules are delivered per project, never machine-wide: `uze {package}` in a project copies \
         them into its `.agents/rules/`"
    )
}

/// The package's `rules/`, when it has one, as every harness reports it: not
/// delivered on this scope, and why.
pub(crate) fn withheld(
    package: &uze_core::store::StoredPackage,
) -> Option<uze_core::integration::WithheldPath> {
    fs::symlink_metadata(package.root.join(PACKAGE_RULES_DIR))
        .is_ok()
        .then(|| uze_core::integration::WithheldPath {
            path: PACKAGE_RULES_DIR.to_owned(),
            reason: package_rules_withheld(package.id.as_str()),
        })
}

/// Why the engine reaches no harness on a platform whose shell it was not
/// written for: there is one implementation, and it is `sh`.
pub(crate) const NO_ENGINE_TEMPLATE: &str = "the rules engine is a POSIX `sh` script and there is no PowerShell template yet, so rules are not delivered on this platform";

/// How one harness starts the engine.
#[derive(Clone, Copy)]
pub(crate) struct RulesTarget {
    /// The harness's name in UZE's own state: where its engine is kept.
    pub key: &'static str,
    pub delivery: RulesDelivery,
}

#[derive(Clone, Copy)]
pub(crate) enum RulesDelivery {
    /// A `SessionStart` entry with no matcher, so it runs on every start
    /// (a compaction and a clear included), and a `PostToolUse` entry
    /// matched on `tools`, in the harness's event-keyed hooks config.
    Entries {
        shape: EntryShape,
        tools: &'static str,
    },
    /// A generated plugin, the harness's own runtime running the engine.
    Plugin,
}

/// What one rules receipt says it delivered.
struct Delivered<'a> {
    /// The config file holding the entry, or the plugin file itself.
    place: PathBuf,
    /// The entry's event; `None` for a plugin.
    event: Option<HookEvent>,
    expected: &'a str,
    engine: PathBuf,
}

impl<'a> Delivered<'a> {
    fn recorded(artifact: &'a ManagedArtifact) -> Option<Self> {
        let ManagedArtifact::IntegrationOwned {
            kind,
            selector,
            detail,
        } = artifact
        else {
            return None;
        };
        if kind != KIND {
            return None;
        }
        let path = |key: &str| detail.get(key)?.as_str().map(PathBuf::from);
        Some(Self {
            place: path("place")?,
            event: event_named(selector),
            expected: detail.get("expected")?.as_str()?,
            engine: path("engine")?,
        })
    }
}

/// Whether `artifact` is one of the rules engine's.
pub(crate) fn is_rules_artifact(artifact: &ManagedArtifact) -> bool {
    matches!(artifact, ManagedArtifact::IntegrationOwned { kind, .. } if kind == KIND)
}

const PLUGIN_SELECTOR: &str = "plugin";

fn event_named(selector: &str) -> Option<HookEvent> {
    [HookEvent::SessionStart, HookEvent::PostToolUse]
        .into_iter()
        .find(|event| hook_event_name(*event) == selector)
}

/// Whether this platform can run the engine at all.
pub(crate) fn deliverable_here() -> bool {
    uze_platform::shell::FAMILY == uze_platform::shell::Family::Posix
}

/// A harness's adapted route, or `Unsupported` on a platform the engine
/// does not run on.
pub(crate) fn adapted_route(reason: &str) -> RulesRoute {
    if deliverable_here() {
        RulesRoute::adapted(reason)
    } else {
        RulesRoute::unsupported(NO_ENGINE_TEMPLATE)
    }
}

impl RulesTarget {
    /// Where this harness's engine is kept: generated tier, beside its hook
    /// wrapper, and byte-identical on every harness.
    pub(crate) fn engine_path(self, uze_home: &UzeHome) -> PathBuf {
        crate::shared::path::attachment_root(uze_home, self.key).join(engine::RELATIVE_PATH)
    }

    /// The artifacts this harness is handed. `place` is the config file the
    /// entries go into, or the plugin file. Nothing where the engine cannot
    /// run.
    pub(crate) fn artifacts(self, uze_home: &UzeHome, place: &Path) -> Vec<ManagedArtifact> {
        if !deliverable_here() {
            return Vec::new();
        }
        let engine = self.engine_path(uze_home);
        let sessions = uze_home.rules_sessions_dir();
        let artifact = |selector: &str, expected: String| {
            let mut detail = BTreeMap::new();
            detail.insert("place".to_owned(), path_value(place));
            detail.insert("engine".to_owned(), path_value(&engine));
            detail.insert("expected".to_owned(), serde_json::Value::String(expected));
            ManagedArtifact::IntegrationOwned {
                kind: KIND.to_owned(),
                selector: selector.to_owned(),
                detail,
            }
        };
        match self.delivery {
            RulesDelivery::Entries { shape, tools } => {
                [HookEvent::SessionStart, HookEvent::PostToolUse]
                    .into_iter()
                    .map(|event| {
                        let entry = entry(shape, tools, event, &engine, &sessions);
                        artifact(
                            hook_event_name(event),
                            serde_json::to_string(&entry).expect("an entry serializes"),
                        )
                    })
                    .collect()
            }
            RulesDelivery::Plugin => {
                vec![artifact(
                    PLUGIN_SELECTOR,
                    plugin::source(&engine, &sessions),
                )]
            }
        }
    }

    /// Writes the engine, then every entry or file that starts it. What an
    /// earlier receipt of this integration recorded for the same selector is
    /// UZE's own, and is replaced rather than kept beside the new one.
    pub(crate) fn attach(
        self,
        uze_home: &UzeHome,
        integration_id: &str,
        place: &Path,
    ) -> Result<Vec<ManagedArtifact>> {
        let artifacts = self.artifacts(uze_home, place);
        if artifacts.is_empty() {
            return Ok(artifacts);
        }
        // An unreadable ledger has not said "no earlier version"; answering
        // that would merge a second entry beside the first.
        let recorded = uze_core::state::receipts(uze_home, Some(uze_core::state::SETUP_OWNER))?;
        let previous = |selector: &str| -> Vec<String> {
            recorded
                .iter()
                .filter(|receipt| receipt.integration == integration_id)
                .filter_map(|receipt| match &receipt.artifact {
                    ManagedArtifact::IntegrationOwned {
                        kind,
                        selector: recorded_selector,
                        detail,
                    } if kind == KIND && recorded_selector == selector => detail
                        .get("expected")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    _ => None,
                })
                .collect()
        };
        materialize_engine(&self.engine_path(uze_home))?;
        for artifact in &artifacts {
            let ManagedArtifact::IntegrationOwned { selector, .. } = artifact else {
                continue;
            };
            let delivered = Delivered::recorded(artifact).expect("a rules artifact");
            match delivered.event {
                Some(event) => {
                    let entry: serde_json::Value =
                        serde_json::from_str(delivered.expected).expect("UZE's own entry parses");
                    merge_event_entry(&delivered.place, event, &entry, &previous(selector))?;
                }
                None => write_plugin(&delivered.place, delivered.expected, &previous(selector))?,
            }
        }
        Ok(artifacts)
    }
}

/// One entry, in the form `shape` takes: the engine started directly with
/// the sessions directory as its one argument.
fn entry(
    shape: EntryShape,
    tools: &str,
    event: HookEvent,
    engine: &Path,
    sessions: &Path,
) -> serde_json::Value {
    let (program, mut arguments) = uze_platform::shell::script(&engine.display().to_string());
    arguments.push(sessions.display().to_string());
    let mut handler = serde_json::Map::new();
    handler.insert("type".to_owned(), serde_json::json!("command"));
    if shape == EntryShape::EventExec {
        handler.insert("command".to_owned(), serde_json::json!(program));
        handler.insert("args".to_owned(), serde_json::json!(arguments));
    } else {
        handler.insert(
            "command".to_owned(),
            serde_json::json!(uze_platform::shell::command_line(&program, &arguments)),
        );
    }
    handler.insert(
        "timeout".to_owned(),
        serde_json::json!(ENGINE_TIMEOUT_SECONDS),
    );
    let mut group = serde_json::Map::new();
    if event == HookEvent::PostToolUse {
        group.insert("matcher".to_owned(), serde_json::json!(tools));
    }
    group.insert(
        "hooks".to_owned(),
        serde_json::Value::Array(vec![serde_json::Value::Object(handler)]),
    );
    serde_json::Value::Object(group)
}

fn path_value(path: &Path) -> serde_json::Value {
    serde_json::Value::String(path.display().to_string())
}

/// Writes the engine, executable, unless it is already exactly that: a
/// harness may be running it right now.
fn materialize_engine(path: &Path) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|current| current == engine::SOURCE) {
        return if uze_platform::executable::is_marked_runnable(path) {
            Ok(())
        } else {
            make_runnable(path)
        };
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
    }
    uze_core::persistence::write_atomic_preserving(path, engine::SOURCE.as_bytes())?;
    make_runnable(path)
}

fn make_runnable(path: &Path) -> Result<()> {
    uze_platform::executable::make_runnable(path).map_err(|source| UzeError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes the plugin file, refusing one at that path that is neither absent
/// nor UZE's: the operator's own plugin keeps its name.
fn write_plugin(path: &Path, content: &str, previous: &[String]) -> Result<()> {
    match fs::read_to_string(path) {
        Ok(current) if current == content => return Ok(()),
        Ok(current) if !current.starts_with(plugin::HEADER) && !previous.contains(&current) => {
            return Err(UzeError::ManagedEntryConflict(path.to_path_buf()));
        }
        _ => {}
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
    }
    uze_core::persistence::write_atomic(path, content.as_bytes())
}

/// The delivered artifact's state: what starts the engine first, then the
/// engine.
pub(crate) fn inspect(artifact: &ManagedArtifact) -> AttachmentInspection {
    let Some(delivered) = Delivered::recorded(artifact) else {
        return blocked("the rules receipt is unreadable");
    };
    let starter = match delivered.event {
        Some(event) => inspect_entry(&delivered, event),
        None => inspect_plugin(&delivered),
    };
    if starter.state != AttachmentState::Matched {
        return starter;
    }
    inspect_engine(&delivered.engine)
}

fn inspect_entry(delivered: &Delivered, event: HookEvent) -> AttachmentInspection {
    let Ok(config) = json_config::read_object(&delivered.place) else {
        return blocked("the hook config is missing or unreadable");
    };
    let Ok(expected) = serde_json::from_str::<serde_json::Value>(delivered.expected) else {
        return blocked("the rules receipt carries an unreadable entry");
    };
    let entries = config
        .get("hooks")
        .and_then(|hooks| hooks.get(hook_event_name(event)))
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if entries.contains(&expected) {
        return state(
            AttachmentState::Matched,
            "the rules engine's entry matches the receipt",
        );
    }
    // Content identity alone would read an edited entry as absent and let
    // removal forget a hook that still runs the engine.
    let engine = path_value(&delivered.engine).to_string();
    let engine = engine.trim_matches('"');
    if entries
        .iter()
        .any(|candidate| candidate.to_string().contains(engine))
    {
        return state(
            AttachmentState::Drifted,
            "the rules engine's entry was edited after UZE wrote it",
        );
    }
    state(
        AttachmentState::Missing,
        "the rules engine's entry is absent",
    )
}

fn inspect_plugin(delivered: &Delivered) -> AttachmentInspection {
    match fs::read_to_string(&delivered.place) {
        Ok(current) if current == delivered.expected => state(
            AttachmentState::Matched,
            "the rules plugin matches the receipt",
        ),
        Ok(_) => state(
            AttachmentState::Drifted,
            "the rules plugin differs from what UZE wrote",
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            state(AttachmentState::Missing, "the rules plugin is absent")
        }
        Err(_) => blocked("the rules plugin is unreadable"),
    }
}

/// The engine is the other half of every delivery. One an earlier build
/// wrote is still UZE's: it is generated tier, and the next setup writes the
/// current one, so it is not drift that blocks a removal.
fn inspect_engine(path: &Path) -> AttachmentInspection {
    let Ok(current) = fs::read_to_string(path) else {
        return state(AttachmentState::Missing, "the rules engine is absent");
    };
    if !current.starts_with(HEADER_LINE) {
        return state(
            AttachmentState::Drifted,
            "the rules engine does not match what UZE writes",
        );
    }
    if !uze_platform::executable::is_marked_runnable(path) {
        return state(
            AttachmentState::Drifted,
            "the rules engine is not executable",
        );
    }
    if current == engine::SOURCE {
        state(
            AttachmentState::Matched,
            "the rules engine matches the receipt",
        )
    } else {
        state(
            AttachmentState::Matched,
            "the rules engine matches the receipt; it was generated by an earlier build and the next setup rewrites it",
        )
    }
}

const HEADER_LINE: &str = engine::HEADER;

fn state(state: AttachmentState, reason: &str) -> AttachmentInspection {
    AttachmentInspection {
        state,
        reason: reason.to_owned(),
    }
}

/// Removes a matched delivery, then the engine once nothing starts it.
/// Anything not matched is answered with its inspection and left alone.
pub(crate) fn detach(artifact: &ManagedArtifact) -> Result<AttachmentInspection> {
    let inspection = inspect(artifact);
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    let delivered = Delivered::recorded(artifact).expect("an inspected rules artifact");
    match delivered.event {
        Some(event) => {
            remove_event_entry(&delivered.place, event, delivered.expected)?;
        }
        None => {
            crate::hooks::remove_bridge_file(&delivered.place)?;
        }
    }
    if !still_started(&delivered) {
        let _ = fs::remove_file(&delivered.engine);
        if let Some(parent) = delivered.engine.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
    Ok(state(
        AttachmentState::Missing,
        "the rules engine's delivery was detached",
    ))
}

/// Whether anything at this delivery's place still names the engine: the
/// other event's entry, read from the file rather than from the ledger,
/// which still lists every receipt of a removal until it returns.
fn still_started(delivered: &Delivered) -> bool {
    let engine = path_value(&delivered.engine).to_string();
    let engine = engine.trim_matches('"');
    fs::read_to_string(&delivered.place).is_ok_and(|text| {
        text.contains(engine) || text.contains(&delivered.engine.display().to_string())
    })
}

#[cfg(test)]
mod tests;

/// The generated engine against real `sh`, `jq`, `git` and `awk`.
// The engine is POSIX `sh`, run with Unix file modes.
#[cfg(all(test, unix))]
mod engine_tests;
