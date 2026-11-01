//! Taking UZE off this machine: what it would remove, keep or leave, and
//! then doing it (design D6 of `vendor-stores-and-uninstall`).
//!
//! Two modes. The default removes the program — the binary, the workspace
//! links into the home, and the generated and remembered tiers — and keeps
//! everything a reinstall adopts, so every delivered plugin keeps working.
//! A purge first detaches every receipt that still matches, through its
//! integration, readers before the material they read, then removes the
//! home last. What drifted, conflicts, cannot be inspected, or belongs to a
//! harness whose CLI is gone is left and listed, never edited by hand.
//!
//! Nothing here touches a project's files, checkouts, branches or refs:
//! the only thing removed inside a project is a link UZE made that points
//! into its home.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use uze_core::{
    Result, UzeError,
    exposure::ManagedArtifact,
    integration::{AttachmentInspection, AttachmentReceipt, AttachmentState, IntegrationPort},
    provisioning::ProvisionAction,
    state,
};

use crate::application::UzeApplication;

/// Which of the two ways off the machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UninstallMode {
    /// Remove the program, keep what it delivered working.
    Program,
    /// Remove every trace, what was delivered included.
    Purge,
}

/// What uninstall would do, read before anything is done: the plan a
/// person confirms, and the one recomputed under the lock when it runs.
#[derive(Clone, Debug, Serialize)]
pub struct UninstallPlan {
    pub mode: UninstallMode,
    /// `$UZE_HOME`, and whether there is one left to act on.
    pub home: PathBuf,
    pub home_present: bool,
    /// Per harness, what UZE delivered there and what becomes of it.
    pub harnesses: Vec<HarnessUninstall>,
    /// Each top-level entry of the home, and its fate.
    pub entries: Vec<HomeEntry>,
    /// Per project UZE knows, the work and links it holds.
    pub projects: Vec<ProjectWork>,
    /// Harnesses `setup` installed through the vendor's route: theirs to
    /// keep, and said so.
    pub provisioned: Vec<String>,
    pub binary: BinaryPlan,
}

impl UninstallPlan {
    /// Whether running it would change nothing at all.
    pub fn is_empty(&self) -> bool {
        !self.home_present
            && self.projects.iter().all(|project| project.links.is_empty())
            && !matches!(self.binary, BinaryPlan::Placed { .. })
    }

    /// The receipts a purge leaves, with the harness they belong to.
    pub fn left(&self) -> impl Iterator<Item = (&HarnessUninstall, &ReceiptUninstall)> {
        self.harnesses.iter().flat_map(|harness| {
            harness
                .receipts
                .iter()
                .filter(|receipt| receipt.fate == ReceiptFate::Leave)
                .map(move |receipt| (harness, receipt))
        })
    }
}

/// One harness's share of the plan.
#[derive(Clone, Debug, Serialize)]
pub struct HarnessUninstall {
    pub id: String,
    pub name: String,
    /// Whether its CLI is still here to be asked: without it, its
    /// registrations are left and never edited by hand.
    pub cli_present: bool,
    pub receipts: Vec<ReceiptUninstall>,
}

/// One thing UZE delivered, as uninstall sees it.
#[derive(Clone, Debug, Serialize)]
pub struct ReceiptUninstall {
    pub package: String,
    /// What a person recognizes it by: a plugin's selector, a file's path,
    /// an entry's name.
    pub subject: String,
    pub state: AttachmentState,
    pub reason: String,
    pub fate: ReceiptFate,
    #[serde(skip)]
    receipt: AttachmentReceipt,
}

/// What becomes of one delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptFate {
    /// Kept working: the default mode delivers on.
    Keep,
    /// Detached through its integration.
    Detach,
    /// Already gone: only the record of it is forgotten.
    Gone,
    /// Left where it is, and listed.
    Leave,
}

/// One top-level entry of `$UZE_HOME`.
#[derive(Clone, Debug, Serialize)]
pub struct HomeEntry {
    pub name: String,
    pub path: PathBuf,
    /// What it is, in the words `uze usage` uses for the tiers.
    pub holds: &'static str,
    pub bytes: u64,
    pub remove: bool,
}

