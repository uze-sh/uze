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

/// A project whose `AGENTS.md` still carries regions an earlier release
/// wrote, which `uze install` removes.
pub(super) struct RetiredRegionsNotice {
    pub(super) file: PathBuf,
}

/// Looks, off the frame like every other repository touch, for regions an
/// earlier release left in each directory's primary `AGENTS.md`. It only
/// reads: the workspace writes nothing into the operator's files.
pub(super) fn spawn_retired_region_check(
    home: &UzeHome,
    directories: Vec<PathBuf>,
    sender: mpsc::Sender<RetiredRegionsNotice>,
) {
    if directories.is_empty() {
        return;
    }
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.retired_region_check", parent: &parent);
        let Ok(app) = tui_application(home) else {
            return;
        };
        let mut seen = std::collections::BTreeSet::new();
        for directory in directories {
            let Some(plan) = app.workspace().retired_regions(&directory) else {
                continue;
            };
            if !seen.insert(plan.file.clone()) {
                continue;
            }
            let _ = sender.send(RetiredRegionsNotice { file: plan.file });
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

/// Reads the machine facts agents proposed, off the frame: a walk of one
/// directory of small files, and nothing a frame waits on.
pub(super) fn spawn_memory_proposals(home: &UzeHome, sender: mpsc::Sender<ProposalsResolution>) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.memory_proposals").entered();
        let proposals = answered_or(
            || {
                tui_application(home)
                    .map(|app| app.memory().report(None).proposals)
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(ProposalsResolution { proposals });
    });
}

/// Accepts or discards a proposed machine fact, off the frame.
pub(super) fn spawn_proposal_answer(
    home: &UzeHome,
    name: String,
    choice: ProposalChoice,
    sender: mpsc::Sender<ProposalAnswerResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.proposal_answer").entered();
        let outcome = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| match choice {
                        ProposalChoice::Accept => app.memory().accept(&name).map(|_| ()),
                        _ => app.memory().discard(&name).map(|_| ()),
                    })
                    .map_err(|error| error.to_string())
            },
            Err("answering failed".to_owned()),
        );
        let _ = sender.send(ProposalAnswerResolution {
            name,
            choice,
            outcome,
        });
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
    /// Which evaluation of this key answered, counted by the scheduler: a
    /// remote half older than an evaluation already absorbed is dropped.
    pub(super) generation: u64,
    /// What the directory's repository holds, or `None` when the
    /// directory turned out not to be a Git working tree.
    pub(super) answered: Option<EvaluationAnswer>,
    /// The remote's half of a pass whose own answer already arrived: the
    /// repository's views after the forge was asked about requests, and
    /// nothing else — the branch, target and sync are the first half's to
    /// say. It releases no reservation, since the first half already did.
    pub(super) remote: Option<(PathBuf, Vec<AgentView>)>,
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
    generation: u64,
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
                tui_application(home.clone()).ok().and_then(|app| {
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
                        evaluation: workspace.read_tasks(&primary, &occupied),
                        primary,
                    })
                })
            },
            None,
        );
        // The states first, the remote after: a `git ls-remote` takes
        // seconds, and what the checkouts already say is not made to wait
        // for an answer about a request.
        let question = answered
            .as_ref()
            .map(|answer| (answer.primary.clone(), answer.evaluation.requests.clone()));
        let _ = sender.send(WorkResolution {
            key: key.clone(),
            generation,
            answered,
            remote: None,
        });
        let Some((primary, question)) = question else {
            return;
        };
        let asked = answered_or(
            || {
                tui_application(home)
                    .ok()
                    .and_then(|app| app.workspace().ask_about_requests(&question))
            },
            None,
        );
        if let Some(tasks) = asked {
            let _ = sender.send(WorkResolution {
                key,
                generation,
                answered: None,
                remote: Some((primary, tasks)),
            });
        }
    });
}

/// What a pull the operator asked for did, and where.
pub(super) struct PullResolution {
    pub(super) cwd: PathBuf,
    pub(super) report: Option<uze_application::PullReport>,
}

/// Pulls the target into the checkout `cwd` belongs to: a fetch and a
/// fast-forward, seconds of network that no frame may wait on.
pub(super) fn spawn_pull(home: &UzeHome, cwd: PathBuf, sender: mpsc::Sender<PullResolution>) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.pull", parent: &parent);
        // Answers even when it could not ask: the pending flag is released
        // on arrival, and a silent thread would leave the count unclickable.
        let report = answered_or(
            || {
                tui_application(home)
                    .ok()
                    .and_then(|app| app.workspace().pull_target(&cwd))
            },
            None,
        );
        let _ = sender.send(PullResolution { cwd, report });
    });
}

