//! The record of launch overrides: what each agent was launched with, per
//! harness, and the sets the operator used lately, per harness.
//!
//! One document for the machine rather than one per project: most
//! overrides are the operator's habits (`--model`, `--verbose`), and a
//! history per project would have them typed again in every repository.
//! The few that belong to a project are handled by order instead: each set
//! remembers the project it was last used in, and that project lists its
//! own first. Owner-only, written under its lock, and read without it by
//! a launch, which an atomic write never shows half-written.
//!
//! What is typed is kept as the words it split into, and read as entries
//! again at each launch against the catalog of that day: an override
//! recorded before UZE came to own its flag is left out then, not obeyed.

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

/// Sets kept per harness, across every project.
const RECENT_PER_HARNESS: usize = 20;
/// How long a write waits for another one, each a small rewrite.
const LOCK_PATIENCE: Duration = Duration::from_secs(2);
/// A variable whose name, upper-cased, holds one of these anywhere reads as
/// a secret, and is not remembered in the history: `GH_TOKEN`,
/// `OPENAI_APIKEY`, `DB_PASSWD`. Remembering one too few is the cost to
/// avoid; `AUTHOR` withheld too is the price.
const SECRET_FRAGMENTS: &[&str] = &["TOKEN", "SECRET", "PASS", "CRED", "KEY", "AUTH"];
/// Too short to look for anywhere, so read as a secret only as a whole
/// `_`-separated part: `GH_PAT`, not `PATH`.
const SECRET_PARTS: &[&str] = &["PAT"];

/// A set of overrides used before, as the history offers it again.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentSet {
    /// The words of the set, without the variables that read as secrets.
    pub tokens: Vec<String>,
    /// How many variables were not kept because they read as secrets.
    #[serde(default)]
    pub withheld: usize,
    /// The project the set was last used in.
    pub project: PathBuf,
    pub last_used_unix: u64,
}

impl RecentSet {
    /// Whether the set was last used in `project_root`.
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
    recent: BTreeMap<String, Vec<RecentSet>>,
}

