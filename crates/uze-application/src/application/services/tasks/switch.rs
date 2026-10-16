//! Continuing an agent on another harness.

use uze_core::integration::IntegrationPort;
use uze_workspace::{
    briefing::{self, GitFacts, Handover},
    conversation::{ConversationRecord, PendingBriefing},
};

use super::*;

/// What the record said about the agent, read under the lock that switched it.
struct Switched {
    id: AgentId,
    label: String,
    directory: PathBuf,
    placement: Placement,
    base: Option<String>,
}

impl Workspace<'_> {
    /// Moves `agent_id` from harness `from` to harness `to`, keeping its
    /// identity, its checkout, its branch and every harness's conversation,
    /// and answers the placement the client relaunches it at.
    ///
    /// The harness is changed only while it is still `from`: two clients
    /// switching one agent at once would otherwise each issue a key, and the
    /// process launched for the first would start as nobody's. Before
    /// answering, it writes the agent's briefing and leaves it pending for
    /// `to`, whose first launch is pointed at it once.
    #[tracing::instrument(name = "workspace.switch_harness", skip_all, fields(cwd = %cwd.display(), agent = %agent_id, from = %from, to = %to), err)]
    pub fn switch_harness(
        &self,
        cwd: &Path,
        agent_id: &str,
        from: &str,
        to: &str,
    ) -> Result<AgentPlacement> {
        let refused = |reason: String| UzeError::AgentPlacement(reason);
        let integration = |id: &str| {
            self.0
                .integrations
                .iter()
                .find(|candidate| candidate.id() == id)
                .map(|found| found.as_ref())
        };
        let incoming =
            integration(to).ok_or_else(|| refused(format!("`{to}` is not a harness UZE knows")))?;
        let root = conversation::project_of(&self.0.home, cwd)
            .map(|root| canonical(&root))
            .ok_or_else(|| refused("this agent is not one UZE launched here".to_owned()))?;

        let switched = task::locked(&self.0.home, &root, |store| {
            let Some(agent) = store.agent_mut(agent_id) else {
                return Ok(Err("this agent is not one UZE launched here".to_owned()));
            };
            if agent.harness != from {
                return Ok(Err(format!(
                    "this agent already continued in {}",
                    agent.harness
                )));
            }
            let (directory, placement, base) = match &agent.isolation {
                Some(isolation) => {
                    let (Some(directory), Some(checkout)) = (
                        landing::slot_path(&root, isolation),
                        isolation.checkout.clone(),
                    ) else {
                        return Ok(Err(
                            "this agent's checkout is gone; resume it instead".to_owned()
                        ));
                    };
                    (
                        directory,
                        Placement::Isolated {
                            task: agent.id.clone(),
                            checkout,
                            branch: isolation.branch.clone(),
                            reused: true,
                        },
                        Some(isolation.base_commit.clone()).filter(|base| !base.is_empty()),
                    )
                }
                None => (
                    root.clone(),
                    Placement::InPlace {
                        id: agent.id.clone(),
                    },
                    None,
                ),
            };
            agent.harness = to.to_owned();
            Ok(Ok(Switched {
                id: agent.id.clone(),
                label: agent.label.clone(),
                directory,
                placement,
                base,
            }))
        })?
        .map_err(refused)?;

        let briefing = self.brief(&root, &switched, integration(from), incoming);
        let mut warnings = Vec::new();
        if let Some(path) = briefing
            && incoming.first_turn_args("").is_empty()
        {
            warnings.push(format!(
                "{} cannot be pointed at its briefing; ask it to read {}",
                incoming.display_name(),
                path.display()
            ));
        }

        let view = self
            .repository_context(&root)
            .ok()
            .flatten()
            .and_then(|(primary, policy)| {
                self.placed_view(&primary, switched.id.as_str(), &policy)
            });
        self.keyed(AgentPlacement {
            project: root,
            cwd: switched.directory,
            placement: switched.placement,
            warnings,
            awaiting_approval: None,
            view,
            launch_key: String::new(),
        })
    }

    /// Writes the agent's briefing and leaves it pending for `incoming`,
    /// answering where it is. Best-effort: a briefing that cannot be written is an agent that
    /// starts knowing only what its checkout says, never a switch refused.
    fn brief(
        &self,
        root: &Path,
        switched: &Switched,
        outgoing: Option<&dyn IntegrationPort>,
        incoming: &dyn IntegrationPort,
    ) -> Option<PathBuf> {
        let record = conversation::load(&self.0.home, root, &switched.id);
        let session = outgoing
            .and_then(|outgoing| record.get(outgoing.id()))
            .and_then(|entry| entry.conversation.clone());
        let returning_since = returning_since(&record, incoming.id());
        let digest = outgoing
            .zip(session.as_ref())
            .and_then(|(outgoing, session)| {
                outgoing.conversation_digest(session, &switched.directory, returning_since)
            });
        let git = GitFacts::read(
            &switched.directory,
            switched.base.as_deref(),
            returning_since,
        );
        let text = briefing::render(
            &Handover {
                label: &switched.label,
                from: outgoing.map_or("the previous harness", |outgoing| outgoing.display_name()),
                to: incoming.display_name(),
                session: session.as_ref(),
                returning_since,
            },
            &git,
            digest.as_ref(),
        );
        let written =
            briefing::write(&self.0.home, root, switched.id.as_str(), &text).and_then(|path| {
                conversation::update(&self.0.home, root, &switched.id, |stored| {
                    stored.briefing = Some(PendingBriefing {
                        integration: incoming.id().to_owned(),
                        path: path.clone(),
                    });
                })
                .map(|()| path)
            });
        written
            .inspect_err(|error| {
                tracing::warn!(agent = %switched.id, %error, "the briefing could not be written");
            })
            .ok()
    }
}

