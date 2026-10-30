//! The record of launch overrides: what each agent was launched with, per
//! harness, the entries the operator used lately, per harness, and the
//! selection the last new agent was launched with, per harness, which the
//! next one opens on.
//!
//! One document for the machine rather than one per project: most
//! overrides are the operator's habits (`--model`, `--verbose`), and a
//! history per project would have them typed again in every repository.
//! The few that belong to a project are handled by order instead: each
//! entry remembers the project it was last used in, and that project lists
//! its own first. The history keeps entries, not whole launches, so one
//! launch's five flags can be taken back three at a time. Owner-only, written under its lock, and read without it by
//! a launch, which an atomic write never shows half-written.
//!
//! What is typed is kept as the words it split into, and read as entries
//! again at each launch against the catalog of that day: an override
//! recorded before UZE came to own its flag is left out then, not obeyed.
//!
//! The last selection was added without a new shape: a build from before
//! it reads the record and, rewriting it, drops the selection, which costs
//! one popup opening on nothing checked.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use uze_core::{flag_catalog::FlagCatalog, path::Canonical as _};

use crate::{
    Result, harness_runtime::project_id_for, home::UzeHome, persistence::write_atomic_private,
    task::AgentId,
};

/// Entries kept per harness, across every project.
const RECENT_PER_HARNESS: usize = 20;
/// How long a write waits for another one, each a small rewrite.
const LOCK_PATIENCE: Duration = Duration::from_secs(2);
/// A variable whose name, upper-cased, holds one of these anywhere reads as
/// a secret, and is not remembered in the history: `GH_TOKEN`,
/// `OPENAI_APIKEY`, `DB_PASSWD`, `DB_PWD`, `SENTRY_DSN`. Remembering one
/// too few is the cost to avoid; `AUTHOR` withheld too is the price.
const SECRET_FRAGMENTS: &[&str] = &[
    "TOKEN", "SECRET", "PASS", "PWD", "CRED", "KEY", "AUTH", "DSN",
];
/// Too short to look for anywhere, so read as a secret only as a whole
/// `_`-separated part: `GH_PAT`, not `PATH`.
const SECRET_PARTS: &[&str] = &["PAT"];

/// One override used before, as the history offers it again: a flag with
/// the value it took (`--model opus`), or a variable (`DEBUG=1`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentEntry {
    /// The words of the entry, as typed.
    pub words: Vec<String>,
    /// The project the entry was last used in.
    pub project: PathBuf,
    pub last_used_unix: u64,
}

impl RecentEntry {
    /// Whether the entry was last used in `project_root`.
    pub fn used_in(&self, project_root: &Path) -> bool {
        self.project == canonical(project_root)
    }
}

/// An agent's overrides, by harness.
type ByHarness = BTreeMap<String, Vec<String>>;

#[derive(Debug, Deserialize, Serialize)]
struct LaunchOverridesRecord {
    schema_version: u32,
    /// By project id, then agent id, then harness.
    #[serde(default)]
    agents: BTreeMap<String, BTreeMap<String, ByHarness>>,
    /// By harness, most recent first.
    #[serde(default)]
    recent: BTreeMap<String, Vec<RecentEntry>>,
    /// By harness, the entries the last new agent was launched with.
    #[serde(default)]
    last: BTreeMap<String, Vec<Vec<String>>>,
}

impl Default for LaunchOverridesRecord {
    fn default() -> Self {
        Self {
            schema_version: <Self as uze_document::Shaped>::SHAPE,
            agents: BTreeMap::new(),
            recent: BTreeMap::new(),
            last: BTreeMap::new(),
        }
    }
}

/// A record: only what the operator typed says it.
impl uze_document::Shaped for LaunchOverridesRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "launch overrides";
}

/// The overrides `agent` of `project_root` was last launched with on
/// `integration`, as words. None when there are none, or the record does
/// not read: a launch is never held back by its overrides.
pub fn recorded(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    integration: &str,
) -> Vec<String> {
    read(home)
        .agents
        .get(&project_id(project_root))
        .and_then(|agents| agents.get(agent.as_str()))
        .and_then(|harnesses| harnesses.get(integration))
        .cloned()
        .unwrap_or_default()
}

