//! What the runtime's processes use, sampled over one window when asked:
//! nothing is read while nobody asks.

use std::collections::{BTreeMap, BTreeSet};

use uze_platform::{process::pane, usage};

use super::*;
use crate::{Load, USAGE_WINDOW, launch};

/// How long a sample answers the requests after it. A person runs the
/// command and reads it; something asking in a loop gets the same answer
/// rather than keeping the server reading every process's environment.
pub(super) const USAGE_REUSED_FOR: Duration = Duration::from_secs(5);

/// Which total of a [`UsageReport`] a tab counts in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Agent,
    Shell,
}

/// Every pane's total: an agent tab's own, and a shell born of it with it;
/// every other tab, a finished agent's included, as a shell. A pane counted
/// in `tabs` is one the tab total covers.
pub(super) fn kinds(session: &Session) -> BTreeMap<PaneId, (Kind, bool)> {
    let tabs: Vec<&crate::Tab> = session
        .workspace
        .spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .collect();
    let agents: BTreeSet<TabId> = tabs
        .iter()
        .filter(|tab| is_agent(tab))
        .map(|tab| tab.id)
        .collect();
    tabs.iter()
        .map(|tab| {
            let entry = if agents.contains(&tab.id) {
                (Kind::Agent, true)
            } else if tab.agent.is_some_and(|agent| agents.contains(&agent)) {
                (Kind::Agent, false)
            } else {
                (Kind::Shell, true)
            };
            (tab.pane.id, entry)
        })
        .collect()
}

fn is_agent(tab: &crate::Tab) -> bool {
    tab.env
        .iter()
        .any(|(name, _)| name == launch::AGENT_IDENTITY_VARIABLE)
}

/// One reading of a set of processes.
#[derive(Default)]
struct Sample {
    processes: BTreeMap<u32, usage::Reading>,
    /// Who started each process, where the census says.
    parents: BTreeMap<u32, u32>,
    /// The total a Job Object keeps of everything it held, ended processes
    /// included, where there is one.
    accounted: Option<Duration>,
}

impl Sample {
    fn of(
        pids: impl IntoIterator<Item = u32>,
        accounted: Option<Duration>,
        census: &pane::Census,
    ) -> Self {
        let processes: BTreeMap<u32, usage::Reading> = pids
            .into_iter()
            .filter_map(|pid| Some((pid, usage::of(pid)?)))
            .collect();
        let parents = processes
            .keys()
            .filter_map(|&pid| Some((pid, census.parent_of(pid)?)))
            .collect();
        Self {
            processes,
            parents,
            accounted,
        }
    }

    /// The CPU spent between `before` and this reading.
    ///
    /// A process in both is its own and its reaped children's time since,
    /// so a compiler that lived half a second is carried by the build that
    /// waited for it. One only in this reading started inside the window
    /// when its stamp is newer than every stamp `before` had, and counts in
    /// full; otherwise the first reading missed it, and its lifetime is not
    /// one window's. One only in `before` ended, and the parent that
    /// waited for it gains its whole lifetime as reaped time: what it had
    /// spent by the first reading is taken back off that parent.
    fn spent_since(&self, before: &Sample, newest_before: Option<u64>) -> Duration {
        if let (Some(then), Some(now)) = (before.accounted, self.accounted) {
            return now.saturating_sub(then);
        }
        let total =
            |reading: &usage::Reading| reading.cpu.own + reading.cpu.children.unwrap_or_default();
        let lives_on = |pid: &u32, then: &usage::Reading| {
            self.processes
                .get(pid)
                .is_some_and(|now| now.started == then.started)
        };
        let spent_before_ending = |parent: u32| -> Duration {
            before
                .processes
                .iter()
                .filter(|(pid, then)| {
                    !lives_on(pid, then) && before.parents.get(pid) == Some(&parent)
                })
                .map(|(_, then)| total(then))
                .sum()
        };
        self.processes
            .iter()
            .map(|(pid, now)| match before.processes.get(pid) {
                Some(then) if then.started == now.started => {
                    total(now).saturating_sub(total(then) + spent_before_ending(*pid))
                }
                _ if started_after(now.started, newest_before) => total(now),
                _ => Duration::ZERO,
            })
            .sum()
    }