/// When `incoming` last stopped working on this agent — the launch of the
/// first harness that ran after it — for a harness that resumes a
/// conversation of its own and only needs what it missed.
fn returning_since(record: &ConversationRecord, incoming: &str) -> Option<u64> {
    let own = record.get(incoming)?;
    own.conversation.as_ref()?;
    record
        .harnesses
        .iter()
        .filter(|(id, _)| id.as_str() != incoming)
        .map(|(_, entry)| entry.launched_at_unix)
        .filter(|launched| *launched >= own.launched_at_unix)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UzeApplication;
    use uze_core::{
        capability::Resource,
        exposure::{ExposureMechanism, ExposurePlan},
        router::{CompatibilityRoute, HarnessCapabilities},
        session::{ConversationDigest, SessionId},
    };
    use uze_workspace::conversation::ConversationOrigin;

    /// A harness whose transcript always says the same thing.
    struct Harness(&'static str);

    impl IntegrationPort for Harness {
        fn id(&self) -> &'static str {
            self.0
        }
        fn display_name(&self) -> &'static str {
            self.0
        }
        fn capabilities(&self) -> HarnessCapabilities {
            HarnessCapabilities::default()
        }
        fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
            ExposurePlan {
                route: CompatibilityRoute::Adaptable,
                mechanism: ExposureMechanism::Unsupported {
                    rationale: "test does not attach".to_owned(),
                },
                evidence: "test".to_owned(),
            }
        }
        fn conversation_digest(
            &self,
            session: &SessionId,
            _cwd: &Path,
            since_unix: Option<u64>,
        ) -> Option<ConversationDigest> {
            Some(ConversationDigest {
                goal: Some(format!("the goal of {session}, since {since_unix:?}")),
                ..ConversationDigest::default()
            })
        }
    }

    fn application(label: &str) -> UzeApplication {
        UzeApplication::new(
            UzeHome::at(uze_testkit::temp::scratch(label)),
            vec![Box::new(Harness("left")), Box::new(Harness("right"))],
        )
    }

    /// An agent in `project`'s root that ran on `left` in conversation `c1`.
    fn agent_on_left(app: &UzeApplication, project: &Path) -> AgentId {
        let root = canonical(project);
        uze_core::record::ensure(&app.home, &root).unwrap();
        let agent = Agent::in_the_root("left");
        let id = agent.id.clone();
        task::locked(&app.home, &root, |store| {
            store.upsert(agent);
            Ok(())
        })
        .unwrap();
        conversation::update(&app.home, &root, &id, |record| {
            record.launched(
                "left",
                ConversationOrigin::Assigned,
                Some(SessionId::new("c1")),
                None,
            );
        })
        .unwrap();
        id
    }

    fn project(label: &str) -> PathBuf {
        let project = uze_testkit::temp::scratch(label);
        std::fs::create_dir_all(&project).unwrap();
        project
    }

    #[test]
    fn a_switched_agent_runs_the_new_harness_in_the_same_place_with_a_briefing_pending() {
        let app = application("switch-home");
        let project = project("switch");
        let id = agent_on_left(&app, &project);

        let placed = app
            .workspace()
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();

        let root = canonical(&project);
        assert_eq!(placed.cwd, root);
        assert!(!placed.launch_key.is_empty());
        assert!(
            placed.warnings[0].contains("right cannot be pointed at its briefing; ask it to read"),
            "a harness that takes no first turn is pointed at it by the operator: {:?}",
            placed.warnings
        );
        let store = task::load(&app.home, &root).unwrap();
        assert_eq!(store.agent(id.as_str()).unwrap().harness, "right");
        let record = conversation::load(&app.home, &root, &id);
        let pending = record.briefing.clone().expect("a briefing is pending");
        assert_eq!(pending.integration, "right");
        let text = std::fs::read_to_string(&pending.path).unwrap();
        assert!(text.contains("the goal of c1, since None"), "{text}");
        assert!(
            record.get("left").is_some(),
            "the conversation left behind is kept"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn a_second_switch_from_the_same_harness_is_reported_as_superseded() {
        let app = application("switch-twice-home");
        let project = project("switch-twice");
        let id = agent_on_left(&app, &project);
        app.workspace()
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();

        let refusal = app
            .workspace()
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap_err()
            .to_string();

        assert!(refusal.contains("already continued in right"), "{refusal}");
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn switching_back_briefs_the_returning_harness_only_on_what_it_missed() {
        let app = application("switch-back-home");
        let project = project("switch-back");
        let root = canonical(&project);
        let id = agent_on_left(&app, &project);
        app.workspace()
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();
        conversation::update(&app.home, &root, &id, |record| {
            record.briefing = None;
            record.launched(
                "right",
                ConversationOrigin::Assigned,
                Some(SessionId::new("c2")),
                None,
            );
        })
        .unwrap();
        let right_launched = conversation::load(&app.home, &root, &id)
            .get("right")
            .unwrap()
            .launched_at_unix;

        app.workspace()
            .switch_harness(&project, id.as_str(), "right", "left")
            .unwrap();

        let record = conversation::load(&app.home, &root, &id);
        let text = std::fs::read_to_string(record.briefing.unwrap().path).unwrap();
        assert!(text.contains("You worked on this agent before"), "{text}");
        assert!(
            !text.contains("the goal of c2"),
            "a returning harness is not told the goal again: {text}"
        );
        assert_eq!(
            returning_since(&conversation::load(&app.home, &root, &id), "left"),
            Some(right_launched)
        );
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn an_isolated_agent_continues_in_its_own_checkout_and_is_briefed_on_its_commits() {
        let repository = uze_testkit::git::Repository::new("switch-isolated");
        let root = repository.root().to_path_buf();
        let app = application("switch-isolated-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "left", &[])
            .unwrap();
        let id = placed.placement.agent().clone();
        std::fs::write(placed.cwd.join("work.rs"), b"fn work() {}").unwrap();
        repository.git_in(&placed.cwd, &["add", "."]);
        repository.git_in(&placed.cwd, &["commit", "-qm", "feat: the work so far"]);

        let switched = app
            .workspace()
            .switch_harness(&root, id.as_str(), "left", "right")
            .unwrap();

        assert_eq!(switched.cwd, placed.cwd);
        assert!(matches!(switched.placement, Placement::Isolated { .. }));
        let record = conversation::load(&app.home, &canonical(&root), &id);
        let text = std::fs::read_to_string(record.briefing.unwrap().path).unwrap();
        assert!(text.contains("feat: the work so far"), "{text}");
        assert!(text.contains("work.rs"), "{text}");
    }

    #[test]
    fn a_harness_nobody_registered_is_refused() {
        let app = application("switch-unknown-home");
        let project = project("switch-unknown");
        let id = agent_on_left(&app, &project);

        let refusal = app
            .workspace()
            .switch_harness(&project, id.as_str(), "left", "nowhere")
            .unwrap_err()
            .to_string();

        assert!(refusal.contains("not a harness UZE knows"), "{refusal}");
        let _ = std::fs::remove_dir_all(project);
    }
}