/// Records that `agent` is launched on `integration` with `tokens`, and
/// remembers each of their entries, read against `catalog`, among the
/// recent ones. A launch with none clears what the agent had on that
/// harness and remembers nothing.
pub fn record_launch(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    integration: &str,
    catalog: FlagCatalog,
    tokens: &[String],
) -> Result<()> {
    record_launch_at(
        home,
        project_root,
        &Launch {
            agent,
            integration,
            catalog,
            tokens,
        },
        now_unix(),
    )
}

/// One launch, as it is recorded.
struct Launch<'a> {
    agent: &'a AgentId,
    integration: &'a str,
    catalog: FlagCatalog,
    tokens: &'a [String],
}

fn record_launch_at(
    home: &UzeHome,
    project_root: &Path,
    launch: &Launch<'_>,
    now: u64,
) -> Result<()> {
    let Launch {
        agent,
        integration,
        catalog,
        tokens,
    } = *launch;
    rewrite(home, |record| {
        let project = project_id(project_root);
        if tokens.is_empty() {
            forget_harness(record, &project, agent.as_str(), integration);
            return;
        }
        record
            .agents
            .entry(project)
            .or_default()
            .entry(agent.as_str().to_owned())
            .or_default()
            .insert(integration.to_owned(), tokens.to_vec());
        let entries = super::entries(tokens, catalog);
        remember(record, integration, &entries, &canonical(project_root), now);
    })
}

/// Forgets every override `agent` of `project_root` had, on every harness.
/// What its record going takes with it.
pub fn forget_agent(home: &UzeHome, project_root: &Path, agent: &AgentId) -> Result<()> {
    let project = project_id(project_root);
    if !read(home)
        .agents
        .get(&project)
        .is_some_and(|agents| agents.contains_key(agent.as_str()))
    {
        return Ok(());
    }
    rewrite(home, |record| {
        if let Some(agents) = record.agents.get_mut(&project) {
            agents.remove(agent.as_str());
            if agents.is_empty() {
                record.agents.remove(&project);
            }
        }
    })
}

/// The entries used lately on `integration`: those last used in
/// `project_root` first, then the others, each group most recent first.
pub fn recent_entries(home: &UzeHome, integration: &str, project_root: &Path) -> Vec<RecentEntry> {
    let project = canonical(project_root);
    let (mut here, elsewhere): (Vec<RecentEntry>, Vec<RecentEntry>) = read(home)
        .recent
        .remove(integration)
        .unwrap_or_default()
        .into_iter()
        .partition(|entry| entry.project == project);
    here.extend(elsewhere);
    here
}

/// Forgets the entry `words` on `integration`, in every project and in
/// the last selection, and says whether the history had it.
pub fn forget_recent_entry(home: &UzeHome, integration: &str, words: &[String]) -> Result<bool> {
    let mut forgotten = false;
    rewrite(home, |record| {
        if let Some(entries) = record.recent.get_mut(integration) {
            let before = entries.len();
            entries.retain(|entry| entry.words != words);
            forgotten = entries.len() != before;
            if entries.is_empty() {
                record.recent.remove(integration);
            }
        }
        if let Some(selection) = record.last.get_mut(integration) {
            selection.retain(|entry| entry != words);
            if selection.is_empty() {
                record.last.remove(integration);
            }
        }
    })?;
    Ok(forgotten)
}

/// The entries the last new agent on `integration` was launched with, in
/// its order: what the next one opens on. None when there were none, or
/// the record does not read.
pub fn last_selection(home: &UzeHome, integration: &str) -> Vec<Vec<String>> {
    read(home).last.remove(integration).unwrap_or_default()
}

/// Remembers `tokens`, read against `catalog`, as the selection the next
/// new agent on `integration` opens on: a launch with none remembers that
/// nothing was checked. A variable that reads as a secret is left out, so
/// one is never carried to an agent it was not typed for.
pub fn remember_selection(
    home: &UzeHome,
    integration: &str,
    catalog: FlagCatalog,
    tokens: &[String],
) -> Result<()> {
    let selection = rememberable(&super::entries(tokens, catalog));
    rewrite(home, |record| {
        if selection.is_empty() {
            record.last.remove(integration);
        } else {
            record.last.insert(integration.to_owned(), selection);
        }
    })
}