/// A project's work uninstall leaves, and the links it removes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ProjectWork {
    pub root: PathBuf,
    /// Checkouts whose branch holds commits its target lacks.
    pub unlanded: Vec<UnlandedCheckout>,
    /// Work an agent left on a shelf (`refs/uze/shelf/*`).
    pub shelves: Vec<String>,
    /// Links UZE made that point into its home.
    pub links: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnlandedCheckout {
    pub path: PathBuf,
    pub commits: usize,
}

/// The `uze` binary, and whose it is to remove.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BinaryPlan {
    /// UZE's installer placed it: uninstall removes it, and the `PATH`
    /// entry the installer recorded adding.
    Placed {
        path: PathBuf,
        path_entry: Option<PathBuf>,
    },
    /// Put there some other way, and left to it: `remove_with` is the
    /// command for that way, when it is known.
    Elsewhere {
        path: PathBuf,
        remove_with: Option<String>,
    },
    /// Neither the installer's record nor this process names a binary.
    Unknown,
}

/// What uninstall did.
#[derive(Clone, Debug, Serialize)]
pub struct UninstallReport {
    pub mode: UninstallMode,
    pub home: PathBuf,
    /// Deliveries detached, by harness name.
    pub detached: Vec<(String, String)>,
    /// Registrations withdrawn, by harness name.
    pub withdrawn: Vec<(String, String)>,
    /// Deliveries left where they are, by harness name, with why.
    pub left: Vec<(String, String, String)>,
    pub links_removed: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
    pub home_removed: bool,
    pub binary: BinaryOutcome,
    /// What uninstall could not do itself, each with what the person does
    /// instead: the run went on without it, and its exit status says so.
    pub manual: Vec<String>,
    pub projects: Vec<ProjectWork>,
    pub provisioned: Vec<String>,
}

