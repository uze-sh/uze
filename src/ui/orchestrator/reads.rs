//! Every read and write the workspace client makes off the frame's thread: each `spawn_*` runs one on a thread of its own and answers through a channel with a `*Resolution` carrying the question it was asked, so an answer that arrives after the viewer moved on is dropped rather than drawn. The one file under `orchestrator/` that reaches the application or the extension host, and only inside a `thread::spawn`.

use super::*;

/// Which harness, resolved against which directory. Both halves are the
/// identity of one support resolution: the same agent open in two panes
/// sitting in two different projects has two different answers, and a
/// resolution computed for one must never be shown for the other. This is
/// the whole reason the old session-wide "resolve once at the attach root"
/// read was wrong.
pub(super) type SupportKey = (String, PathBuf);

/// A finished resolution, tagged with the key it answers. `support` is
/// `None` when the read failed outright — kept as a resolved-but-empty
/// answer rather than dropped, so a failing read cannot spin the refresh
/// loop by looking forever unresolved.
pub(super) struct SupportResolution {
    pub(super) key: SupportKey,
    pub(super) support: Option<crate::ui::agent_support::AgentSupport>,
}

/// Computes one agent's support read model in a background thread and
/// delivers it through `sender`.
///
/// Everything about the answer comes from `key`: the harness the pane is
/// actually running, and that pane's own working directory. The
/// application resolves the project from there
/// (`UzeApplication::agent_context_for`), the same way the runtime shim
/// resolves it when it execs the harness from that directory — so the
/// popup reports the delivery a launch here would really perform, not the
/// one that would have happened wherever `uze` itself was started.
/// Runs a background read, answering with `silence` if it panicked.
///
/// Every read in this file reserves a key before it starts and releases
/// it when the answer lands, so a thread that unwinds without answering
/// leaves that feature dead for the rest of the session — the surface
/// still believes a read is out, and asks for nothing more. The panic
/// itself goes to the log: the hook `ui::run` installs leaves the terminal
/// alone for any thread but the one that draws. What this adds is that
/// the *client* carries on, which matters because several of these run
/// code over whatever a repository happens to contain.
///
/// `silence` is what the read would have said had it found nothing —
/// every absorber already draws it.
pub(in crate::ui) fn answered_or<T>(read: impl FnOnce() -> T, silence: T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)).unwrap_or(silence)
}

pub(super) fn spawn_support_refresh(
    home: &UzeHome,
    key: SupportKey,
    sender: mpsc::Sender<SupportResolution>,
) {
    let support_home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.support_refresh", parent: &parent);
        let support = answered_or(
            || {
                crate::ui::tui_application(support_home)
                    .ok()
                    .and_then(|app| {
                        let context = app.context().agent_context_for(&key.0, &key.1).ok()?;
                        let health = app.health().harness(&key.0).ok()?;
                        Some(crate::ui::agent_support::AgentSupport::resolve(
                            health, &context,
                        ))
                    })
            },
            None,
        );
        let _ = sender.send(SupportResolution { key, support });
    });
}

/// Writes back which conversation each live agent is actually in.
///
/// Fire-and-forget: the answer is state on disk that the next launch reads,
/// so nothing comes back to the client and no key is reserved. It runs
/// where the other unbounded reads run — a thread of its own — because one
/// of these can spawn a harness to ask it about its own records.
///
/// This is what keeps a record true while an agent runs: a conversation
/// cleared, forked or switched inside the process is a different identifier
/// in the harness's records, and the launch that recorded the previous one
/// is long over.
pub(super) fn spawn_conversation_refresh(home: &UzeHome, agents: Vec<LaunchedAgent>) {
    if agents.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.conversation_refresh", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        for agent in agents {
            app.workspace().refresh_conversation(
                &agent.integration,
                uze_application::Claim {
                    id: &agent.id,
                    key: &agent.key,
                    cwd: &agent.cwd,
                },
            );
        }
    });
}

/// What keeping one project's `AGENTS.md` in step found that the operator
/// should hear: a workspace section edited by hand, which is left as it is,
/// or one that could not be written. Nothing is sent when it is in step.
pub(super) struct PolicyRegionResolution {
    pub(super) file: PathBuf,
    pub(super) problem: String,
    pub(super) drifted: bool,
}