fn forget_harness(
    record: &mut LaunchOverridesRecord,
    project: &str,
    agent: &str,
    integration: &str,
) {
    let Some(agents) = record.agents.get_mut(project) else {
        return;
    };
    if let Some(harnesses) = agents.get_mut(agent) {
        harnesses.remove(integration);
        if harnesses.is_empty() {
            agents.remove(agent);
        }
    }
    if agents.is_empty() {
        record.agents.remove(project);
    }
}

/// Puts each of a launch's flags and variables at the top of
/// `integration`'s history, in the launch's order, each taking the project
/// it is used in now. An entry is its words, so the same flag with another
/// value is another entry. A variable that reads as a secret is never
/// remembered.
fn remember(
    record: &mut LaunchOverridesRecord,
    integration: &str,
    entries: &[super::Entry],
    project: &Path,
    now: u64,
) {
    let kept = rememberable(entries);
    if kept.is_empty() {
        return;
    }
    let history = record.recent.entry(integration.to_owned()).or_default();
    history.retain(|entry| !kept.contains(&entry.words));
    history.splice(
        0..0,
        kept.into_iter().map(|words| RecentEntry {
            words,
            project: project.to_path_buf(),
            last_used_unix: now,
        }),
    );
    history.truncate(RECENT_PER_HARNESS);
}

/// The words of each of `entries` worth offering again, once each, in
/// their order: flags, and variables that do not read as a secret.
fn rememberable(entries: &[super::Entry]) -> Vec<Vec<String>> {
    entries
        .iter()
        .filter(|entry| match entry {
            super::Entry::Variable { name, value } => !reads_as_secret(name, value),
            super::Entry::Flag { .. } => true,
            super::Entry::Positional(_) | super::Entry::EndOfOptions => false,
        })
        .map(super::Entry::words)
        .fold(Vec::new(), |mut kept, words| {
            if !kept.contains(&words) {
                kept.push(words);
            }
            kept
        })
}

/// Whether the variable `name=value` reads as a secret, by its name or by
/// a URL with credentials in its value.
fn reads_as_secret(name: &str, value: &str) -> bool {
    let name = name.to_ascii_uppercase();
    SECRET_FRAGMENTS
        .iter()
        .any(|fragment| name.contains(fragment))
        || name.split('_').any(|part| SECRET_PARTS.contains(&part))
        || carries_credentials(value)
}

/// Whether `value` holds a URL with credentials before its host
/// (`postgres://user:pw@host`, `https://key@host`), whatever the
/// variable is called.
fn carries_credentials(value: &str) -> bool {
    value.match_indices("://").any(|(at, scheme_end)| {
        let rest = &value[at + scheme_end.len()..];
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        authority.contains('@')
    })
}

/// The record, or none when it is absent or does not read: what a read
/// may not do is fail a launch or a listing.
fn read(home: &UzeHome) -> LaunchOverridesRecord {
    uze_document::read::<LaunchOverridesRecord>(&home.launch_overrides_path())
        .ok()
        .and_then(uze_document::Carried::record)
        .unwrap_or_default()
}

/// One read-modify-write of the record under its lock. A record this build
/// cannot read is set aside rather than written over, and one a newer
/// build wrote is left alone and refused.
fn rewrite(home: &UzeHome, change: impl FnOnce(&mut LaunchOverridesRecord)) -> Result<()> {
    let _held = crate::file_lock::Held::acquire(&home.launch_overrides_lock_path(), LOCK_PATIENCE)?;
    let path = home.launch_overrides_path();
    let mut record = match uze_document::read::<LaunchOverridesRecord>(&path) {
        Ok(carried) => carried.or_default(),
        Err(reason) if uze_document::may_be_set_aside(&reason) => {
            uze_document::set_aside(
                &path,
                <LaunchOverridesRecord as uze_document::Shaped>::KIND,
                &reason,
            )?;
            LaunchOverridesRecord::default()
        }
        Err(reason) => return Err(reason.into()),
    };
    change(&mut record);
    record.schema_version = <LaunchOverridesRecord as uze_document::Shaped>::SHAPE;
    write_atomic_private(
        &path,
        &serde_json::to_vec_pretty(&record).expect("the launch overrides record serializes"),
    )
}

fn project_id(project_root: &Path) -> String {
    project_id_for(&canonical(project_root))
}

fn canonical(root: &Path) -> PathBuf {
    root.canonical().unwrap_or_else(|_| root.to_path_buf())
}