    fn newest(&self) -> Option<u64> {
        self.processes
            .values()
            .filter_map(|reading| reading.started)
            .max()
    }
}

fn started_after(started: Option<u64>, newest_before: Option<u64>) -> bool {
    matches!((started, newest_before), (Some(started), Some(newest)) if started > newest)
}

/// What one sample is taken over: each pane's program and group, and the
/// clients' processes.
struct Watched {
    panes: Vec<(PaneId, Option<u32>, Option<Arc<pane::Group>>)>,
    clients: Vec<u32>,
    /// The process asking, left out with everything it started: a
    /// `uze usage` typed into a pane, and the disk walk it runs while the
    /// server samples, would otherwise count in that pane's row.
    asker: Option<u32>,
}

/// Everything the runtime runs, read once.
struct Reading {
    panes: BTreeMap<PaneId, Sample>,
    server: Sample,
    clients: Sample,
}

impl Reading {
    fn take(watched: &Watched) -> Self {
        let census = pane::census(launch::PANE_UNIT_VARIABLE);
        // The server's own tree is every pane: a request it made of itself
        // leaves nothing out.
        let asking: BTreeSet<u32> = watched
            .asker
            .filter(|&asker| asker != std::process::id())
            .map(|asker| census.tree_of(asker).into_iter().collect())
            .unwrap_or_default();
        let panes = watched
            .panes
            .iter()
            .map(|(pane, leader, group)| {
                let pids = leader
                    .map(|leader| census.of_pane(leader, group.as_deref()))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|pid| !asking.contains(pid));
                let accounted = group.as_ref().and_then(|group| group.cpu_time());
                (*pane, Sample::of(pids, accounted, &census))
            })
            .collect();
        let clients = watched
            .clients
            .iter()
            .flat_map(|&client| census.tree_of(client))
            .collect::<BTreeSet<_>>();
        let mut server = Sample::of([std::process::id()], None, &census);
        // The server reaps every pane's program, so its children's time is
        // the panes', counted there.
        for reading in server.processes.values_mut() {
            reading.cpu.children = None;
        }
        Self {
            panes,
            server,
            clients: Sample::of(clients, None, &census),
        }
    }

    fn newest(&self) -> Option<u64> {
        self.panes
            .values()
            .chain([&self.server, &self.clients])
            .filter_map(Sample::newest)
            .max()
    }
}

fn load(count: usize, samples: &[(&Sample, &Sample)], newest_before: Option<u64>) -> Load {
    let mut load = Load {
        count: u32::try_from(count).unwrap_or(u32::MAX),
        ..Load::default()
    };
    for (before, after) in samples {
        load.cpu += after.spent_since(before, newest_before);
        for reading in after.processes.values() {
            load.memory += reading.memory.bytes;
            load.estimated |= reading.memory.estimated;
        }
        load.processes += u32::try_from(after.processes.len()).unwrap_or(u32::MAX);
    }
    load
}

impl Server {
    /// What the runtime's processes used over the last [`USAGE_WINDOW`]. A
    /// request that arrives while another samples waits for it, and one
    /// within [`USAGE_REUSED_FOR`] of a sample is given that sample's
    /// answer: a loop asking cannot keep the server reading the process
    /// table.
    pub(super) fn usage(&self, asker: Option<u32>) -> UsageReport {
        let mut sampled = self.sampled.lock().expect("usage sample poisoned");
        if let Some((at, report)) = sampled.as_ref()
            && at.elapsed() < USAGE_REUSED_FOR
        {
            return report.clone();
        }
        let report = self.sample_usage(asker);
        *sampled = Some((Instant::now(), report.clone()));
        report
    }

