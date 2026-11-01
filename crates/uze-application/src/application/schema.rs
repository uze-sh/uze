//! The home compared with its schema, under `uze doctor --repair` (design
//! D8 of `doctor-repair`): what `$UZE_HOME` holds, walked against the map
//! of what UZE owns.
//!
//! - a retired path still present, which nothing carries across, is
//!   removed (the bootstrap normally already did);
//! - an empty directory in the bytes, store or record tiers that the map
//!   does not declare, and no declared path lies under, is removed;
//! - a set-aside record and a revision a failed restore left in
//!   `sources/superseded/` are reported with their size, and removed only
//!   when the person asks (`--discard-leftovers`): either may be the only
//!   copy of something;
//! - anything else undeclared in the bytes, store, record or authored tiers
//!   is reported as unknown — possibly a newer build's — and never deleted.
//!
//! The generated and remembered tiers are their collectors' to judge, and
//! the terminal runtime's subtree is opaque: neither is walked.

use serde::Serialize;

use uze_core::home::{Form, OwnedPath, Standing, Tier};

use super::services::disk::measure;
use super::*;

/// What one entry of the home is, against the schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaFinding {
    /// An earlier release's path that nothing reads or carries any more.
    Retired,
    /// An empty directory nothing declares.
    EmptyDirectory,
    /// A record the ladder set aside, kept under a name nothing reads.
    SetAside,
    /// The revision a failed restore left, its only copy.
    Superseded,
    /// Nothing this build declares: possibly a newer build's.
    Unknown,
}

/// One entry of the home the schema does not account for.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SchemaEntry {
    pub path: PathBuf,
    pub tier: &'static str,
    pub finding: SchemaFinding,
    /// What it holds, for what is kept until the person says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    pub removed: bool,
}

impl SchemaEntry {
    /// Whether it is a leftover the person removes with
    /// `--discard-leftovers`.
    pub fn is_leftover(&self) -> bool {
        matches!(
            self.finding,
            SchemaFinding::SetAside | SchemaFinding::Superseded
        )
    }
}

/// Walks `home` against the map, removing what the schema says is safe to
/// remove — and the leftovers too when `discard_leftovers` — and answers
/// every entry it does not account for.
pub(crate) fn compare(home: &UzeHome, discard_leftovers: bool) -> Vec<SchemaEntry> {
    let mut walk = Walk {
        home,
        current: UzeHome::owned_paths()
            .iter()
            .filter(|owned| owned.standing == Standing::Current)
            .collect(),
        retired: UzeHome::owned_paths()
            .iter()
            .filter(|owned| owned.standing == Standing::Retired)
            .collect(),
        found: Vec::new(),
    };
    walk.visit(home.root(), &[]);
    for entry in &mut walk.found {
        entry.removed = match entry.finding {
            SchemaFinding::Retired => super::collection::remove(&entry.path),
            SchemaFinding::EmptyDirectory => std::fs::remove_dir(&entry.path).is_ok(),
            SchemaFinding::SetAside | SchemaFinding::Superseded if discard_leftovers => {
                super::collection::remove(&entry.path)
            }
            _ => false,
        };
    }
    walk.found
}

struct Walk<'a> {
    home: &'a UzeHome,
    current: Vec<&'static OwnedPath>,
    retired: Vec<&'static OwnedPath>,
    found: Vec<SchemaEntry>,
}

