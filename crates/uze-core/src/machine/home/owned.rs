//! The map of what UZE owns, as a list: every path [`UzeHome`] composes,
//! each with what deleting it costs.
//!
//! The builders on [`UzeHome`] say where a thing goes; this says what it
//! is, so a reader can enumerate the home rather than discover it. The
//! health catalogue checks each entry, the coverage tests fail on an entry
//! nothing checks, and a later collector or schema comparison reads the
//! same list: a path added to the map without an entry here fails
//! `every_path_uze_owns_is_named_in_the_map`.
//!
//! A parameterised path (one per project, per plugin, per agent) is listed
//! once, composed with a placeholder segment such as `{project}`, so its
//! place in the tree is still the builder's to say.

use std::path::PathBuf;

use uze_document::earlier::{Earlier, Lasts};

use crate::store::PackageId;

use super::UzeHome;

/// What deleting a path costs, which is what decides where it lives (the
/// `persistence-tiers` rule).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Tier {
    /// `sources/`: the packages, until they are acquired again.
    Bytes,
    /// `store/`: what each harness reads; deleting it uninstalls their
    /// plugins until they are built again.
    Store,
    /// `state/`: the operator's, since nothing else knows it.
    Record,
    /// The root's own files: the operator's own text.
    Authored,
    /// `runtime/`, `shims/`: nothing, it is produced again.
    Generated,
    /// `cache/`: nothing, it is observed again.
    Remembered,
}

impl Tier {
    pub const ALL: [Tier; 6] = [
        Tier::Bytes,
        Tier::Store,
        Tier::Record,
        Tier::Authored,
        Tier::Generated,
        Tier::Remembered,
    ];

    /// The tier a path under `home` sits in, by its top-level entry; `None`
    /// for a path outside the home or the home itself.
    pub fn of(home: &UzeHome, path: &std::path::Path) -> Option<Tier> {
        let relative = path.strip_prefix(home.root()).ok()?;
        let mut components = relative.components();
        let first = components.next()?;
        let nested = components.next().is_some();
        let first = PathBuf::from(first.as_os_str());
        Some(if first == relative_to(home, home.sources_dir()) {
            Tier::Bytes
        } else if first == relative_to(home, home.store_dir()) {
            Tier::Store
        } else if first == relative_to(home, home.state_dir()) {
            Tier::Record
        } else if first == relative_to(home, home.cache_dir()) {
            Tier::Remembered
        } else if first == relative_to(home, home.runtime_dir())
            || first == relative_to(home, home.shims_dir())
        {
            Tier::Generated
        } else if !nested || first == relative_to(home, home.themes_dir()) {
            Tier::Authored
        } else {
            return None;
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Tier::Bytes => "bytes",
            Tier::Store => "store",
            Tier::Record => "record",
            Tier::Authored => "authored",
            Tier::Generated => "generated",
            Tier::Remembered => "remembered",
        }
    }
}

fn relative_to(home: &UzeHome, path: PathBuf) -> PathBuf {
    path.strip_prefix(home.root())
        .map(PathBuf::from)
        .unwrap_or(path)
}

/// What sits at an owned path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Form {
    Directory,
    /// A file read as a whole: a record, a cache entry, a link, a log.
    Document,
    /// A file held for exclusion and never read: a kernel `flock`, which
    /// cannot go stale, so nothing ever clears one.
    Lock,
    /// A subtree another component composes and keeps for itself, live
    /// while it runs: nothing collects it, walks it or compares it with
    /// this list.
    Opaque,
}

/// Whether this build still reads a path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    Current,
    /// Written by an earlier release and read by none this one supports
    /// except the step that carries or removes it. Its name is never used
    /// for anything else.
    Retired,
}

/// One path UZE owns: the builder that composes it, its tier, its form and
/// whether this build still reads it.
#[derive(Clone, Copy)]
pub struct OwnedPath {
    /// The [`UzeHome`] builder that composes it: its stable name.
    pub name: &'static str,
    pub tier: Tier,
    pub form: Form,
    pub standing: Standing,
    /// Whether UZE sweeps it: a generated or remembered path whose owner
    /// decides what of it is still needed, or a retired one nothing
    /// carries across. Each has exactly one collector
    /// (`every_disposable_root_has_one_collector`); the map holds no rule
    /// of its own.
    pub disposable: bool,
    /// For a retired path, how long a machine can still hold it (the
    /// `earlier-releases` rule); a current path lasts as long as it is
    /// written.
    pub lasts: Lasts,
    /// For a retired path, the names only it spells: no production code
    /// outside this map writes them.
    pub spelled: &'static [&'static str],
    compose: fn(&UzeHome) -> PathBuf,
}