impl Default for LaunchOverridesRecord {
    fn default() -> Self {
        Self {
            schema_version: <Self as uze_document::Shaped>::SHAPE,
            agents: BTreeMap::new(),
            recent: BTreeMap::new(),
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
/// remembers the set among the recent ones. A launch with none clears what
/// the agent had on that harness and remembers nothing.
pub fn record_launch(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    integration: &str,
    tokens: &[String],
) -> Result<()> {
    record_launch_at(home, project_root, agent, integration, tokens, now_unix())
}

fn record_launch_at(
    home: &UzeHome,
    project_root: &Path,
    agent: &AgentId,
    integration: &str,
    tokens: &[String],
    now: u64,
) -> Result<()> {
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
        remember(record, integration, tokens, &canonical(project_root), now);
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

/// The sets used lately on `integration`: those last used in
/// `project_root` first, then the others, each group most recent first.
pub fn recent(home: &UzeHome, integration: &str, project_root: &Path) -> Vec<RecentSet> {
    let project = canonical(project_root);
    let (mut here, elsewhere): (Vec<RecentSet>, Vec<RecentSet>) = read(home)
        .recent
        .remove(integration)
        .unwrap_or_default()
        .into_iter()
        .partition(|set| set.project == project);
    here.extend(elsewhere);
    here
}

/// Forgets the set `tokens` on `integration`, in every project, and says
/// whether there was one.
pub fn forget_recent(home: &UzeHome, integration: &str, tokens: &[String]) -> Result<bool> {
    let mut forgotten = false;
    rewrite(home, |record| {
        if let Some(sets) = record.recent.get_mut(integration) {
            let before = sets.len();
            sets.retain(|set| set.tokens != tokens);
            forgotten = sets.len() != before;
            if sets.is_empty() {
                record.recent.remove(integration);
            }
        }
    })?;
    Ok(forgotten)
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

/// Puts the set at the top of `integration`'s history, taking the project
/// it is used in now. A set is the words kept, so the same flags with a
/// different secret are one set.
fn remember(
    record: &mut LaunchOverridesRecord,
    integration: &str,
    tokens: &[String],
    project: &Path,
    now: u64,
) {
    let (tokens, withheld) = without_secrets(tokens);
    if tokens.is_empty() {
        return;
    }
    let sets = record.recent.entry(integration.to_owned()).or_default();
    sets.retain(|set| set.tokens != tokens);
    sets.insert(
        0,
        RecentSet {
            tokens,
            withheld,
            project: project.to_path_buf(),
            last_used_unix: now,
        },
    );
    sets.truncate(RECENT_PER_HARNESS);
}

/// `tokens` without the variables whose name reads as a secret, and how
/// many there were.
fn without_secrets(tokens: &[String]) -> (Vec<String>, usize) {
    let (secrets, kept): (Vec<&String>, Vec<&String>) =
        tokens.iter().partition(|token| reads_as_secret(token));
    (kept.into_iter().cloned().collect(), secrets.len())
}

fn reads_as_secret(token: &str) -> bool {
    match super::entries(&[token.to_owned()], FlagCatalog::default()).as_slice() {
        [super::Entry::Variable { name, .. }] => {
            let name = name.to_ascii_uppercase();
            SECRET_FRAGMENTS
                .iter()
                .any(|fragment| name.contains(fragment))
                || name.split('_').any(|part| SECRET_PARTS.contains(&part))
        }
        _ => false,
    }
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
                &AgentId::adopted(agent),
                harness,
                &crate::overrides::split(line).unwrap(),
                at,
            )
            .unwrap();
        }

        fn recent_lines(&self, harness: &str, project: &Path) -> Vec<String> {
            recent(&self.home, harness, project)
                .into_iter()
                .map(|set| set.tokens.join(" "))
                .collect()
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
    fn the_same_set_twice_is_held_once_at_the_top_taking_the_later_project() {
        let fixture = Fixture::new("overrides-same-set");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.here, "a2", "first", "--verbose", 2);
        fixture.launch(&fixture.there, "a3", "first", "--model opus", 3);

        let sets = recent(&fixture.home, "first", &fixture.there);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].tokens, ["--model", "opus"]);
        assert!(sets[0].used_in(&fixture.there));
        assert_eq!(sets[0].last_used_unix, 3);
    }

    #[test]
    fn this_projects_sets_come_first_then_the_others_each_most_recent_first() {
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
        let sets = recent(&fixture.home, "first", &fixture.here);
        assert!(!sets[2].used_in(&fixture.here));
        assert_eq!(sets[2].project, fixture.there);
    }

    #[test]
    fn the_history_keeps_twenty_sets_per_harness() {
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
    fn a_secret_variable_is_not_remembered_and_is_counted() {
        let fixture = Fixture::new("overrides-secrets");
        fixture.launch(
            &fixture.here,
            "a1",
            "first",
            "API_TOKEN=secret --model opus",
            1,
        );

        let sets = recent(&fixture.home, "first", &fixture.here);
        assert_eq!(sets[0].tokens, ["--model", "opus"]);
        assert_eq!(sets[0].withheld, 1);
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
        ] {
            assert!(reads_as_secret(secret), "{secret}");
        }
        for plain in [
            "DEBUG=1",
            "LOG_LEVEL=debug",
            "PATCH_DIR=x",
            "EXTRA_PATH=x",
            "--api-key=x",
        ] {
            assert!(!reads_as_secret(plain), "{plain}");
        }
    }

    #[test]
    fn a_set_of_secrets_only_is_not_remembered() {
        let fixture = Fixture::new("overrides-secrets-only");
        fixture.launch(&fixture.here, "a1", "first", "API_KEY=x", 1);

        assert!(recent(&fixture.home, "first", &fixture.here).is_empty());
    }

    #[test]
    fn a_forgotten_set_is_offered_in_no_project() {
        let fixture = Fixture::new("overrides-forget-set");
        fixture.launch(&fixture.here, "a1", "first", "--model opus", 1);
        fixture.launch(&fixture.there, "a2", "first", "--verbose", 2);

        let opus = vec!["--model".to_owned(), "opus".to_owned()];
        assert!(forget_recent(&fixture.home, "first", &opus).unwrap());
        assert!(!forget_recent(&fixture.home, "first", &opus).unwrap());

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
}