impl Walk<'_> {
    fn visit(&mut self, dir: &Path, relative: &[String]) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut here = relative.to_vec();
            here.push(name.clone());
            let Some(tier) = Tier::of(self.home, &path) else {
                continue;
            };
            if matches!(tier, Tier::Generated | Tier::Remembered) {
                continue;
            }
            // The entry's own type, never what a link leads to.
            let directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if uze_core::leftovers::is_set_aside(&name) {
                self.found(path, tier, SchemaFinding::SetAside);
                continue;
            }
            if let Some(owned) = self.declared(&here) {
                match owned.form {
                    Form::Opaque => {}
                    _ if owned.name == "superseded_dir" => {
                        self.found(path, tier, SchemaFinding::Superseded);
                    }
                    Form::Directory if directory && self.leads_somewhere(&here) => {
                        self.visit(&path, &here);
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(retired) = self.retired_at(&here) {
                if retired.disposable {
                    self.found(path, tier, SchemaFinding::Retired);
                }
                continue;
            }
            if self
                .retired
                .iter()
                .any(|retired| retired.lies_under(self.home, &here))
            {
                // A layout an earlier release kept, carried across by its
                // own step.
                continue;
            }
            if directory && self.leads_somewhere(&here) {
                self.visit(&path, &here);
            } else if directory
                && matches!(tier, Tier::Bytes | Tier::Store | Tier::Record)
                && std::fs::read_dir(&path).is_ok_and(|mut inside| inside.next().is_none())
            {
                self.found(path, tier, SchemaFinding::EmptyDirectory);
            } else {
                self.found(path, tier, SchemaFinding::Unknown);
            }
        }
    }

    fn declared(&self, here: &[String]) -> Option<&'static OwnedPath> {
        self.current
            .iter()
            .copied()
            .find(|owned| owned.matches(self.home, here))
    }

    fn retired_at(&self, here: &[String]) -> Option<&'static OwnedPath> {
        self.retired
            .iter()
            .copied()
            .find(|owned| owned.matches(self.home, here))
    }

    /// Whether a declared path lies under `here`, so the walk goes on.
    fn leads_somewhere(&self, here: &[String]) -> bool {
        self.current
            .iter()
            .any(|owned| owned.lies_under(self.home, here))
    }

    fn found(&mut self, path: PathBuf, tier: Tier, finding: SchemaFinding) {
        let kept = matches!(finding, SchemaFinding::SetAside | SchemaFinding::Superseded);
        self.found.push(SchemaEntry {
            bytes: kept.then(|| measure(&path, &[]).0),
            path,
            tier: tier.label(),
            finding,
            removed: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(name: &str) -> (UzeHome, PathBuf) {
        let root = uze_testkit::temp::scratch(name);
        let home = UzeHome::at(root.join("uze"));
        home.ensure_layout().unwrap();
        (home, root)
    }

    fn findings(entries: &[SchemaEntry]) -> Vec<(SchemaFinding, String, bool)> {
        entries
            .iter()
            .map(|entry| {
                (
                    entry.finding,
                    entry
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    entry.removed,
                )
            })
            .collect()
    }

    #[test]
    fn a_declared_home_has_nothing_to_say() {
        let (home, root) = home("schema-declared");
        std::fs::write(home.registry_path(), "{}").unwrap();
        std::fs::write(home.config_path(), "").unwrap();
        std::fs::create_dir_all(home.conversations_dir("p1")).unwrap();
        std::fs::write(home.conversation_path("p1", "a1"), "{}").unwrap();
        std::fs::create_dir_all(home.review_path("p1", "a1").parent().unwrap()).unwrap();
        std::fs::write(home.review_path("p1", "a1"), "{}").unwrap();
        std::fs::create_dir_all(home.terminal_runtime_dir().join("anything")).unwrap();
        std::fs::write(home.runtime_dir().join("whatever"), "x").unwrap();

        assert_eq!(compare(&home, false), []);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_unknown_is_reported_and_kept_and_the_retired_removed() {
        let (home, root) = home("schema-unknown");
        std::fs::write(home.state_dir().join("from-a-newer-build.json"), "{}").unwrap();
        std::fs::write(home.root().join("notes.txt"), "mine").unwrap();
        std::fs::write(home.retired_profiles_path(), "{}").unwrap();
        std::fs::create_dir_all(home.state_dir().join("empty")).unwrap();

        let entries = compare(&home, false);

        assert_eq!(
            findings(&entries),
            [
                (SchemaFinding::Unknown, "notes.txt".to_owned(), false),
                (SchemaFinding::EmptyDirectory, "empty".to_owned(), true),
                (
                    SchemaFinding::Unknown,
                    "from-a-newer-build.json".to_owned(),
                    false
                ),
                (SchemaFinding::Retired, "profiles.json".to_owned(), true),
            ]
        );
        assert!(home.state_dir().join("from-a-newer-build.json").is_file());
        assert!(home.root().join("notes.txt").is_file());
        assert!(!home.retired_profiles_path().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_leftover_is_kept_with_its_size_until_the_person_discards_it() {
        let (home, root) = home("schema-leftovers");
        let set_aside = home.state_dir().join("hosts.json.unreadable-1700000000");
        std::fs::write(&set_aside, b"0123456789").unwrap();
        let id = uze_core::store::PackageId::from_qualified(
            "flow@ai",
            std::path::Path::new("plugin.json"),
        )
        .unwrap();
        std::fs::create_dir_all(home.superseded_dir(&id)).unwrap();
        std::fs::write(home.superseded_dir(&id).join("plugin.json"), b"{}").unwrap();

        let kept = compare(&home, false);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert!(
            kept.iter()
                .all(|entry| entry.is_leftover() && !entry.removed)
        );
        assert!(kept.iter().all(|entry| entry.bytes.is_some()));
        assert!(set_aside.exists() && home.superseded_dir(&id).exists());

        let discarded = compare(&home, true);
        assert!(discarded.iter().all(|entry| entry.removed), "{discarded:?}");
        assert!(!set_aside.exists() && !home.superseded_dir(&id).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_terminal_runtime_is_never_walked() {
        let (home, root) = home("schema-terminal");
        let terminal = home.terminal_runtime_dir();
        std::fs::create_dir_all(terminal.join("empty")).unwrap();
        std::fs::write(terminal.join("workspace.json.unreadable-1700000000"), b"x").unwrap();
        std::fs::write(terminal.join("unknown"), b"x").unwrap();

        assert_eq!(compare(&home, true), []);
        assert!(terminal.join("empty").is_dir());
        assert!(
            terminal
                .join("workspace.json.unreadable-1700000000")
                .exists()
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