impl std::fmt::Debug for OwnedPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnedPath")
            .field("name", &self.name)
            .field("tier", &self.tier)
            .field("form", &self.form)
            .field("standing", &self.standing)
            .field("disposable", &self.disposable)
            .field("lasts", &self.lasts)
            .finish()
    }
}

impl OwnedPath {
    /// Where it is in `home`, parameterised segments spelled as
    /// placeholders (`{project}`).
    pub fn path(&self, home: &UzeHome) -> PathBuf {
        (self.compose)(home)
    }

    /// Its place relative to the home, one segment per component, every
    /// parameterised part spelled as a placeholder: `{project}`, and a
    /// package's `{package}`, `{marketplace}` and `{plugin}`.
    pub fn pattern(&self, home: &UzeHome) -> Vec<String> {
        let path = self.path(home);
        let relative = path.strip_prefix(home.root()).unwrap_or(&path);
        relative
            .components()
            .map(|component| {
                let segment = component.as_os_str().to_string_lossy();
                match PACKAGE_SEGMENTS
                    .iter()
                    .find(|(literal, _)| *literal == segment)
                {
                    Some((_, placeholder)) => (*placeholder).to_owned(),
                    None => segment.into_owned(),
                }
            })
            .collect()
    }

    /// Whether `relative` (segments under the home) is this path, each
    /// placeholder standing for any text within one segment.
    pub fn matches(&self, home: &UzeHome, relative: &[String]) -> bool {
        let pattern = self.pattern(home);
        pattern.len() == relative.len()
            && pattern
                .iter()
                .zip(relative)
                .all(|(pattern, segment)| segment_matches(pattern, segment))
    }

    /// Whether `relative` is a directory above this path, which a walk of
    /// the home descends into to reach it.
    pub fn lies_under(&self, home: &UzeHome, relative: &[String]) -> bool {
        let pattern = self.pattern(home);
        pattern.len() > relative.len()
            && pattern
                .iter()
                .zip(relative)
                .all(|(pattern, segment)| segment_matches(pattern, segment))
    }

    const fn swept(self) -> Self {
        OwnedPath {
            disposable: true,
            ..self
        }
    }

    /// The names only this retired path spells.
    const fn spelled(self, spelled: &'static [&'static str]) -> Self {
        OwnedPath { spelled, ..self }
    }
}

/// How [`package`]'s placeholder id is spelled in a path, and the
/// placeholder each spelling stands for.
const PACKAGE_SEGMENTS: [(&str, &str); 3] = [
    ("plugin@marketplace", "{package}"),
    ("marketplace", "{marketplace}"),
    ("plugin", "{plugin}"),
];

/// Whether `segment` is what `pattern` spells, each `{…}` in it standing
/// for any text, none included.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    let mut pieces = Vec::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        pieces.push(&rest[..open]);
        rest = &rest[open + close + 1..];
    }
    pieces.push(rest);
    if pieces.len() == 1 {
        return pattern == segment;
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if !segment.starts_with(first) || !segment[first.len()..].ends_with(last) {
        return false;
    }
    let mut middle = &segment[first.len()..segment.len() - last.len()];
    for piece in &pieces[1..pieces.len() - 1] {
        match middle.find(piece) {
            Some(at) => middle = &middle[at + piece.len()..],
            None => return false,
        }
    }
    segment.len() >= first.len() + last.len()
}

const fn owned(
    name: &'static str,
    tier: Tier,
    form: Form,
    compose: fn(&UzeHome) -> PathBuf,
) -> OwnedPath {
    OwnedPath {
        name,
        tier,
        form,
        standing: Standing::Current,
        disposable: false,
        lasts: Lasts::Unbounded,
        spelled: &[],
        compose,
    }
}

const fn retired(
    name: &'static str,
    tier: Tier,
    form: Form,
    compose: fn(&UzeHome) -> PathBuf,
) -> OwnedPath {
    OwnedPath {
        name,
        tier,
        form,
        standing: Standing::Retired,
        disposable: false,
        lasts: Lasts::Unbounded,
        spelled: &[],
        compose,
    }
}