/// Keeps the workspace's region of each directory's `AGENTS.md` in step
/// with what its project declares, in the primary checkout only. Off the
/// frame like every other repository touch: it reads `agents.yaml`, may ask
/// Git about linked files, and may write the file. A sync with nothing new
/// writes nothing, so asking on the refresh clock is how an edit to
/// `agents.yaml` reaches the file without a command.
pub(super) fn spawn_policy_region_sync(
    home: &UzeHome,
    directories: Vec<PathBuf>,
    sender: mpsc::Sender<PolicyRegionResolution>,
) {
    if directories.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.policy_region_sync", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        let mut seen = std::collections::BTreeSet::new();
        for directory in directories {
            let resolution = match app.workspace().sync_policy_region(&directory) {
                Ok(Some(region)) if !seen.insert(region.file.clone()) => continue,
                Ok(Some(region)) => match region.state {
                    uze_application::AttachmentState::Drifted => PolicyRegionResolution {
                        file: region.file,
                        problem: region.reason,
                        drifted: true,
                    },
                    uze_application::AttachmentState::Blocked => PolicyRegionResolution {
                        file: region.file,
                        problem: region.reason,
                        drifted: false,
                    },
                    _ => continue,
                },
                Ok(None) => continue,
                Err(error) => PolicyRegionResolution {
                    file: directory,
                    problem: error.to_string(),
                    drifted: false,
                },
            };
            let _ = sender.send(resolution);
        }
    });
}

/// Brings each project's target in line with its remote, off the frame:
/// a fetch is a network round trip. One answer per project, however many
/// of the directories belong to it.
pub(super) fn spawn_target_sync(
    home: &UzeHome,
    directories: Vec<PathBuf>,
    sender: mpsc::Sender<TargetSyncReport>,
) {
    if directories.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.target_sync", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        let workspace = app.workspace();
        let mut synced = std::collections::BTreeSet::new();
        for directory in directories {
            let Some(primary) = workspace.primary_of(&directory) else {
                continue;
            };
            if !synced.insert(primary.clone()) {
                continue;
            }
            if let Some(report) = workspace.sync_target(&primary) {
                let _ = sender.send(report);
            }
        }
    });
}

/// A project's gates this machine's shell has no spelling for, and the
/// spelling they lack.
pub(super) struct UnspelledGates {
    pub(super) project: PathBuf,
    pub(super) gates: Vec<String>,
    pub(super) platform: &'static str,
}

/// Reads, where a space first opens, which of its project's gates this
/// machine cannot run: what refuses every delivery from here, said before
/// one is asked for. Off the frame: it reads `agents.yaml`.
pub(super) fn spawn_unspelled_gates(
    home: &UzeHome,
    directories: Vec<PathBuf>,
    sender: mpsc::Sender<UnspelledGates>,
) {
    if directories.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.unspelled_gates", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        for directory in directories {
            let unspelled: Vec<_> = app
                .workspace()
                .steps_not_spelled_here(&directory)
                .into_iter()
                .filter(|step| step.step == uze_application::PolicyStep::Gate)
                .collect();
            let Some(platform) = unspelled.first().map(|step| step.platform) else {
                continue;
            };
            let _ = sender.send(UnspelledGates {
                project: uze_application::slot_key(&directory),
                gates: unspelled.into_iter().map(|step| step.command).collect(),
                platform,
            });
        }
    });
}

/// Asks, for each directory first seen, whether its project's commands
/// wait for the operator's approval. Off the frame: reading the policy
/// asks Git whether the files it links are ignored.
pub(super) fn spawn_commands_awaiting(
    home: &UzeHome,
    directories: Vec<PathBuf>,
    sender: mpsc::Sender<uze_application::CommandsAwaitingApproval>,
) {
    if directories.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.commands_awaiting", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        for directory in directories {
            if let Some(awaiting) = app.workspace().commands_awaiting_approval(&directory) {
                let _ = sender.send(awaiting);
            }
        }
    });
}

/// Records the operator's approval and prepares the checkout that waited
/// for it, which runs the project's setup: nothing a frame waits on.
pub(super) fn spawn_command_approval(
    home: &UzeHome,
    awaiting: uze_application::CommandsAwaitingApproval,
    sender: mpsc::Sender<ApprovalResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.command_approval").entered();
        let project = awaiting.project.clone();
        // Answered on every path: the header says the checkout is being
        // prepared until this arrives.
        let outcome = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.workspace().approve_commands(&awaiting))
                    .map_err(|error| error.to_string())
            },
            Err("approving failed".to_owned()),
        );
        let _ = sender.send(ApprovalResolution { project, outcome });
    });
}

