//! The one place a declared extension's source is answered
//! (`add-extension-platform` D3, D4).
//!
//! An extension's engine asks a question (its identity, a source id, the
//! input and the checkout) and never says what runs. The runner reads the
//! extension's directory once, finds the source in that compile, and only
//! then knows whether a fixture or a script answers it: a fixture from the
//! bytes it read, a script only while the operator consented to those
//! bytes and enabled the extension in the checkout's project, run over the
//! checkout and nothing else.
//!
//! What a source's script answered is remembered for the session, in
//! memory, by (identity, source, checkout, input digest) and the digest of
//! the files it ran from: two surfaces asking the same question share one
//! run, and a save of the extension is a new question. A refresh runs the
//! script and replaces what was remembered; an action's script runs on
//! every gesture, under the same consent, and is never remembered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use uze_contract::{Answer, Asking, Does, Envelope, Identity, Question, SourceKind};
use uze_core::capability::extension::Loaded;
use uze_script::{Grants, ScriptCache};

use super::{CheckoutReach, ExtensionCheckout, ExtensionState, NotACheckout};
use crate::application::services::Extensions;

/// What a session's answer is remembered by.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Asked {
    identity: Identity,
    /// What was asked: a source's answer is remembered and an action's
    /// never is, and a refresh runs whatever is remembered.
    asking: Asking,
    declaration: String,
    checkout: PathBuf,
    /// `sha256:` of the input as JSON.
    input: String,
    /// The digest of the extension's files the script ran from.
    files: String,
}

/// Answers declared extensions' questions for one session.
#[derive(Debug, Default)]
pub struct ExtensionRunner {
    answered: Mutex<HashMap<Asked, Envelope>>,
    scripts: Mutex<ScriptCache>,
    runs: AtomicU64,
}

impl ExtensionRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times a script ran this session: a remembered answer is
    /// not a run.
    pub fn runs(&self) -> u64 {
        self.runs.load(Ordering::Relaxed)
    }

    /// Answers `question` from what is installed and consented to now.
    #[tracing::instrument(name = "extensions.answer", skip_all, fields(extension = %question.identity, source = %question.declaration))]
    pub fn answer(&self, extensions: &Extensions<'_>, question: &Question) -> Answer {
        self.answer_over(extensions, question, |path| extensions.checkout(path))
    }

    /// [`Self::answer`], with the checkout resolved by `checkout`.
    pub(crate) fn answer_over(
        &self,
        extensions: &Extensions<'_>,
        question: &Question,
        checkout: impl FnOnce(&Path) -> Result<ExtensionCheckout, NotACheckout>,
    ) -> Answer {
        let identity = &question.identity;
        let Some(installed) = extensions
            .installed()
            .into_iter()
            .find(|installed| installed.identity == *identity)
        else {
            return Answer::withheld(format!("no installed plugin carries {identity}"));
        };
        let (load, files) = match extensions.load_with_files(&installed) {
            Ok(read) => read,
            Err(diagnostics) => {
                let first = diagnostics
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                return Answer::withheld(format!("it does not compile: {first}"));
            }
        };
        let script = match question.asking {
            Asking::Source | Asking::Refresh => {
                let Some(source) = load.compiled.manifest.source(&question.declaration) else {
                    return failed(format!("it declares no source `{}`", question.declaration));
                };
                match &source.kind {
                    SourceKind::Fixture(file) => {
                        return match super::compiled::loaded_fixture(&files, file) {
                            Ok(data) => Answer::Answered {
                                envelope: Envelope::data(data),
                                trace: Vec::new(),
                            },
                            Err(reason) => failed(reason),
                        };
                    }
                    SourceKind::Run(_) => {
                        return Answer::withheld("programs arrive with a later change (change 5)");
                    }
                    SourceKind::Script(file) => file,
                }
            }
            Asking::Action => {
                let Some(action) = load.compiled.manifest.action(&question.declaration) else {
                    return failed(format!("it declares no action `{}`", question.declaration));
                };
                match &action.does {
                    Does::Script(file) => file,
                    _ => {
                        return failed(format!(
                            "the action `{}` runs no script",
                            question.declaration
                        ));
                    }
                }
            }
        };
        let Some(asked) = &question.checkout else {
            return Answer::withheld("no checkout is in front to read");
        };
        let checkout = match checkout(asked) {
            Ok(checkout) => checkout,
            Err(refused) => return Answer::withheld(refused.reason()),
        };
        let project = crate::project_root_of(&checkout.root);
        match extensions.judged(&load, &project) {
            Ok(ExtensionState::Enabled) => {}
            Ok(state) => return Answer::withheld(withheld_because(&state)),
            Err(error) => return Answer::withheld(error.to_string()),
        }
        let grants: Grants = load.compiled.manifest.wants.iter().copied().collect();
        let asked = Asked {
            identity: identity.clone(),
            asking: question.asking,
            declaration: question.declaration.clone(),
            checkout: checkout.root.clone(),
            input: input_digest(&question.input),
            files: load.digest.clone(),
        };
        self.run_script(asked, &files, script, &grants, &question.input)
    }

    fn run_script(
        &self,
        asked: Asked,
        files: &Loaded,
        file: &str,
        grants: &Grants,
        input: &serde_json::Value,
    ) -> Answer {
        let remembered = Asked {
            asking: Asking::Source,
            ..asked.clone()
        };
        if asked.asking == Asking::Source
            && let Some(envelope) = lock(&self.answered).get(&remembered)
        {
            return Answer::Answered {
                envelope: envelope.clone(),
                trace: Vec::new(),
            };
        }
        let Some(bytes) = files.bytes(file) else {
            return failed(format!("{file}: not found"));
        };
        let compiled = lock(&self.scripts).get_or_compile(&uze_core::digest::sha256(bytes), bytes);
        let script = match compiled {
            Ok(script) => script,
            Err(error) => return failed(format!("{file}: {error}")),
        };
        let declared = match asked.asking {
            Asking::Action => "action",
            Asking::Source | Asking::Refresh => "source",
        };
        let context = serde_json::json!({
            "checkout": asked.checkout.display().to_string(),
            "extension": asked.identity.id,
            declared: asked.declaration,
        });
        let reach = Arc::new(CheckoutReach::new(asked.checkout.clone()));
        self.runs.fetch_add(1, Ordering::Relaxed);
        let answered = script.answer(input, &context, grants, reach);
        let trace = answered
            .trace
            .lines
            .iter()
            .map(|line| line.text.clone())
            .collect();
        match answered.outcome {
            Ok(envelope) => {
                if asked.asking != Asking::Action {
                    lock(&self.answered).insert(remembered, envelope.clone());
                }
                Answer::Answered { envelope, trace }
            }
            Err(error) => Answer::Failed {
                reason: format!("{file}: {error}"),
                trace,
            },
        }
    }
}