impl UninstallReport {
    /// Whether something is left for the person to finish by hand.
    pub fn needs_the_person(&self) -> bool {
        !self.manual.is_empty()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BinaryOutcome {
    Removed {
        path: PathBuf,
        path_entry_removed: Option<PathBuf>,
    },
    Left {
        path: PathBuf,
        remove_with: Option<String>,
    },
    /// The installer's binary, which could not be removed: said in the
    /// report's manual steps.
    NotRemoved {
        path: PathBuf,
        reason: String,
    },
    Unknown,
}

/// The installer's receipt, as far as uninstall reads it: the file it
/// placed, and the `PATH` entry it added, when it added one.
#[derive(serde::Deserialize)]
struct InstallReceipt {
    binary: PathBuf,
    #[serde(default)]
    path_entry: Option<PathBuf>,
}

impl UzeApplication {
    /// What `uze uninstall` would do in `mode`, changing nothing. A purge
    /// inspects every receipt through its integration, which asks the
    /// vendor CLIs; the default mode keeps every delivery and asks nothing.
    #[tracing::instrument(name = "uninstall.plan", skip_all, fields(mode = ?mode))]
    pub fn uninstall_plan(&self, mode: UninstallMode) -> UninstallPlan {
        let home_present = self.home.root().is_dir();
        let harnesses = if home_present {
            self.harnesses_to_uninstall(mode)
        } else {
            Vec::new()
        };
        UninstallPlan {
            mode,
            home: self.home.root().to_path_buf(),
            home_present,
            harnesses,
            entries: self.home_entries(mode),
            projects: self.projects_work(),
            provisioned: self.provisioned_harnesses(),
            binary: self.binary_plan(),
        }
    }

    /// Takes UZE off this machine in `plan`'s mode. The plan is computed
    /// again under the mutation lock, since a delivery may have changed
    /// since it was shown, and every step is safe to run again: a later run
    /// continues from whatever is left.
    ///
    /// A purge stops before the binary when a detach fails, so the run can
    /// be repeated; everything left by inspection is listed, not retried.
    #[tracing::instrument(name = "uninstall.run", skip_all, fields(mode = ?plan.mode), err)]
    pub fn uninstall(&self, plan: &UninstallPlan) -> Result<UninstallReport> {
        let lock = if self.home.root().is_dir() {
            Some(uze_core::persistence::MutationLock::acquire(&self.home)?)
        } else {
            None
        };
        // What the installer recorded is read before anything is removed:
        // `state/` goes with a purge, and the binary is removed after it.
        let plan = self.uninstall_plan(plan.mode);
        let mut report = UninstallReport {
            mode: plan.mode,
            home: plan.home.clone(),
            detached: Vec::new(),
            withdrawn: Vec::new(),
            left: plan
                .left()
                .map(|(harness, receipt)| {
                    (
                        harness.name.clone(),
                        receipt.subject.clone(),
                        receipt.reason.clone(),
                    )
                })
                .collect(),
            links_removed: Vec::new(),
            removed: Vec::new(),
            kept: Vec::new(),
            home_removed: false,
            binary: BinaryOutcome::Unknown,
            manual: Vec::new(),
            projects: plan.projects.clone(),
            provisioned: plan.provisioned.clone(),
        };
        if plan.mode == UninstallMode::Purge {
            self.detach_everything(&plan, &mut report)?;
        }
        for project in &plan.projects {
            for link in &project.links {
                if uze_platform::fs::remove_link(link).is_ok() {
                    report.links_removed.push(link.clone());
                }
            }
            let uze_links = project
                .root
                .join(uze_workspace::worktree::WORKTREES_DIRECTORY)
                .join("~uze");
            let _ = fs::remove_dir(uze_links);
        }
        report.binary = remove_binary(
            &plan.binary,
            &mut report.manual,
            uze_platform::environment::remove_from_user_path,
            uze_platform::executable::remove_running,
        );
        for entry in &plan.entries {
            if !entry.remove {
                report.kept.push(entry.path.clone());
            } else if plan.mode == UninstallMode::Program {
                remove_path(&entry.path)?;
                report.removed.push(entry.path.clone());
            }
        }
        drop(lock);
        if plan.mode == UninstallMode::Purge && plan.home_present {
            remove_path(&plan.home)?;
            report.removed.push(plan.home.clone());
            report.home_removed = true;
        }
        Ok(report)
    }

    fn harnesses_to_uninstall(&self, mode: UninstallMode) -> Vec<HarnessUninstall> {
        let receipts = state::receipts(&self.home, None).unwrap_or_default();
        let mut harnesses: Vec<HarnessUninstall> = Vec::new();
        for receipt in receipts {
            let integration = self.integration_named(&receipt.integration);
            let at = match harnesses
                .iter()
                .position(|harness| harness.id == receipt.integration)
            {
                Some(at) => at,
                None => {
                    harnesses.push(HarnessUninstall {
                        id: receipt.integration.clone(),
                        name: integration.map_or_else(
                            || receipt.integration.clone(),
                            |integration| integration.display_name().to_owned(),
                        ),
                        cli_present: mode == UninstallMode::Program
                            || integration.is_some_and(|integration| integration.detect().present),
                        receipts: Vec::new(),
                    });
                    harnesses.len() - 1
                }
            };
            let harness = &mut harnesses[at];
            let planned = plan_receipt(mode, integration, harness, receipt);
            harness.receipts.push(planned);
        }
        harnesses
    }

    /// A purge's first step: per harness, every delivery that still matches
    /// is detached, readers before the material they read, with UZE's own
    /// registrations withdrawn once nothing installed through them is left.
    fn detach_everything(&self, plan: &UninstallPlan, report: &mut UninstallReport) -> Result<()> {
        let mut failures = Vec::new();
        for harness in &plan.harnesses {
            let Some(integration) = self.integration_named(&harness.id) else {
                continue;
            };
            let mut order: Vec<&ReceiptUninstall> = harness
                .receipts
                .iter()
                .filter(|receipt| matches!(receipt.fate, ReceiptFate::Detach | ReceiptFate::Gone))
                .collect();
            order.sort_by_key(|receipt| Reads::of(&receipt.receipt.artifact));
            let mut withdrawn = false;
            for receipt in order {
                let reads = Reads::of(&receipt.receipt.artifact);
                if !withdrawn && reads > Reads::Delivery {
                    withdrawn = true;
                    self.withdraw(integration, harness, report, &mut failures);
                }
                if receipt.fate == ReceiptFate::Gone {
                    state::forget_receipt(&self.home, &receipt.receipt)?;
                    continue;
                }
                match integration.detach_receipt(&receipt.receipt) {
                    Ok(AttachmentInspection {
                        state: AttachmentState::Missing,
                        ..
                    }) => {
                        state::forget_receipt(&self.home, &receipt.receipt)?;
                        report
                            .detached
                            .push((harness.name.clone(), receipt.subject.clone()));
                    }
                    Ok(inspection) => failures.push(format!(
                        "{} on {}: {}",
                        receipt.subject, harness.name, inspection.reason
                    )),
                    Err(error) => {
                        failures.push(format!("{} on {}: {error}", receipt.subject, harness.name))
                    }
                }
            }
            if !withdrawn {
                self.withdraw(integration, harness, report, &mut failures);
            }
        }
        if failures.is_empty() {
            return Ok(());
        }
        Err(UzeError::LifecycleBlocked(format!(
            "uninstall stopped before removing uze: {}; run it again to continue",
            failures.join("; ")
        )))
    }

    /// UZE's own registrations with one harness, withdrawn only while its
    /// CLI is here to do it and nothing of UZE's is left installed there.
    fn withdraw(
        &self,
        integration: &dyn IntegrationPort,
        harness: &HarnessUninstall,
        report: &mut UninstallReport,
        failures: &mut Vec<String>,
    ) {
        if !harness.cli_present
            || harness
                .receipts
                .iter()
                .any(|receipt| receipt.fate == ReceiptFate::Leave)
        {
            return;
        }
        match integration.withdraw_registrations() {
            Ok(names) => report
                .withdrawn
                .extend(names.into_iter().map(|name| (harness.name.clone(), name))),
            Err(error) => failures.push(format!("{}: {error}", harness.name)),
        }
    }

    fn home_entries(&self, mode: UninstallMode) -> Vec<HomeEntry> {
        let home = &self.home;
        let known: Vec<(PathBuf, &'static str, bool)> = vec![
            (home.runtime_dir(), "generated", true),
            (home.shims_dir(), "generated", true),
            (home.cache_dir(), "cache", true),
            (home.sources_dir(), "packages", false),
            (home.store_dir(), "delivered plugins", false),
            (home.state_dir(), "records", false),
            (home.config_path(), "settings", false),
            (home.keymap_path(), "settings", false),
            (home.theme_overrides_path(), "settings", false),
            (home.themes_dir(), "settings", false),
        ];
        let Ok(listing) = fs::read_dir(home.root()) else {
            return Vec::new();
        };
        let mut present: Vec<PathBuf> = listing
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .collect();
        present.sort();
        present
            .into_iter()
            .map(|path| {
                let (holds, generated) = known
                    .iter()
                    .find(|(known, _, _)| *known == path)
                    .map_or(("not uze's", false), |(_, holds, generated)| {
                        (*holds, *generated)
                    });
                HomeEntry {
                    name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    bytes: crate::application::services::measure(&path, &[]).0,
                    remove: mode == UninstallMode::Purge || generated,
                    holds,
                    path,
                }
            })
            .collect()
    }

    fn projects_work(&self) -> Vec<ProjectWork> {
        uze_core::record::roots(&self.home)
            .into_iter()
            .filter(|root| root.is_dir())
            .map(|root| self.project_work(root))
            .filter(|work| {
                !(work.unlanded.is_empty() && work.shelves.is_empty() && work.links.is_empty())
            })
            .collect()
    }

    fn project_work(&self, root: PathBuf) -> ProjectWork {
        let agents = uze_workspace::task::load(&self.home, &root)
            .map(|store| store.agents)
            .unwrap_or_default();
        let unlanded = agents
            .iter()
            .filter_map(|agent| {
                let isolation = agent.isolation()?;
                let path = uze_workspace::landing::slot_path(&root, isolation)?;
                let commits = uze_workspace::checkout::commits_ahead(
                    &root,
                    &isolation.target,
                    &isolation.branch,
                );
                (commits > 0).then_some(UnlandedCheckout { path, commits })
            })
            .collect();
        let shelves = uze_workspace::checkout::shelf::list(&root)
            .into_iter()
            .map(|shelf| {
                if shelf.label.is_empty() {
                    shelf.task
                } else {
                    shelf.label
                }
            })
            .collect();
        ProjectWork {
            links: links_into(&root, self.home.root()),
            root,
            unlanded,
            shelves,
        }
    }

    fn provisioned_harnesses(&self) -> Vec<String> {
        self.integrations
            .iter()
            .filter(|integration| {
                state::provisioning(&self.home, integration.id())
                    .ok()
                    .flatten()
                    .is_some_and(|record| record.action == ProvisionAction::Install)
            })
            .map(|integration| integration.display_name().to_owned())
            .collect()
    }

    fn binary_plan(&self) -> BinaryPlan {
        let receipt = fs::read(self.home.install_receipt_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InstallReceipt>(&bytes).ok())
            .filter(|receipt| receipt.binary.exists());
        if let Some(receipt) = receipt {
            return BinaryPlan::Placed {
                path: receipt.binary,
                path_entry: receipt.path_entry,
            };
        }
        match std::env::current_exe() {
            Ok(path) => BinaryPlan::Elsewhere {
                remove_with: removal_command(&path),
                path,
            },
            Err(_) => BinaryPlan::Unknown,
        }
    }
}

/// The order a purge undoes one harness's deliveries in: what the harness
/// discovers and loads first, then UZE's registrations with it, then the
/// entries in its configuration, its hook groups after every other entry,
/// and last the builds those entries name.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Reads {
    Delivery,
    Entry,
    Hook,
    Material,
}

impl Reads {
    fn of(artifact: &ManagedArtifact) -> Self {
        match artifact {
            ManagedArtifact::IntegrationOwned { kind, .. } if kind == "build" => Self::Material,
            ManagedArtifact::IntegrationOwned { kind, .. } if kind == "rules-engine" => Self::Entry,
            ManagedArtifact::IntegrationOwned { .. }
            | ManagedArtifact::SymlinkReference { .. }
            | ManagedArtifact::GeneratedFile { .. }
            | ManagedArtifact::GeneratedTree { .. } => Self::Delivery,
            ManagedArtifact::VendorConfigEntry { .. }
            | ManagedArtifact::ManagedTextRegion { .. }
            | ManagedArtifact::ManagedHookFile { .. } => Self::Entry,
            ManagedArtifact::HookConfigEntry { .. } => Self::Hook,
        }
    }
}

fn plan_receipt(
    mode: UninstallMode,
    integration: Option<&dyn IntegrationPort>,
    harness: &HarnessUninstall,
    receipt: AttachmentReceipt,
) -> ReceiptUninstall {
    let subject = subject_of(&receipt);
    let (state, reason, fate) = match (mode, integration) {
        (UninstallMode::Program, _) => (AttachmentState::Matched, String::new(), ReceiptFate::Keep),
        (UninstallMode::Purge, None) => (
            AttachmentState::Blocked,
            format!("{} is not an agent this uze knows", harness.name),
            ReceiptFate::Leave,
        ),
        (UninstallMode::Purge, Some(_)) if !harness.cli_present => (
            AttachmentState::Blocked,
            format!("{} is not installed, so it cannot be asked", harness.name),
            ReceiptFate::Leave,
        ),
        (UninstallMode::Purge, Some(integration)) => {
            let inspection = integration.inspect_receipt(&receipt);
            let fate = match inspection.state {
                AttachmentState::Matched => ReceiptFate::Detach,
                AttachmentState::Missing => ReceiptFate::Gone,
                AttachmentState::Drifted | AttachmentState::Conflict | AttachmentState::Blocked => {
                    ReceiptFate::Leave
                }
            };
            (inspection.state, inspection.reason, fate)
        }
    };
    ReceiptUninstall {
        package: receipt.package_id.clone(),
        subject,
        state,
        reason,
        fate,
        receipt,
    }
}

/// What a person recognizes a delivery by.
fn subject_of(receipt: &AttachmentReceipt) -> String {
    match &receipt.artifact {
        ManagedArtifact::IntegrationOwned { selector, .. } => selector.clone(),
        ManagedArtifact::VendorConfigEntry { entry_name, .. }
        | ManagedArtifact::HookConfigEntry { entry_name, .. } => entry_name.clone(),
        ManagedArtifact::SymlinkReference { path, .. }
        | ManagedArtifact::ManagedHookFile { path }
        | ManagedArtifact::GeneratedFile { path, .. }
        | ManagedArtifact::GeneratedTree { path, .. } => path.display().to_string(),
        ManagedArtifact::ManagedTextRegion { target_file, .. } => target_file.display().to_string(),
    }
}

/// The links in `root`'s `.worktrees/` that point into `home`: beside a
/// checkout as `<id>~uze`, or under `~uze/` for an agent in the primary.
fn links_into(root: &Path, home: &Path) -> Vec<PathBuf> {
    let worktrees = root.join(uze_workspace::worktree::WORKTREES_DIRECTORY);
    let mut links: Vec<PathBuf> = fs::read_dir(&worktrees)
        .into_iter()
        .chain(fs::read_dir(worktrees.join("~uze")))
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| fs::read_link(path).is_ok_and(|target| target.starts_with(home)))
        .collect();
    links.sort();
    links
}

