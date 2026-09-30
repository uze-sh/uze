//! `agents.yaml` — what a project declares about its agent environment.
//!
//! This is the authored half of the pair. Everything a person decides
//! lives here: which marketplaces the project draws from, which plugins it
//! wants, and how isolated work is delivered. [`project_lock`] holds the
//! other half — what resolving these declarations produced — and carries
//! no intent, so deleting it loses nothing.
//!
//! Reading is serde and rejects unknown fields: a typo in a file a person
//! wrote must be an error, never silence. Writing goes through
//! [`edit`], which patches the document in place so comments and
//! formatting survive.
//!
//! One file, sections by owner. This module owns the file, its scaffold and
//! the list of keys its root may hold, and reads the package manager's
//! section (`marketplaces`). The workspace's sections ([`Section`]) are
//! carried unread and parsed by the workspace, so a mistake in one is
//! reported by the part of UZE that uses it and never stops a package
//! command that does not.
//!
//! [`project_lock`]: crate::project_lock

pub mod edit;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use noyalib::{DuplicateKeyPolicy, ParserConfig, compat::serde_yaml, from_str_with_config};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, UzeError};

pub const MANIFEST_FILE_NAME: &str = "agents.yaml";

/// The marketplace UZE carries inside its own binary. A project never
/// declares it: it is part of how UZE works rather than something the
/// project chose, so a plugin may name it with no `marketplaces:` entry
/// behind it — and an entry claiming the name is refused, since the only
/// thing it could express is shadowing the built-in one.
pub const BUILT_IN_MARKETPLACE: &str = "uze-official";

/// The manifest as declared. Every field is optional: a project that
/// declares only an isolation policy is as valid as one that declares only
/// plugins.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktrees: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub marketplaces: BTreeMap<String, DeclaredMarketplace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifacts: Option<serde_yaml::Value>,
}

/// A root section of `agents.yaml` this module carries but does not read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    /// How the workspace isolates agents and delivers their work.
    Worktrees,
    /// Where the project keeps its architecture artifacts.
    Artifacts,
}

impl Section {
    pub const ALL: [Section; 2] = [Section::Worktrees, Section::Artifacts];

    pub fn key(self) -> &'static str {
        match self {
            Section::Worktrees => "worktrees",
            Section::Artifacts => "artifacts",
        }
    }
}

impl ProjectManifest {
    /// Whether the project wrote the section at all, a question that needs no
    /// parsing and that a reader of only the package manager's section may
    /// ask.
    pub fn declares(&self, section: Section) -> bool {
        self.raw(section).is_some()
    }

    /// The section as its owner's type, or `None` when it is not declared. A
    /// section that does not fit the type is a malformed manifest, reported
    /// with the section's key.
    pub fn section<T: DeserializeOwned + 'static>(
        &self,
        section: Section,
        path: &Path,
    ) -> Result<Option<T>> {
        let Some(value) = self.raw(section) else {
            return Ok(None);
        };
        serde_yaml::from_value(value.clone())
            .map(Some)
            .map_err(|error| UzeError::MalformedManifest {
                path: path.to_path_buf(),
                reason: explain(&format!("{}: {error}", section.key())),
            })
    }

    fn raw(&self, section: Section) -> Option<&serde_yaml::Value> {
        match section {
            Section::Worktrees => self.worktrees.as_ref(),
            Section::Artifacts => self.artifacts.as_ref(),
        }
    }

    /// Every plugin the project declares, with the marketplace it comes
    /// from — the `plugin@marketplace` pair ADR-036 makes the identity,
    /// which this file spells structurally rather than by repeating the
    /// marketplace on each entry.
    pub fn declared_plugins(&self) -> impl Iterator<Item = (&str, &str)> {
        self.marketplaces
            .iter()
            .flat_map(|(marketplace, declared)| {
                declared
                    .plugins
                    .iter()
                    .map(move |plugin| (plugin.as_str(), marketplace.as_str()))
            })
    }

    /// The marketplace a plugin was declared under, if any.
    pub fn marketplace_of(&self, plugin: &str) -> Option<&str> {
        self.declared_plugins()
            .find(|(declared, _)| *declared == plugin)
            .map(|(_, marketplace)| marketplace)
    }
}