/// Asks the registry, once, which names a harness launched through a shim
/// runs under. Off the frame: composing the application reads the machine.
pub(super) fn spawn_launcher_names(home: &UzeHome, sender: mpsc::Sender<Vec<String>>) {
    let home = home.clone();
    thread::spawn(move || {
        let names = answered_or(
            || {
                tui_application(home)
                    .map(|app| app.workspace().launcher_names())
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(names);
    });
}

/// An agent UZE launched, as the session reports it: the harness running
/// it, the identity its launch carried, and the directory it stands in.
pub(super) struct LaunchedAgent {
    pub(super) integration: String,
    pub(super) id: String,
    /// The key the tab's launch carries, as the server echoes it.
    pub(super) key: String,
    pub(super) cwd: PathBuf,
}

/// What a background evaluation answered.
pub(super) struct WorkResolution {
    /// The key [`WorkspaceModel::schedule_evaluation`] reserved, released
    /// on arrival whatever the answer was. It travels with the request
    /// because the two ends resolve a repository differently — the
    /// scheduler lexically, off the path it already holds, the evaluation
    /// by asking Git — and a key removed under the second spelling never
    /// matches the one inserted under the first, which leaves that
    /// directory reserved for the life of the session and its status
    /// frozen at whatever it last read.
    pub(super) key: PathBuf,
    /// What the directory's repository holds, or `None` when the
    /// directory turned out not to be a Git working tree.
    pub(super) answered: Option<EvaluationAnswer>,
}

/// One evaluated directory: the repository its tasks hang off, the branch
/// checked out at the directory the evaluation is *keyed* under (and,
/// when that branch is the delivery target, how it stands against its
/// upstream), and what the repository now holds. The key, not the `cwd`
/// that asked: a slot is keyed by its primary, and a slot's pane going
/// quiet must not write the slot's own branch where an agent outside any
/// slot — the one case the sidebar has no task to read a branch from —
/// then reads the primary's.
pub(super) struct EvaluationAnswer {
    pub(super) primary: PathBuf,
    pub(super) branch: Option<String>,
    /// The repository's delivery target — what the timeline measures a
    /// commit as ahead of.
    pub(super) target: Option<String>,
    pub(super) sync: Option<UpstreamSync>,
    pub(super) evaluation: Evaluation,
}

/// What a background delivery answered.
pub(super) struct DeliveryResolution {
    pub(super) cwd: PathBuf,
    /// The task the press reserved in `delivery_pending`, carried the way
    /// [`WorkResolution`] carries its key and released on arrival
    /// whatever came back.
    ///
    /// Releasing by walking `reports` alone is only correct while there
    /// is always a report: every empty answer — a checkout removed under
    /// the agent, an id the store no longer holds, an application that
    /// would not open — left the task drawn as "delivering" for the rest
    /// of the session, undeliverable again, and repainting on the
    /// spinner's clock forever because a pending delivery is one of the
    /// three things that keep it turning.
    pub(super) reserved: Option<String>,
    pub(super) reports: Vec<DeliveryReport>,
}

/// Re-reads the tasks of the repository `cwd` belongs to, off the UI
/// thread: every evaluation asks Git, and a delivery may run a gate.
/// What a sweep of the machine's preserved work answered.
pub(super) struct PreservedResolution {
    pub(super) work: Vec<uze_application::PreservedWork>,
}

/// Re-reads every project's preserved work, off the UI thread.
///
/// Answers even when it found nothing, for the same reason the task
/// evaluation does: a request that returns in silence never clears its
/// pending flag, and the sweep is then never asked for again.
pub(super) fn spawn_preserved_sweep(home: &UzeHome, sender: mpsc::Sender<PreservedResolution>) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.preserved_sweep", parent: &parent);
        let work = answered_or(
            || {
                crate::ui::tui_application(home)
                    .ok()
                    .map(|app| app.workspace().preserved_work())
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(PreservedResolution { work });
    });
}

pub(super) fn spawn_task_evaluation(
    home: &UzeHome,
    key: PathBuf,
    cwd: PathBuf,
    occupied: Vec<PathBuf>,
    sender: mpsc::Sender<WorkResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.task_evaluation", parent: &parent);
        // Every path out of here answers, including the ones that found
        // nothing: a request that returns in silence never releases its
        // key, and the directory is then never evaluated again.
        let answered = answered_or(
            || {
                tui_application(home).ok().and_then(|app| {
                    let workspace = app.workspace();
                    // Every question here is about the *repository*, and `cwd`
                    // is only how the caller named it. A slot removed from
                    // under its pane names nothing Git can answer for, so
                    // the repository is the slot's key, derived lexically
                    // before this thread started — otherwise the client
                    // keeps a view that still believes the task has its
                    // checkout, which is what the way back in is gated on.
                    let primary = workspace.primary_of(&cwd).or_else(|| {
                        uze_application::is_isolated_checkout(&cwd).then(|| key.clone())
                    })?;
                    Some(EvaluationAnswer {
                        branch: workspace.current_branch(&key),
                        target: workspace
                            .delivery_policy(&key)
                            .and_then(|policy| policy.target),
                        sync: workspace.target_upstream_sync(&key),
                        evaluation: workspace.evaluate_tasks(&primary, &occupied),
                        primary,
                    })
                })
            },
            None,
        );
        let _ = sender.send(WorkResolution { key, answered });
    });
}