fn failed(reason: impl Into<String>) -> Answer {
    Answer::Failed {
        reason: reason.into(),
        trace: Vec::new(),
    }
}

/// Why a script of an extension in `state` does not run, as the surface
/// says it.
fn withheld_because(state: &ExtensionState) -> String {
    match state {
        ExtensionState::NeedsConsent { .. } => "needs consent".to_owned(),
        ExtensionState::NeedsEnabling => "needs enabling in this project".to_owned(),
        ExtensionState::Held { new_grants } => {
            let grants: Vec<&str> = new_grants.iter().map(|grant| grant.name()).collect();
            format!(
                "held: its update wants {}, which nobody accepted",
                grants.join(", ")
            )
        }
        ExtensionState::FixturesOnly | ExtensionState::Enabled => "runs nothing".to_owned(),
    }
}

fn input_digest(input: &serde_json::Value) -> String {
    uze_core::digest::sha256(serde_json::to_string(input).unwrap_or_default().as_bytes())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::application::extensions::checkout::{Surroundings, extension_checkout};
    use crate::{Asker, UzeApplication};

    /// The `gsd` fixture plugin installed from a marketplace linked to its
    /// working tree, and a checkout of a project whose state file its
    /// `state.rhai` counts the lines of, in a world whose temporary
    /// directory is elsewhere.
    struct World {
        scratch: PathBuf,
        app: UzeApplication,
        band: PathBuf,
        checkout: PathBuf,
        around: Surroundings,
    }

    impl World {
        fn new(label: &str) -> Self {
            let scratch =
                uze_platform::path::canonical(&uze_testkit::temp::scratch(label)).unwrap();
            let market = scratch.join("market");
            let manifest = serde_json::json!({
                "name": "studio",
                "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD." }],
            });
            uze_testkit::marketplace::stage(
                &market,
                &manifest.to_string(),
                &[("gsd".to_owned(), uze_testkit::fixtures::canonical("gsd"))],
            );
            let app = UzeApplication::new(uze_core::UzeHome::at(scratch.join("uze")), Vec::new());
            app.marketplace()
                .add(&market.display().to_string())
                .unwrap();
            app.marketplace().link("studio", &market).unwrap();
            app.marketplace()
                .install_plugin("gsd@studio", &crate::AlwaysTrust)
                .unwrap();
            let checkout = scratch.join("project");
            fs::create_dir_all(checkout.join(".planning")).unwrap();
            uze_testkit::git::isolated_git_in(&checkout, &["init", "-q"]);
            fs::write(checkout.join(".planning/STATE.md"), "one\ntwo\nthree").unwrap();
            let around = Surroundings {
                home: Some(scratch.join("home")),
                uze_home: scratch.join("uze"),
                temp: vec![scratch.join("tmp")],
                volume_roots: Vec::new(),
            };
            Self {
                band: market.join("plugins/gsd/extensions/gsd-band"),
                scratch,
                app,
                checkout,
                around,
            }
        }

        fn band(&self) -> Identity {
            Identity {
                marketplace: "studio".to_owned(),
                plugin: "gsd".to_owned(),
                id: "gsd-band".to_owned(),
            }
        }

        fn question(&self, source: &str) -> Question {
            Question {
                identity: self.band(),
                declaration: source.to_owned(),
                asking: Asking::Source,
                input: serde_json::json!({}),
                checkout: Some(self.checkout.join(".planning")),
            }
        }

        fn ask(&self, runner: &ExtensionRunner, question: &Question) -> Answer {
            runner.answer_over(&self.app.extensions(), question, |path| {
                extension_checkout(path, &self.around)
            })
        }

        fn enabled(&self) {
            let extensions = self.app.extensions();
            extensions.consent(&self.band(), Asker::Operator).unwrap();
            extensions
                .enable(&self.band(), &self.checkout, Asker::Operator)
                .unwrap();
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.scratch);
        }
    }

    fn data(answer: Answer) -> serde_json::Value {
        match answer {
            Answer::Answered { envelope, .. } => envelope.data,
            other => panic!("not answered: {other:?}"),
        }
    }

    fn withheld(answer: Answer) -> String {
        match answer {
            Answer::Withheld { reason } => reason,
            other => panic!("not withheld: {other:?}"),
        }
    }

    #[test]
    fn a_script_answers_only_once_consented_to_and_enabled_in_the_project() {
        let world = World::new("runner-enabled");
        let runner = ExtensionRunner::new();
        let question = world.question("state");

        assert_eq!(withheld(world.ask(&runner, &question)), "needs consent");
        world
            .app
            .extensions()
            .consent(&world.band(), Asker::Operator)
            .unwrap();
        assert_eq!(
            withheld(world.ask(&runner, &question)),
            "needs enabling in this project"
        );
        assert_eq!(runner.runs(), 0, "nothing ran");

        world
            .app
            .extensions()
            .enable(&world.band(), &world.checkout, Asker::Operator)
            .unwrap();
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "lines": 3 })
        );
        assert_eq!(runner.runs(), 1);

        world
            .app
            .extensions()
            .disable(&world.band(), &world.checkout)
            .unwrap();
        assert_eq!(
            withheld(world.ask(&runner, &question)),
            "needs enabling in this project",
            "a remembered answer is not served once it may not run"
        );
    }

    #[test]
    fn two_surfaces_asking_the_same_question_share_one_run() {
        let world = World::new("runner-shared");
        world.enabled();
        let runner = ExtensionRunner::new();
        let question = world.question("state");

        let first = data(world.ask(&runner, &question));
        fs::write(world.checkout.join(".planning/STATE.md"), "changed").unwrap();
        let second = data(world.ask(&runner, &question));
        assert_eq!(first, second, "the session's answer");
        assert_eq!(runner.runs(), 1);

        let mut other = question.clone();
        other.input = serde_json::json!({ "phase": "12" });
        assert_eq!(
            data(world.ask(&runner, &other)),
            serde_json::json!({ "lines": 1 }),
            "another input is another question"
        );
        assert_eq!(runner.runs(), 2);

        let script = world.band.join("state.rhai");
        let text = fs::read_to_string(&script).unwrap();
        fs::write(&script, text.replace("lines:", "count:")).unwrap();
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "count": 1 }),
            "a save of the extension is a new question"
        );
        assert_eq!(runner.runs(), 3);
    }

    #[test]
    fn a_refresh_runs_again_and_an_action_is_never_remembered() {
        let world = World::new("runner-action");
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            format!("{text}\naction \"count\" label=\"Count\" on=\"board\" {{\n    script \"state.rhai\"\n}}\n"),
        )
        .unwrap();
        world.enabled();
        let runner = ExtensionRunner::new();
        let question = world.question("state");
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "lines": 3 })
        );

        fs::write(world.checkout.join(".planning/STATE.md"), "one").unwrap();
        let refresh = Question {
            asking: Asking::Refresh,
            ..question.clone()
        };
        assert_eq!(
            data(world.ask(&runner, &refresh)),
            serde_json::json!({ "lines": 1 })
        );
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "lines": 1 }),
            "the refreshed answer is the one remembered"
        );
        assert_eq!(runner.runs(), 2);

        let action = Question {
            declaration: "count".to_owned(),
            asking: Asking::Action,
            input: serde_json::json!({ "item": null }),
            ..question.clone()
        };
        world.ask(&runner, &action);
        world.ask(&runner, &action);
        assert_eq!(runner.runs(), 4, "an action runs on every gesture");

        let refresh_action = Question {
            declaration: "refresh".to_owned(),
            ..action.clone()
        };
        assert!(matches!(
            world.ask(&runner, &refresh_action),
            Answer::Failed { reason, .. } if reason.contains("runs no script")
        ));
        world
            .app
            .extensions()
            .disable(&world.band(), &world.checkout)
            .unwrap();
        assert_eq!(
            withheld(world.ask(&runner, &action)),
            "needs enabling in this project"
        );
    }

    /// The bytes a script runs from are the load the consent was judged
    /// over: a save under the same grants is recorded as that load's
    /// digest, and the saved script is the one that answers.
    #[test]
    fn what_runs_is_the_load_whose_digest_was_accepted() {
        let world = World::new("runner-digest");
        world.enabled();
        fs::write(
            world.band.join("state.rhai"),
            "fn answer(input, context) { #{ data: #{ saved: true } } }\n",
        )
        .unwrap();
        let runner = ExtensionRunner::new();
        assert_eq!(
            data(world.ask(&runner, &world.question("state"))),
            serde_json::json!({ "saved": true })
        );
        let home = uze_core::UzeHome::at(world.scratch.join("uze"));
        let accepted =
            uze_workspace::extensions::consent::consent(&home, &crate::consent_key(&world.band()))
                .expect("recorded")
                .accepted_digest;
        assert_eq!(accepted, Loaded::read(&world.band).unwrap().digest());
    }

    #[test]
    fn an_update_that_widens_the_grants_runs_nothing_until_consented_to() {
        let world = World::new("runner-widened");
        world.enabled();
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            text.replace(
                "wants \"read:checkout\"",
                "wants \"read:checkout\" \"clipboard\"",
            ),
        )
        .unwrap();
        let runner = ExtensionRunner::new();
        let question = world.question("state");
        assert_eq!(
            withheld(world.ask(&runner, &question)),
            "held: its update wants clipboard, which nobody accepted"
        );
        assert_eq!(runner.runs(), 0, "nothing ran");

        world
            .app
            .extensions()
            .consent(&world.band(), Asker::Operator)
            .unwrap();
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "lines": 3 })
        );
    }

    #[test]
    fn a_refused_checkout_runs_nothing_and_says_why() {
        let world = World::new("runner-checkout");
        world.enabled();
        let runner = ExtensionRunner::new();

        let mut outside = world.question("state");
        outside.checkout = Some(world.scratch.join("tmp"));
        fs::create_dir_all(world.scratch.join("tmp")).unwrap();
        assert!(
            withheld(world.ask(&runner, &outside)).contains("not inside a Git checkout"),
            "{outside:?}"
        );
        let mut nowhere = world.question("state");
        nowhere.checkout = None;
        assert_eq!(
            withheld(world.ask(&runner, &nowhere)),
            "no checkout is in front to read"
        );
        assert_eq!(runner.runs(), 0);
    }

    #[test]
    fn a_fixture_is_answered_by_the_same_path_without_consent() {
        let world = World::new("runner-fixture");
        let runner = ExtensionRunner::new();
        let mut question = world.question("milestone");
        question.checkout = None;

        let milestone = data(world.ask(&runner, &question));
        assert_eq!(milestone["milestone"], "v1.23", "{milestone}");
        assert_eq!(runner.runs(), 0, "a fixture is not a run");
        assert!(matches!(
            world.ask(&runner, &world.question("nothing")),
            Answer::Failed { reason, .. } if reason.contains("no source `nothing`")
        ));
    }

    #[test]
    fn a_failing_script_says_its_bound_and_keeps_its_trace() {
        let world = World::new("runner-failing");
        world.enabled();
        fs::write(
            world.band.join("state.rhai"),
            "fn answer(input, context) { print(\"looking\"); loop {} }",
        )
        .unwrap();
        let runner = ExtensionRunner::new();
        // The update wants no new grant, so it is accepted as it is judged.
        match world.ask(&runner, &world.question("state")) {
            Answer::Failed { reason, trace } => {
                assert!(reason.starts_with("state.rhai: "), "{reason}");
                assert!(reason.contains("bound"), "{reason}");
                assert_eq!(trace, ["looking"]);
            }
            other => panic!("not failed: {other:?}"),
        }
    }
}