/// Where a marketplace comes from. The name a project refers to it by is
/// UX; this is its identity.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredMarketplace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdirectory: Option<PathBuf>,
    /// What the project installs from it. Bare names: the source above is
    /// what says where they come from, and the `ref:` above is what moves
    /// them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<String>,
}

pub fn manifest_path_for(root: &Path) -> PathBuf {
    root.join(MANIFEST_FILE_NAME)
}

/// Reads the project's manifest, or `None` when the project has never
/// declared one. A project with no manifest behaves as one declaring
/// nothing — never as an error, and never as a prompt to create it.
pub fn load(root: &Path) -> Result<Option<ProjectManifest>> {
    let path = manifest_path_for(root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|source| UzeError::Read {
        path: path.clone(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| UzeError::MalformedManifest {
        path: path.clone(),
        reason: format!("{MANIFEST_FILE_NAME} is not valid UTF-8"),
    })?;
    parse(&text, &path).map(Some)
}

/// One of the sections this module carries unread, parsed as its owner's
/// type; `None` when there is no manifest or it does not declare the
/// section.
pub fn load_section<T: DeserializeOwned + 'static>(
    root: &Path,
    section: Section,
) -> Result<Option<T>> {
    match load(root)? {
        Some(manifest) => manifest.section(section, &manifest_path_for(root)),
        None => Ok(None),
    }
}

/// The file UZE writes when it creates the manifest itself: every key the
/// schema understands, with the default already in force spelled out beside
/// it, so the vocabulary is discoverable by opening the file rather than by
/// reading documentation.
///
/// Only `completion` is live. Everything else is commented, because a value
/// written here would be a decision UZE made on the project's behalf —
/// uncommenting a line is what changes behavior, never the file appearing.
pub const SCAFFOLD: &str = r"# This project's agent environment. UZE reads this file and writes
# agents.lock from it — edit this one; the lock regenerates.
#
# Every key UZE understands is below. A commented line carries the default
# already in force: uncomment it to make the choice the project's own.

worktrees:
  # handoff | merge | pr — what UZE does with an agent's finished branch.
  # `handoff` leaves it for you to integrate.
  completion: handoff

  # in-place | isolated — where an agent launched here starts. In place,
  # it shares the project's own checkout and is isolated when somebody
  # asks; isolated, every agent gets a checkout of its own at launch.
  # default: in-place

  # The branch finished work targets. Undeclared, it is the branch the
  # primary checkout is on when the task is created.
  # target: main

  # conventional | gitflow | flat | agent — or this project's own list of
  # types, e.g. [feat, fix, docs, ui]. What an agent's own name for its
  # work is judged against: it proposes `<type>/<subject>`, UZE accepts it
  # only if the type is here. Undeclared (`agent`), work is not named and
  # the branch stays the generated identifier.
  # branch: conventional

  # Ignored files a fresh checkout links from the primary one. Relative,
  # inside the repository, and ignored by it — a symlink the agent writes
  # through reaches the primary.
  # link: [.env, .env.local]

  # Prepares a fresh checkout, run in it after linking. Its failure warns
  # and never blocks a launch.
  # setup: pnpm install

  # Run in the checkout on the rebased commits; the first non-zero exit
  # refuses delivery and is named. One command, or a list run in order —
  # a list is what makes a failure say which step failed.
  # gate:
  #   - pnpm test
  #   - pnpm lint

  # The most checkouts that may exist at once. Undeclared, peak
  # concurrency is the only bound.
  # slots: 3

  # Free checkouts kept warm for the next agents, the most recently used
  # first; every other free one is removed, its branch kept. And the days
  # a free checkout may sit unused before it is removed too.
  # spare: 2
  # idle_days: 3

# The marketplaces this project draws from, and what it takes from each.
# Exactly one source per marketplace — `git:` or `path:` — and a Git one may
# be narrowed by `ref:` and `subdirectory:`. The `ref:` is also the pin: it
# is what moves the plugins listed under it.
#
# UZE's own marketplace is not here. It is built into the binary, its
# plugins are installed for every project, and neither is yours to declare
# — `uze market` shows both, this file carries only what the project chose.
#
# `uze <plugin>@<market>` writes these entries and records what they
# resolved to in agents.lock; `uze install` reproduces that lock on
# another machine.
# marketplaces:
#   ours:
#     git: https://github.com/acme/plugins
#     ref: main
#     subdirectory: market
#     plugins:
#       - review
#       - changelog
#   local:
#     path: ../marketplace
#     plugins:
#       - bench-runner

# Where this project keeps the artifacts that describe it: Mermaid files
# (`.mmd`), which the workspace's architect surface draws. A directory
# inside the project, read as deep as it goes. Nothing lists the files —
# each says what it is in its own first word (`C4Context`,
# `sequenceDiagram`, `flowchart`), and may name itself with a `title:`.
# artifacts:
#   path: docs/architecture

";

/// Creates the manifest with the built-in defaults written out, when the
/// project has none. Idempotent: an existing manifest is left exactly as it
/// is, comments and all.
///
/// Called only from an explicit act of setting a project up. Nothing that
/// merely *reads* a project — opening the client, inspecting, planning —
/// may call this: a repository somebody is only trying UZE against must
/// come back unchanged.
pub fn ensure_exists(root: &Path) -> Result<bool> {
    let (document, created) = open_or_scaffold(root)?;
    if created {
        document.save()?;
    }
    Ok(created)
}

/// The project's manifest document, or the scaffold that would become it
/// when the project has none — and whether it is the scaffold.
fn open_or_scaffold(root: &Path) -> Result<(edit::ManifestDocument, bool)> {
    let path = manifest_path_for(root);
    if path.exists() {
        return Ok((edit::ManifestDocument::open(&path)?, false));
    }
    let mut document = edit::ManifestDocument::empty(&path)?;
    document.append_block(SCAFFOLD)?;
    Ok((document, true))
}

/// Writes one scalar into one of the sections this module carries, creating
/// the manifest when the project has none, and reports whether it had to. The
/// owner of the section decides what the value means and re-reads it; this
/// only patches the document in place, so the comments around it survive.
pub fn set_scalar(root: &Path, section: Section, key: &str, value: &str) -> Result<bool> {
    let (mut document, created) = open_or_scaffold(root)?;
    document.upsert(
        section.key(),
        key,
        &serde_yaml::Value::String(value.to_owned()),
    )?;
    document.save()?;
    load(root)?;
    Ok(created)
}

/// Declares a plugin under the marketplace it comes from, declared as
/// `source`, creating the manifest when the project has none. The document
/// is patched in place, so a comment a person wrote beside an unrelated
/// entry survives.
pub fn declare_plugin(
    root: &Path,
    plugin: &str,
    marketplace: &str,
    source: &DeclaredMarketplace,
) -> Result<()> {
    let (mut document, _) = open_or_scaffold(root)?;
    let already_declared =
        load(root)?.is_some_and(|manifest| manifest.marketplaces.contains_key(marketplace));
    if already_declared {
        // Pushing into the list rather than rewriting the entry: the source
        // above it is the author's, comments and all.
        document.push_unique(&format!("marketplaces.{marketplace}.plugins"), plugin)?;
    } else {
        let mut declared = source.clone();
        declared.plugins = vec![plugin.to_owned()];
        document.upsert("marketplaces", marketplace, &declared.declaration()?)?;
    }
    document.save()?;
    // Re-read through the typed path: a write that produces a manifest the
    // schema rejects is a bug in this function, and must surface here rather
    // than on the next command.
    load(root)?;
    Ok(())
}

/// Removes a plugin's declaration. Reports whether it was there, so a
/// caller can tell "removed" from "never declared". The marketplace stays:
/// it may be feeding another plugin, and a source declared with nothing
/// taken from it is inert rather than wrong.
pub fn undeclare_plugin(root: &Path, plugin: &str) -> Result<bool> {
    let path = manifest_path_for(root);
    if !path.exists() {
        return Ok(false);
    }
    let Some(marketplace) = load(root)?
        .as_ref()
        .and_then(|manifest| manifest.marketplace_of(plugin))
        .map(str::to_owned)
    else {
        return Ok(false);
    };
    let mut document = edit::ManifestDocument::open(&path)?;
    document.remove_from(&format!("marketplaces.{marketplace}.plugins"), plugin)?;
    document.save()?;
    Ok(true)
}

impl DeclaredMarketplace {
    /// The value UZE writes for this declaration. Typed, never a fragment of
    /// YAML text: the spelling — indentation, quoting, whether a path
    /// holding a comma needs quotes — belongs to the emitter.
    fn declaration(&self) -> Result<serde_yaml::Value> {
        serde_yaml::to_value(self).map_err(|error| UzeError::MalformedManifest {
            path: PathBuf::from(MANIFEST_FILE_NAME),
            reason: format!("a marketplace declaration could not be written ({error})"),
        })
    }
}

pub fn parse(text: &str, path: &Path) -> Result<ProjectManifest> {
    // A file that is empty, or holds only comments, declares nothing —
    // which is a valid manifest, not a type error about a missing mapping.
    if text
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    {
        return Ok(ProjectManifest::default());
    }
    // A key written twice is a mistake, not a precedence question. YAML's
    // own answer — and the loader's default — is to keep the last one, so
    // declaring `flow` twice would silently drop one declaration in a file
    // whose whole contract is that a typo is an error rather than silence.
    let config = ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    let document: serde_yaml::Value =
        from_str_with_config(text, &config).map_err(|error| UzeError::MalformedManifest {
            path: path.to_path_buf(),
            reason: explain(&error.to_string()),
        })?;
    // Asked of the document before the schema sees it, the way the lock
    // refuses the keys that moved here: a key whose answer is about the
    // *file* cannot be told apart from a same-named key inside an entry
    // once serde has reduced both to "unknown field".
    for (key, why) in REFUSED_ROOT_KEYS {
        if document
            .as_mapping()
            .is_some_and(|mapping| mapping.contains_key(key))
        {
            return Err(UzeError::MalformedManifest {
                path: path.to_path_buf(),
                reason: format!("`{key}` is not a key of agents.yaml: {why}"),
            });
        }
    }
    let manifest: ProjectManifest =
        serde_yaml::from_value(document).map_err(|error| UzeError::MalformedManifest {
            path: path.to_path_buf(),
            reason: explain(&error.to_string()),
        })?;
    validate(&manifest, path)?;
    Ok(manifest)
}

/// Keys that are refused at the root of the manifest, with the reason a
/// reader needs. Each is a key somebody reasonably expects to exist; the
/// refusal explains why it does not, rather than reporting it as a typo.
const REFUSED_ROOT_KEYS: [(&str, &str); 1] = [(
    "version",
    "this file carries no schema version. A change to the schema is reported by naming the key it \
     affects, and a number to compare would only be needed to keep reading a file UZE no longer \
     understands",
)];

/// serde's "unknown field" message is accurate and unhelpful for the two
/// mistakes a person actually makes: writing a resolution into the
/// manifest, or writing the policy into the lock.
fn explain(reason: &str) -> String {
    // A plugin version is resolved nowhere — not here and not in the lock —
    // so the answer is what can be pinned instead, never a pointer to a
    // field that does not exist.
    if reason.contains("unknown field") && reason.contains("version") {
        return "UZE does not resolve plugin versions yet — nothing in a plugin or a marketplace \
                catalog declares one. Pin the marketplace with its `ref:` instead."
            .to_owned();
    }
    for resolved in ["revision", "integrity"] {
        // Matched loosely on purpose: the wording of an unknown-field error
        // belongs to the YAML library, and this message must survive it
        // changing.
        if reason.contains("unknown field") && reason.contains(resolved) {
            return format!(
                "`{resolved}` is written by resolution, not declared: it belongs to agents.lock, \
                 which UZE regenerates. Remove it here."
            );
        }
    }
    reason.to_owned()
}

fn validate(manifest: &ProjectManifest, path: &Path) -> Result<()> {
    let malformed = |reason: String| UzeError::MalformedManifest {
        path: path.to_path_buf(),
        reason,
    };
    for (name, marketplace) in &manifest.marketplaces {
        if name == BUILT_IN_MARKETPLACE {
            return Err(malformed(format!(
                "`{name}` is the marketplace built into UZE; it is always available and cannot \
                 be declared. Remove the entry — the plugins that come from it stay."
            )));
        }
        let sources =
            usize::from(marketplace.git.is_some()) + usize::from(marketplace.path.is_some());
        if sources == 0 {
            return Err(malformed(format!(
                "marketplace `{name}` declares no source; give it a `git:` or a `path:`"
            )));
        }
        if sources > 1 {
            return Err(malformed(format!(
                "marketplace `{name}` declares more than one source; a marketplace comes from one \
                 place"
            )));
        }
    }
    // A plugin is a bare name under the marketplace that carries it, so
    // the questions left are about the name itself: that it is one, and
    // that only one entry claims it.
    let mut claimed: BTreeMap<&str, &str> = BTreeMap::new();
    for (marketplace, declared) in &manifest.marketplaces {
        for plugin in &declared.plugins {
            if plugin.trim().is_empty() {
                return Err(malformed(format!(
                    "marketplace `{marketplace}` declares a plugin with no name"
                )));
            }
            if let Some(first) = claimed.insert(plugin, marketplace) {
                return Err(malformed(format!(
                    "plugin `{plugin}` is declared under `{first}` and under `{marketplace}`. \
                     They are two different plugins, and this file has no way to say which one \
                     answers to `{plugin}` locally — take it from one of them."
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Result<ProjectManifest> {
        parse(text, Path::new("/p/agents.yaml"))
    }

    #[test]
    fn an_empty_manifest_declares_nothing() {
        assert_eq!(parsed("").unwrap(), ProjectManifest::default());
    }

    #[test]
    fn a_plugin_is_a_name_under_the_marketplace_that_carries_it() {
        let manifest = parsed(
            "marketplaces:\n  ai:\n    git: https://example.invalid/ai\n    plugins:\n      - \
             flow\n      - review\n",
        )
        .unwrap();
        assert_eq!(
            manifest.declared_plugins().collect::<Vec<_>>(),
            vec![("flow", "ai"), ("review", "ai")]
        );
        assert_eq!(manifest.marketplace_of("review"), Some("ai"));
        assert_eq!(manifest.marketplace_of("never-declared"), None);
    }

    /// ADR-036 makes `plugin@marketplace` the identity, so two of them can
    /// coexist in the Store under different local names — but this file has
    /// no way to spell which one answers to the bare name, so declaring
    /// both is refused here rather than at install.
    #[test]
    fn a_plugin_declared_under_two_marketplaces_is_refused() {
        let error = parsed(
            "marketplaces:\n  one:\n    git: https://example.invalid/one\n    plugins: [git]\n  \
             two:\n    git: https://example.invalid/two\n    plugins: [git]\n",
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("`one`") && message.contains("`two`"),
            "{message}"
        );
    }

    #[test]
    fn a_plugin_with_no_name_is_refused() {
        let error = parsed(
            "marketplaces:\n  ai:\n    git: https://example.invalid/ai\n    plugins: [\"\"]\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("no name"), "{error}");
    }

    #[test]
    fn declaring_the_built_in_marketplace_is_refused() {
        let error = parsed(&format!(
            "marketplaces:\n  {BUILT_IN_MARKETPLACE}:\n    git: https://example.invalid/fake\n"
        ))
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains(BUILT_IN_MARKETPLACE), "{message}");
        assert!(message.contains("built into UZE"), "{message}");
    }

    #[test]
    fn a_marketplace_may_declare_a_source_and_take_nothing_from_it() {
        let manifest =
            parsed("marketplaces:\n  ai:\n    git: https://example.invalid/ai\n").unwrap();
        assert!(manifest.marketplaces.contains_key("ai"));
        assert_eq!(manifest.declared_plugins().count(), 0);
    }

    #[test]
    fn a_marketplace_with_two_sources_is_rejected() {
        let error = parsed("marketplaces:\n  ai:\n    git: https://a.invalid\n    path: ../a\n")
            .unwrap_err();
        assert!(
            error.to_string().contains("more than one source"),
            "{error}"
        );
    }

    #[test]
    fn a_marketplace_with_no_source_is_rejected() {
        let error = parsed("marketplaces:\n  ai:\n    ref: main\n").unwrap_err();
        assert!(error.to_string().contains("declares no source"), "{error}");
    }

    #[test]
    fn a_key_declared_twice_is_an_error_rather_than_the_last_one_winning() {
        let error = parsed(
            "marketplaces:\n  ai:\n    git: https://example.invalid/ai\nplugins:\n  flow: ai\n  \
             flow: ai\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("flow"), "{error}");
    }

    #[test]
    fn a_pin_belongs_to_the_marketplace_that_carries_the_plugins() {
        let manifest = parsed(
            "marketplaces:\n  ai:\n    git: https://example.invalid/ai\n    ref: v0.3.1\n    \
             plugins: [flow]\n",
        )
        .unwrap();
        assert_eq!(manifest.marketplaces["ai"].r#ref.as_deref(), Some("v0.3.1"));
        assert_eq!(manifest.marketplace_of("flow"), Some("ai"));
    }

    #[test]
    fn a_plugin_entry_carrying_fields_of_its_own_is_refused() {
        let error = parsed(
            "marketplaces:\n  ai:\n    git: https://example.invalid/ai\n    plugins:\n      - \
             flow:\n          ref: v1\n",
        )
        .unwrap_err();
        assert!(
            !error.to_string().is_empty(),
            "a mapping item must not load"
        );
    }

    #[test]
    fn a_version_is_answered_with_what_uze_can_actually_pin() {
        let error =
            parsed("marketplaces:\n  ai:\n    git: https://a.invalid\n    version: \"^0.3\"\n")
                .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("does not resolve plugin versions"),
            "{message}"
        );
        assert!(message.contains("`ref:`"), "{message}");
        assert!(
            !message.contains("agents.lock"),
            "the lock resolves no version either; do not send anyone there: {message}"
        );
    }

    #[test]
    fn a_resolution_field_is_rejected_by_name_and_points_at_the_lock() {
        let error =
            parsed("marketplaces:\n  ai:\n    git: https://a.invalid\n    revision: deadbeef\n")
                .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("agents.lock"), "{message}");
        assert!(message.contains("revision"), "{message}");
    }

    #[test]
    fn a_schema_version_is_refused_and_says_why_there_is_none() {
        let error = parsed("version: 1\nworktrees:\n  completion: pr\n").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("no schema version"), "{message}");
        assert!(
            !message.contains("plugin versions"),
            "the plugin answer must not be given to the schema question: {message}"
        );
    }

    #[test]
    fn ensure_exists_never_touches_a_manifest_somebody_wrote() {
        let root = uze_testkit::temp::scratch("manifest-ensure-existing");
        let authored = "# mine\nworktrees:\n  completion: pr   # deliberate\n";
        fs::write(manifest_path_for(&root), authored).unwrap();
        assert!(!ensure_exists(&root).unwrap());
        assert_eq!(
            fs::read_to_string(manifest_path_for(&root)).unwrap(),
            authored
        );
    }

    #[test]
    fn declaring_a_plugin_creates_the_manifest_when_the_project_has_none() {
        let root = uze_testkit::temp::scratch("manifest-declare");
        declare_plugin(
            &root,
            "flow",
            "ai",
            &DeclaredMarketplace {
                git: Some("https://example.invalid/ai".to_owned()),
                path: None,
                r#ref: None,
                subdirectory: None,
                plugins: Vec::new(),
            },
        )
        .unwrap();

        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(
            written.starts_with("# This project's agent environment."),
            "{written}"
        );
        let manifest = load(&root).unwrap().unwrap();
        assert_eq!(manifest.marketplace_of("flow"), Some("ai"));
        assert_eq!(
            manifest.marketplaces["ai"].git.as_deref(),
            Some("https://example.invalid/ai")
        );
    }

    #[test]
    fn declaring_a_second_plugin_keeps_the_first_and_its_comments() {
        let root = uze_testkit::temp::scratch("manifest-declare-second");
        let source = DeclaredMarketplace {
            git: Some("https://example.invalid/ai".to_owned()),
            path: None,
            r#ref: None,
            subdirectory: None,
            plugins: Vec::new(),
        };
        declare_plugin(&root, "flow", "ai", &source).unwrap();

        let path = manifest_path_for(&root);
        let annotated = fs::read_to_string(&path)
            .unwrap()
            .replace("      - flow", "      # pinned deliberately\n      - flow");
        fs::write(&path, &annotated).unwrap();

        declare_plugin(&root, "git", "ai", &source).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("# pinned deliberately"), "{written}");
        let manifest = load(&root).unwrap().unwrap();
        assert_eq!(manifest.declared_plugins().count(), 2);
        assert_eq!(
            manifest.marketplaces["ai"].git.as_deref(),
            Some("https://example.invalid/ai"),
            "the source the author wrote is untouched by a second push"
        );
    }

    #[test]
    fn undeclaring_reports_whether_it_was_there_and_empties_cleanly() {
        let root = uze_testkit::temp::scratch("manifest-undeclare");
        declare_plugin(
            &root,
            "flow",
            "ai",
            &DeclaredMarketplace {
                git: Some("https://example.invalid/ai".to_owned()),
                path: None,
                r#ref: None,
                subdirectory: None,
                plugins: Vec::new(),
            },
        )
        .unwrap();

        assert!(!undeclare_plugin(&root, "never-declared").unwrap());
        assert!(undeclare_plugin(&root, "flow").unwrap());

        let written = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert!(!written.contains("{}"), "left an empty mapping: {written}");
        assert_eq!(load(&root).unwrap().unwrap().declared_plugins().count(), 0);
    }

    /// serde quotes for block context; `inline` writes into a flow
    /// mapping. A path or a ref carrying a comma or a brace used to be
    /// spliced in bare, and the re-read at the end of `declare_plugin`
    /// then refused the file UZE had just written.
    #[test]
    fn a_value_carrying_a_flow_indicator_stays_one_entry() {
        let root = uze_testkit::temp::scratch("manifest-flow-quoting");
        declare_plugin(
            &root,
            "flow",
            "odd",
            &DeclaredMarketplace {
                git: None,
                path: Some(PathBuf::from("../my, dir")),
                r#ref: Some("release/{next}".to_owned()),
                subdirectory: None,
                plugins: Vec::new(),
            },
        )
        .unwrap();

        let manifest = load(&root).unwrap().unwrap();
        let declared = &manifest.marketplaces["odd"];
        assert_eq!(declared.path.as_deref(), Some(Path::new("../my, dir")));
        assert_eq!(declared.r#ref.as_deref(), Some("release/{next}"));
    }

    #[test]
    fn a_value_that_would_change_meaning_unquoted_is_written_quoted() {
        let root = uze_testkit::temp::scratch("manifest-quoting");
        declare_plugin(
            &root,
            "flow",
            "no",
            &DeclaredMarketplace {
                git: None,
                path: Some(PathBuf::from("1.10")),
                r#ref: Some("2.0".to_owned()),
                subdirectory: None,
                plugins: Vec::new(),
            },
        )
        .unwrap();
        let manifest = load(&root).unwrap().unwrap();
        let declared = &manifest.marketplaces["no"];
        assert_eq!(declared.path.as_deref(), Some(Path::new("1.10")));
        assert_eq!(declared.r#ref.as_deref(), Some("2.0"));
    }

    #[test]
    fn ensure_exists_creates_the_scaffold_once() {
        let root = uze_testkit::temp::scratch("manifest-ensure");
        assert!(ensure_exists(&root).unwrap(), "the first call creates it");
        let first = fs::read_to_string(manifest_path_for(&root)).unwrap();
        assert_eq!(first, SCAFFOLD);
        assert!(
            !ensure_exists(&root).unwrap(),
            "the second call changes nothing"
        );
        assert_eq!(fs::read_to_string(manifest_path_for(&root)).unwrap(), first);
    }

    /// The scaffold is documentation that must not drift from the schema.
    /// Each struct literal is exhaustive, so a field added to the package
    /// manager's section has to be named here, and this then asks the
    /// created file to offer it. The workspace asks the same of its own
    /// sections.
    #[test]
    fn a_created_manifest_offers_every_key_this_module_reads() {
        let marketplace = DeclaredMarketplace {
            git: Some("https://example.invalid/ai".to_owned()),
            path: Some(PathBuf::from("../ai")),
            r#ref: Some("main".to_owned()),
            subdirectory: Some(PathBuf::from("plugins")),
            plugins: vec!["flow".to_owned()],
        };
        let keys: Vec<String> = serde_yaml::to_string(&marketplace)
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with(char::is_whitespace))
            .filter_map(|line| line.split_once(':').map(|(key, _)| key.to_owned()))
            .chain(["marketplaces".to_owned()])
            .chain(Section::ALL.iter().map(|section| section.key().to_owned()))
            .collect();
        assert!(keys.len() > 5, "the shapes emitted nothing to check");
        for key in keys {
            assert!(
                SCAFFOLD.contains(&format!("{key}:")),
                "the created manifest never mentions `{key}`:\n{SCAFFOLD}"
            );
        }
    }

    #[test]
    fn a_section_this_module_does_not_read_is_carried_whatever_it_holds() {
        let manifest = parsed("worktrees:\n  completon: pr\n  slots: 0\n").unwrap();
        assert!(manifest.declares(Section::Worktrees));
        assert!(!manifest.declares(Section::Artifacts));
    }

    #[test]
    fn a_misspelled_root_key_is_still_refused() {
        let error = parsed("worktree:\n  completion: pr\n").unwrap_err();
        assert!(error.to_string().contains("worktree"), "{error}");
    }
}
