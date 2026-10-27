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

use uze_contract::{
    Answer, Asking, Does, Envelope, Identity, Output, Program, Question, ResidentCall, SourceKind,
    StoreScope,
};
use uze_core::capability::extension::Loaded;
use uze_script::{Grants, Resident, ResidentScript, ScriptCache};
use uze_workspace::extensions::store::{self, StoreOf};

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

/// Answers declared extensions' questions for one session. Its default
/// confines, as [`ExtensionRunner::new`] does: a runner built without
/// saying so never runs a program unconfined.
#[derive(Debug)]
pub struct ExtensionRunner {
    answered: Mutex<HashMap<Asked, Envelope>>,
    scripts: Mutex<ScriptCache>,
    runs: AtomicU64,
    /// The resident sources running now, by extension and source: each
    /// opened by its `Start` and closed by its `Stop`, its first failure or
    /// the operator's consent changing.
    residents: Mutex<HashMap<(Identity, String), Arc<Running>>>,
    /// The program that confines an extension's programs: this binary,
    /// where this machine confines. `None` runs them unconfined, which
    /// only an extension accepted to run as the operator may.
    helper: Option<PathBuf>,
}

/// A resident session, and what applies its answers' store patches.
struct Running {
    session: Resident,
    file: String,
    keep: Box<dyn Fn(Envelope) -> Envelope + Send + Sync>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running").field("file", &self.file).finish()
    }
}

