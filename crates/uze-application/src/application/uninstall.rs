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
    integration::{
        AttachmentInspection, AttachmentReceipt, AttachmentState, IntegrationPort, RegistrationRead,
    },
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
    /// The `uze` running this uninstall, when it is not the binary the
    /// installer placed: left, with the command that removes it.
    pub running: Option<LeftBinary>,
}

/// A `uze` binary uninstall leaves, and the command for the way it was
/// put there, when a path says it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LeftBinary {
    pub path: PathBuf,
    pub remove_with: Option<String>,
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
    /// Its vendor stores a purge keeps, because something left reads them:
    /// a registration never outlives the root it names, nor an entry the
    /// build it runs.
    pub keeps: Vec<PathBuf>,
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
    /// The running `uze`, left because the installer did not place it.
    pub running: Option<LeftBinary>,
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
        let (binary, running) = self.binary_plan();
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
            binary,
            running,
        }
    }

    /// Takes UZE off this machine in `plan`'s mode. The plan is computed
    /// again under the mutation lock, since a delivery may have changed
    /// since it was shown, and every step is safe to run again: a later run
    /// continues from whatever is left.
    ///
    /// A purge stops before the binary when a detach fails, so the run can
    /// be repeated; everything left by inspection is listed, not retried.
    ///
    /// `still_clear` is asked once the lock is held, right before anything
    /// is removed: what runs (an agent, an update) may have started since
    /// the plan was shown, and its refusal removes nothing.
    #[tracing::instrument(name = "uninstall.run", skip_all, fields(mode = ?plan.mode), err)]
    pub fn uninstall(
        &self,
        plan: &UninstallPlan,
        still_clear: &dyn Fn() -> Result<()>,
    ) -> Result<UninstallReport> {
        let lock = if self.home.root().is_dir() {
            Some(uze_core::persistence::MutationLock::acquire(&self.home)?)
        } else {
            None
        };
        still_clear()?;
        if plan.mode == UninstallMode::Program && self.home.root().is_dir() {
            self.converge_to_vendor_stores()?;
        }
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
            running: plan.running.clone(),
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
        let kept: Vec<PathBuf> = plan
            .harnesses
            .iter()
            .flat_map(|harness| harness.keeps.iter().cloned())
            .collect();
        if plan.mode == UninstallMode::Purge && plan.home_present && !kept.is_empty() {
            // What is left still reads these stores: everything else goes,
            // and the home stays to hold them.
            self.remove_all_but(&kept, &mut report)?;
            report.kept.extend(kept.iter().cloned());
        }
        drop(lock);
        if plan.mode == UninstallMode::Purge && plan.home_present && kept.is_empty() {
            remove_path(&plan.home)?;
            report.removed.push(plan.home.clone());
            report.home_removed = true;
        }
        Ok(report)
    }

    /// Moves a machine still in the layout before vendor stores onto them
    /// before the generated tier goes: its harnesses read `runtime/`, which
    /// the default mode removes, and only a delivery moves them (`setup`,
    /// `install`, `update`), which uninstall runs no bootstrap to reach.
    /// Refuses, removing nothing, when a harness still reads it after.
    fn converge_to_vendor_stores(&self) -> Result<()> {
        if self.reads_the_runtime_tree().is_empty() {
            return Ok(());
        }
        for integration in &self.integrations {
            let detection = self.detect_cached(integration.as_ref());
            if !detection.present {
                continue;
            }
            if let Err(error) = integration.install(&self.home, &detection) {
                tracing::warn!(integration = integration.id(), %error, "not prepared again");
            }
            if let Err(error) = self.attach_stored_packages_to(integration.as_ref()) {
                tracing::warn!(integration = integration.id(), %error, "not delivered again");
            }
        }
        let still = self.reads_the_runtime_tree();
        if still.is_empty() {
            return Ok(());
        }
        Err(UzeError::UninstallRefused {
            reason: format!(
                "nothing was removed: {} still read uze's generated files, which uninstall \
                 removes, and could not be moved to their stores",
                still.join(", ")
            ),
            next: "uze setup, then uze uninstall".to_owned(),
        })
    }

    /// The harnesses that read something under `runtime/`: a receipt
    /// naming the package copies every harness was once pointed at, or a
    /// marketplace registered under the runtime tree, as an earlier
    /// release left them.
    fn reads_the_runtime_tree(&self) -> Vec<String> {
        let copies = serde_json::to_string(
            &crate::application::earlier::package_copies_dir(&self.home).to_string_lossy(),
        )
        .unwrap_or_default();
        let copies = copies.trim_matches('"');
        let receipts = state::receipts(&self.home, None).unwrap_or_default();
        let mut harnesses: Vec<String> = Vec::new();
        for integration in &self.integrations {
            let named = receipts
                .iter()
                .filter(|receipt| receipt.integration == integration.id())
                .any(|receipt| {
                    serde_json::to_string(receipt).is_ok_and(|spelled| spelled.contains(copies))
                });
            let registered = self.detect_cached(integration.as_ref()).present
                && integration
                    .registrations(RegistrationRead::Live)
                    .iter()
                    .any(|registration| {
                        registration
                            .registered
                            .as_ref()
                            .is_some_and(|root| root.starts_with(self.home.runtime_dir()))
                    });
            if named || registered {
                harnesses.push(integration.display_name().to_owned());
            }
        }
        harnesses
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
                        keeps: Vec::new(),
                    });
                    harnesses.len() - 1
                }
            };
            let harness = &mut harnesses[at];
            let planned = plan_receipt(mode, integration, harness, receipt);
            harness.receipts.push(planned);
        }
        for harness in &mut harnesses {
            harness.keeps = self.stores_kept_for(harness);
        }
        harnesses
    }

    /// The vendor stores what `harness` leaves still reads: those its
    /// receipts name, once any receipt left reads a build.
    fn stores_kept_for(&self, harness: &HarnessUninstall) -> Vec<PathBuf> {
        let reads_a_build = harness.receipts.iter().any(|receipt| {
            receipt.fate == ReceiptFate::Leave && reads_builds(&receipt.receipt.artifact)
        });
        if !reads_a_build {
            return Vec::new();
        }
        let named = serde_json::to_string(
            &harness
                .receipts
                .iter()
                .map(|receipt| &receipt.receipt)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        let mut stores: Vec<PathBuf> = fs::read_dir(self.home.store_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|store| {
                // Spelled as the ledger's JSON spells it, so a Windows
                // path's separators compare as written there.
                let spelled = serde_json::to_string(&store.to_string_lossy()).unwrap_or_default();
                let spelled = spelled.trim_matches('"');
                named.contains(&format!("{spelled}/")) || named.contains(&format!("{spelled}\\\\"))
            })
            .collect();
        if stores.is_empty() {
            // Nothing names one: every store is kept rather than guessed at.
            stores.push(self.home.store_dir());
        }
        stores.sort();
        stores
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
            // A reader that would not detach still reads what follows it:
            // a plugin its marketplace, any entry the builds it names.
            let mut plugin_kept = false;
            let mut reader_failed = false;
            // What the plan leaves reads the builds as well.
            let mut reader_kept = !harness.keeps.is_empty();
            for receipt in order {
                let reads = Reads::of(&receipt.receipt.artifact);
                if !withdrawn && reads > Reads::Delivery {
                    withdrawn = true;
                    if !plugin_kept {
                        self.withdraw(integration, harness, report, &mut failures);
                    }
                }
                if reads == Reads::Material && reader_kept {
                    continue;
                }
                if receipt.fate == ReceiptFate::Gone {
                    state::forget_receipt(&self.home, &receipt.receipt)?;
                    continue;
                }
                let failure = match integration.detach_receipt(&receipt.receipt) {
                    Ok(AttachmentInspection {
                        state: AttachmentState::Missing,
                        ..
                    }) => {
                        state::forget_receipt(&self.home, &receipt.receipt)?;
                        report
                            .detached
                            .push((harness.name.clone(), receipt.subject.clone()));
                        continue;
                    }
                    Ok(inspection) => format!(
                        "{} on {}: {}",
                        receipt.subject, harness.name, inspection.reason
                    ),
                    Err(error) => format!("{} on {}: {error}", receipt.subject, harness.name),
                };
                failures.push(failure);
                if reads != Reads::Material {
                    reader_kept = true;
                    reader_failed = true;
                    plugin_kept |= installed_through_a_marketplace(&receipt.receipt.artifact);
                }
            }
            if !withdrawn && !plugin_kept {
                self.withdraw(integration, harness, report, &mut failures);
            }
            if reader_failed {
                failures.push(format!(
                    "what {} still reads of uze's is kept until that is removed",
                    harness.name
                ));
            }
        }
        // A harness no receipt names any more may still list UZE's
        // marketplace, when an earlier run forgot its receipts and then
        // failed to withdraw it: asked of every harness whose CLI is here,
        // since only a root inside the home is ever withdrawn.
        for integration in &self.integrations {
            if plan
                .harnesses
                .iter()
                .any(|harness| harness.id == integration.id())
                || !integration.detect().present
            {
                continue;
            }
            let harness = HarnessUninstall {
                id: integration.id().to_owned(),
                name: integration.display_name().to_owned(),
                cli_present: true,
                receipts: Vec::new(),
                keeps: Vec::new(),
            };
            self.withdraw(integration.as_ref(), &harness, report, &mut failures);
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
            || harness.receipts.iter().any(|receipt| {
                receipt.fate == ReceiptFate::Leave
                    && installed_through_a_marketplace(&receipt.receipt.artifact)
            })
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

    /// Removes every entry of the home but `kept` and the directories
    /// that hold them.
    fn remove_all_but(&self, kept: &[PathBuf], report: &mut UninstallReport) -> Result<()> {
        let mut pending = vec![self.home.root().to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).into_iter().flatten().flatten() {
                let path = entry.path();
                if kept.contains(&path) {
                    continue;
                }
                if kept.iter().any(|keep| keep.starts_with(&path)) {
                    pending.push(path);
                    continue;
                }
                remove_path(&path)?;
                report.removed.push(path);
            }
        }
        Ok(())
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

    fn binary_plan(&self) -> (BinaryPlan, Option<LeftBinary>) {
        let receipt = fs::read(self.home.install_receipt_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InstallReceipt>(&bytes).ok());
        binary_plan(receipt, std::env::current_exe().ok())
    }
}

/// Whose the binary is to remove: the one the installer's `receipt` names
/// while it exists, else the `running` one, left to whatever put it there.
/// When both exist and differ, the running one is left and named too
/// (decision U6), so a person is never told uze is gone while it is not.
fn binary_plan(
    receipt: Option<InstallReceipt>,
    running: Option<PathBuf>,
) -> (BinaryPlan, Option<LeftBinary>) {
    let left = |path: PathBuf| LeftBinary {
        remove_with: removal_command(&path),
        path,
    };
    match receipt.filter(|receipt| receipt.binary.exists()) {
        Some(receipt) => {
            let same = |path: &Path| {
                uze_platform::path::canonical(path).ok()
                    == uze_platform::path::canonical(&receipt.binary).ok()
            };
            let running = running.filter(|path| !same(path)).map(left);
            (
                BinaryPlan::Placed {
                    path: receipt.binary,
                    path_entry: receipt.path_entry,
                },
                running,
            )
        }
        None => match running {
            Some(path) => {
                let LeftBinary { path, remove_with } = left(path);
                (BinaryPlan::Elsewhere { path, remove_with }, None)
            }
            None => (BinaryPlan::Unknown, None),
        },
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

/// Whether `artifact` is a plugin the harness installed through UZE's
/// marketplace, which withdrawing that marketplace would break.
fn installed_through_a_marketplace(artifact: &ManagedArtifact) -> bool {
    matches!(
        artifact,
        ManagedArtifact::IntegrationOwned { kind, .. } if kind != "build" && kind != "rules-engine"
    )
}

/// Whether `artifact`, left where it is, still reads a build in its
/// harness's vendor store: a plugin installed from it, an entry or hook
/// that runs from it, or the build itself. A copy in a discovery root
/// reads nothing there.
fn reads_builds(artifact: &ManagedArtifact) -> bool {
    match artifact {
        ManagedArtifact::IntegrationOwned { .. }
        | ManagedArtifact::VendorConfigEntry { .. }
        | ManagedArtifact::ManagedTextRegion { .. }
        | ManagedArtifact::ManagedHookFile { .. }
        | ManagedArtifact::HookConfigEntry { .. } => true,
        ManagedArtifact::SymlinkReference { .. }
        | ManagedArtifact::GeneratedFile { .. }
        | ManagedArtifact::GeneratedTree { .. } => false,
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
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use uze_core::{
        UzeHome,
        exposure::{ExposureMechanism, ExposurePlan},
        integration::{HarnessDetection, VendorRegistration, VendorRegistrationKind},
        provisioning::ProvisioningResult,
        router::{CompatibilityRoute, HarnessCapabilities},
    };

    use super::*;

    /// What a scripted harness answers, and what it was asked, in order.
    #[derive(Default)]
    struct Script {
        /// What inspecting the receipt at a location answers: matched
        /// unless named here.
        states: BTreeMap<PathBuf, AttachmentState>,
        /// Locations whose detach the harness answers as still there.
        holds_on_to: Vec<PathBuf>,
        registrations: Vec<VendorRegistration>,
        /// Whether preparing it again moves its registrations off the
        /// generated tree.
        moves_to_its_stores: bool,
        asked: Vec<String>,
    }

    struct Scripted {
        id: &'static str,
        name: &'static str,
        present: bool,
        script: Arc<Mutex<Script>>,
    }

    impl IntegrationPort for Scripted {
        fn id(&self) -> &'static str {
            self.id
        }

        fn display_name(&self) -> &'static str {
            self.name
        }

        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities::default()
        }

        fn exposure_plan(&self, _resource: &uze_core::capability::Resource) -> ExposurePlan {
            ExposurePlan {
                route: CompatibilityRoute::Adaptable,
                mechanism: ExposureMechanism::Unsupported {
                    rationale: "scripted".to_owned(),
                },
                evidence: "scripted".to_owned(),
            }
        }

        fn detect(&self) -> HarnessDetection {
            HarnessDetection {
                present: self.present,
                version: None,
            }
        }

        fn install(&self, _home: &UzeHome, _detection: &HarnessDetection) -> Result<()> {
            let mut script = self.script.lock().unwrap();
            script.asked.push("install".to_owned());
            if script.moves_to_its_stores {
                script.registrations.clear();
            }
            Ok(())
        }

        fn inspect_receipt(&self, receipt: &AttachmentReceipt) -> AttachmentInspection {
            let state = self
                .script
                .lock()
                .unwrap()
                .states
                .get(&receipt.artifact.location())
                .copied()
                .unwrap_or(AttachmentState::Matched);
            AttachmentInspection {
                state,
                reason: format!("scripted as {state:?}"),
            }
        }

        fn detach_receipt(&self, receipt: &AttachmentReceipt) -> Result<AttachmentInspection> {
            let location = receipt.artifact.location();
            let mut script = self.script.lock().unwrap();
            script.asked.push(format!("detach {}", location.display()));
            if script.holds_on_to.contains(&location) {
                return Ok(AttachmentInspection {
                    state: AttachmentState::Drifted,
                    reason: "edited since".to_owned(),
                });
            }
            script.states.insert(location, AttachmentState::Missing);
            Ok(AttachmentInspection {
                state: AttachmentState::Missing,
                reason: String::new(),
            })
        }

        fn withdraw_registrations(&self) -> Result<Vec<String>> {
            self.script
                .lock()
                .unwrap()
                .asked
                .push("withdraw".to_owned());
            Ok(vec!["uze".to_owned()])
        }

        fn registrations(&self, _read: RegistrationRead) -> Vec<VendorRegistration> {
            self.script.lock().unwrap().registrations.clone()
        }
    }

    /// A scratch UZE home and the harnesses it knows, each scripted.
    struct Machine {
        root: PathBuf,
        home: UzeHome,
        scripts: Vec<Arc<Mutex<Script>>>,
        app: UzeApplication,
    }

    impl Machine {
        /// `harnesses` names each harness by id, display name and whether
        /// its CLI is here.
        fn new(label: &str, harnesses: &[(&'static str, &'static str, bool)]) -> Self {
            let root = uze_testkit::temp::scratch(label);
            let home = UzeHome::at(root.join("uze"));
            home.ensure_layout().unwrap();
            let scripts: Vec<Arc<Mutex<Script>>> = harnesses
                .iter()
                .map(|_| Arc::new(Mutex::new(Script::default())))
                .collect();
            let integrations: Vec<Box<dyn IntegrationPort>> = harnesses
                .iter()
                .zip(&scripts)
                .map(|(&(id, name, present), script)| {
                    Box::new(Scripted {
                        id,
                        name,
                        present,
                        script: script.clone(),
                    }) as Box<dyn IntegrationPort>
                })
                .collect();
            let app = UzeApplication::new(home.clone(), integrations);
            Self {
                root,
                home,
                scripts,
                app,
            }
        }

        fn script(&self, at: usize) -> std::sync::MutexGuard<'_, Script> {
            self.scripts[at].lock().unwrap()
        }

        fn asked(&self, at: usize) -> Vec<String> {
            self.script(at).asked.clone()
        }

        fn deliver(&self, receipt: AttachmentReceipt) -> AttachmentReceipt {
            state::record_receipt(&self.home, receipt.clone()).unwrap();
            receipt
        }

        fn receipts(&self) -> Vec<AttachmentReceipt> {
            state::receipts(&self.home, None).unwrap()
        }

        fn purge(&self) -> Result<UninstallReport> {
            let plan = self.app.uninstall_plan(UninstallMode::Purge);
            self.app.uninstall(&plan, &|| Ok(()))
        }

        fn program(&self) -> Result<UninstallReport> {
            let plan = self.app.uninstall_plan(UninstallMode::Program);
            self.app.uninstall(&plan, &|| Ok(()))
        }
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn owned(integration: &str, package: &str, kind: &str, selector: &str) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: package.to_owned(),
            // One receipt per package and identity: a build has its own.
            resource_identity: (kind == uze_core::integration::BUILD_KIND)
                .then(|| uze_core::integration::BUILD_IDENTITY.to_owned()),
            integration: integration.to_owned(),
            artifact: ManagedArtifact::IntegrationOwned {
                kind: kind.to_owned(),
                selector: selector.to_owned(),
                detail: BTreeMap::new(),
            },
        }
    }

    /// A plugin the harness installed from UZE's marketplace.
    fn plugin(integration: &str, selector: &str) -> AttachmentReceipt {
        owned(integration, selector, "plugin", selector)
    }

    /// A plugin installed from a build under `root`.
    fn plugin_reading(integration: &str, selector: &str, root: &Path) -> AttachmentReceipt {
        let mut receipt = plugin(integration, selector);
        if let ManagedArtifact::IntegrationOwned { detail, .. } = &mut receipt.artifact {
            detail.insert(
                "root".to_owned(),
                serde_json::Value::String(root.join("build").to_string_lossy().into_owned()),
            );
        }
        receipt
    }

    fn server(integration: &str, name: &str) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: "demo@uze".to_owned(),
            resource_identity: Some(format!("demo@uze/mcp/{name}")),
            integration: integration.to_owned(),
            artifact: ManagedArtifact::VendorConfigEntry {
                entry_name: name.to_owned(),
                transport: "stdio".to_owned(),
                command: PathBuf::from("server"),
                args: Vec::new(),
                cwd: None,
                environment: Vec::new(),
                enabled: None,
            },
        }
    }

    fn link(integration: &str, path: &Path) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: "demo@uze".to_owned(),
            resource_identity: Some("demo@uze/skill/demo".to_owned()),
            integration: integration.to_owned(),
            artifact: ManagedArtifact::SymlinkReference {
                path: path.to_path_buf(),
                target: path.with_extension("target"),
            },
        }
    }

    fn location(receipt: &AttachmentReceipt) -> String {
        format!("detach {}", receipt.artifact.location().display())
    }

    fn placed() -> BinaryPlan {
        BinaryPlan::Placed {
            path: PathBuf::from("/opt/uze/bin/uze"),
            path_entry: Some(PathBuf::from("/opt/uze/bin")),
        }
    }

    #[test]
    fn something_started_after_the_plan_stops_uninstall_before_it_removes_anything() {
        let root = uze_testkit::temp::scratch("uninstall-still-clear");
        let home = uze_core::UzeHome::at(root.join("uze"));
        fs::create_dir_all(home.runtime_dir()).unwrap();
        let app = UzeApplication::new(home.clone(), Vec::new());
        let plan = app.uninstall_plan(UninstallMode::Program);

        let refused = app.uninstall(&plan, &|| {
            Err(UzeError::UninstallRefused {
                reason: "still running in the workspace: 1 agent".to_owned(),
                next: "end the agents, then uze workspace stop".to_owned(),
            })
        });

        assert!(
            matches!(refused, Err(UzeError::UninstallRefused { .. })),
            "{refused:?}"
        );
        assert!(home.runtime_dir().is_dir(), "something was removed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_running_binary_other_than_the_installers_is_left_and_named() {
        let root = uze_testkit::temp::scratch("uninstall-two-binaries");
        let placed = root.join("installed/uze");
        let running = root.join("cargo/bin/uze");
        for binary in [&placed, &running] {
            fs::create_dir_all(binary.parent().unwrap()).unwrap();
            fs::write(binary, "").unwrap();
        }
        let receipt = InstallReceipt {
            binary: placed.clone(),
            path_entry: None,
        };

        let (plan, left) = binary_plan(Some(receipt), Some(running.clone()));

        assert!(
            matches!(&plan, BinaryPlan::Placed { path, .. } if *path == placed),
            "{plan:?}"
        );
        assert_eq!(left.map(|left| left.path), Some(running));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_installers_binary_running_is_not_named_twice() {
        let root = uze_testkit::temp::scratch("uninstall-one-binary");
        let placed = root.join("installed/uze");
        fs::create_dir_all(placed.parent().unwrap()).unwrap();
        fs::write(&placed, "").unwrap();
        let receipt = InstallReceipt {
            binary: placed.clone(),
            path_entry: None,
        };

        let (_, left) = binary_plan(Some(receipt), Some(placed));

        assert_eq!(left, None);
        let _ = fs::remove_dir_all(&root);
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

    #[test]
    fn a_purge_takes_deliveries_back_readers_first_and_withdraws_between() {
        let machine = Machine::new("uninstall-purge-order", &[("alpha", "Alpha", true)]);
        let store = machine.home.store_dir().join("demo-1");
        let build = machine.deliver(owned("alpha", "demo@uze", "build", "demo"));
        let entry = machine.deliver(server("alpha", "demo-server"));
        let installed = machine.deliver(plugin_reading("alpha", "demo@uze", &store));

        let report = machine.purge().unwrap();

        assert_eq!(
            machine.asked(0),
            vec![
                location(&installed),
                "withdraw".to_owned(),
                location(&entry),
                location(&build),
            ]
        );
        assert!(machine.receipts().is_empty(), "{:?}", machine.receipts());
        assert_eq!(report.detached.len(), 3, "{:?}", report.detached);
        assert_eq!(
            report.withdrawn,
            vec![("Alpha".to_owned(), "uze".to_owned())]
        );
        assert!(report.home_removed);
        assert_eq!(report.removed, vec![machine.home.root().to_path_buf()]);
        assert!(!machine.home.root().exists());
    }

    #[test]
    fn a_harness_holding_only_plugins_is_withdrawn_from_once_they_are_gone() {
        let machine = Machine::new("uninstall-purge-plugins", &[("alpha", "Alpha", true)]);
        let installed = machine.deliver(plugin("alpha", "demo@uze"));

        machine.purge().unwrap();

        assert_eq!(
            machine.asked(0),
            vec![location(&installed), "withdraw".to_owned()]
        );
    }

    #[test]
    fn a_plugin_that_will_not_come_off_keeps_its_marketplace_and_the_builds_it_reads() {
        let machine = Machine::new("uninstall-purge-held", &[("alpha", "Alpha", true)]);
        let build = machine.deliver(owned("alpha", "demo@uze", "build", "demo"));
        let entry = machine.deliver(server("alpha", "demo-server"));
        let installed = machine.deliver(plugin("alpha", "demo@uze"));
        machine
            .script(0)
            .holds_on_to
            .push(installed.artifact.location());

        let stopped = machine.purge();

        let Err(UzeError::LifecycleBlocked(message)) = &stopped else {
            panic!("the purge went on: {stopped:?}");
        };
        assert!(message.contains("Alpha still reads"), "{message}");
        assert_eq!(
            machine.asked(0),
            vec![location(&installed), location(&entry)]
        );
        let left = machine.receipts();
        assert!(
            left.contains(&installed) && left.contains(&build),
            "{left:?}"
        );
        assert!(!left.contains(&entry), "{left:?}");
        assert!(machine.home.root().is_dir());
    }

    #[test]
    fn a_delivery_already_gone_is_forgotten_without_asking_the_harness() {
        let machine = Machine::new("uninstall-purge-gone", &[("alpha", "Alpha", true)]);
        let installed = machine.deliver(plugin("alpha", "demo@uze"));
        machine
            .script(0)
            .states
            .insert(installed.artifact.location(), AttachmentState::Missing);

        machine.purge().unwrap();

        assert_eq!(machine.asked(0), vec!["withdraw".to_owned()]);
    }

    #[test]
    fn a_purge_withdraws_from_every_harness_here_whether_it_holds_receipts_or_not() {
        let machine = Machine::new(
            "uninstall-purge-every-harness",
            &[
                ("alpha", "Alpha", true),
                ("beta", "Beta", true),
                ("gamma", "Gamma", false),
            ],
        );
        let installed = machine.deliver(plugin("alpha", "demo@uze"));

        machine.purge().unwrap();

        assert_eq!(
            machine.asked(0),
            vec![location(&installed), "withdraw".to_owned()]
        );
        assert_eq!(machine.asked(1), vec!["withdraw".to_owned()]);
        assert!(machine.asked(2).is_empty(), "{:?}", machine.asked(2));
    }

    #[test]
    fn a_purge_keeps_only_the_store_a_plugin_left_behind_still_reads() {
        let machine = Machine::new("uninstall-purge-keeps", &[("alpha", "Alpha", true)]);
        let read = machine.home.store_dir().join("demo-1");
        let unread = machine.home.store_dir().join("other-2");
        fs::create_dir_all(read.join("build")).unwrap();
        fs::create_dir_all(&unread).unwrap();
        let installed = machine.deliver(plugin_reading("alpha", "demo@uze", &read));
        machine
            .script(0)
            .states
            .insert(installed.artifact.location(), AttachmentState::Drifted);

        let report = machine.purge().unwrap();

        assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
        assert_eq!(report.left.len(), 1, "{:?}", report.left);
        assert!(read.join("build").is_dir());
        assert!(!unread.exists());
        assert!(!machine.home.runtime_dir().exists());
        assert!(!machine.home.state_dir().exists());
        assert!(!report.home_removed);
        assert!(report.kept.contains(&read), "{:?}", report.kept);
    }

    #[test]
    fn a_copy_left_in_a_discovery_root_keeps_no_store() {
        let machine = Machine::new("uninstall-purge-copy-left", &[("alpha", "Alpha", true)]);
        let copy = machine.deliver(link("alpha", &machine.root.join("skills").join("demo")));
        machine.deliver(plugin("alpha", "demo@uze"));
        machine
            .script(0)
            .states
            .insert(copy.artifact.location(), AttachmentState::Drifted);

        let plan = machine.app.uninstall_plan(UninstallMode::Purge);

        assert_eq!(plan.harnesses.len(), 1);
        assert!(
            plan.harnesses[0].keeps.is_empty(),
            "{:?}",
            plan.harnesses[0].keeps
        );
    }

    #[test]
    fn a_purge_plan_groups_receipts_by_harness_and_leaves_what_cannot_be_asked() {
        let machine = Machine::new(
            "uninstall-plan-harnesses",
            &[("alpha", "Alpha", true), ("beta", "Beta", false)],
        );
        machine.deliver(plugin("alpha", "one@uze"));
        machine.deliver(plugin("beta", "two@uze"));
        machine.deliver(plugin("alpha", "three@uze"));
        machine.deliver(plugin("ghost", "four@uze"));

        let plan = machine.app.uninstall_plan(UninstallMode::Purge);

        let ids: Vec<&str> = plan.harnesses.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "beta", "ghost"]);
        let alpha = &plan.harnesses[0];
        assert!(alpha.cli_present);
        let subjects: Vec<&str> = alpha.receipts.iter().map(|r| r.subject.as_str()).collect();
        assert_eq!(subjects, ["one@uze", "three@uze"]);
        assert!(alpha.receipts.iter().all(|r| r.fate == ReceiptFate::Detach));
        let beta = &plan.harnesses[1];
        assert!(!beta.cli_present);
        assert_eq!(beta.receipts[0].fate, ReceiptFate::Leave);
        assert!(
            beta.receipts[0].reason.contains("not installed"),
            "{}",
            beta.receipts[0].reason
        );
        let ghost = &plan.harnesses[2];
        assert_eq!(ghost.name, "ghost");
        assert_eq!(ghost.receipts[0].fate, ReceiptFate::Leave);
        let left: Vec<(&str, &str)> = plan
            .left()
            .map(|(harness, receipt)| (harness.id.as_str(), receipt.subject.as_str()))
            .collect();
        assert_eq!(left, [("beta", "two@uze"), ("ghost", "four@uze")]);
    }

    #[test]
    fn the_program_plan_keeps_every_delivery_and_asks_no_harness() {
        let machine = Machine::new("uninstall-plan-program", &[("beta", "Beta", false)]);
        machine.deliver(plugin("beta", "two@uze"));

        let plan = machine.app.uninstall_plan(UninstallMode::Program);

        assert!(plan.harnesses[0].cli_present);
        assert_eq!(plan.harnesses[0].receipts[0].fate, ReceiptFate::Keep);
        assert_eq!(plan.left().count(), 0);
    }

    #[test]
    fn the_program_uninstall_removes_the_generated_tier_and_keeps_what_a_reinstall_adopts() {
        let machine = Machine::new("uninstall-program", &[("alpha", "Alpha", true)]);
        machine.deliver(plugin("alpha", "demo@uze"));
        fs::create_dir_all(machine.home.store_dir().join("demo-1")).unwrap();
        let foreign = machine.home.root().join("notes");
        fs::write(&foreign, "mine").unwrap();

        let plan = machine.app.uninstall_plan(UninstallMode::Program);
        let notes = plan
            .entries
            .iter()
            .find(|entry| entry.path == foreign)
            .unwrap();
        assert_eq!((notes.holds, notes.remove), ("not uze's", false));
        let report = machine.app.uninstall(&plan, &|| Ok(())).unwrap();

        assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
        assert!(!machine.home.runtime_dir().exists());
        assert!(!machine.home.cache_dir().exists());
        assert!(machine.home.store_dir().join("demo-1").is_dir());
        assert!(machine.home.state_dir().is_dir());
        assert!(foreign.is_file());
        assert!(
            report.removed.contains(&machine.home.runtime_dir()),
            "{:?}",
            report.removed
        );
        assert!(
            report.kept.contains(&machine.home.store_dir()),
            "{:?}",
            report.kept
        );
        assert_eq!(machine.receipts().len(), 1);
    }

    #[test]
    fn a_purge_marks_every_home_entry_for_removal() {
        let machine = Machine::new("uninstall-plan-purge-entries", &[]);
        fs::write(machine.home.root().join("notes"), "mine").unwrap();

        let plan = machine.app.uninstall_plan(UninstallMode::Purge);

        assert!(!plan.entries.is_empty());
        assert!(
            plan.entries.iter().all(|entry| entry.remove),
            "{:?}",
            plan.entries
        );
    }

    /// A harness still pointed at a marketplace under the generated tree.
    fn reads_the_generated_tree(machine: &Machine, at: usize) {
        machine.script(at).registrations.push(VendorRegistration {
            kind: VendorRegistrationKind::Marketplace,
            name: "uze".to_owned(),
            expected: machine.home.store_dir(),
            registered: Some(machine.home.runtime_dir().join("marketplace")),
        });
    }

    #[test]
    fn a_harness_still_reading_the_generated_tree_stops_the_program_uninstall() {
        let machine = Machine::new("uninstall-program-refused", &[("alpha", "Alpha", true)]);
        reads_the_generated_tree(&machine, 0);

        let refused = machine.program();

        let Err(UzeError::UninstallRefused { reason, .. }) = &refused else {
            panic!("the uninstall went on: {refused:?}");
        };
        assert!(reason.contains("Alpha"), "{reason}");
        assert!(machine.home.runtime_dir().is_dir());
    }

    #[test]
    fn a_harness_moved_to_its_stores_on_the_way_lets_the_program_uninstall_go_on() {
        let machine = Machine::new("uninstall-program-converges", &[("alpha", "Alpha", true)]);
        reads_the_generated_tree(&machine, 0);
        machine.script(0).moves_to_its_stores = true;

        machine.program().unwrap();

        assert!(machine.asked(0).contains(&"install".to_owned()));
        assert!(!machine.home.runtime_dir().exists());
    }

    #[test]
    fn a_purge_does_not_wait_for_a_harness_to_leave_the_generated_tree() {
        let machine = Machine::new("uninstall-purge-generated", &[("alpha", "Alpha", true)]);
        reads_the_generated_tree(&machine, 0);

        let report = machine.purge().unwrap();

        assert!(report.home_removed);
    }

    #[test]
    fn a_receipt_naming_the_package_copies_counts_against_its_own_harness() {
        let machine = Machine::new(
            "uninstall-runtime-receipts",
            &[("alpha", "Alpha", true), ("beta", "Beta", true)],
        );
        let copies = crate::application::earlier::package_copies_dir(&machine.home);
        machine.deliver(link("alpha", &copies.join("demo")));

        assert_eq!(
            machine.app.reads_the_runtime_tree(),
            vec!["Alpha".to_owned()]
        );
    }

    #[test]
    fn only_a_harness_setup_installed_is_named_as_provisioned() {
        let machine = Machine::new(
            "uninstall-provisioned",
            &[("alpha", "Alpha", true), ("beta", "Beta", true)],
        );
        let detection = HarnessDetection {
            present: true,
            version: None,
        };
        let installed =
            ProvisioningResult::verified(ProvisionAction::Install, "official", detection.clone());
        let updated = ProvisioningResult::verified(ProvisionAction::Update, "official", detection);
        state::record_provisioning(&machine.home, "alpha", &installed).unwrap();
        state::record_provisioning(&machine.home, "beta", &updated).unwrap();

        let plan = machine.app.uninstall_plan(UninstallMode::Program);

        assert_eq!(plan.provisioned, vec!["Alpha".to_owned()]);
    }

    // Unix reads a link's target back as it was written; a Windows junction
    // reads back in its verbatim form, which this comparison does not model.
    #[cfg(unix)]
    #[test]
    fn a_project_is_listed_by_the_links_it_holds_into_the_home() {
        let machine = Machine::new("uninstall-projects", &[]);
        let linked = machine.root.join("linked");
        let plain = machine.root.join("plain");
        let worktrees = linked.join(uze_workspace::worktree::WORKTREES_DIRECTORY);
        fs::create_dir_all(&worktrees).unwrap();
        fs::create_dir_all(worktrees.join("elsewhere")).unwrap();
        fs::create_dir_all(&plain).unwrap();
        let into_home = worktrees.join("a1~uze");
        uze_platform::fs::symlink(&machine.home.runtime_dir(), &into_home).unwrap();
        let outside = worktrees.join("b2~uze");
        uze_platform::fs::symlink(&machine.root, &outside).unwrap();
        for project in [&linked, &plain] {
            uze_core::record::ensure(&machine.home, project).unwrap();
        }

        let plan = machine.app.uninstall_plan(UninstallMode::Program);

        assert_eq!(plan.projects.len(), 1, "{:?}", plan.projects);
        assert_eq!(
            uze_platform::path::canonical(&plan.projects[0].root).unwrap(),
            uze_platform::path::canonical(&linked).unwrap()
        );
        assert_eq!(plan.projects[0].links, vec![into_home]);
    }

    #[test]
    fn only_a_checkout_holding_commits_its_target_lacks_is_named_as_unlanded() {
        let repository = uze_testkit::git::Repository::new("uninstall-unlanded");
        let project = uze_platform::path::canonical(repository.root()).unwrap();
        let machine = Machine::new("uninstall-unlanded-home", &[]);
        let place = || {
            machine
                .app
                .workspace()
                .place_new_agent(
                    &project,
                    Some(crate::application::services::PlacementKind::Isolated),
                    "keeper",
                    &[],
                )
                .unwrap()
                .cwd
        };
        let worked = place();
        let idle = place();
        fs::write(worked.join("work.txt"), "unlanded").unwrap();
        repository.git_in(&worked, &["add", "work.txt"]);
        repository.git_in(&worked, &["commit", "--quiet", "-m", "work"]);

        let plan = machine.app.uninstall_plan(UninstallMode::Program);

        let unlanded: Vec<(PathBuf, usize)> = plan
            .projects
            .iter()
            .flat_map(|project| &project.unlanded)
            .map(|checkout| {
                (
                    uze_platform::path::canonical(&checkout.path).unwrap(),
                    checkout.commits,
                )
            })
            .collect();
        assert_eq!(
            unlanded,
            vec![(uze_platform::path::canonical(&worked).unwrap(), 1)],
            "idle at {}",
            idle.display()
        );
    }

    fn empty_plan() -> UninstallPlan {
        UninstallPlan {
            mode: UninstallMode::Program,
            home: PathBuf::from("uze"),
            home_present: false,
            harnesses: Vec::new(),
            entries: Vec::new(),
            projects: Vec::new(),
            provisioned: Vec::new(),
            binary: BinaryPlan::Unknown,
            running: None,
        }
    }

    #[test]
    fn a_plan_is_empty_only_with_no_home_no_links_and_no_placed_binary() {
        assert!(empty_plan().is_empty());
        assert!(
            !UninstallPlan {
                home_present: true,
                ..empty_plan()
            }
            .is_empty()
        );
        assert!(
            !UninstallPlan {
                projects: vec![ProjectWork {
                    links: vec![PathBuf::from("a~uze")],
                    ..ProjectWork::default()
                }],
                ..empty_plan()
            }
            .is_empty()
        );
        assert!(
            !UninstallPlan {
                binary: placed(),
                ..empty_plan()
            }
            .is_empty()
        );
    }

    #[test]
    fn a_report_needs_the_person_only_when_a_step_is_left_to_them() {
        let mut report = UninstallReport {
            mode: UninstallMode::Program,
            home: PathBuf::from("uze"),
            detached: Vec::new(),
            withdrawn: Vec::new(),
            left: Vec::new(),
            links_removed: Vec::new(),
            removed: Vec::new(),
            kept: Vec::new(),
            home_removed: false,
            binary: BinaryOutcome::Unknown,
            running: None,
            manual: Vec::new(),
            projects: Vec::new(),
            provisioned: Vec::new(),
        };
        assert!(!report.needs_the_person());

        report.manual.push("remove it yourself".to_owned());

        assert!(report.needs_the_person());
    }

    #[test]
    fn a_purge_undoes_deliveries_then_entries_and_builds_last() {
        let build = owned("alpha", "demo@uze", "build", "demo").artifact;
        let engine = owned("alpha", "demo@uze", "rules-engine", "rules").artifact;
        let installed = plugin("alpha", "demo@uze").artifact;
        let entry = server("alpha", "demo").artifact;

        assert_eq!(Reads::of(&build), Reads::Material);
        assert_eq!(Reads::of(&engine), Reads::Entry);
        assert_eq!(Reads::of(&installed), Reads::Delivery);
        assert_eq!(Reads::of(&entry), Reads::Entry);
    }

    #[test]
    fn only_a_plugin_is_installed_through_the_marketplace() {
        assert!(installed_through_a_marketplace(
            &plugin("alpha", "demo@uze").artifact
        ));
        for kind in ["build", "rules-engine"] {
            assert!(!installed_through_a_marketplace(
                &owned("alpha", "demo@uze", kind, "demo").artifact
            ));
        }
    }

    #[test]
    fn the_command_that_removes_a_binary_is_read_from_where_it_was_put() {
        let cargo = removal_command(Path::new("/home/someone/.cargo/bin/uze"));
        assert_eq!(cargo.as_deref(), Some("cargo uninstall uze"));
        assert_eq!(
            removal_command(Path::new("/usr/local/Cellar/uze/1.0/bin/uze")).as_deref(),
            Some("brew uninstall uze")
        );
        assert_eq!(
            removal_command(Path::new("/opt/homebrew/bin/uze")).as_deref(),
            Some("brew uninstall uze")
        );
        assert_eq!(removal_command(Path::new("/opt/tools/bin/uze")), None);
        assert_eq!(removal_command(Path::new("/home/someone/.cargo/uze")), None);
    }

    #[test]
    fn removing_a_path_takes_a_file_or_a_tree_and_accepts_one_already_gone() {
        let root = uze_testkit::temp::scratch("uninstall-remove-path");
        let file = root.join("file");
        let tree = root.join("tree");
        fs::write(&file, "x").unwrap();
        fs::create_dir_all(tree.join("inner")).unwrap();
        fs::write(tree.join("inner/file"), "x").unwrap();

        remove_path(&file).unwrap();
        remove_path(&tree).unwrap();
        remove_path(&root.join("never-there")).unwrap();

        assert!(!file.exists() && !tree.exists());
        let _ = fs::remove_dir_all(&root);
    }

    // POSIX permissions make a removal fail here; Windows has no read-only
    // directory that refuses an unlink the same way.
    #[cfg(unix)]
    #[test]
    fn a_path_that_cannot_be_removed_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let root = uze_testkit::temp::scratch("uninstall-remove-refused");
        let sealed = root.join("sealed");
        fs::create_dir_all(&sealed).unwrap();
        let file = sealed.join("file");
        fs::write(&file, "x").unwrap();
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o555)).unwrap();
        // An account that ignores permissions cannot be refused at all.
        let ignores_permissions = fs::write(sealed.join("probe"), "x").is_ok();

        let removed = remove_path(&file);

        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o755)).unwrap();
        if !ignores_permissions {
            assert!(
                matches!(removed, Err(UzeError::Write { .. })),
                "{removed:?}"
            );
            assert!(file.exists());
        }
        let _ = fs::remove_dir_all(&root);
    }
}