const PROJECT: &str = "{project}";
const AGENT: &str = "{agent}";
const PLUGIN: &str = "{plugin}";
const EXTENSION: &str = "{extension}";
const VENDOR: &str = "{vendor}";

fn package() -> PackageId {
    PackageId::from_qualified("plugin@marketplace", std::path::Path::new("plugin.json"))
        .expect("a valid placeholder id")
}

use Form::{Directory, Document, Lock, Opaque};
use Tier::{Authored, Bytes, Generated, Record, Remembered, Store};

/// Every path [`UzeHome`] composes. Order follows the tiers.
const OWNED: &[OwnedPath] = &[
    // Bytes.
    owned("sources_dir", Bytes, Directory, UzeHome::sources_dir),
    owned("plugins_dir", Bytes, Directory, UzeHome::plugins_dir),
    owned("plugin_dir", Bytes, Directory, |home| {
        home.plugin_dir(&package())
    }),
    owned("superseded_dir", Bytes, Directory, |home| {
        home.superseded_dir(&package())
    }),
    owned("restoring_dir", Bytes, Directory, |home| {
        home.restoring_dir(&package())
    }),
    retired("pre_sources_layout", Bytes, Directory, |home| {
        home.pre_sources_layout()[0].0.clone()
    }),
    // Store.
    owned("store_dir", Store, Directory, UzeHome::store_dir),
    owned("vendor_store_dir", Store, Directory, |home| {
        home.vendor_store_dir(VENDOR)
    }),
    owned("build_dir", Store, Directory, |home| {
        home.build_dir(VENDOR, PLUGIN)
    }),
    // Records.
    owned("state_dir", Record, Directory, UzeHome::state_dir),
    owned("registry_path", Record, Document, UzeHome::registry_path),
    owned(
        "attachments_path",
        Record,
        Document,
        UzeHome::attachments_path,
    ),
    owned(
        "undelivered_path",
        Record,
        Document,
        UzeHome::undelivered_path,
    ),
    owned(
        "provisioning_state_path",
        Record,
        Document,
        UzeHome::provisioning_state_path,
    ),
    owned(
        "marketplaces_path",
        Record,
        Document,
        UzeHome::marketplaces_path,
    ),
    owned("hosts_path", Record, Document, UzeHome::hosts_path),
    owned(
        "launch_overrides_path",
        Record,
        Document,
        UzeHome::launch_overrides_path,
    ),
    owned(
        "extensions_record_path",
        Record,
        Document,
        UzeHome::extensions_record_path,
    ),
    owned(
        "extension_stores_dir",
        Record,
        Directory,
        UzeHome::extension_stores_dir,
    ),
    owned("extension_plugin_stores_dir", Record, Directory, |home| {
        home.extension_plugin_stores_dir(PLUGIN)
    }),
    owned("extension_store_path", Record, Document, |home| {
        home.extension_store_path(PLUGIN, EXTENSION, Some(PROJECT))
    }),
    owned("extension_secrets_set_path", Record, Document, |home| {
        home.extension_secrets_set_path(PLUGIN, EXTENSION)
    }),
    owned("extension_secret_path", Record, Document, |home| {
        home.extension_secret_path(PLUGIN, EXTENSION, "{name}")
    }),
    owned("extension_plugin_secrets_dir", Record, Directory, |home| {
        home.extension_plugin_secrets_dir(PLUGIN)
    }),
    owned(
        "client_layout_path",
        Record,
        Document,
        UzeHome::client_layout_path,
    ),
    owned("binary_path", Record, Document, UzeHome::binary_path),
    owned(
        "install_receipt_path",
        Record,
        Document,
        UzeHome::install_receipt_path,
    ),
    owned(
        "machine_usage_path",
        Record,
        Document,
        UzeHome::machine_usage_path,
    ),
    owned(
        "machine_memory_dir",
        Record,
        Directory,
        UzeHome::machine_memory_dir,
    ),
    owned(
        "proposed_memory_dir",
        Record,
        Directory,
        UzeHome::proposed_memory_dir,
    ),
    owned("projects_dir", Record, Directory, UzeHome::projects_dir),
    owned("project_dir", Record, Directory, |home| {
        home.project_dir(PROJECT)
    }),
    owned("project_marker_path", Record, Document, |home| {
        home.project_marker_path(PROJECT)
    }),
    owned("tasks_path", Record, Document, |home| {
        home.tasks_path(PROJECT)
    }),
    owned("approved_commands_path", Record, Document, |home| {
        home.approved_commands_path(PROJECT)
    }),
    owned("conversations_dir", Record, Directory, |home| {
        home.conversations_dir(PROJECT)
    }),
    owned("conversation_path", Record, Document, |home| {
        home.conversation_path(PROJECT, AGENT)
    }),
    owned("review_path", Record, Document, |home| {
        home.review_path(PROJECT, AGENT)
    }),
    owned("prompt_history_path", Record, Document, |home| {
        home.prompt_history_path(PROJECT)
    }),
    owned("project_memory_dir", Record, Directory, |home| {
        home.project_memory_dir(PROJECT)
    }),
    owned(
        "terminal_runtime_dir",
        Record,
        Opaque,
        UzeHome::terminal_runtime_dir,
    ),
    // Carried into `projects/<id>/` by `record::ensure` when a project's
    // directory is first made: only as often as that happens, so unbounded.
    retired("scattered_project_records", Record, Document, |home| {
        home.scattered_project_records(PROJECT).agents
    })
    .spelled(&["prompt-history"]),
    // Locks, beside the records they guard and never under a swept tree.
    owned(
        "mutation_lock_path",
        Record,
        Lock,
        UzeHome::mutation_lock_path,
    ),
    owned("config_lock_path", Record, Lock, UzeHome::config_lock_path),
    owned(
        "launch_overrides_lock_path",
        Record,
        Lock,
        UzeHome::launch_overrides_lock_path,
    ),
    owned(
        "extensions_record_lock_path",
        Record,
        Lock,
        UzeHome::extensions_record_lock_path,
    ),
    owned("extension_store_lock_path", Record, Lock, |home| {
        home.extension_store_lock_path(PLUGIN, EXTENSION, Some(PROJECT))
    }),
    owned("extension_secrets_set_lock_path", Record, Lock, |home| {
        home.extension_secrets_set_lock_path(PLUGIN, EXTENSION)
    }),
    owned(
        "binary_check_lock_path",
        Record,
        Lock,
        UzeHome::binary_check_lock_path,
    ),
    owned(
        "marketplace_fetch_lock_path",
        Record,
        Lock,
        UzeHome::marketplace_fetch_lock_path,
    ),
    owned("marketplace_mirror_lock_path", Record, Lock, |home| {
        home.marketplace_mirror_lock_path("{entry}")
    }),
    owned(
        "machine_usage_lock_path",
        Record,
        Lock,
        UzeHome::machine_usage_lock_path,
    ),
    owned("agents_lock_path", Record, Lock, |home| {
        home.agents_lock_path(PROJECT)
    }),
    owned("delivery_lock_path", Record, Lock, |home| {
        home.delivery_lock_path(PROJECT)
    }),
    owned("agents_md_lock_path", Record, Lock, |home| {
        home.agents_md_lock_path(PROJECT)
    }),
    owned("conversations_lock_path", Record, Lock, |home| {
        home.conversations_lock_path(PROJECT)
    }),
    owned("review_lock_path", Record, Lock, |home| {
        home.review_lock_path(PROJECT)
    }),
    owned("prompt_history_lock_path", Record, Lock, |home| {
        home.prompt_history_lock_path(PROJECT)
    }),
    owned("memory_lock_path", Record, Lock, |home| {
        home.memory_lock_path(&home.machine_memory_dir())
    }),
    owned(
        "agent_reports_lock_path",
        Record,
        Lock,
        UzeHome::agent_reports_lock_path,
    ),
    // Authored.
    owned("config_path", Authored, Document, UzeHome::config_path),
    owned("keymap_path", Authored, Document, UzeHome::keymap_path),
    owned(
        "theme_overrides_path",
        Authored,
        Document,
        UzeHome::theme_overrides_path,
    ),
    owned("themes_dir", Authored, Directory, UzeHome::themes_dir),
    // Generated.
    owned("runtime_dir", Generated, Directory, UzeHome::runtime_dir).swept(),
    owned(
        "runtime_projects_dir",
        Generated,
        Directory,
        UzeHome::runtime_projects_dir,
    ),
    owned("runtime_project_dir", Generated, Directory, |home| {
        home.runtime_project_dir(PROJECT)
    })
    .swept(),
    owned("runtime_projection_dir", Generated, Directory, |home| {
        home.runtime_projection_dir(VENDOR, PROJECT)
    })
    .swept(),
    owned("agent_context_dir", Generated, Directory, |home| {
        home.agent_context_dir(PROJECT, AGENT)
    }),
    // Where earlier releases kept what harnesses read; nothing writes here
    // since #345. Each integration removes what it left once no receipt
    // names it, which happens only when its package is next delivered or
    // detached: unbounded, and still a runtime tenant so no sweep takes a
    // path a receipt names.
    retired(
        "runtime_attachments_dir",
        Generated,
        Directory,
        UzeHome::runtime_attachments_dir,
    ),
    retired("generated_attachments_dir", Generated, Directory, |home| {
        home.generated_attachments_dir(VENDOR)
    }),
    owned(
        "delivered_packages_dir",
        Generated,
        Directory,
        UzeHome::delivered_packages_dir,
    ),
    owned("delivered_package_dir", Generated, Directory, |home| {
        home.delivered_package_dir(&package())
    })
    .swept(),
    owned(
        "runtime_sessions_dir",
        Generated,
        Directory,
        UzeHome::runtime_sessions_dir,
    ),
    owned("session_context_dir", Generated, Directory, |home| {
        home.session_context_dir(VENDOR)
    })
    .swept(),
    owned(
        "runtime_rules_dir",
        Generated,
        Directory,
        UzeHome::runtime_rules_dir,
    ),
    owned(
        "rules_sessions_dir",
        Generated,
        Directory,
        UzeHome::rules_sessions_dir,
    )
    .swept(),
    owned(
        "compiled_extensions_dir",
        Generated,
        Directory,
        UzeHome::compiled_extensions_dir,
    ),
    owned("compiled_extension_path", Generated, Document, |home| {
        home.compiled_extension_path("{digest}")
    })
    .swept(),
    owned(
        "extension_scratch_root",
        Generated,
        Directory,
        UzeHome::extension_scratch_root,
    ),
    owned("extension_scratch_dir", Generated, Directory, |home| {
        home.extension_scratch_dir("{run}")
    })
    .swept(),
    owned(
        "extension_pictures_root",
        Generated,
        Directory,
        UzeHome::extension_pictures_root,
    ),
    owned("extension_pictures_dir", Generated, Directory, |home| {
        home.extension_pictures_dir(PLUGIN, EXTENSION)
    })
    .swept(),
    owned(
        "extension_costs_path",
        Generated,
        Document,
        UzeHome::extension_costs_path,
    ),
    owned(
        "runtime_agents_dir",
        Generated,
        Directory,
        UzeHome::runtime_agents_dir,
    ),
    owned("agent_runtime_dir", Generated, Directory, |home| {
        home.agent_runtime_dir(AGENT)
    })
    .swept(),
    owned("agent_reports_path", Generated, Document, |home| {
        home.agent_reports_path(AGENT)
    }),
    owned("shims_dir", Generated, Directory, UzeHome::shims_dir),
    owned("shim_path", Generated, Document, |home| {
        home.shim_path("{harness}")
    }),
    // Remembered.
    owned("cache_dir", Remembered, Directory, UzeHome::cache_dir),
    owned(
        "harnesses_cache_path",
        Remembered,
        Document,
        UzeHome::harnesses_cache_path,
    ),
    owned(
        "harness_detection_cache_path",
        Remembered,
        Document,
        UzeHome::harness_detection_cache_path,
    ),
    owned(
        "requirement_probe_cache_path",
        Remembered,
        Document,
        UzeHome::requirement_probe_cache_path,
    ),
    owned(
        "shell_observation_cache_path",
        Remembered,
        Document,
        UzeHome::shell_observation_cache_path,
    ),
    owned(
        "machine_overview_cache_path",
        Remembered,
        Document,
        UzeHome::machine_overview_cache_path,
    ),
    owned(
        "machine_usage_cache_path",
        Remembered,
        Document,
        UzeHome::machine_usage_cache_path,
    ),
    owned(
        "inspection_cache_path",
        Remembered,
        Document,
        UzeHome::inspection_cache_path,
    ),
    owned(
        "appearance_cache_path",
        Remembered,
        Document,
        UzeHome::appearance_cache_path,
    ),
    owned(
        "release_notes_cache_path",
        Remembered,
        Document,
        UzeHome::release_notes_cache_path,
    ),
    owned(
        "marketplace_cache_dir",
        Remembered,
        Directory,
        UzeHome::marketplace_cache_dir,
    )
    .swept(),
    owned(
        "collection_record_path",
        Remembered,
        Document,
        UzeHome::collection_record_path,
    ),
    owned("logs_dir", Remembered, Directory, UzeHome::logs_dir).swept(),
    owned("gate_log_path", Remembered, Document, |home| {
        home.gate_log_path(AGENT)
    }),
];