/// The command that removes a binary put where `path` is by something
/// other than UZE's installer, when the way is one a path says.
fn removal_command(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
    let in_cargo = parent.file_name().is_some_and(|name| name == "bin")
        && parent.parent().is_some_and(|grand| {
            grand.file_name().is_some_and(|name| name == ".cargo")
                || cargo_home.as_deref() == Some(grand)
        });
    if in_cargo {
        return Some("cargo uninstall uze".to_owned());
    }
    let spelled = path.to_string_lossy();
    (spelled.contains("/Cellar/") || spelled.contains("/homebrew/"))
        .then(|| "brew uninstall uze".to_owned())
}

/// Removes the binary the installer placed and the `PATH` entry it added,
/// through `unlist` and `remove`. One that cannot be done is said in
/// `manual`, with what the person does instead, and the rest goes on.
fn remove_binary(
    plan: &BinaryPlan,
    manual: &mut Vec<String>,
    unlist: impl Fn(&Path) -> std::io::Result<bool>,
    remove: impl Fn(&Path) -> std::io::Result<()>,
) -> BinaryOutcome {
    match plan {
        BinaryPlan::Placed { path, path_entry } => {
            let path_entry_removed = match path_entry {
                Some(directory) => match unlist(directory) {
                    Ok(removed) => removed.then(|| directory.clone()),
                    Err(error) => {
                        manual.push(format!(
                            "{} is still on your Path ({error}): take it off yourself",
                            directory.display()
                        ));
                        None
                    }
                },
                None => None,
            };
            match remove(path) {
                Ok(()) => BinaryOutcome::Removed {
                    path: path.clone(),
                    path_entry_removed,
                },
                Err(error) => {
                    manual.push(format!(
                        "{} was not removed ({error}): remove it yourself",
                        path.display()
                    ));
                    BinaryOutcome::NotRemoved {
                        path: path.clone(),
                        reason: error.to_string(),
                    }
                }
            }
        }
        BinaryPlan::Elsewhere { path, remove_with } => BinaryOutcome::Left {
            path: path.clone(),
            remove_with: remove_with.clone(),
        },
        BinaryPlan::Unknown => BinaryOutcome::Unknown,
    }
}