/// What each checkout's stamp read, `None` for one that is no longer a
/// working tree.
pub(super) struct StampResolution {
    pub(super) stamps: Vec<(PathBuf, Option<u64>)>,
    /// The stamp of what the open overview's project declares, with the
    /// project it was read for.
    pub(super) declaration: Option<(PathBuf, u64)>,
}

/// Reads the change stamp of every checkout on screen. Off the render
/// path like every other read, cheap as it is: a directory on a slow mount
/// still answers a `stat` at its own pace. Without a `background_pass!`:
/// it runs every second, and a span each time would be most of the
/// journal.
pub(super) fn spawn_checkout_watch(
    home: &UzeHome,
    checkouts: Vec<(PathBuf, Option<String>)>,
    overview: Option<PathBuf>,
    sender: mpsc::Sender<StampResolution>,
) {
    let home = home.clone();
    thread::spawn(move || {
        let (stamps, declaration) = answered_or(
            || {
                tui_application(home)
                    .map(|app| {
                        let workspace = app.workspace();
                        let stamps = checkouts
                            .into_iter()
                            .map(|(checkout, published)| {
                                let stamp =
                                    workspace.checkout_stamp(&checkout, published.as_deref());
                                (checkout, stamp)
                            })
                            .collect();
                        let declaration = overview.map(|root| {
                            let stamp = workspace.declaration_stamp(&root);
                            (root, stamp)
                        });
                        (stamps, declaration)
                    })
                    .unwrap_or_default()
            },
            (Vec::new(), None),
        );
        let _ = sender.send(StampResolution {
            stamps,
            declaration,
        });
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
    /// Thrown away: the checkout removed, the branch deleted.
    Discard,
    /// Its record and its conversation let go of, there being no work
    /// left to throw away.
    Forget,
}

impl WorkMutation {
    /// What the operator is told while it runs, and what they are told
    /// when it is done.
    pub(super) fn underway(self) -> &'static str {
        match self {
            Self::Discard => "discarding",
            Self::Forget => "forgetting",
        }
    }

    pub(super) fn done(self) -> &'static str {
        match self {
            Self::Discard => "discarded",
            Self::Forget => "forgotten",
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

/// Discards or forgets one preserved task, off the UI thread.
///
/// Discard is `git worktree remove`, then `git branch -D`, then a
/// recursive removal of the checkout — a slot holding a build directory
/// is tens of thousands of files. It used to run where the keystroke was
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
                        WorkMutation::Discard => app.workspace().discard_task(&cwd, &task),
                        WorkMutation::Forget => app.workspace().forget_task(&cwd, &task),
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
                    .and_then(|app| app.workspace().checkouts(&project))
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

/// What recording an operator's name for an agent answered.
pub(super) struct LabelResolution {
    pub(super) label: String,
    pub(super) outcome: std::result::Result<(), String>,
}

/// Records the name the operator typed over an agent's tab on the agent's
/// record, off the UI thread: it is a write of the project's task store
/// under its lock.
pub(super) fn spawn_agent_label(
    home: &UzeHome,
    cwd: PathBuf,
    agent: String,
    label: String,
    sender: mpsc::Sender<LabelResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.label_agent").entered();
        let outcome = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.workspace().label_agent(&cwd, &agent, &label))
                    .map_err(|error| error.to_string())
            },
            Err("naming the agent failed".to_owned()),
        );
        let _ = sender.send(LabelResolution { label, outcome });
    });
}

/// Joins a subagent's checkout into its agent, off the UI thread: the
/// join replays commits onto the agent's branch and removes a directory
/// that may hold a build's worth of files.
pub(super) fn spawn_join(
    home: &UzeHome,
    project: PathBuf,
    join: Join,
    sender: mpsc::Sender<JoinResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.join").entered();
        let answer = answered_or(
            || {
                tui_application(home)
                    .map_err(|error| error.to_string())
                    .and_then(|app| {
                        app.workspace()
                            .join_unfinished_work(&project, &join.parent_id, &join.topic)
                            .map_err(|refusal| refusal.to_string())
                    })
            },
            Err(format!("joining {} failed", join.topic)),
        );
        let _ = sender.send(JoinResolution {
            project,
            join,
            answer,
        });
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
    /// The space a new agent was asked for from, which it opens in
    /// whichever space is in front when the placement lands. `None` for
    /// work resumed or isolated, which lands where its project or its row
    /// is, and takes the operator there.
    pub(super) space: Option<SpaceId>,
}