/// Delivers one task, or every ready one when `task` is `None`.
pub(super) fn spawn_delivery(
    home: &UzeHome,
    cwd: PathBuf,
    task: Option<String>,
    sender: mpsc::Sender<DeliveryResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.delivery").entered();
        // Every path out of here answers, including the ones that
        // delivered nothing: the reservation this was started under is
        // released on arrival, so a thread that returns in silence leaves
        // its task drawn as "delivering" for good.
        let reports = answered_or(
            || {
                tui_application(home)
                    .ok()
                    .map(|app| match &task {
                        Some(one) => app
                            .workspace()
                            .deliver_task(&cwd, one)
                            .into_iter()
                            .collect(),
                        None => app.workspace().deliver_ready(&cwd),
                    })
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(DeliveryResolution {
            cwd,
            reserved: task,
            reports,
        });
    });
}

/// What a preserved task is asked to become.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkMutation {
    /// Closed, its slot released. The work itself is kept.
    Finish,
    /// Thrown away: the checkout removed, the branch deleted.
    Discard,
}

impl WorkMutation {
    /// What the operator is told while it runs, and what they are told
    /// when it is done.
    pub(super) fn underway(self) -> &'static str {
        match self {
            Self::Finish => "finishing",
            Self::Discard => "discarding",
        }
    }

    pub(super) fn done(self) -> &'static str {
        match self {
            Self::Finish => "finished",
            Self::Discard => "discarded",
        }
    }
}

/// What a task mutation answered.
pub(super) struct MutationResolution {
    pub(super) cwd: PathBuf,
    /// The task it was reserved under, released on arrival whichever way
    /// it went — the same rule every other reservation in this file
    /// follows.
    pub(super) task: String,
    pub(super) label: String,
    pub(super) mutation: WorkMutation,
    pub(super) outcome: std::result::Result<(), String>,
}

/// Finishes or discards one preserved task, off the UI thread.
///
/// Discard is `git worktree remove`, then `git branch -D`, then a
/// recursive removal of the checkout — a slot holding a build directory
/// is tens of thousands of files — and finish opens the repository and
/// rewrites the task store. Both used to run where the keystroke was
/// handled, which froze the client and every pane in it for as long as
/// the filesystem took.
pub(super) fn spawn_task_mutation(
    home: &UzeHome,
    cwd: PathBuf,
    task: String,
    label: String,
    mutation: WorkMutation,
    sender: mpsc::Sender<MutationResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.task_mutation").entered();
        // Every path answers: the reservation that stops a second Enter
        // from starting a second removal is released nowhere else.
        let outcome = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| match mutation {
                        WorkMutation::Finish => app.workspace().finish_task(&cwd, &task),
                        WorkMutation::Discard => app.workspace().discard_task(&cwd, &task),
                    })
                    .map_err(|error| error.to_string())
            },
            Err(format!("{} the task failed", mutation.underway())),
        );
        let _ = sender.send(MutationResolution {
            cwd,
            task,
            label,
            mutation,
            outcome,
        });
    });
}

/// Reads every checkout of the repository `project` belongs to, measuring
/// each — a walk of every directory, which no frame may wait on.
///
/// Answers even when it found nothing: the pending flag is released on
/// arrival, and a read that returned in silence would never be asked again.
pub(super) fn spawn_checkouts(
    home: &UzeHome,
    project: PathBuf,
    asked: u64,
    occupied: Vec<PathBuf>,
    sender: mpsc::Sender<CheckoutsResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.checkouts_read", parent: &parent);
        let view = answered_or(
            || {
                tui_application(home)
                    .ok()
                    .and_then(|app| app.workspace().checkouts(&project, &occupied))
            },
            None,
        );
        let _ = sender.send(CheckoutsResolution {
            project,
            asked,
            view,
        });
    });
}