fn now_unix() -> u64 {
    uze_platform::clock::since_epoch().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::flag_catalog::{FlagGrammar, FlagOwner, FlagSpec, FlagValue};

    /// A catalog that knows which flags take a value, naming no harness.
    const CATALOG: FlagCatalog = FlagCatalog::new(
        FlagGrammar::Posix,
        &[
            FlagSpec::new(
                &["--model"],
                FlagValue::Free("model"),
                "model",
                FlagOwner::Operator,
            ),
            FlagSpec::new(
                &["--add-dir"],
                FlagValue::Free("dir"),
                "dir",
                FlagOwner::Operator,
            ),
        ],
    );

    struct Fixture {
        home: UzeHome,
        here: PathBuf,
        there: PathBuf,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let root = uze_testkit::temp::scratch(label);
            let (here, there) = (root.join("here"), root.join("there"));
            std::fs::create_dir_all(&here).unwrap();
            std::fs::create_dir_all(&there).unwrap();
            Self {
                home: UzeHome::at(root.join("uze")),
                here: canonical(&here),
                there: canonical(&there),
            }
        }

        fn launch(&self, project: &Path, agent: &str, harness: &str, line: &str, at: u64) {
            record_launch_at(
                &self.home,
                project,
                &Launch {
                    agent: &AgentId::adopted(agent),
                    integration: harness,
                    catalog: CATALOG,
                    tokens: &crate::overrides::split(line).unwrap(),
                },
                at,
            )
            .unwrap();
        }

        fn recent_lines(&self, harness: &str, project: &Path) -> Vec<String> {
            recent_entries(&self.home, harness, project)
                .into_iter()
                .map(|entry| entry.words.join(" "))
                .collect()
        }
    }

    /// Whether `word` is a variable the history would not keep.
    fn remembered_as_secret(word: &str) -> bool {
        match crate::overrides::entries(&[word.to_owned()], CATALOG).as_slice() {
            [crate::overrides::Entry::Variable { name, value }] => reads_as_secret(name, value),
            _ => false,
        }
    }

    fn agent(id: &str) -> AgentId {
        AgentId::adopted(id)
    }

    #[test]
    fn an_agent_keeps_its_overrides_per_harness() {
        let fixture = Fixture::new("overrides-per-harness");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a1", "second", "--verbose", 2);

        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a1"), "first"),
            ["--model", "opus"]
        );
        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a1"), "second"),
            ["--verbose"]
        );
        assert!(recorded(&fixture.home, &fixture.there, &agent("a1"), "first").is_empty());
        assert!(recorded(&fixture.home, &fixture.here, &agent("a2"), "first").is_empty());
    }

    #[test]
    fn a_launch_with_none_clears_that_harness_only() {
        let fixture = Fixture::new("overrides-cleared");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a1", "second", "--verbose", 2);

        fixture.launch(&fixture.here, "a1", "first", "", 3);

        assert!(recorded(&fixture.home, &fixture.here, &agent("a1"), "first").is_empty());
        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a1"), "second"),
            ["--verbose"]
        );
        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--model opus"]
        );
    }

    #[test]
    fn forgetting_an_agent_takes_every_harness_and_keeps_the_history() {
        let fixture = Fixture::new("overrides-forget-agent");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a1", "second", "--verbose", 2);
        fixture.launch(&fixture.here, "a2", "first", "--debug", 3);

        forget_agent(&fixture.home, &fixture.here, &agent("a1")).unwrap();
        forget_agent(&fixture.home, &fixture.here, &agent("never")).unwrap();

        assert!(recorded(&fixture.home, &fixture.here, &agent("a1"), "first").is_empty());
        assert!(recorded(&fixture.home, &fixture.here, &agent("a1"), "second").is_empty());
        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a2"), "first"),
            ["--debug"]
        );
        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--debug", "--model opus"]
        );
    }

    #[test]
    fn a_launch_is_remembered_entry_by_entry_in_its_order() {
        let fixture = Fixture::new("overrides-entries");
        fixture.launch(&fixture.here, "a1", "first", "--verbose", 1);
        fixture.launch(
            &fixture.here,
            "a2",
            "first",
            "--model opus DEBUG=1 --add-dir ../lib",
            2,
        );

        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--model opus", "DEBUG=1", "--add-dir ../lib", "--verbose"]
        );
    }

    #[test]
    fn the_same_entry_twice_is_held_once_at_the_top_taking_the_later_project() {
        let fixture = Fixture::new("overrides-same-entry");
        fixture.launch(&fixture.here, "a1", "first", "--model opus --verbose", 1);
        fixture.launch(&fixture.here, "a2", "first", "--debug", 2);
        fixture.launch(&fixture.there, "a3", "first", "--model opus", 3);

        let entries = recent_entries(&fixture.home, "first", &fixture.there);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].words, ["--model", "opus"]);
        assert!(entries[0].used_in(&fixture.there));
        assert_eq!(entries[0].last_used_unix, 3);
        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--debug", "--verbose", "--model opus"]
        );
    }

    #[test]
    fn the_same_flag_with_another_value_is_another_entry() {
        let fixture = Fixture::new("overrides-other-value");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a2", "first", "--model sonnet", 2);

        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--model sonnet", "--model opus"]
        );
    }

    #[test]
    fn this_projects_entries_come_first_then_the_others_each_most_recent_first() {
        let fixture = Fixture::new("overrides-order");
        fixture.launch(&fixture.there, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a2", "first", "--verbose", 2);
        fixture.launch(&fixture.there, "a3", "first", "--debug", 3);
        fixture.launch(&fixture.here, "a4", "first", "--add-dir ../lib", 4);
        fixture.launch(&fixture.here, "a5", "second", "--other", 5);

        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--add-dir ../lib", "--verbose", "--debug", "--model opus"]
        );
        let entries = recent_entries(&fixture.home, "first", &fixture.here);
        assert!(!entries[2].used_in(&fixture.here));
        assert_eq!(entries[2].project, fixture.there);
    }

    #[test]
    fn the_history_keeps_twenty_entries_per_harness() {
        let fixture = Fixture::new("overrides-cap");
        for index in 0..25 {
            fixture.launch(&fixture.here, "a1", "first", &format!("--n{index}"), index);
        }
        fixture.launch(&fixture.here, "a1", "second", "--kept", 30);

        let lines = fixture.recent_lines("first", &fixture.here);
        assert_eq!(lines.len(), RECENT_PER_HARNESS);
        assert_eq!(lines.first().unwrap(), "--n24");
        assert_eq!(lines.last().unwrap(), "--n5");
        assert_eq!(fixture.recent_lines("second", &fixture.here), ["--kept"]);
    }

    #[test]
    fn a_secret_variable_is_not_remembered() {
        let fixture = Fixture::new("overrides-secrets");
        fixture.launch(
            &fixture.here,
            "a1",
            "first",
            "API_TOKEN=secret --model opus",
            1,
        );

        assert_eq!(
            fixture.recent_lines("first", &fixture.here),
            ["--model opus"]
        );
        // The agent's own launch still carries it.
        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a1"), "first"),
            ["API_TOKEN=secret", "--model", "opus"]
        );
    }

    #[test]
    fn a_name_reads_as_a_secret_by_its_parts() {
        for secret in [
            "API_TOKEN=x",
            "aws_secret_access_key=x",
            "DB_PASSWORD=x",
            "PASS=x",
            "GOOGLE_APPLICATION_CREDENTIALS=x",
            "GH_AUTH=x",
            "GH_PAT=x",
            "OPENAI_APIKEY=x",
            "DB_PASSWD=x",
            "MY_CREDS=x",
            "githubtoken=x",
            "OAUTH_CLIENT=x",
            "DB_PWD=x",
            "SENTRY_DSN=x",
        ] {
            assert!(remembered_as_secret(secret), "{secret}");
        }
        for plain in [
            "DEBUG=1",
            "LOG_LEVEL=debug",
            "PATCH_DIR=x",
            "EXTRA_PATH=x",
            "--api-key=x",
        ] {
            assert!(!remembered_as_secret(plain), "{plain}");
        }
    }

    #[test]
    fn a_value_carrying_credentials_in_a_url_reads_as_a_secret() {
        for secret in [
            "DATABASE_URL=postgres://u:pw@h/db",
            "UPSTREAM=https://key@o1.ingest.example.io/42",
            "PROXY=http://user@proxy:8080",
            "MIRRORS=a,https://u:p@m.example/x",
        ] {
            assert!(remembered_as_secret(secret), "{secret}");
        }
        for plain in [
            "HOMEPAGE=https://example.com/a@b",
            "UPSTREAM=https://example.com?to=a@b",
            "MAILTO=me@example.com",
            "REPO=git@github.com:o/r",
        ] {
            assert!(!remembered_as_secret(plain), "{plain}");
        }
    }

    #[test]
    fn a_launch_of_secrets_only_remembers_nothing() {
        let fixture = Fixture::new("overrides-secrets-only");
        fixture.launch(&fixture.here, "a1", "first", "API_KEY=x", 1);

        assert!(recent_entries(&fixture.home, "first", &fixture.here).is_empty());
    }

    #[test]
    fn a_forgotten_entry_is_offered_in_no_project() {
        let fixture = Fixture::new("overrides-forget-entry");
        fixture.launch(&fixture.here, "a1", "first", "--model opus --verbose", 1);
        fixture.launch(&fixture.there, "a2", "first", "--verbose", 2);

        let opus = vec!["--model".to_owned(), "opus".to_owned()];
        assert!(forget_recent_entry(&fixture.home, "first", &opus).unwrap());
        assert!(!forget_recent_entry(&fixture.home, "first", &opus).unwrap());

        assert_eq!(fixture.recent_lines("first", &fixture.here), ["--verbose"]);
        assert_eq!(fixture.recent_lines("first", &fixture.there), ["--verbose"]);
    }

    #[test]
    fn a_record_that_does_not_read_records_nothing_and_is_set_aside_on_write() {
        let fixture = Fixture::new("overrides-unreadable");
        let path = fixture.home.launch_overrides_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();

        assert!(recorded(&fixture.home, &fixture.here, &agent("a1"), "first").is_empty());
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);

        assert_eq!(
            recorded(&fixture.home, &fixture.here, &agent("a1"), "first"),
            ["--model", "opus"]
        );
    }

    // Unix file modes, which Windows does not keep.
    #[cfg(unix)]
    #[test]
    fn the_record_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new("overrides-mode");
        fixture.launch(&fixture.here, "a1", "first", "API_TOKEN=x", 1);

        let mode = std::fs::metadata(fixture.home.launch_overrides_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    fn remember(fixture: &Fixture, harness: &str, line: &str) {
        remember_selection(
            &fixture.home,
            harness,
            CATALOG,
            &crate::overrides::split(line).unwrap(),
        )
        .unwrap();
    }

    fn selection_lines(fixture: &Fixture, harness: &str) -> Vec<String> {
        last_selection(&fixture.home, harness)
            .into_iter()
            .map(|entry| entry.join(" "))
            .collect()
    }

    #[test]
    fn the_last_selection_is_kept_per_harness_and_replaced_whole() {
        let fixture = Fixture::new("overrides-last-selection");
        remember(&fixture, "first", "--model opus --yolo DEBUG=1");
        remember(&fixture, "second", "--verbose");

        remember(&fixture, "first", "--model opus DEBUG=1");

        assert_eq!(
            selection_lines(&fixture, "first"),
            ["--model opus", "DEBUG=1"]
        );
        assert_eq!(selection_lines(&fixture, "second"), ["--verbose"]);
    }

    #[test]
    fn a_launch_with_nothing_checked_is_remembered_as_nothing() {
        let fixture = Fixture::new("overrides-last-empty");
        remember(&fixture, "first", "--model opus");

        remember(&fixture, "first", "");

        assert!(last_selection(&fixture.home, "first").is_empty());
    }

    #[test]
    fn a_secret_variable_is_never_in_the_last_selection() {
        let fixture = Fixture::new("overrides-last-secret");
        remember(
            &fixture,
            "first",
            "GH_TOKEN=x DB=postgres://u:p@host DEBUG=1 --model opus",
        );

        assert_eq!(
            selection_lines(&fixture, "first"),
            ["DEBUG=1", "--model opus"]
        );
    }

    #[test]
    fn forgetting_an_entry_takes_it_out_of_the_last_selection_too() {
        let fixture = Fixture::new("overrides-last-forget");
        fixture.launch(&fixture.here, "a1", "first", "--model opus --verbose", 1);
        remember(&fixture, "first", "--model opus --verbose");

        forget_recent_entry(&fixture.home, "first", &["--verbose".to_owned()]).unwrap();

        assert_eq!(selection_lines(&fixture, "first"), ["--model opus"]);
    }
}