/// What answers a question, found in the compile that was consented to.
enum Answerer<'a> {
    Script(&'a str),
    Resident(&'a str),
    Program {
        program: &'a Program,
        output: &'a Output,
        with_store: bool,
    },
}

impl Default for ExtensionRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionRunner {
    /// A runner for this session, confining programs through this very
    /// binary's `__confine` where the machine can.
    pub fn new() -> Self {
        let helper = super::confinement()
            .is_available()
            .then(std::env::current_exe)
            .and_then(Result::ok);
        Self::confining_through(helper)
    }

    /// A runner whose programs are confined by `helper`, or run
    /// unconfined when it is `None`.
    pub fn confining_through(helper: Option<PathBuf>) -> Self {
        Self {
            answered: Mutex::default(),
            scripts: Mutex::default(),
            runs: AtomicU64::default(),
            residents: Mutex::default(),
            helper,
        }
    }

    /// How many times a script ran this session: a remembered answer is
    /// not a run.
    pub fn runs(&self) -> u64 {
        self.runs.load(Ordering::Relaxed)
    }

    /// Lets go of what `identity`'s sources answered this session: what
    /// the operator allowed it changed, so nothing it answered before
    /// stands for what it would answer now.
    pub fn forget(&self, identity: &Identity) {
        lock(&self.answered).retain(|asked, _| asked.identity != *identity);
        lock(&self.residents).retain(|(running, _), _| running != identity);
    }

    /// The bytes of the picture `name` an `Image` or a sprite of
    /// `identity` names: a file of the extension as installed, or
    /// `scratch:<file>`, one its programs left in their scratch and the
    /// run kept. Read no further than [`uze_contract::IMAGE_FILE_MAX`] and
    /// a byte, so the engine refuses a larger one without it being held.
    pub fn media(
        &self,
        extensions: &Extensions<'_>,
        identity: &Identity,
        name: &str,
    ) -> Result<Vec<u8>, String> {
        let Some(installed) = extensions
            .installed()
            .into_iter()
            .find(|installed| installed.identity == *identity)
        else {
            return Err(format!("no installed plugin carries {identity}"));
        };
        if let Some(file) = name.strip_prefix(uze_contract::SCRATCH_PREFIX) {
            if !super::program::is_picture_name(file) {
                return Err(format!("`{name}` names no picture of its scratch"));
            }
            let kept = extensions
                .0
                .home
                .extension_pictures_dir(&super::consent::plugin_key(identity), &identity.id);
            return read_bounded(&kept.join(file))
                .map_err(|_| format!("its program has not written `{file}`"));
        }
        let files = Loaded::read(&installed.dir)
            .map_err(|refusal| format!("its files cannot be read: {refusal}"))?;
        let bytes = files
            .bytes(name)
            .ok_or_else(|| format!("`{name}` is not in the extension"))?;
        Ok(bytes[..bytes.len().min(uze_contract::IMAGE_FILE_MAX + 1)].to_vec())
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
        // A running session was consented to when it started, and is
        // closed when what the operator allowed changes: its calls, up to
        // thirty a second, read nothing again.
        if let Asking::Resident(call) = question.asking
            && call != ResidentCall::Start
        {
            return self.call_resident(question, call);
        }
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
        let manifest = &load.compiled.manifest;
        let answerer = match question.asking {
            Asking::Source | Asking::Refresh | Asking::Resident(_) => {
                let Some(source) = manifest.source(&question.declaration) else {
                    return failed(format!("it declares no source `{}`", question.declaration));
                };
                if matches!(question.asking, Asking::Resident(_)) && source.resident().is_none() {
                    return failed(format!(
                        "the source `{}` is not resident",
                        question.declaration
                    ));
                }
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
                    SourceKind::Run(program) => Answerer::Program {
                        program,
                        output: &source.output,
                        with_store: source.with_store,
                    },
                    SourceKind::Script(file) => Answerer::Script(file),
                    SourceKind::Resident(file)
                        if matches!(question.asking, Asking::Resident(_)) =>
                    {
                        Answerer::Resident(file)
                    }
                    SourceKind::Events(_) | SourceKind::Agents | SourceKind::Resident(_) => {
                        return failed(format!(
                            "the source `{}` is answered by the workspace, from what it sees",
                            question.declaration
                        ));
                    }
                }
            }
            Asking::Action => {
                let Some(action) = manifest.action(&question.declaration) else {
                    return failed(format!("it declares no action `{}`", question.declaration));
                };
                match &action.does {
                    Does::Script(file) => Answerer::Script(file),
                    Does::Run(program) => Answerer::Program {
                        program,
                        output: &Output::Json,
                        with_store: false,
                    },
                    _ => {
                        return failed(format!(
                            "the action `{}` runs nothing the host answers",
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
        // A source that names its preferences is asked with them, as they
        // are set now: a different setting is a different question.
        let with_preferences = matches!(question.asking, Asking::Source | Asking::Refresh)
            && manifest
                .source(&question.declaration)
                .is_some_and(|source| source.with_preferences);
        let mut input = question.input.clone();
        if with_preferences && let serde_json::Value::Object(fields) = &mut input {
            fields.insert(
                "preferences".to_owned(),
                super::consent::admitted_preferences(&extensions.0.home, identity, manifest),
            );
        }
        let question = &Question {
            input,
            ..question.clone()
        };
        let grants: Grants = manifest.wants.iter().cloned().collect();
        let asked = Asked {
            identity: identity.clone(),
            asking: question.asking,
            declaration: question.declaration.clone(),
            checkout: checkout.root.clone(),
            input: input_digest(&question.input),
            files: load.digest.clone(),
        };
        let plugin = super::consent::plugin_key(identity);
        let store_of = manifest.store().map(|scope| StoreOf {
            plugin: &plugin,
            id: &identity.id,
            project: (scope == StoreScope::Project).then_some(project.as_path()),
        });
        let home = &extensions.0.home;
        let pictures = home.extension_pictures_dir(&plugin, &identity.id);
        let read_store = || {
            store_of
                .as_ref()
                .map(|of| serde_json::Value::Object(store::read(home, of)))
        };
        let keep = |envelope: Envelope| keep_store(home, store_of.as_ref(), envelope);
        let consent = extensions.consent_record(identity);
        let ask_program = |program: &Program,
                           output: &Output,
                           store: Option<&serde_json::Value>,
                           literal: bool| {
            let reads: Vec<String> = if program.is_packaged() {
                Vec::new()
            } else {
                manifest.reads_of(program.program()).to_vec()
            };
            let ask = super::program::Asked {
                home,
                extension_dir: &installed.dir,
                files: Some(&files),
                program,
                output,
                reads: &reads,
                asking: question.asking,
                declaration: &question.declaration,
                extension: &identity.id,
                checkout: &checkout,
                input: &question.input,
                store,
                consent: consent.as_ref(),
                helper: self.helper.as_deref(),
                literal,
                pictures: Some(&pictures),
            };
            if literal {
                Err(super::program::reply(&ask))
            } else {
                Ok(super::program::answer(&ask))
            }
        };
        let reach = || {
            let mut reach = CheckoutReach::new(checkout.root.clone());
            if let Some(store) = read_store() {
                reach = reach.with_store(store);
            }
            if manifest.programs().next().is_some() {
                reach = reach.with_programs(self.programs_for(
                    home,
                    &installed.dir,
                    manifest,
                    identity,
                    &checkout,
                    consent.clone(),
                ));
            }
            reach
        };
        match answerer {
            Answerer::Resident(file) => {
                let plugin = plugin.clone();
                let id = identity.id.clone();
                let owned_home = home.clone();
                let scope = manifest.store();
                let project = project.clone();
                let keep = move |envelope: Envelope| {
                    let of = scope.map(|scope| StoreOf {
                        plugin: &plugin,
                        id: &id,
                        project: (scope == StoreScope::Project).then_some(project.as_path()),
                    });
                    keep_store(&owned_home, of.as_ref(), envelope)
                };
                self.start_resident(
                    question,
                    &files,
                    file,
                    &grants,
                    Arc::new(reach()),
                    Box::new(keep),
                )
            }
            Answerer::Script(script) => self.run_script(
                asked,
                &files,
                script,
                &grants,
                &question.input,
                Arc::new(reach()),
                &keep,
            ),
            Answerer::Program {
                program,
                output,
                with_store,
            } => self.remembered_or(asked, || {
                let store = with_store.then(read_store).flatten();
                match ask_program(program, output, store.as_ref(), false) {
                    Ok(Answer::Answered { envelope, trace }) => Answer::Answered {
                        envelope: keep(envelope),
                        trace,
                    },
                    Ok(other) => other,
                    Err(_) => unreachable!("a declaration's program answers"),
                }
            }),
        }
    }

    /// Opens the session of a resident source, closing any it had: its
    /// script compiled from the consented bytes, started with the
    /// engine's context and what the runner knows of where it runs.
    fn start_resident(
        &self,
        question: &Question,
        files: &Loaded,
        file: &str,
        grants: &Grants,
        reach: Arc<dyn uze_script::Reach>,
        keep: Box<dyn Fn(Envelope) -> Envelope + Send + Sync>,
    ) -> Answer {
        let key = (question.identity.clone(), question.declaration.clone());
        if let Some(running) = lock(&self.residents).remove(&key) {
            running.session.stop();
        }
        let Some(bytes) = files.bytes(file) else {
            return failed(format!("{file}: not found"));
        };
        let script = match ResidentScript::compile(bytes) {
            Ok(script) => Arc::new(script),
            Err(error) => return failed(format!("{file}: {error}")),
        };
        let mut context = question.input.clone();
        if let serde_json::Value::Object(fields) = &mut context {
            fields.insert("extension".to_owned(), question.identity.id.clone().into());
            fields.insert("source".to_owned(), question.declaration.clone().into());
            if let Some(checkout) = &question.checkout {
                fields.insert("checkout".to_owned(), checkout.display().to_string().into());
            }
        }
        self.runs.fetch_add(1, Ordering::Relaxed);
        let (session, answered) = Resident::start(script, grants, reach, &context);
        let running = Arc::new(Running {
            session,
            file: file.to_owned(),
            keep,
        });
        let answer = resident_answer(&running, answered);
        if matches!(answer, Answer::Answered { .. }) {
            lock(&self.residents).insert(key, running);
        }
        answer
    }

    /// One call into a running resident source's session. Its first
    /// failure closes it: what it says is the bound it crossed.
    fn call_resident(&self, question: &Question, call: ResidentCall) -> Answer {
        let key = (question.identity.clone(), question.declaration.clone());
        let running = lock(&self.residents).get(&key).cloned();
        let Some(running) = running else {
            return match call {
                ResidentCall::Stop => Answer::Answered {
                    envelope: Envelope::default(),
                    trace: Vec::new(),
                },
                _ => failed(format!(
                    "the resident source `{}` is not running",
                    question.declaration
                )),
            };
        };
        let session = &running.session;
        let answered = match call {
            ResidentCall::Start => unreachable!("a start opens a session"),
            ResidentCall::Event => session.event(&question.input),
            ResidentCall::Input => session.input(&question.input),
            ResidentCall::Render => {
                let side = |name: &str| {
                    question
                        .input
                        .get(name)
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|side| u16::try_from(side).ok())
                        .unwrap_or(0)
                };
                session.render(side("width"), side("height"))
            }
            ResidentCall::Stop => {
                lock(&self.residents).remove(&key);
                session.stop()
            }
        };
        let answer = resident_answer(&running, answered);
        if matches!(answer, Answer::Failed { .. }) {
            lock(&self.residents).remove(&key);
        }
        answer
    }

    /// How many resident sources are running now.
    pub fn residents(&self) -> usize {
        lock(&self.residents).len()
    }

    /// What runs the programs a script of `identity` calls `run_program`
    /// with: the same pinning, environment, bounds and confinement as a
    /// `run` source, with the script's arguments taken as they are.
    fn programs_for(
        &self,
        home: &uze_core::UzeHome,
        extension_dir: &std::path::Path,
        manifest: &uze_contract::Manifest,
        identity: &Identity,
        checkout: &ExtensionCheckout,
        consent: Option<super::ExtensionConsent>,
    ) -> super::reach::RunsPrograms {
        let home = home.clone();
        let extension_dir = extension_dir.to_path_buf();
        let reads: std::collections::BTreeMap<String, Vec<String>> = manifest
            .programs()
            .map(|name| (name.to_owned(), manifest.reads_of(name).to_vec()))
            .collect();
        let id = identity.id.clone();
        let pictures = home.extension_pictures_dir(&super::consent::plugin_key(identity), &id);
        let checkout = checkout.clone();
        let helper = self.helper.clone();
        Arc::new(move |name: &str, args: &[String]| {
            let program = Program::new(
                std::iter::once(name.to_owned())
                    .chain(args.iter().cloned())
                    .collect(),
            );
            let ask = super::program::Asked {
                home: &home,
                extension_dir: &extension_dir,
                files: None,
                program: &program,
                output: &Output::Json,
                reads: reads.get(name).map_or(&[], Vec::as_slice),
                asking: Asking::Source,
                declaration: "run_program",
                extension: &id,
                checkout: &checkout,
                input: &serde_json::Value::Null,
                store: None,
                consent: consent.as_ref(),
                helper: helper.as_deref(),
                literal: true,
                pictures: Some(&pictures),
            };
            super::program::reply(&ask)
        })
    }

    /// What a source asked as `asked` answered this session, or what
    /// `run` answers now, remembered when it is a source's: an action is
    /// run on every gesture.
    fn remembered_or(&self, asked: Asked, run: impl FnOnce() -> Answer) -> Answer {
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
        self.runs.fetch_add(1, Ordering::Relaxed);
        let answer = run();
        if asked.asking != Asking::Action
            && let Answer::Answered { envelope, .. } = &answer
        {
            lock(&self.answered).insert(remembered, envelope.clone());
        }
        answer
    }

    /// Runs the script that answers `source` of the extension in `dir`,
    /// as `uze agent plugin check --run` does: the author's files as they
    /// are on disk, asked with the input the source declares, over the
    /// checkout `checkout` sits in. No consent is asked: a check runs what
    /// the person running it is writing, and the engine still reaches only
    /// what the manifest's grants register, beneath that checkout.
    /// `None` when `source` is not answered by a script.
    #[tracing::instrument(name = "extensions.rehearse", skip_all, fields(source))]
    pub fn rehearse(
        &self,
        extensions: &Extensions<'_>,
        dir: &Path,
        identity: &Identity,
        source: &str,
        checkout: &Path,
    ) -> Option<Answer> {
        self.rehearse_over(
            &extensions.0.home,
            dir,
            identity,
            source,
            checkout,
            |path| extensions.checkout(path),
        )
    }

    /// [`Self::rehearse`], with the checkout resolved by `resolve`.
    pub(crate) fn rehearse_over(
        &self,
        home: &uze_core::UzeHome,
        dir: &Path,
        identity: &Identity,
        source: &str,
        checkout: &Path,
        resolve: impl FnOnce(&Path) -> Result<ExtensionCheckout, NotACheckout>,
    ) -> Option<Answer> {
        let files = match Loaded::read(dir) {
            Ok(files) => files,
            Err(refusal) => return Some(Answer::withheld(refusal.to_string())),
        };
        let compiled =
            match uze_core::capability::extension::compile_loaded(identity.clone(), &files) {
                Ok(compiled) => compiled,
                Err(diagnostics) => {
                    let first = diagnostics
                        .first()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    return Some(Answer::withheld(format!("it does not compile: {first}")));
                }
            };
        let declared = compiled.manifest.source(source)?;
        let checkout = match resolve(checkout) {
            Ok(checkout) => checkout,
            Err(refused) => return Some(Answer::withheld(refused.reason())),
        };
        let mut input = serde_json::to_value(&declared.input).unwrap_or_default();
        // A check is asked with the preferences as declared: their defaults.
        if declared.with_preferences
            && let serde_json::Value::Object(fields) = &mut input
        {
            let defaults = compiled
                .manifest
                .preferences
                .iter()
                .map(|decl| (decl.name.clone(), decl.default.clone()))
                .collect();
            fields.insert(
                "preferences".to_owned(),
                serde_json::Value::Object(defaults),
            );
        }
        let file = match &declared.kind {
            SourceKind::Script(file) => file,
            SourceKind::Fixture(_)
            | SourceKind::Events(_)
            | SourceKind::Agents
            | SourceKind::Resident(_) => return None,
            SourceKind::Run(program) => {
                return Some(self.rehearse_program(
                    home,
                    dir,
                    &files,
                    &compiled.manifest,
                    identity,
                    declared,
                    program,
                    &checkout,
                    &input,
                ));
            }
        };
        let grants: Grants = compiled.manifest.wants.iter().cloned().collect();
        let asked = Asked {
            identity: identity.clone(),
            asking: Asking::Source,
            declaration: declared.id.clone(),
            checkout: checkout.root.clone(),
            input: input_digest(&input),
            files: files.digest().to_owned(),
        };
        // A check rehearses what the author wrote; it keeps nothing, and
        // starts from an empty store. The programs it calls run only where
        // this machine confines them, resolved now (S12).
        let manifest = &compiled.manifest;
        let mut reach = CheckoutReach::new(checkout.root.clone());
        if manifest.store().is_some() {
            reach = reach.with_store(serde_json::json!({}));
        }
        if manifest.programs().next().is_some() && self.helper.is_some() {
            reach = reach.with_programs(self.programs_for(
                home,
                dir,
                manifest,
                identity,
                &checkout,
                Some(rehearsed_consent(manifest, home)),
            ));
        }
        let reach = Arc::new(reach);
        Some(
            self.run_script(asked, &files, file, &grants, &input, reach, &|envelope| {
                Envelope {
                    store: None,
                    ..envelope
                }
            }),
        )
    }

    /// A program source rehearsed as a check runs it: its programs
    /// resolved now, and run only where this machine confines them, since
    /// nobody consented to running them as the author (S12).
    #[allow(clippy::too_many_arguments)]
    fn rehearse_program(
        &self,
        home: &uze_core::UzeHome,
        dir: &Path,
        files: &Loaded,
        manifest: &uze_contract::Manifest,
        identity: &Identity,
        declared: &uze_contract::SourceDecl,
        program: &Program,
        checkout: &ExtensionCheckout,
        input: &serde_json::Value,
    ) -> Answer {
        if self.helper.is_none() {
            return Answer::withheld(
                "this machine does not confine programs, so a check runs none; its recorded \
                 answers stand in",
            );
        }
        let rehearsed = rehearsed_consent(manifest, home);
        let reads: Vec<String> = if program.is_packaged() {
            Vec::new()
        } else {
            manifest.reads_of(program.program()).to_vec()
        };
        let ask = super::program::Asked {
            home,
            extension_dir: dir,
            files: Some(files),
            program,
            output: &declared.output,
            reads: &reads,
            asking: Asking::Source,
            declaration: &declared.id,
            extension: &identity.id,
            checkout,
            input,
            store: None,
            consent: Some(&rehearsed),
            helper: self.helper.as_deref(),
            literal: false,
            pictures: None,
        };
        let answer = super::program::answer(&ask);
        match answer {
            // A check keeps nothing.
            Answer::Answered { envelope, trace } => Answer::Answered {
                envelope: Envelope {
                    store: None,
                    ..envelope
                },
                trace,
            },
            other => other,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_script(
        &self,
        asked: Asked,
        files: &Loaded,
        file: &str,
        grants: &Grants,
        input: &serde_json::Value,
        reach: Arc<dyn uze_script::Reach>,
        keep: &dyn Fn(Envelope) -> Envelope,
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
            Asking::Source | Asking::Refresh | Asking::Resident(_) => "source",
        };
        let context = serde_json::json!({
            "checkout": asked.checkout.display().to_string(),
            "extension": asked.identity.id,
            declared: asked.declaration,
        });
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
                let envelope = keep(envelope);
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

/// What a check runs a program under: its programs resolved now, as
/// confined as an enabled run, recorded nowhere.
fn rehearsed_consent(
    manifest: &uze_contract::Manifest,
    home: &uze_core::UzeHome,
) -> super::ExtensionConsent {
    let pins = super::consent::resolve_programs(manifest, home);
    super::ExtensionConsent {
        exec_paths: Some(
            pins.iter()
                .filter_map(|pin| Some((pin.name.clone(), pin.path.clone()?)))
                .collect(),
        ),
        enforced_at_accept: Some(true),
        ..super::ExtensionConsent::default()
    }
}

/// Applies an answer's store patch, as answers arrive, where the extension
/// keeps a store; says in the answer's notice why one was not kept.
fn keep_store(
    home: &uze_core::UzeHome,
    of: Option<&StoreOf<'_>>,
    mut envelope: Envelope,
) -> Envelope {
    let Some(patch) = envelope.store.take() else {
        return envelope;
    };
    let unkept = match of {
        None => Some("it asked to keep something, and does not want `store`".to_owned()),
        Some(of) => store::patch(home, of, &patch)
            .err()
            .map(|error| error.to_string()),
    };
    if let Some(why) = unkept {
        envelope.notice = Some(match envelope.notice.take() {
            Some(notice) => format!("{notice} · {why}"),
            None => why,
        });
    }
    envelope
}

/// `path`'s bytes, no further than one past [`uze_contract::IMAGE_FILE_MAX`].
fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(uze_contract::IMAGE_FILE_MAX as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// A resident call's answer, as the engine takes a source's: its store
/// patch applied, its trace kept, and a failure naming the script.
fn resident_answer(running: &Running, answered: uze_script::Answered) -> Answer {
    let trace = answered
        .trace
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect();
    match answered.outcome {
        Ok(envelope) => Answer::Answered {
            envelope: (running.keep)(envelope),
            trace,
        },
        Err(error) => Answer::Failed {
            reason: format!("{}: {error}", running.file),
            trace,
        },
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
            let grants: Vec<String> = new_grants
                .iter()
                .map(|grant| grant.name().into_owned())
                .collect();
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
pub(super) mod tests {
    use std::fs;

    use super::*;
    use crate::application::extensions::checkout::{Surroundings, extension_checkout};
    use crate::{Asker, UzeApplication};

    /// The `gsd` fixture plugin installed from a marketplace linked to its
    /// working tree, and a checkout of a project whose state file its
    /// `state.rhai` counts the lines of, in a world whose temporary
    /// directory is elsewhere.
    pub(crate) struct World {
        scratch: PathBuf,
        pub(crate) app: UzeApplication,
        pub(crate) band: PathBuf,
        pub(crate) checkout: PathBuf,
        pub(crate) around: Surroundings,
    }

    impl World {
        pub(crate) fn new(label: &str) -> Self {
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

        pub(crate) fn band(&self) -> Identity {
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

        pub(crate) fn enabled(&self) {
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
    fn an_extension_forgotten_answers_again_and_another_keeps_its_answer() {
        let world = World::new("runner-forget");
        world.enabled();
        let runner = ExtensionRunner::new();
        let question = world.question("state");

        data(world.ask(&runner, &question));
        fs::write(world.checkout.join(".planning/STATE.md"), "changed").unwrap();
        runner.forget(&Identity {
            id: "another".to_owned(),
            ..world.band()
        });
        data(world.ask(&runner, &question));
        assert_eq!(runner.runs(), 1, "another extension's record moved");

        runner.forget(&world.band());
        assert_eq!(
            data(world.ask(&runner, &question)),
            serde_json::json!({ "lines": 1 }),
            "what it allowed changed, so it runs again"
        );
        assert_eq!(runner.runs(), 2);
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
            Answer::Failed { reason, .. } if reason.contains("runs nothing the host answers")
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
    /// The band given a resident source answered by `script`.
    fn with_resident(world: &World, script: &str) {
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap().replace(
            "wants \"read:checkout\"",
            "wants \"read:checkout\" \"resident\"\n\nsource \"game\" {\n    resident \"game.rhai\"\n}",
        );
        fs::write(&manifest, text).unwrap();
        fs::write(world.band.join("game.rhai"), script).unwrap();
    }

    fn resident(world: &World, call: ResidentCall, input: serde_json::Value) -> Question {
        Question {
            asking: Asking::Resident(call),
            input,
            ..world.question("game")
        }
    }

    #[test]
    fn a_resident_source_keeps_its_session_until_it_is_stopped() {
        let world = World::new("runner-resident");
        with_resident(
            &world,
            "fn start(context) { this.keys = []; this.checkout = context.checkout; true }
             fn input(input) { this.keys.push(input.key); true }
             fn render(size) { #{ type: \"text\", args: [`${this.keys.len()} ${size.width}`] } }",
        );
        let runner = ExtensionRunner::new();
        let start = resident(&world, ResidentCall::Start, serde_json::json!({}));
        assert!(
            withheld(world.ask(&runner, &start)).contains("consent"),
            "nothing starts before consent"
        );
        world.enabled();
        assert_eq!(data(world.ask(&runner, &start)), serde_json::json!(true));
        assert_eq!(runner.residents(), 1);
        for key in ["up", "left"] {
            let input = resident(
                &world,
                ResidentCall::Input,
                serde_json::json!({ "key": key }),
            );
            assert_eq!(data(world.ask(&runner, &input)), serde_json::json!(true));
        }
        let render = resident(
            &world,
            ResidentCall::Render,
            serde_json::json!({ "width": 30, "height": 8 }),
        );
        assert_eq!(
            data(world.ask(&runner, &render)),
            serde_json::json!({ "type": "text", "args": ["2 30"] })
        );
        let stop = resident(&world, ResidentCall::Stop, serde_json::json!({}));
        assert!(matches!(world.ask(&runner, &stop), Answer::Answered { .. }));
        assert_eq!(runner.residents(), 0);
        assert!(matches!(
            world.ask(&runner, &render),
            Answer::Failed { reason, .. } if reason.contains("not running")
        ));
    }

    #[test]
    fn a_resident_source_past_its_bound_is_stopped_naming_it() {
        let world = World::new("runner-resident-bound");
        with_resident(&world, "fn render(size) { loop {} }");
        world.enabled();
        let runner = ExtensionRunner::new();
        let start = resident(&world, ResidentCall::Start, serde_json::json!({}));
        assert_eq!(data(world.ask(&runner, &start)), serde_json::json!(true));
        let render = resident(&world, ResidentCall::Render, serde_json::json!({}));
        let Answer::Failed { reason, .. } = world.ask(&runner, &render) else {
            panic!("a runaway render fails");
        };
        assert!(
            reason.starts_with("game.rhai: the script crossed its"),
            "{reason}"
        );
        assert!(reason.contains("bound"), "{reason}");
        assert_eq!(runner.residents(), 0, "a breach stops the session");
        let source = Question {
            asking: Asking::Source,
            ..world.question("game")
        };
        assert!(
            matches!(world.ask(&runner, &source), Answer::Failed { .. }),
            "a resident source is never answered per question"
        );
    }

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
    fn a_picture_is_read_from_the_extension_or_what_its_programs_kept() {
        let world = World::new("runner-media");
        let runner = ExtensionRunner::new();
        let extensions = world.app.extensions();
        fs::write(world.band.join("cat.png"), b"cat").unwrap();
        assert_eq!(
            runner.media(&extensions, &world.band(), "cat.png").unwrap(),
            b"cat"
        );
        let kept = uze_core::UzeHome::at(world.scratch.join("uze"))
            .extension_pictures_dir("gsd@studio", "gsd-band");
        fs::create_dir_all(&kept).unwrap();
        fs::write(kept.join("shot.png"), b"shot").unwrap();
        assert_eq!(
            runner
                .media(&extensions, &world.band(), "scratch:shot.png")
                .unwrap(),
            b"shot"
        );
        for refused in ["scratch:../cat.png", "scratch:notes.txt", "missing.png"] {
            assert!(
                runner.media(&extensions, &world.band(), refused).is_err(),
                "{refused}"
            );
        }
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

    #[test]
    fn a_check_rehearses_a_script_without_consent_and_only_a_script() {
        let world = World::new("runner-rehearse");
        let runner = ExtensionRunner::new();
        let rehearse = |source: &str, checkout: &Path| {
            runner.rehearse_over(
                &world.app.home,
                &world.band,
                &world.band(),
                source,
                checkout,
                |path| extension_checkout(path, &world.around),
            )
        };
        assert_eq!(
            data(rehearse("state", &world.checkout).unwrap()),
            serde_json::json!({ "lines": 3 }),
            "nobody consented, and the check still runs it"
        );
        assert!(
            rehearse("milestone", &world.checkout).is_none(),
            "a fixture is no run"
        );
        assert!(rehearse("nothing", &world.checkout).is_none());
        fs::create_dir_all(world.scratch.join("tmp")).unwrap();
        assert!(
            withheld(rehearse("state", &world.scratch.join("tmp")).unwrap())
                .contains("not inside a Git checkout")
        );
    }
}