/// Adopts, removes, joins or cleans up checkouts, off the UI thread: a removal is
/// `git worktree remove` over a directory that may hold a build's worth of
/// files, and a clean-up is several of them.
pub(super) fn spawn_checkout_change(
    home: &UzeHome,
    project: PathBuf,
    change: CheckoutChange,
    occupied: Vec<PathBuf>,
    sender: mpsc::Sender<CheckoutChangeResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.checkout_change").entered();
        let failed = |name: &str| format!("changing {name} failed");
        let outcome = answered_or(
            || {
                let app = tui_application(home).map_err(|error| error.to_string());
                let workspace = app.as_ref().map(|app| app.workspace());
                match &change {
                    CheckoutChange::Adopt { path, name } => CheckoutOutcome::Adopted {
                        name: name.clone(),
                        answer: workspace.map_err(Clone::clone).and_then(|workspace| {
                            workspace
                                .adopt_checkout(&project, path)
                                .map_err(|refusal| refusal.to_string())
                        }),
                    },
                    CheckoutChange::Remove { path, name } => CheckoutOutcome::Removed {
                        name: name.clone(),
                        answer: workspace.map_err(Clone::clone).and_then(|workspace| {
                            workspace
                                .remove_checkout(&project, path, &occupied)
                                .map_err(|refusal| refusal.to_string())
                        }),
                    },
                    CheckoutChange::Join {
                        parent_id,
                        parent,
                        topic,
                    } => CheckoutOutcome::Joined {
                        topic: topic.clone(),
                        parent: parent.clone(),
                        answer: workspace.map_err(Clone::clone).and_then(|workspace| {
                            workspace
                                .join_parked_work(&project, parent_id, topic)
                                .map_err(|refusal| refusal.to_string())
                        }),
                    },
                    CheckoutChange::CleanUp => CheckoutOutcome::CleanedUp(
                        workspace
                            .map(|workspace| workspace.clean_up_checkouts(&project, &occupied))
                            .unwrap_or_default(),
                    ),
                }
            },
            match &change {
                CheckoutChange::Adopt { name, .. } => CheckoutOutcome::Adopted {
                    name: name.clone(),
                    answer: Err(failed(name)),
                },
                CheckoutChange::Remove { name, .. } => CheckoutOutcome::Removed {
                    name: name.clone(),
                    answer: Err(failed(name)),
                },
                CheckoutChange::Join { parent, topic, .. } => CheckoutOutcome::Joined {
                    topic: topic.clone(),
                    parent: parent.clone(),
                    answer: Err(failed(topic)),
                },
                CheckoutChange::CleanUp => {
                    CheckoutOutcome::CleanedUp(uze_application::CleanUp::default())
                }
            },
        );
        let _ = sender.send(CheckoutChangeResolution { project, outcome });
    });
}

/// What one background Git read was asked to produce, and produced.
///
/// The working tree and the history behind it move at different speeds
/// (see [`GIT_BADGE_REFRESH`]/[`TIMELINE_REFRESH`]), so a read that only
/// owes a summary says so in its answer rather than handing back a
/// `None` the receiver would have to tell apart from "there is no
/// history".
pub(super) enum GitAnswer {
    Summary(Option<code::ChangeSummary>),
    Full {
        summary: Option<code::ChangeSummary>,
        timeline: Option<code::Timeline>,
    },
}

/// A finished Git read, tagged with the checkout it answers about: the
/// selection can move while a read is in flight, and an answer about a
/// checkout the workspace has since left must never be drawn as the
/// current one.
pub(super) struct GitResolution {
    pub(super) cwd: PathBuf,
    pub(super) answer: GitAnswer,
    /// How long the read took — what the next one waits in proportion to.
    pub(super) took: Duration,
}

/// Reads the badge — and, when `history` is set, the timeline behind it —
/// off the UI thread.
///
/// Every one of these launches `git` several times (`rev-parse`,
/// `status`, `log`, `rev-list`), which is why it cannot run where a frame
/// is drawn: on a large repository `status --untracked-files=all` alone
/// outlasts several frames, and it used to run inside the `dirty` branch
/// immediately before `terminal.draw`.
pub(super) fn spawn_git_read(
    cwd: PathBuf,
    target: Option<String>,
    history: bool,
    sender: mpsc::Sender<GitResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.git_read", parent: &parent);
        let started = Instant::now();
        let answer = answered_or(
            || {
                let summary = code::change_summary(&WorkspaceHost, &cwd);
                if history {
                    GitAnswer::Full {
                        summary,
                        timeline: code::timeline(
                            &WorkspaceHost,
                            &cwd,
                            TIMELINE_COMMITS,
                            target.as_deref(),
                        ),
                    }
                } else {
                    GitAnswer::Summary(summary)
                }
            },
            GitAnswer::Summary(None),
        );
        let took = started.elapsed();
        let _ = sender.send(GitResolution { cwd, answer, took });
    });
}

/// One commit's account, read off the UI thread.
///
/// Tagged with the commit asked about so an answer that arrives after the
/// popup was dismissed — or after another row was clicked — is dropped
/// instead of replacing what the viewer is now looking at.
pub(super) struct CommitDetailResolution {
    pub(super) hash: String,
    pub(super) anchor: Rect,
    pub(super) target: Option<String>,
    pub(super) detail: Option<code::CommitDetail>,
}

/// A release's notes, read off the UI thread: reading them may reach the
/// network. Tagged with the release asked about, so an answer for a modal
/// since closed and opened on another is dropped.
pub(super) struct ReleaseNotesResolution {
    pub(super) version: String,
    pub(super) notes: Option<crate::self_update::ReleaseNotes>,
}

pub(super) fn spawn_release_notes(
    home: UzeHome,
    version: String,
    sender: mpsc::Sender<ReleaseNotesResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.release_notes").entered();
        let notes = crate::self_update::release_notes(&home, &version);
        let _ = sender.send(ReleaseNotesResolution { version, notes });
    });
}