/// Removes a file, a link or a whole directory; one already gone is
/// removed.
fn remove_path(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let removed = if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match removed {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(UzeError::Write {
            path: path.to_path_buf(),
            source: error,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed() -> BinaryPlan {
        BinaryPlan::Placed {
            path: PathBuf::from("/opt/uze/bin/uze"),
            path_entry: Some(PathBuf::from("/opt/uze/bin")),
        }
    }

    #[test]
    fn a_binary_that_cannot_be_removed_is_a_step_for_the_person() {
        let mut manual = Vec::new();

        let outcome = remove_binary(
            &placed(),
            &mut manual,
            |_| Ok(true),
            |_| {
                Err(std::io::Error::other(
                    "remove /opt/uze/bin/uze.old yourself",
                ))
            },
        );

        assert!(
            matches!(&outcome, BinaryOutcome::NotRemoved { path, .. } if path == Path::new("/opt/uze/bin/uze")),
            "{outcome:?}"
        );
        assert_eq!(manual.len(), 1, "{manual:?}");
        assert!(
            manual[0].contains("remove /opt/uze/bin/uze.old yourself"),
            "{manual:?}"
        );
    }

    #[test]
    fn a_path_entry_that_cannot_be_undone_is_a_step_and_the_binary_still_goes() {
        let mut manual = Vec::new();

        let outcome = remove_binary(
            &placed(),
            &mut manual,
            |_| Err(std::io::Error::other("take /opt/uze/bin off it yourself")),
            |_| Ok(()),
        );

        assert!(
            matches!(
                &outcome,
                BinaryOutcome::Removed {
                    path_entry_removed: None,
                    ..
                }
            ),
            "{outcome:?}"
        );
        assert_eq!(manual.len(), 1, "{manual:?}");
        assert!(manual[0].contains("/opt/uze/bin"), "{manual:?}");
    }
}