    /// Two readings a window apart, with the runtime's locks held only to
    /// copy what is read: the panes' output never waits on a sample.
    fn sample_usage(&self, asker: Option<u32>) -> UsageReport {
        let kinds = kinds(&self.session.lock().expect("session poisoned"));
        let watched = Watched {
            panes: self
                .runtimes()
                .into_iter()
                .map(|runtime| (runtime.id, runtime.leader, runtime.group.clone()))
                .collect(),
            clients: self
                .clients
                .lock()
                .expect("clients poisoned")
                .iter()
                .filter_map(|client| client.pid)
                .collect(),
            asker,
        };
        let attached = self.clients.lock().expect("clients poisoned").len();
        let opened = Instant::now();
        let before = Reading::take(&watched);
        thread::sleep(USAGE_WINDOW);
        let after = Reading::take(&watched);
        let window = opened.elapsed();
        let newest = before.newest();
        let empty = Sample::default();
        let of_kind = |kind: Kind| {
            let panes: Vec<(&Sample, &Sample)> = after
                .panes
                .iter()
                .filter(|(pane, _)| kinds.get(pane).is_some_and(|(of, _)| *of == kind))
                .map(|(pane, now)| (before.panes.get(pane).unwrap_or(&empty), now))
                .collect();
            let tabs = kinds
                .values()
                .filter(|(of, counted)| *of == kind && *counted)
                .count();
            load(tabs, &panes, newest)
        };
        UsageReport {
            started: self
                .started
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            window,
            agents: of_kind(Kind::Agent),
            shells: of_kind(Kind::Shell),
            server: load(1, &[(&before.server, &after.server)], newest),
            clients: load(attached, &[(&before.clients, &after.clients)], newest),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpaceSeat;
    use uze_platform::process::CpuTime;

    fn session_of(tabs: impl FnOnce(&mut Session, SpaceId, TabId)) -> Session {
        let mut session = Session::new(
            SpaceSeat {
                project: PathBuf::from("/project"),
            },
            80,
            24,
        );
        let space = session.workspace.selected_space;
        let bootstrap = session.selected_tab().id;
        tabs(&mut session, space, bootstrap);
        session
    }

    fn stamp_agent(session: &mut Session, pane: PaneId) {
        let tab = session
            .workspace
            .spaces
            .iter_mut()
            .flat_map(|space| &mut space.tabs)
            .find(|tab| tab.pane.id == pane)
            .unwrap();
        tab.env = vec![
            (launch::AGENT_IDENTITY_VARIABLE.into(), "abc".into()),
            (launch::AGENT_KEY_VARIABLE.into(), "secret".into()),
        ];
    }

    fn tab_of(session: &Session, pane: PaneId) -> TabId {
        session
            .workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .find(|tab| tab.pane.id == pane)
            .unwrap()
            .id
    }

    /// An agent's shells count with it, and every other tab — the space's
    /// own shell, a finished agent's — as a shell.
    #[test]
    fn a_shell_born_of_an_agent_counts_with_it() {
        let mut panes = Vec::new();
        let session = session_of(|session, space, _| {
            let agent = session.add_tab(space, "agent".into(), None, 80, 24, "/p".into());
            stamp_agent(session, agent);
            let agent_tab = tab_of(session, agent);
            let born = session.add_tab(space, "sh".into(), Some(agent_tab), 80, 24, "/p".into());
            let finished = session.add_tab(space, "done".into(), None, 80, 24, "/p".into());
            let finished_tab = tab_of(session, finished);
            let born_of_finished =
                session.add_tab(space, "sh".into(), Some(finished_tab), 80, 24, "/p".into());
            panes = vec![agent, born, finished, born_of_finished];
        });

        let kinds = kinds(&session);

        assert_eq!(kinds[&panes[0]], (Kind::Agent, true));
        assert_eq!(kinds[&panes[1]], (Kind::Agent, false));
        assert_eq!(kinds[&panes[2]], (Kind::Shell, true));
        assert_eq!(kinds[&panes[3]], (Kind::Shell, true));
        let bootstrap = session.workspace.spaces[0].tabs[0].pane.id;
        assert_eq!(kinds[&bootstrap], (Kind::Shell, true));
    }

    fn reading(started: u64, own: u64, children: u64, memory: u64) -> usage::Reading {
        usage::Reading {
            started: Some(started),
            cpu: CpuTime {
                own: Duration::from_millis(own),
                children: Some(Duration::from_millis(children)),
            },
            memory: usage::Memory {
                bytes: memory,
                estimated: false,
            },
        }
    }

    fn sample(processes: &[(u32, usage::Reading)]) -> Sample {
        Sample {
            processes: processes.iter().copied().collect(),
            ..Sample::default()
        }
    }

    /// What a window spent: a survivor's own and reaped children's time
    /// since, a process started inside the window in full, and nothing of
    /// one the first reading missed or of a pid handed to a new process.
    #[test]
    fn a_window_counts_only_what_was_spent_inside_it() {
        let before = sample(&[
            (10, reading(100, 1_000, 0, 0)),
            (11, reading(101, 5_000, 0, 0)),
        ]);
        let after = sample(&[
            // A build tool that waited for a compiler in the window.
            (10, reading(100, 1_100, 400, 0)),
            // Its pid now names a later process, started in the window.
            (11, reading(150, 30, 0, 0)),
            // Started in the window.
            (12, reading(140, 200, 0, 0)),
            // Missed by the first reading: older than its newest stamp.
            (13, reading(50, 90_000, 0, 0)),
        ]);

        let spent = after.spent_since(&before, before.newest());

        assert_eq!(spent, Duration::from_millis(100 + 400 + 30 + 200));
    }

    /// A child alive before the window that ends inside it, reaped by a
    /// parent still running, lands its whole lifetime in the parent's
    /// children time; only what it spent after the first reading is the
    /// window's.
    #[test]
    fn a_child_reaped_inside_the_window_counts_only_its_time_inside_it() {
        let mut before = sample(&[
            (10, reading(100, 1_000, 0, 0)),
            (11, reading(101, 30_000, 0, 0)),
        ]);
        before.parents.insert(11, 10);
        let after = sample(&[(10, reading(100, 1_000, 30_400, 0))]);

        let spent = after.spent_since(&before, before.newest());

        assert_eq!(spent, Duration::from_millis(400));
    }

    /// The process asking for usage from inside a pane, and what it
    /// started, are not that pane's: the report is the same wherever it is
    /// asked from.
    #[cfg(unix)]
    #[test]
    fn the_process_asking_is_left_out_of_its_pane() {
        let mut asker = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let watched = |asking: Option<u32>| Watched {
            panes: vec![(PaneId(1), Some(asker.id()), None)],
            clients: Vec::new(),
            asker: asking,
        };

        let counted = Reading::take(&watched(None));
        let asked = Reading::take(&watched(Some(asker.id())));

        assert!(
            counted.panes[&PaneId(1)]
                .processes
                .contains_key(&asker.id())
        );
        assert!(asked.panes[&PaneId(1)].processes.is_empty());
        let _ = asker.kill();
        let _ = asker.wait();
    }

    /// Where a Job Object keeps a total, it is the whole answer: it counts
    /// every process the job held, however short.
    #[test]
    fn a_job_total_is_the_whole_answer() {
        let mut before = sample(&[(10, reading(100, 1_000, 0, 0))]);
        let mut after = sample(&[(10, reading(100, 1_100, 0, 0))]);
        before.accounted = Some(Duration::from_millis(2_000));
        after.accounted = Some(Duration::from_millis(2_750));

        assert_eq!(
            after.spent_since(&before, before.newest()),
            Duration::from_millis(750)
        );
    }

    #[test]
    fn a_load_sums_memory_and_counts_processes_at_the_end() {
        let before = sample(&[(10, reading(100, 0, 0, 10))]);
        let after = sample(&[(10, reading(100, 0, 0, 30)), (12, reading(140, 0, 0, 12))]);

        let load = load(2, &[(&before, &after)], before.newest());

        assert_eq!(load.count, 2);
        assert_eq!(load.memory, 42);
        assert_eq!(load.processes, 2);
        assert!(!load.estimated);
    }
}