pub(super) fn spawn_commit_detail(
    cwd: PathBuf,
    hash: String,
    anchor: Rect,
    target: Option<String>,
    sender: mpsc::Sender<CommitDetailResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.commit_detail").entered();
        let detail = answered_or(|| code::commit_detail(&WorkspaceHost, &cwd, &hash), None);
        let _ = sender.send(CommitDetailResolution {
            hash,
            anchor,
            target,
            detail,
        });
    });
}

/// The answer to a placement request: where the agent goes, or why it
/// cannot — in which case no tab opens.
pub(super) struct PlacementResolution {
    pub(super) label: String,
    pub(super) command: Vec<String>,
    pub(super) placement: std::result::Result<uze_application::AgentPlacement, String>,
    /// The tab this placement takes over from: the agent standing in a
    /// checkout that is gone, whose row the resume was clicked on. Closed
    /// once the new tab is open, and only then — a resume that failed
    /// leaves the operator the row they asked from.
    pub(super) replacing: Option<TabId>,
}

/// What a placement is asked for: a record for a brand-new agent, where
/// the project says agents start; a checkout of its own for one already
/// running; or the slot a preserved agent lost, its branch checked out
/// again as it stands.
pub(super) enum PlacementRequest {
    New {
        from: PathBuf,
        harness: String,
    },
    /// The agent this checkout is being cut for, and what the operator
    /// answered about the changes their own tree holds.
    Isolate {
        from: PathBuf,
        agent: String,
        carry: uze_application::Carry,
    },
    Resume {
        primary: PathBuf,
        task: String,
    },
}

impl PlacementRequest {
    /// Which of the three was asked for, for the journal.
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::New { .. } => "new",
            Self::Isolate { .. } => "isolate",
            Self::Resume { .. } => "resume",
        }
    }

    /// The directory the answer is resolved against — the space's own
    /// shell's, or the project a preserved task belongs to. The one field
    /// that decides which space the agent ends up in, so it is the one
    /// worth having written down.
    pub(super) fn from(&self) -> &Path {
        match self {
            Self::New { from, .. } | Self::Isolate { from, .. } => from,
            Self::Resume { primary, .. } => primary,
        }
    }
}

/// Asks the application where an agent should start, off the frame:
/// materializing a checkout may run the project's setup command, which is
/// nothing a render loop waits on. `occupied` is every checkout a live
/// pane still sits in, and none of those may be handed to the new agent
/// even when its task record reads as done (see `sync_slot_occupancy`).
pub(super) fn spawn_agent_placement(
    home: &UzeHome,
    request: PlacementRequest,
    occupied: Vec<PathBuf>,
    label: String,
    command: Vec<String>,
    replacing: Option<TabId>,
    sender: mpsc::Sender<PlacementResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        // What was asked for, beside how it was answered: the modal that
        // takes the pick is not a gesture of its own (`Attach::press`
        // resolves it inside its own guard), so this span is where the
        // journal says which agent, on which harness, and from where.
        let _span = tracing::info_span!(
            parent: &parent,
            "tui.agent_placement",
            label = %label,
            asked = request.name(),
            from = %request.from().display(),
        )
        .entered();
        // Answered on every path, including the one that could not even
        // build an application: the request holds the only reservation
        // there is, and a silent return would leave this client unable to
        // create another agent for the rest of the session.
        let placement = answered_or(
            || match request {
                // A placement that cannot do what the space's kind asks
                // answers with the reason and opens nothing: the operator
                // chose the kind, and an agent landing anywhere else is
                // the one outcome a notice could not undo.
                PlacementRequest::New { from, harness } => tui_application(home)
                    .and_then(|app| {
                        // No kind travels: where an agent starts is the
                        // project's to declare, and isolating one is an
                        // action on it afterwards.
                        app.workspace()
                            .place_new_agent(&from, None, &harness, &occupied)
                    })
                    .map_err(|error| error.to_string()),
                PlacementRequest::Isolate { from, agent, carry } => tui_application(home)
                    .and_then(|app| app.workspace().isolate(&from, &agent, carry, &occupied))
                    .map_err(|error| error.to_string()),
                PlacementRequest::Resume { primary, task } => tui_application(home)
                    .and_then(|app| app.workspace().resume_task(&primary, &task, &occupied))
                    .map_err(|error| error.to_string()),
            },
            Err("placing the agent failed".to_owned()),
        );
        let _ = sender.send(PlacementResolution {
            label,
            command,
            placement,
            replacing,
        });
    });
}

/// What one pass of slot reconciliation freed.
pub(super) struct OccupancyResolution {
    pub(super) reconciliation: uze_application::Reconciliation,
}

