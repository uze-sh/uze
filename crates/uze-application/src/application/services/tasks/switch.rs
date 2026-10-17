//! Continuing an agent on another harness.

use uze_core::integration::IntegrationPort;
use uze_workspace::{
    briefing::{self, GitFacts, Handover},
    continuity,
    conversation::{Briefing, ConversationRecord},
};

use super::*;

/// What the record said about the agent, read under the lock that switched it.
struct Switched {
    id: AgentId,
    label: String,
    directory: PathBuf,
    placement: Placement,
    base: Option<String>,
    launch_key: String,
}

impl Workspace<'_> {
    /// Moves `agent_id` from harness `from` to harness `to`, keeping its
    /// identity, its checkout, its branch and every harness's conversation,
    /// and answers the placement the client relaunches it at.
    ///
    /// The harness is changed only while it is still `from`, and in the
    /// same write that issues the launch its key: two clients switching one
    /// agent at once would otherwise each issue a key, and the process
    /// launched for the first would start as nobody's. Before
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
        let incoming = self
            .integration(to)
            .ok_or_else(|| refused(format!("`{to}` is not a harness UZE knows")))?;
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
            // In the same write as the harness: a key that cannot be
            // issued leaves the record naming the harness it had, rather
            // than one no launch is coming for.
            let launch_key = agent.issue_launch_key().map_err(|error| {
                UzeError::AgentPlacement(format!(
                    "the agent's launch could not be issued a key: {error}"
                ))
            })?;
            Ok(Ok(Switched {
                id: agent.id.clone(),
                label: agent.label.clone(),
                directory,
                placement,
                base,
                launch_key,
            }))
        })?
        .map_err(refused)?;

        self.brief(&root, &switched, self.integration(from), incoming);

        let view = self
            .repository_context(&root)
            .ok()
            .flatten()
            .and_then(|(primary, policy)| {
                self.placed_view(&primary, switched.id.as_str(), &policy)
            });
        Ok(AgentPlacement {
            project: root,
            cwd: switched.directory,
            placement: switched.placement,
            warnings: Vec::new(),
            told: Some(switched_notice(incoming)),
            awaiting_approval: None,
            view,
            launch_key: switched.launch_key,
        })
    }

    /// Records the agent's briefing for `incoming`, which every launch on
    /// that harness is given as part of the agent's context until the work
    /// moves past the commit it was switched at. Best-effort: a briefing
    /// that cannot be recorded is an agent that starts knowing only what its
    /// checkout says, never a switch refused.
    fn brief(
        &self,
        root: &Path,
        switched: &Switched,
        outgoing: Option<&dyn IntegrationPort>,
        incoming: &dyn IntegrationPort,
    ) {
        let owner = conversation::Owner {
            project_root: root.to_path_buf(),
            agent: switched.id.clone(),
            isolated: matches!(switched.placement, Placement::Isolated { .. }),
        };
        let session = outgoing.and_then(|outgoing| {
            continuity::conversation_now(&self.0.home, &owner, &switched.directory, outgoing)
        });
        let record = conversation::load(&self.0.home, root, &switched.id);
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
        // The next time the outgoing harness comes back, its delta starts
        // here.
        let recorded = conversation::update(&self.0.home, root, &switched.id, |stored| {
            if let Some(outgoing) = outgoing {
                stored.left(outgoing.id());
            }
            stored.briefing = Some(Briefing {
                integration: incoming.id().to_owned(),
                text,
                head_at_switch: git.head.clone(),
                first_turn_given: false,
            });
        });
        if let Err(error) = recorded {
            tracing::warn!(agent = %switched.id, %error, "the switch could not be recorded");
        }
    }
}

/// What the operator is told once a switch placed the agent: the incoming
/// harness opens with no turn of its own, so it waits for theirs.
fn switched_notice(incoming: &dyn IntegrationPort) -> String {
    if incoming.context_channel().is_degraded() || incoming.context_channel_unavailable().is_some()
    {
        format!(
            "Continued in {}, pointed at its briefing by one first turn.",
            incoming.display_name()
        )
    } else {
        format!(
            "Continued in {}. It holds the briefing and waits for your next message.",
            incoming.display_name()
        )
    }
}

