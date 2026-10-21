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
    panes: BTreeMap<PaneId, usage::Sample>,
    server: usage::Sample,
    clients: usage::Sample,
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
                (*pane, usage::Sample::of(pids, &census, group.as_deref()))
            })
            .collect();
        let clients = watched
            .clients
            .iter()
            .flat_map(|&client| census.tree_of(client))
            .collect::<BTreeSet<_>>();
        Self {
            panes,
            // The server reaps every pane's program, so its children's time
            // is the panes', counted there.
            server: usage::Sample::alone(std::process::id()),
            clients: usage::Sample::of(clients, &census, None),
        }
    }
}

fn load(count: usize, samples: &[(&usage::Sample, &usage::Sample)]) -> Load {
    let mut load = Load {
        count: u32::try_from(count).unwrap_or(u32::MAX),
        ..Load::default()
    };
    for (before, after) in samples {
        load.cpu += after.spent_since(before);
        let memory = after.memory();
        load.memory += memory.bytes;
        load.estimated |= memory.estimated;
        load.processes += u32::try_from(after.len()).unwrap_or(u32::MAX);
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
        let mut watched = Watched {
            panes: self
                .runtimes()
                .into_iter()
                .map(|runtime| (runtime.id, runtime.leader, runtime.group.clone()))
                .collect(),
            clients: Vec::new(),
            asker,
        };
        let attached = {
            let clients = self.clients.lock().expect("clients poisoned");
            watched.clients = clients.iter().filter_map(|client| client.pid).collect();
            clients.len()
        };
        let opened = Instant::now();
        let before = Reading::take(&watched);
        thread::sleep(USAGE_WINDOW);
        let after = Reading::take(&watched);
        let window = opened.elapsed();
        let empty = usage::Sample::default();
        let of_kind = |kind: Kind| {
            let panes: Vec<(&usage::Sample, &usage::Sample)> = after
                .panes
                .iter()
                .filter(|(pane, _)| kinds.get(pane).is_some_and(|(of, _)| *of == kind))
                .map(|(pane, now)| (before.panes.get(pane).unwrap_or(&empty), now))
                .collect();
            let tabs = kinds
                .values()
                .filter(|(of, counted)| *of == kind && *counted)
                .count();
            load(tabs, &panes)
        };
        UsageReport {
            started: self
                .started
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            window,
            agents: of_kind(Kind::Agent),
            shells: of_kind(Kind::Shell),
            server: load(1, &[(&before.server, &after.server)]),
            clients: load(attached, &[(&before.clients, &after.clients)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpaceSeat;

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

    /// The process asking for usage from inside a pane, and what it
    /// started, are not that pane's: the report is the same wherever it is
    /// asked from.
    // Unix only: the asker is a POSIX shell that says its child's pid.
    #[cfg(unix)]
    #[test]
    fn the_process_asking_is_left_out_of_its_pane() {
        use std::io::BufRead;
        let mut asker = std::process::Command::new("sh")
            .args(["-c", "sleep 30 & echo $!; wait"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut said = std::io::BufReader::new(asker.stdout.take().unwrap()).lines();
        let started: u32 = said.next().unwrap().unwrap().trim().parse().unwrap();
        let watched = |asking: Option<u32>| Watched {
            panes: vec![(PaneId(1), Some(asker.id()), None)],
            clients: Vec::new(),
            asker: asking,
        };

        let counted = Reading::take(&watched(None));
        let asked = Reading::take(&watched(Some(asker.id())));

        let counted = &counted.panes[&PaneId(1)];
        assert!(counted.holds(asker.id()) && counted.holds(started));
        assert!(asked.panes[&PaneId(1)].is_empty());
        uze_platform::process::terminate(started);
        let _ = asker.kill();
        let _ = asker.wait();
    }
}