/// Reconciles slot occupancy off the UI thread.
///
/// Releasing a slot rewrites the task store and collecting one asks Git
/// about every branch of the repository — the sweep a client runs before
/// it can place its first agent is the most expensive thing an attach
/// does, and it used to run inline in the loop.
///
/// `held` is every checkout a live pane still sits in and `echoed` every
/// agent a live tab was launched for; both travel with the request because
/// only this client knows them. Both halves need them: a task no pane is
/// in front of ends, and a directory a pane *is* in is never collected,
/// whatever its record says.
pub(super) fn spawn_occupancy_reconcile(
    home: &UzeHome,
    look_in: Vec<PathBuf>,
    held: Vec<PathBuf>,
    echoed: Vec<String>,
    sender: mpsc::Sender<OccupancyResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.occupancy_reconcile", parent: &parent);
        let reconciliation = answered_or(
            || {
                tui_application(home)
                    .map(|app| {
                        // Shares this pass rather than earning a thread of its own:
                        // the runtime projections left by destroyed checkouts are
                        // swept by exactly the same event that notices a slot is
                        // gone, and the sweep is a `readdir` next to the repository
                        // work already happening here.
                        app.health().prune_runtime_projections();
                        app.workspace()
                            .reconcile_occupancy(&look_in, &held, &echoed)
                    })
                    .unwrap_or_default()
            },
            uze_application::Reconciliation::default(),
        );
        // Answered even when nothing changed: the pending flag is
        // released here, and a pass that returns in silence would never
        // let another one run.
        let _ = sender.send(OccupancyResolution { reconciliation });
    });
}

/// A re-read of the open surface's changes half, tagged with the checkout
/// it was read for.
///
/// The changes only, never the whole view: the same surface holds a file
/// someone may be typing into, and a refresh that could reach it would be
/// a refresh that eats what was typed.
pub(super) struct ChangesResolution {
    pub(super) root: PathBuf,
    pub(super) refreshed: code::RefreshedChanges,
}

/// One file's diff, read because the selection moved — tagged like a
/// refresh, and dropped by the view if the selection has moved again.
pub(super) struct DiffResolution {
    pub(super) root: PathBuf,
    pub(super) answer: code::DiffAnswer,
}

/// One answered [`code::FileRequest`], tagged the same way and for the
/// same reason: an answer landing after the viewer moved to another tab
/// describes a tree nobody is looking at any more.
pub(super) struct FileResolution {
    pub(super) root: PathBuf,
    pub(super) answer: code::FileAnswer,
}

/// Reading a directory, reading and highlighting a file, writing one:
/// every one of them is unbounded, and none of them may happen on the
/// thread that draws.
pub(super) fn spawn_file_request(
    root: PathBuf,
    request: code::FileRequest,
    sender: mpsc::Sender<FileResolution>,
) {
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.code_file_request");
        // Highlighting runs syntect over whatever the tree listed, which
        // is the one read here whose input nobody controls.
        let silence = code::unanswered(&request, "reading it failed");
        let answer = answered_or(|| code::fulfill(&WorkspaceHost, request), silence);
        let _ = sender.send(FileResolution { root, answer });
    });
}

/// One diff, on its own: moving the selection is a `git diff` of that
/// file, never a `status` of the whole checkout in front of it.
pub(super) fn spawn_diff_read(
    root: PathBuf,
    request: code::DiffRequest,
    sender: mpsc::Sender<DiffResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.code_diff_read", parent: &parent);
        let silence = code::DiffAnswer::failed(&request, "reading the diff failed".to_owned());
        let answer = answered_or(
            || code::CodeView::read_diff(&WorkspaceHost, &root, request),
            silence,
        );
        let _ = sender.send(DiffResolution { root, answer });
    });
}

pub(super) fn spawn_changes_refresh(
    root: PathBuf,
    placement: code::ViewPlacement,
    sender: mpsc::Sender<ChangesResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.code_changes_refresh", parent: &parent);
        let silence =
            code::RefreshedChanges::failed(placement.clone(), "reading the changes".to_owned());
        let refreshed = answered_or(
            || code::CodeView::refresh(&WorkspaceHost, root.clone(), placement),
            silence,
        );
        let _ = sender.send(ChangesResolution { root, refreshed });
    });
}

/// The artifacts a project declares, read for the checkout they were
/// asked about.
pub(super) struct ArtifactsResolution {
    pub(super) root: PathBuf,
    pub(super) answer: architect::ArtifactsAnswer,
}

/// The changes in flight in a checkout, counted, for the sidebar.
pub(super) struct SpecSummaryResolution {
    pub(super) cwd: PathBuf,
    pub(super) summary: Option<spec::Summary>,
}

/// What the sidebar's spec section last read, and when.
pub(super) struct SpecSummaryState {
    pub(super) cwd: PathBuf,
    pub(super) summary: Option<spec::Summary>,
    pub(super) checked_at: Instant,
}

/// A checkout's specs, read for the checkout they were asked about.
pub(super) struct SpecResolution {
    pub(super) root: PathBuf,
    pub(super) answer: spec::SpecAnswer,
}