/// What a placement is asked for: a record for a brand-new agent, where
/// the project says agents start; a checkout of its own for one already
/// running; or the slot a preserved agent lost, its branch checked out
/// again as it stands.
pub(super) enum PlacementRequest {
    /// A new agent in `from`, whose tab opens in the space whose `new`
    /// was pressed.
    New {
        from: PathBuf,
        harness: String,
        space: SpaceId,
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
        harness: String,
    },
    /// The agent continuing on another harness, where it already stands.
    Switch {
        from: PathBuf,
        agent: String,
        was: String,
        to: String,
    },
}

impl PlacementRequest {
    /// Which of the three was asked for, for the journal.
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::New { .. } => "new",
            Self::Isolate { .. } => "isolate",
            Self::Resume { .. } => "resume",
            Self::Switch { .. } => "switch",
        }
    }

    /// The directory the answer is resolved against — the one chosen for a
    /// new agent, the agent's own, or the project a preserved task belongs
    /// to. The one field that decides which project the agent works in, so
    /// it is the one worth having written down.
    pub(super) fn from(&self) -> &Path {
        match self {
            Self::New { from, .. } | Self::Isolate { from, .. } | Self::Switch { from, .. } => from,
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
    let space = match &request {
        PlacementRequest::New { space, .. } => Some(*space),
        _ => None,
    };
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
                PlacementRequest::New { from, harness, .. } => tui_application(home)
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
                PlacementRequest::Switch {
                    from,
                    agent,
                    was,
                    to,
                } => tui_application(home)
                    .and_then(|app| app.workspace().switch_harness(&from, &agent, &was, &to))
                    .map_err(|error| error.to_string()),
                PlacementRequest::Resume {
                    primary,
                    task,
                    harness,
                } => tui_application(home)
                    .and_then(|app| {
                        app.workspace()
                            .resume_task(&primary, &task, &harness, &occupied)
                    })
                    .map_err(|error| error.to_string()),
            },
            Err("placing the agent failed".to_owned()),
        );
        let _ = sender.send(PlacementResolution {
            label,
            command,
            placement,
            replacing,
            space,
        });
    });
}

/// What one pass of slot reconciliation freed.
/// What an occupancy pass is for: a change in the panes, which reconciles
/// everything, or only a release owed from an earlier pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OccupancyPass {
    Full,
    RetryReleases,
}

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
    pass: OccupancyPass,
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
                        if pass == OccupancyPass::RetryReleases {
                            return app.workspace().retry_releases(&look_in, &held, &echoed);
                        }
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

/// An agent's recorded prompts, tagged with whose they are.
pub(super) struct PromptHistoryResolution {
    pub(super) of: PromptsOf,
    pub(super) entries: Vec<uze_application::PromptEntry>,
}

/// How many of an agent's prompts the overview reads.
pub(super) const DRAWER_PROMPT_LIMIT: usize = 100;

/// Reads an agent's prompt history off the render thread. One small file,
/// but a file all the same, and nothing the client draws waits on one.
pub(super) fn spawn_prompt_history(
    home: &UzeHome,
    of: PromptsOf,
    sender: mpsc::Sender<PromptHistoryResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.prompt_history", parent: &parent);
        let entries = answered_or(
            || {
                tui_application(home)
                    .map(|app| {
                        app.workspace()
                            .prompt_history(&of.project, &of.agent, DRAWER_PROMPT_LIMIT)
                    })
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(PromptHistoryResolution { of, entries });
    });
}

/// Forgets an agent's prompts, and answers with the empty history that
/// leaves, so the overview redraws from what is on disk rather than from a
/// guess about it.
pub(super) fn spawn_clear_prompt_history(
    home: &UzeHome,
    of: PromptsOf,
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
                        let _ = app.workspace().clear_prompt_history(&of.project, &of.agent);
                        app.workspace()
                            .prompt_history(&of.project, &of.agent, DRAWER_PROMPT_LIMIT)
                    })
                    .unwrap_or_default()
            },
            Vec::new(),
        );
        let _ = sender.send(PromptHistoryResolution { of, entries });
    });
}