impl UzeHome {
    /// Every path this home's builders compose, each with its tier, form
    /// and standing: the map of what UZE owns, as one list.
    pub fn owned_paths() -> &'static [OwnedPath] {
        OWNED
    }

    /// The owned path a builder composes, by the builder's name.
    pub fn owned_path(name: &str) -> Option<&'static OwnedPath> {
        OWNED.iter().find(|owned| owned.name == name)
    }

    /// Every retired path, as the entries the release floor holds.
    pub fn earlier() -> Vec<Earlier> {
        OWNED
            .iter()
            .filter(|owned| owned.standing == Standing::Retired)
            .map(|owned| Earlier {
                name: owned.name,
                lasts: owned.lasts,
                spelled: owned.spelled,
                site: file!(),
            })
            .collect()
    }

    /// Every path UZE sweeps: each has exactly one collector.
    pub fn disposable_paths() -> impl Iterator<Item = &'static OwnedPath> {
        OWNED.iter().filter(|owned| owned.disposable)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_owned_path_is_named_once() {
        let mut seen = BTreeSet::new();
        for owned in UzeHome::owned_paths() {
            assert!(seen.insert(owned.name), "{} is listed twice", owned.name);
        }
    }

    /// The tier an entry claims is the tier its place says: a path in a
    /// tier that claims a different cost than it has is how
    /// `state/attachments/` once sat one letter from the ledger.
    #[test]
    fn every_current_path_sits_in_the_tier_it_claims() {
        let home = UzeHome::at("/uze-home");
        for owned in UzeHome::owned_paths()
            .iter()
            .filter(|owned| owned.standing == Standing::Current)
        {
            assert_eq!(
                Tier::of(&home, &owned.path(&home)),
                Some(owned.tier),
                "{} at {}",
                owned.name,
                owned.path(&home).display()
            );
        }
    }

    /// Only what costs nothing to delete is swept: a generated or
    /// remembered path, or a retired one. A record, bytes, a build or the
    /// operator's text is never a collector's to judge.
    #[test]
    fn only_what_costs_nothing_is_disposable() {
        for owned in UzeHome::disposable_paths() {
            assert!(
                matches!(owned.tier, Tier::Generated | Tier::Remembered)
                    || owned.standing == Standing::Retired,
                "{} is disposable in the {:?} tier",
                owned.name,
                owned.tier
            );
            assert_ne!(owned.form, Form::Opaque, "{}", owned.name);
        }
    }

    #[test]
    fn a_placeholder_stands_for_any_text_in_one_segment() {
        let home = UzeHome::at("/uze-home");
        let conversation = UzeHome::owned_path("conversation_path").unwrap();
        let path = |text: &str| text.split('/').map(str::to_owned).collect::<Vec<_>>();
        assert!(conversation.matches(&home, &path("state/projects/abc/conversations/a1.json")));
        assert!(!conversation.matches(&home, &path("state/projects/abc/conversations/a1.toml")));
        assert!(conversation.lies_under(&home, &path("state/projects/abc")));
        let plugin = UzeHome::owned_path("plugin_dir").unwrap();
        assert!(plugin.matches(&home, &path("sources/plugins/ai/flow")));
        let superseded = UzeHome::owned_path("superseded_dir").unwrap();
        assert!(superseded.matches(&home, &path("sources/superseded/flow@ai")));
        assert!(segment_matches("gate-{agent}.log", "gate-a.log"));
        assert!(!segment_matches("gate-{agent}.log", "gate.log"));
    }

    #[test]
    fn every_tier_holds_a_path() {
        for tier in Tier::ALL {
            assert!(
                UzeHome::owned_paths()
                    .iter()
                    .any(|owned| owned.tier == tier),
                "{tier:?}"
            );
        }
    }
}