/// The checkout measured for the code surface's map, tagged with the
/// checkout it was measured from — an answer landing after the viewer
/// moved to another tab describes a repository nobody is looking at.
pub(super) struct MeasureResolution {
    pub(super) root: PathBuf,
    /// `None` outside a repository, where there is nothing to measure.
    pub(super) measure: Option<code::Measure>,
}

/// Resolving the manifest, walking the declared directory and reading
/// every file in it: three unbounded reads, none of them the render
/// thread's to make.
pub(super) fn spawn_artifacts_read(root: PathBuf, sender: mpsc::Sender<ArtifactsResolution>) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.architect_artifacts", parent: &parent);
        let silence = architect::ArtifactsAnswer {
            branch: String::new(),
            artifacts: architect::Artifacts::Nothing {
                text: "Reading the project's artifacts failed".to_owned(),
                hint: "Close this and open it again.".to_owned(),
            },
        };
        let answer = answered_or(
            || {
                let source = crate::ui::extension_host::artifacts_declared_in(&root);
                architect::read_artifacts(&WorkspaceHost, &root, source)
            },
            silence,
        );
        let _ = sender.send(ArtifactsResolution { root, answer });
    });
}

/// Counting the changes in flight: a directory walk and a read of every
/// task list, none of it for the render thread.
pub(super) fn spawn_spec_summary(
    cwd: PathBuf,
    target: Option<String>,
    sender: mpsc::Sender<SpecSummaryResolution>,
) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.spec_summary", parent: &parent);
        let summary = answered_or(
            || spec::summary(&WorkspaceHost, &cwd, target.as_deref()),
            None,
        );
        let _ = sender.send(SpecSummaryResolution { cwd, summary });
    });
}

/// Walking the dialect's directories, reading every document in them and
/// asking Git which of them the checkout touched — none of it the render
/// thread's to wait on. The branch the checkout delivers to comes from
/// the application, the one question here that is not the checkout's own.
pub(super) fn spawn_spec_read(home: &UzeHome, root: PathBuf, sender: mpsc::Sender<SpecResolution>) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.spec_read", parent: &parent);
        let silence = spec::SpecAnswer {
            root: root.clone(),
            branch: String::new(),
            theme: String::new(),
            found: spec::Found::NoLayout,
            subjects: Vec::new(),
        };
        let answer = answered_or(
            || {
                let target = tui_application(home.clone())
                    .ok()
                    .and_then(|app| app.workspace().delivery_policy(&root))
                    .and_then(|policy| policy.target);
                let places = crate::ui::extension_host::artifacts_declared_in(&root);
                spec::read_spec(&WorkspaceHost, &root, target.as_deref(), &places)
            },
            silence,
        );
        let _ = sender.send(SpecResolution { root, answer });
    });
}

/// Measuring the checkout for the code surface's map: a `git grep` over
/// every file in it, which is as unbounded as a read gets.
pub(super) fn spawn_code_measure(root: PathBuf, sender: mpsc::Sender<MeasureResolution>) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.code_measure", parent: &parent);
        let measure = answered_or(|| code::measure(&WorkspaceHost, &root).ok(), None);
        let _ = sender.send(MeasureResolution { root, measure });
    });
}

/// A space's recorded prompts, tagged with the root they were read for.
pub(super) struct PromptHistoryResolution {
    pub(super) root: PathBuf,
    pub(super) entries: Vec<uze_application::PromptEntry>,
}

/// How many of a space's prompts the drawer reads.
pub(super) const DRAWER_PROMPT_LIMIT: usize = 100;

/// Reads a space's prompt history off the render thread. One small file,
/// but a file all the same, and nothing the client draws waits on one.
pub(super) fn spawn_prompt_history(
    home: &UzeHome,
    root: PathBuf,
    sender: mpsc::Sender<PromptHistoryResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.prompt_history", parent: &parent);
        let entries = answered_or(
            || {
                tui_application(home)
                    .map(|app| app.workspace().prompt_history(&root, DRAWER_PROMPT_LIMIT))
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(PromptHistoryResolution { root, entries });
    });
}

/// Forgets a space's prompts, and answers with the empty history that
/// leaves, so the drawer redraws from what is on disk rather than from a
/// guess about it.
pub(super) fn spawn_clear_prompt_history(
    home: &UzeHome,
    root: PathBuf,
    sender: mpsc::Sender<PromptHistoryResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.clear_prompt_history").entered();
        let entries = answered_or(
            || {
                tui_application(home)
                    .map(|app| {
                        let _ = app.workspace().clear_prompt_history(&root);
                        app.workspace().prompt_history(&root, DRAWER_PROMPT_LIMIT)
                    })
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(PromptHistoryResolution { root, entries });
    });
}