/// When the agent last moved on from `incoming`, for a harness that resumes
/// a conversation of its own and only needs what it missed.
fn returning_since(record: &ConversationRecord, incoming: &str) -> Option<u64> {
    let own = record.get(incoming)?;
    own.conversation.as_ref()?;
    own.left_at_unix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UzeApplication;
    use uze_core::{
        capability::Resource,
        exposure::{ExposureMechanism, ExposurePlan},
        integration::{ObservationContext, SessionContinuity},
        router::{CompatibilityRoute, HarnessCapabilities},
        session::{ConversationDigest, SessionId, Speaker, Turn},
    };
    use uze_workspace::conversation::ConversationOrigin;

    /// A harness that names its own conversations — always `observed` when
    /// asked — and whose transcript always says the same thing.
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
        fn session_continuity(&self) -> SessionContinuity {
            SessionContinuity::Observed
        }
        fn observe_session(&self, _ctx: &ObservationContext) -> Option<SessionId> {
            Some(SessionId::new("observed"))
        }
        fn conversation_digest(
            &self,
            session: &SessionId,
            _cwd: &Path,
            since_unix: Option<u64>,
        ) -> Option<ConversationDigest> {
            Some(ConversationDigest {
                goal: Some(format!("the goal of {session}, since {since_unix:?}")),
                turns: vec![Turn {
                    speaker: Speaker::Operator,
                    text: format!("said in {session} since {since_unix:?}"),
                    at_unix: None,
                }],
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
        let store = task::load(&app.home, &root).unwrap();
        let agent = store.agent(id.as_str()).unwrap();
        assert_eq!(agent.harness, "right");
        let claim = Claim {
            id: id.as_str(),
            key: &placed.launch_key,
            cwd: &root,
        };
        assert!(
            conversation::owner_of(&app.home, claim).is_some(),
            "the key the placement carries is the one the switch recorded"
        );
        let record = conversation::load(&app.home, &root, &id);
        let pending = record.briefing.clone().expect("a briefing is pending");
        assert_eq!(pending.integration, "right");
        let text = pending.text.clone();
        assert!(text.contains("the goal of c1, since None"), "{text}");
        assert!(
            record.get("left").is_some(),
            "the conversation left behind is kept"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    /// Switched away before any refresh read the conversation back: the
    /// switch asks the harness itself rather than briefing from Git alone.
    #[test]
    fn a_conversation_not_read_back_yet_is_observed_for_the_briefing() {
        let app = application("switch-pending-home");
        let project = project("switch-pending");
        let root = canonical(&project);
        let id = agent_on_left(&app, &project);
        conversation::update(&app.home, &root, &id, |record| {
            record.launched("left", ConversationOrigin::Observed, None, None);
        })
        .unwrap();

        app.workspace()
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();

        let record = conversation::load(&app.home, &root, &id);
        assert_eq!(
            record.get("left").unwrap().conversation,
            Some(SessionId::new("observed")),
            "the observation is recorded, not only used"
        );
        let text = record.briefing.unwrap().text;
        assert!(text.contains("the goal of observed"), "{text}");
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

    /// What the launch `right`'s first switch causes would record.
    fn right_launched(app: &UzeApplication, root: &Path, id: &AgentId) {
        conversation::update(&app.home, root, id, |record| {
            record.briefing = None;
            record.launched(
                "right",
                ConversationOrigin::Assigned,
                Some(SessionId::new("c2")),
                None,
            );
        })
        .unwrap();
    }

    fn briefing_text(app: &UzeApplication, root: &Path, id: &AgentId) -> String {
        let record = conversation::load(&app.home, root, id);
        record.briefing.unwrap().text
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
        right_launched(&app, &root, &id);

        app.workspace()
            .switch_harness(&project, id.as_str(), "right", "left")
            .unwrap();

        let record = conversation::load(&app.home, &root, &id);
        let left = record.get("left").unwrap().left_at_unix;
        assert!(left.is_some(), "the first switch marked when it left");
        let text = briefing_text(&app, &root, &id);
        assert!(text.contains("You worked on this agent before"), "{text}");
        assert!(
            !text.contains("the goal of c2"),
            "a returning harness is not told the goal again: {text}"
        );
        assert!(
            text.contains(&format!("said in c2 since {left:?}")),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    /// A resume records no launch, so a harness that comes back a second
    /// time is told what it missed by when it left, not by when it was
    /// first launched.
    #[test]
    fn a_harness_returning_a_second_time_is_still_briefed_on_what_it_missed() {
        let app = application("switch-cycle-home");
        let project = project("switch-cycle");
        let root = canonical(&project);
        let id = agent_on_left(&app, &project);
        let workspace = app.workspace();
        workspace
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();
        right_launched(&app, &root, &id);
        workspace
            .switch_harness(&project, id.as_str(), "right", "left")
            .unwrap();

        workspace
            .switch_harness(&project, id.as_str(), "left", "right")
            .unwrap();

        let record = conversation::load(&app.home, &root, &id);
        let right_left = record.get("right").unwrap().left_at_unix;
        assert_eq!(returning_since(&record, "right"), right_left);
        let text = briefing_text(&app, &root, &id);
        assert!(text.contains("You worked on this agent before"), "{text}");
        assert!(
            text.contains(&format!("said in c1 since {right_left:?}")),
            "{text}"
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
        let text = record.briefing.unwrap().text;
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