/// Reads a space's overview: `uze status`'s computation, which re-runs a
/// harness's CLI when its inspection is not cached, plus the plugin index
/// and the `workspace:` declaration, which asks Git about each link.
pub(super) fn spawn_space_overview(
    home: &UzeHome,
    space: SpaceId,
    asked: u64,
    root: PathBuf,
    sender: mpsc::Sender<SpaceOverviewResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.space_overview", parent: &parent);
        let answer = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.workspace().space_overview(&root))
                    .map_err(|error| error.to_string())
            },
            Err("reading the overview failed".to_owned()),
        );
        let _ = sender.send(SpaceOverviewResolution {
            space,
            root,
            asked,
            answer,
        });
    });
}

/// Measures the worktrees of the project at `root`, for the
/// overview's Worktrees section: [`spawn_checkouts`]' read, a walk of every
/// directory of every checkout, answered by space so an overview put away
/// meanwhile drops it.
pub(super) fn spawn_space_worktrees(
    home: &UzeHome,
    space: SpaceId,
    asked: u64,
    root: PathBuf,
    sender: mpsc::Sender<WorktreesResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _pass = crate::telemetry::background_pass!("tui.space_worktrees", parent: &parent);
        let view = answered_or(
            || {
                // The project's worktrees, wherever the overview stands: an
                // agent's own checkout is one of them, and Git is not run
                // inside it on its behalf.
                tui_application(home)
                    .ok()
                    .and_then(|app| app.workspace().checkouts(&uze_application::slot_key(&root)))
            },
            None,
        );
        let _ = sender.send(WorktreesResolution {
            space,
            root,
            asked,
            view,
        });
    });
}

/// What removing a free worktree is asked with.
pub(super) struct WorktreeRemovalRequest {
    pub(super) space: SpaceId,
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) bytes: u64,
    /// Every checkout a pane of this client stands in.
    pub(super) occupied: Vec<PathBuf>,
}

/// Removes a free worktree the operator answered for: a rename under the
/// repository's lock, then a recursive deletion of what may be gigabytes
/// of build output. Answers either way, since the overview takes no
/// second removal until this one does.
pub(super) fn spawn_worktree_removal(
    home: &UzeHome,
    request: WorktreeRemovalRequest,
    sender: mpsc::Sender<RemovalResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.worktree_removal").entered();
        let WorktreeRemovalRequest {
            space,
            root,
            path,
            name,
            bytes,
            occupied,
        } = request;
        let outcome = answered_or(
            || {
                tui_application(home)
                    .map_err(|error| error.to_string())
                    .and_then(|app| {
                        app.workspace().remove_free_checkout(
                            &uze_application::slot_key(&root),
                            &path,
                            &occupied,
                        )
                    })
            },
            Err("removing the worktree failed".to_owned()),
        );
        let _ = sender.send(RemovalResolution {
            space,
            root,
            name,
            bytes,
            outcome,
        });
    });
}

/// Reads one of the project's facts, for the overview's drawer.
pub(super) fn spawn_fact_body(
    home: &UzeHome,
    space: SpaceId,
    root: PathBuf,
    scope: uze_application::application::memory::MemoryScope,
    name: String,
    sender: mpsc::Sender<FactBodyResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.fact_body").entered();
        use uze_application::application::memory::MemoryCaller;
        let body = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| {
                        app.memory()
                            .show(MemoryCaller::Operator { cwd: &root }, scope, &name)
                    })
                    .map(|shown| shown.body)
                    .map_err(|error| error.to_string())
            },
            Err("reading the fact failed".to_owned()),
        );
        let _ = sender.send(FactBodyResolution {
            space,
            root,
            name,
            body,
        });
    });
}

/// Writes the overview's `workspace:` edits into `agents.yaml`, or has
/// them refused with every problem the workspace finds, off the frame: a
/// link is checked by asking Git whether it is ignored.
pub(super) fn spawn_declaration_save(
    home: &UzeHome,
    space: SpaceId,
    root: PathBuf,
    sent: SentEdits,
    sender: mpsc::Sender<DeclarationSaveResolution>,
) {
    let home = home.clone();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _span = tracing::info_span!(parent: &parent, "tui.declaration_save").entered();
        let edits = sent.edits();
        let outcome = answered_or(
            || {
                tui_application(home)
                    .and_then(|app| app.workspace().apply_declaration(&root, &edits))
                    .map_err(|error| error.to_string())
            },
            Err("saving failed".to_owned()),
        );
        let _ = sender.send(DeclarationSaveResolution {
            space,
            root,
            sent,
            outcome,
        });
    });
}
