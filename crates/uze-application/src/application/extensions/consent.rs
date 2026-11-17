//! Where each declared extension stands with the operator in a project:
//! whether it needs consent, enabling, or nothing at all, and the three
//! answers that move it (`extension-platform` D5).
//!
//! What a state is judged against is what one load of the extension read:
//! its grants from that compile, its digest from those bytes. An update
//! that wants no new grant is applied as it is judged, recording the new
//! digest and its date; one that wants more holds the extension.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use uze_contract::{Does, Grant, Identity, Manifest, SourceKind};
use uze_core::capability::extension::program::is_general_runner;
use uze_core::machine::extension_program::{self, Unresolved};
use uze_core::{Result, UzeError};
use uze_workspace::extensions::consent::{self, Judgement, Pinned, Wants};

pub use uze_workspace::extensions::consent::Consent as ExtensionConsent;

use super::{ExtensionLoad, InstalledExtension};
use crate::application::services::Extensions;

/// Who is asking for consent or enabling. Only the reader that has the
/// environment can tell, so it says which: an agent's pane is never the
/// operator, whatever it types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Asker {
    /// A person at UZE's own surfaces, or at a terminal of their own.
    Operator,
    /// A process carrying the identity of an agent UZE launched.
    AgentPane,
}

/// Where a declared extension stands in one project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtensionState {
    /// Declares no script and no grant: runs nothing, needs no consent and
    /// draws from its fixtures.
    FixturesOnly,
    /// Runs code nobody on this machine accepted; these are what it wants.
    NeedsConsent { grants: BTreeSet<Grant> },
    /// Consented to, and not switched on in this project.
    NeedsEnabling,
    /// Consented to and switched on here: its scripts may answer.
    Enabled,
    /// An update wants grants, or reach of a program, nobody accepted;
    /// nothing of it runs until someone does.
    Held {
        new_grants: BTreeSet<Grant>,
        /// What else it wants anew, one reach a line: `exec:sh reads
        /// /etc/gsd`.
        widened: Vec<String>,
    },
}

impl ExtensionState {
    /// What a held update wants anew, each as a report spells it: its
    /// grants, then the rest of its reach. Empty for any other state.
    pub fn wanted_anew(&self) -> Vec<String> {
        match self {
            Self::Held {
                new_grants,
                widened,
            } => new_grants
                .iter()
                .map(|grant| grant.name().into_owned())
                .chain(widened.iter().cloned())
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl ExtensionState {
    /// Whether its scripts may answer in the project this was judged for.
    /// A fixture never runs, so a fixtures-only extension says no too.
    pub fn runs_scripts(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// One declared extension in one project, as the Extensions screen and
/// the CLI report it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionStanding {
    pub installed: InstalledExtension,
    /// Its state, or why it could not be judged: an extension that does
    /// not compile wants nothing anybody could accept.
    pub state:
        std::result::Result<ExtensionState, Vec<uze_core::capability::extension::Diagnostic>>,
    /// What was accepted for it, when anything was.
    pub consent: Option<ExtensionConsent>,
}

/// What [`Extensions::consent`] recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Consented {
    pub identity: Identity,
    pub grants: BTreeSet<Grant>,
    pub digest: String,
    pub accepted_at: u64,
}

/// What [`Extensions::enable`] or [`Extensions::disable`] recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Switched {
    pub identity: Identity,
    pub project: PathBuf,
    /// Whether the record changed: `false` when it already said so.
    pub changed: bool,
    /// Whether the answer holds for every project rather than `project`
    /// alone. An extension that reaches agents is enabled in `project`
    /// even when every project was asked for.
    pub everywhere: bool,
}

/// Whether `manifest` wants a grant that reaches the workspace's agents:
/// such an extension sees and prompts the agents of the projects it is
/// enabled in, so it is never enabled in every project
/// (`extension-agent-bridge` D42).
pub fn reaches_agents(manifest: &Manifest) -> bool {
    manifest.wants.iter().any(|grant| {
        matches!(
            grant,
            Grant::AgentObserve | Grant::AgentPrompt | Grant::AgentPromptAuto | Grant::AgentRestart
        )
    })
}

/// The key an identity's consent is recorded under.
pub fn consent_key(identity: &Identity) -> String {
    consent::identity_key(&identity.plugin, &identity.marketplace, &identity.id)
}

/// Whether `compiled` is enabled project by project, after consent: it
/// runs or reaches something beyond drawing its fixtures. One that does
/// not is on wherever its switch is.
pub fn enabled_per_project(compiled: &uze_contract::CompiledExtension) -> bool {
    needs_consent(&compiled.manifest)
}

/// Whether `manifest` runs anything or reaches anything beyond drawing.
fn needs_consent(manifest: &Manifest) -> bool {
    !manifest.wants.is_empty()
        || manifest
            .sources
            .iter()
            .any(|source| !matches!(source.kind, SourceKind::Fixture(_)))
        || manifest
            .actions
            .iter()
            .any(|action| matches!(action.does, Does::Call { .. } | Does::Run(_)))
}

/// One program an extension's `exec:` grants name, as the consent step
/// and the Extensions screen show it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ProgramPin {
    pub name: String,
    /// The real path a run executes, when it was found.
    pub path: Option<PathBuf>,
    /// Why it was not pinned, when it was not.
    pub unresolved: Option<String>,
    /// Whether it runs whatever it is handed (`sh`, `python3`, `git`).
    pub runs_anything: bool,
    /// What it reads beyond the host's own paths, as its `exec` declares.
    pub reads: Vec<String>,
    /// Whether it is a file packaged with the extension (`./bin/meter`),
    /// run from the consented bytes and never pinned: its `name` is all
    /// there is to say about where it is.
    pub packaged: bool,
}

impl ProgramPin {
    /// A program packaged with the extension, by the file it names.
    fn packaged(file: &str) -> Self {
        Self {
            name: format!("./{file}"),
            path: None,
            unresolved: None,
            runs_anything: false,
            reads: Vec::new(),
            packaged: true,
        }
    }
}

/// Whether `manifest` runs any program: one an `exec:` grant names, or one
/// packaged with it, which no grant names.
pub(super) fn runs_programs(manifest: &Manifest) -> bool {
    manifest.programs().next().is_some() || !manifest.packaged_programs().is_empty()
}

/// Each program packaged with `manifest`, as the consent step lists it.
fn packaged_pins(manifest: &Manifest) -> impl Iterator<Item = ProgramPin> + '_ {
    manifest
        .packaged_programs()
        .into_iter()
        .map(ProgramPin::packaged)
}

/// `plugin@marketplace`, as the stores and preferences are kept under.
pub(super) fn plugin_key(identity: &Identity) -> String {
    format!("{}@{}", identity.plugin, identity.marketplace)
}

/// Each preference `manifest` declares, at the operator's value where it
/// may hold it and at its default otherwise. A secret is never among
/// them: neither a bind template's `{pref.*}` nor a source's
/// `input.preferences` reads one.
pub(super) fn admitted_preferences(
    home: &uze_core::UzeHome,
    identity: &Identity,
    manifest: &Manifest,
) -> serde_json::Value {
    super::preferences::admitted(home, &consent_key(identity), &manifest.preferences)
}

/// Keeps what the operator typed as the secret `name` of `identity`, or
/// forgets it when they typed nothing. Its value is in no message.
fn set_secret(
    home: &uze_core::UzeHome,
    identity: &Identity,
    name: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let plugin = plugin_key(identity);
    match value.as_str() {
        Some("") => {
            uze_workspace::extensions::secrets::delete(home, &plugin, &identity.id, name)?;
        }
        Some(typed) if uze_contract::is_secret_value(typed) => {
            uze_workspace::extensions::secrets::store(home, &plugin, &identity.id, name, typed)?;
        }
        _ => {
            return Err(refusal(format!("the secret `{name}` is one line of text")));
        }
    }
    Ok(())
}

/// Whether this machine confines programs, right by right.
pub fn confinement() -> uze_platform::confine::Availability {
    uze_platform::confine::availability()
}

/// The places an `exec:` name may not be pinned beneath: where an
/// extension or an agent could have put a program of the same name.
fn unpinnable(home: &uze_core::UzeHome) -> Vec<PathBuf> {
    vec![home.root().to_path_buf(), std::env::temp_dir()]
}

/// Each program `manifest` names, resolved now on the host's search path.
pub(super) fn resolve_programs(manifest: &Manifest, home: &uze_core::UzeHome) -> Vec<ProgramPin> {
    let search = uze_core::harness_runtime::harness_search_path();
    let refused = unpinnable(home);
    manifest
        .programs()
        .map(|name| {
            let resolved = extension_program::resolve(name, &search, &refused);
            ProgramPin {
                name: name.to_owned(),
                unresolved: resolved.as_ref().err().map(|why| match why {
                    Unresolved::NotFound => format!("`{name}` not found"),
                    refused => format!("`{name}` {refused}"),
                }),
                path: resolved.ok(),
                runs_anything: is_general_runner(name),
                reads: manifest.reads_of(name).to_vec(),
                packaged: false,
            }
        })
        .chain(packaged_pins(manifest))
        .collect()
}

fn pinned_paths(pins: &[ProgramPin]) -> BTreeMap<String, PathBuf> {
    pins.iter()
        .filter(|pin| !pin.packaged)
        .filter_map(|pin| Some((pin.name.clone(), pin.path.clone()?)))
        .collect()
}

fn wanted(manifest: &Manifest) -> BTreeSet<Grant> {
    manifest.wants.iter().cloned().collect()
}

/// What the consent record compares for `manifest`: its grants and what
/// each of its programs declares it reads.
fn wants_of(manifest: &Manifest) -> Wants {
    Wants {
        grants: spelled(&wanted(manifest)),
        reads: manifest
            .programs()
            .filter(|name| !manifest.reads_of(name).is_empty())
            .map(|name| {
                (
                    name.to_owned(),
                    manifest.reads_of(name).iter().cloned().collect(),
                )
            })
            .collect(),
        packaged: manifest
            .packaged_programs()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    }
}

/// Each packaged program nobody accepted, as a held update says it.
fn widened_packaged(new_packaged: &BTreeSet<String>) -> impl Iterator<Item = String> + '_ {
    new_packaged.iter().map(|file| format!("run ./{file}"))
}

/// Each program's new reads, as a held update says them.
fn widened_reads(new_reads: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    new_reads
        .iter()
        .map(|(program, reads)| {
            let reads: Vec<&str> = reads.iter().map(String::as_str).collect();
            format!("{}{program} reads {}", Grant::EXEC, reads.join(", "))
        })
        .collect()
}

fn spelled(grants: &BTreeSet<Grant>) -> BTreeSet<String> {
    grants
        .iter()
        .map(|grant| grant.name().into_owned())
        .collect()
}

pub(super) fn refusal(reason: impl Into<String>) -> UzeError {
    UzeError::ExtensionConsent(reason.into())
}

fn refuse_an_agent(asker: Asker, what: &str) -> Result<()> {
    match asker {
        Asker::Operator => Ok(()),
        Asker::AgentPane => Err(refusal(format!(
            "an agent cannot {what} an extension, nor can anything in a sandbox; the operator \
             does, on the workspace's Extensions screen or from an unsandboxed terminal of \
             their own"
        ))),
    }
}

fn now() -> u64 {
    uze_platform::clock::since_epoch().as_secs()
}

impl Extensions<'_> {
    /// Every declared extension the installed plugins carry, judged for
    /// the project rooted at `project`. Applies an update that wants no
    /// new grant as it finds one.
    #[tracing::instrument(name = "extensions.standings", skip_all)]
    pub fn standings(&self, project: &Path) -> Vec<ExtensionStanding> {
        self.installed()
            .into_iter()
            .map(|installed| {
                let state = self.load(&installed).and_then(|load| {
                    self.judged(&load, project).map_err(|error| {
                        vec![uze_core::capability::extension::Diagnostic::whole(
                            ".",
                            error.to_string(),
                            None,
                        )]
                    })
                });
                let consent = consent::consent(&self.0.home, &consent_key(&installed.identity));
                ExtensionStanding {
                    installed,
                    state,
                    consent,
                }
            })
            .collect()
    }

    /// Where the extension `identity` stands in the project rooted at
    /// `project`.
    pub fn state(&self, identity: &Identity, project: &Path) -> Result<ExtensionState> {
        let load = self.load_identity(identity)?;
        self.judged(&load, project)
    }

    /// What is recorded for `identity` now: what was accepted for it and
    /// the projects it is enabled in. `None` before any consent.
    pub fn consent_record(&self, identity: &Identity) -> Option<ExtensionConsent> {
        consent::consent(&self.0.home, &consent_key(identity))
    }

    /// Records that the operator accepted what `identity` wants, for the
    /// files whose digest they were shown (`shown`, an
    /// [`ExtensionReport`](super::ExtensionReport)'s `digest`). Refused
    /// from an agent's pane, for an extension with nothing to consent to,
    /// and when its files changed while the operator was asked: what they
    /// accepted is not what would run.
    #[tracing::instrument(name = "extensions.consent", skip_all, fields(extension = %identity), err)]
    pub fn consent(&self, identity: &Identity, shown: &str, asker: Asker) -> Result<Consented> {
        refuse_an_agent(asker, "consent to")?;
        let load = self.load_identity(identity)?;
        if load.digest != shown {
            return Err(refusal(format!(
                "{identity} changed while you were asked; ask again"
            )));
        }
        let manifest = &load.compiled.manifest;
        if !needs_consent(manifest) {
            return Err(refusal(format!(
                "{identity} runs nothing and reaches nothing; it needs no consent"
            )));
        }
        let grants = wanted(manifest);
        let pinned = runs_programs(manifest).then(|| Pinned {
            paths: pinned_paths(&resolve_programs(manifest, &self.0.home)),
            enforced: confinement().is_available(),
        });
        let recorded = consent::accept(
            &self.0.home,
            &consent_key(identity),
            &wants_of(manifest),
            &load.digest,
            now(),
            pinned,
        )?;
        if reaches_agents(manifest) {
            consent::keep_to_named_projects(&self.0.home, &consent_key(identity))?;
        }
        Ok(Consented {
            identity: identity.clone(),
            grants,
            digest: recorded.accepted_digest,
            accepted_at: recorded.accepted_at,
        })
    }

    /// The digest of `identity`'s files now: what a surface that shows the
    /// extension from here hands back to [`Self::consent`].
    pub fn digest(&self, identity: &Identity) -> Result<String> {
        Ok(self.load_identity(identity)?.digest)
    }

    /// The programs `identity` wants, each as it would be pinned now: what
    /// the consent step shows before the operator accepts.
    pub fn programs_to_pin(&self, identity: &Identity) -> Result<Vec<ProgramPin>> {
        let load = self.load_identity(identity)?;
        Ok(resolve_programs(&load.compiled.manifest, &self.0.home))
    }

    /// `identity`'s preferences as its surfaces read them: each declared
    /// one, the operator's value where it may hold it, else its default.
    pub fn preferences(&self, identity: &Identity) -> Result<serde_json::Value> {
        let load = self.load_identity(identity)?;
        Ok(admitted_preferences(
            &self.0.home,
            identity,
            &load.compiled.manifest,
        ))
    }

    /// Sets the preference `name` of `identity` to `value`, refused when it
    /// declares no such preference or `value` is not one it may hold.
    /// Anyone may: a preference changes what is drawn, not what is reached.
    #[tracing::instrument(name = "extensions.set_preference", skip_all, fields(extension = %identity, preference = name), err)]
    pub fn set_preference(
        &self,
        identity: &Identity,
        name: &str,
        value: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let load = self.load_identity(identity)?;
        let manifest = &load.compiled.manifest;
        let Some(decl) = manifest.preference(name) else {
            return Err(refusal(format!(
                "{identity} declares no preference `{name}`"
            )));
        };
        if decl.is_secret() {
            set_secret(&self.0.home, identity, name, &value)?;
            return Ok(admitted_preferences(&self.0.home, identity, manifest));
        }
        super::preferences::set(
            &self.0.home,
            &consent_key(identity),
            &manifest.preferences,
            name,
            &value,
        )
    }

    /// The programs `identity` wants, each with the path its consent
    /// pinned, or why it has none.
    pub fn pinned_programs(&self, identity: &Identity) -> Result<Vec<ProgramPin>> {
        let load = self.load_identity(identity)?;
        let manifest = &load.compiled.manifest;
        let recorded = self
            .consent_record(identity)
            .and_then(|consent| consent.exec_paths);
        Ok(manifest
            .programs()
            .map(|name| {
                let path = recorded.as_ref().and_then(|paths| paths.get(name).cloned());
                ProgramPin {
                    name: name.to_owned(),
                    unresolved: path.is_none().then(|| format!("`{name}` not found")),
                    path,
                    runs_anything: is_general_runner(name),
                    reads: manifest.reads_of(name).to_vec(),
                    packaged: false,
                }
            })
            .chain(packaged_pins(manifest))
            .collect())
    }

    /// Resolves `identity`'s programs again, as the operator asked on the
    /// Extensions screen, and records where they are now. Refused from an
    /// agent's pane, and before consent.
    #[tracing::instrument(name = "extensions.resolve_again", skip_all, fields(extension = %identity), err)]
    pub fn resolve_again(&self, identity: &Identity, asker: Asker) -> Result<Vec<ProgramPin>> {
        refuse_an_agent(asker, "resolve the programs of")?;
        let load = self.load_identity(identity)?;
        let pins = resolve_programs(&load.compiled.manifest, &self.0.home);
        consent::repin(&self.0.home, &consent_key(identity), pinned_paths(&pins))?.ok_or_else(|| {
            refusal(format!(
                "{identity} has not been consented to on this machine; its programs are pinned when it is"
            ))
        })?;
        Ok(pins)
    }

    /// Switches `identity` on in the project rooted at `project`. Refused
    /// from an agent's pane, before consent, and while an update holds it.
    #[tracing::instrument(name = "extensions.enable", skip_all, fields(extension = %identity), err)]
    pub fn enable(&self, identity: &Identity, project: &Path, asker: Asker) -> Result<Switched> {
        self.enable_where(identity, project, asker, false)
    }

    /// Switches `identity` on in every project, asked from the one rooted
    /// at `project`: refused as [`Self::enable`] is, and enabled in
    /// `project` alone when it reaches agents.
    #[tracing::instrument(name = "extensions.enable_everywhere", skip_all, fields(extension = %identity), err)]
    pub fn enable_everywhere(
        &self,
        identity: &Identity,
        project: &Path,
        asker: Asker,
    ) -> Result<Switched> {
        self.enable_where(identity, project, asker, true)
    }

    fn enable_where(
        &self,
        identity: &Identity,
        project: &Path,
        asker: Asker,
        everywhere: bool,
    ) -> Result<Switched> {
        refuse_an_agent(asker, "enable")?;
        let load = self.load_identity(identity)?;
        match self.judged(&load, project)? {
            ExtensionState::FixturesOnly => Err(refusal(format!(
                "{identity} runs nothing; it draws wherever its plugin is installed"
            ))),
            ExtensionState::NeedsConsent { .. } => Err(refusal(format!(
                "{identity} has not been consented to on this machine; consent comes before \
                 enabling"
            ))),
            held @ ExtensionState::Held { .. } => Err(refusal(format!(
                "{identity} is held: its update wants {}, which nobody accepted",
                quoted_list(&held.wanted_anew())
            ))),
            ExtensionState::NeedsEnabling | ExtensionState::Enabled => {
                let key = consent_key(identity);
                let everywhere = everywhere && !reaches_agents(&load.compiled.manifest);
                let changed = if everywhere {
                    consent::enable_everywhere(&self.0.home, &key)?
                } else {
                    consent::enable(&self.0.home, &key, project)?
                };
                Ok(Switched {
                    identity: identity.clone(),
                    project: project.to_path_buf(),
                    changed,
                    everywhere,
                })
            }
        }
    }

    /// Switches `identity` off in the project rooted at `project`. Anyone
    /// may: taking a reach away is never what an agent's pane is refused.
    #[tracing::instrument(name = "extensions.disable", skip_all, fields(extension = %identity), err)]
    pub fn disable(&self, identity: &Identity, project: &Path) -> Result<Switched> {
        let changed = consent::disable(&self.0.home, &consent_key(identity), project)?;
        Ok(Switched {
            identity: identity.clone(),
            project: project.to_path_buf(),
            changed,
            everywhere: false,
        })
    }

    /// Switches `identity` off in every project, keeping what was accepted:
    /// the Extensions screen's Disable, which answers for the machine.
    #[tracing::instrument(name = "extensions.disable_everywhere", skip_all, fields(extension = %identity), err)]
    pub fn disable_everywhere(&self, identity: &Identity, project: &Path) -> Result<Switched> {
        let changed = consent::disable_everywhere(&self.0.home, &consent_key(identity))?;
        Ok(Switched {
            identity: identity.clone(),
            project: project.to_path_buf(),
            changed,
            everywhere: true,
        })
    }

    /// Forgets what was accepted for every extension of the plugin
    /// `plugin@marketplace`, and what they kept, as it leaves the machine.
    /// Their preferences stay: the operator's text in `config.toml`, found
    /// again when the plugin is. A secret the keychain keeps does not keep
    /// the consents: it is reported once they are forgotten.
    pub(crate) fn forget_plugin(&self, plugin: &str, marketplace: &str) -> Result<Vec<String>> {
        uze_workspace::extensions::store::forget_plugin(
            &self.0.home,
            &format!("{plugin}@{marketplace}"),
        )?;
        let secrets = uze_workspace::extensions::secrets::forget_plugin(
            &self.0.home,
            &format!("{plugin}@{marketplace}"),
        );
        let forgotten = consent::forget_plugin(&self.0.home, plugin, marketplace)?;
        secrets.map(|()| forgotten)
    }

    fn load_identity(&self, identity: &Identity) -> Result<ExtensionLoad> {
        let installed = self
            .installed()
            .into_iter()
            .find(|installed| installed.identity == *identity)
            .ok_or_else(|| refusal(format!("no installed plugin carries {identity}")))?;
        self.load(&installed).map_err(|diagnostics| {
            let first = diagnostics
                .first()
                .map(ToString::to_string)
                .unwrap_or_default();
            refusal(format!("{identity} does not compile: {first}"))
        })
    }

    pub(super) fn judged(&self, load: &ExtensionLoad, project: &Path) -> Result<ExtensionState> {
        let manifest = &load.compiled.manifest;
        if !needs_consent(manifest) {
            return Ok(ExtensionState::FixturesOnly);
        }
        let key = consent_key(&load.compiled.identity);
        let grants = wanted(manifest);
        let recorded = consent::consent(&self.0.home, &key);
        // Accepted as confined on a machine that no longer confines: what
        // the operator agreed to is not what would run (S18).
        let confinement_lost = runs_programs(manifest)
            && recorded
                .as_ref()
                .is_some_and(|consent| consent.enforced_at_accept == Some(true))
            && !confinement().is_available();
        if confinement_lost {
            return Ok(ExtensionState::NeedsConsent { grants });
        }
        let wants = wants_of(manifest);
        let enabled_here = match consent::judge(recorded.as_ref(), &wants, &load.digest) {
            Judgement::Unconsented => return Ok(ExtensionState::NeedsConsent { grants }),
            Judgement::Held {
                new_grants,
                new_reads,
                new_packaged,
            } => {
                return Ok(ExtensionState::Held {
                    new_grants: new_grants
                        .iter()
                        .filter_map(|name| Grant::parse(name))
                        .collect(),
                    widened: widened_reads(&new_reads)
                        .into_iter()
                        .chain(widened_packaged(&new_packaged))
                        .collect(),
                });
            }
            Judgement::Current => recorded.is_some_and(|consent| consent.enabled_in(project)),
            Judgement::Updated => {
                consent::apply_update(&self.0.home, &key, &wants, &load.digest, now())?
                    .or(recorded)
                    .is_some_and(|consent| consent.enabled_in(project))
            }
        };
        Ok(if enabled_here {
            ExtensionState::Enabled
        } else {
            ExtensionState::NeedsEnabling
        })
    }
}

fn quoted_list(wanted: &[String]) -> String {
    wanted
        .iter()
        .map(|wanted| format!("`{wanted}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::application::extensions::runner::tests::World;

    fn accept(world: &World, identity: &Identity) -> Result<Consented> {
        let extensions = world.app.extensions();
        extensions.consent(
            identity,
            &extensions.digest(identity).unwrap(),
            Asker::Operator,
        )
    }

    fn panel(world: &World) -> Identity {
        Identity {
            id: "gsd-panel".to_owned(),
            ..world.band()
        }
    }

    /// Rewrites the `wants` line of the manifest in `dir`, adding one when
    /// it has none.
    fn wants(dir: &Path, grants: &str) {
        let manifest = dir.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        let wanted = if grants.is_empty() {
            String::new()
        } else {
            format!("wants {grants}")
        };
        let edited = if text.lines().any(|line| line.starts_with("wants ")) {
            text.lines()
                .map(|line| {
                    if line.starts_with("wants ") {
                        wanted.clone()
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            format!("{text}\n{wanted}")
        };
        fs::write(manifest, edited + "\n").unwrap();
    }

    /// Has the band want `exec:sh`, declared to read `reads`.
    fn sh_reads(band: &Path, reads: &str) {
        wants(band, "\"read:checkout\" \"exec:sh\"");
        let manifest = band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        let text = text.split("exec \"sh\"").next().unwrap_or_default();
        fs::write(
            manifest,
            format!("{text}exec \"sh\" {{\n    reads {reads}\n}}\n"),
        )
        .unwrap();
    }

    /// Adds a source to the band that runs `file`, a program packaged with
    /// it, which no grant names.
    fn packaged_program(band: &Path, source: &str, file: &str) {
        fs::create_dir_all(band.join("bin")).unwrap();
        fs::write(band.join(file), b"\x7fELF\x02\x01").unwrap();
        let manifest = band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            manifest,
            format!("{text}source \"{source}\" {{\n    run \"./{file}\"\n}}\n"),
        )
        .unwrap();
    }

    #[test]
    fn only_an_enabled_extension_runs_its_scripts() {
        assert!(ExtensionState::Enabled.runs_scripts());
        assert!(!ExtensionState::NeedsEnabling.runs_scripts());
        assert!(!ExtensionState::FixturesOnly.runs_scripts());
        assert!(
            !ExtensionState::NeedsConsent {
                grants: BTreeSet::new()
            }
            .runs_scripts()
        );
    }

    /// What the operator accepts is recorded for that extension alone, with
    /// the digest they were shown and when, and pins nothing for one that
    /// runs no program.
    #[test]
    fn consent_is_recorded_for_the_extension_accepted_and_no_other() {
        let world = World::new("consent-recorded");
        let extensions = world.app.extensions();
        assert_ne!(consent_key(&world.band()), consent_key(&panel(&world)));
        let states = |extensions: &Extensions| -> Vec<(String, bool, bool)> {
            extensions
                .standings(&world.checkout)
                .into_iter()
                .map(|standing| {
                    (
                        standing.installed.identity.id,
                        matches!(standing.state, Ok(ExtensionState::NeedsConsent { .. })),
                        standing.consent.is_some(),
                    )
                })
                .collect()
        };
        assert_eq!(
            states(&extensions),
            [
                ("gsd-band".to_owned(), true, false),
                ("gsd-panel".to_owned(), false, false)
            ]
        );
        assert_eq!(extensions.consent_record(&world.band()), None);

        let before = uze_platform::clock::since_epoch().as_secs();
        let consented = accept(&world, &world.band()).unwrap();
        assert!(consented.accepted_at >= before, "{consented:?}");
        assert_eq!(consented.digest, extensions.digest(&world.band()).unwrap());

        let recorded = extensions.consent_record(&world.band()).expect("recorded");
        assert_eq!(recorded.accepted_digest, consented.digest);
        assert_eq!(recorded.accepted_at, consented.accepted_at);
        assert_eq!(recorded.exec_paths, None, "it runs no program");
        assert_eq!(recorded.enforced_at_accept, None);
        assert_eq!(extensions.consent_record(&panel(&world)), None);
        assert_eq!(
            states(&extensions),
            [
                ("gsd-band".to_owned(), false, true),
                ("gsd-panel".to_owned(), false, false)
            ]
        );
    }

    /// Consent and resolving programs again are the operator's: an agent's
    /// pane is refused both, and nothing is recorded.
    #[test]
    fn an_agents_pane_can_neither_consent_nor_resolve_programs_again() {
        let world = World::new("consent-agent-pane");
        sh_reads(&world.band, "\"/etc/gsd\"");
        let extensions = world.app.extensions();
        let digest = extensions.digest(&world.band()).unwrap();

        assert!(
            extensions
                .consent(&world.band(), &digest, Asker::AgentPane)
                .is_err()
        );
        assert_eq!(extensions.consent_record(&world.band()), None);

        accept(&world, &world.band()).unwrap();
        assert!(
            extensions
                .resolve_again(&world.band(), Asker::AgentPane)
                .is_err()
        );
    }

    /// An extension runs code, and so needs consent, when it wants any
    /// grant or reads anything but fixtures; one that only draws them
    /// needs none.
    #[test]
    fn a_grant_or_a_script_each_need_consent() {
        let world = World::new("consent-needed");
        let extensions = world.app.extensions();
        let panel_dir = world.band.with_file_name("gsd-panel");
        let per_project = |identity: &Identity| {
            let installed = extensions
                .installed()
                .into_iter()
                .find(|installed| installed.identity == *identity)
                .unwrap();
            crate::enabled_per_project(&extensions.compile(&installed).unwrap())
        };
        assert_eq!(
            extensions.state(&panel(&world), &world.checkout).unwrap(),
            ExtensionState::FixturesOnly
        );
        assert!(!per_project(&panel(&world)));
        assert!(per_project(&world.band()));

        wants(&panel_dir, "\"clipboard\"");
        assert!(matches!(
            extensions.state(&panel(&world), &world.checkout).unwrap(),
            ExtensionState::NeedsConsent { .. }
        ));
        assert!(per_project(&panel(&world)), "a grant alone");

        wants(&world.band, "");
        fs::write(
            world.band.join("state.uze"),
            "fn answer(input, context) { #{ data: #{} } }\n",
        )
        .unwrap();
        assert!(matches!(
            extensions.state(&world.band(), &world.checkout).unwrap(),
            ExtensionState::NeedsConsent { .. }
        ));
        assert!(accept(&world, &world.band()).is_ok(), "a script alone");
    }

    /// A program an `exec:` grant names is pinned where it was found when
    /// consented to, listed with what it reads, and an update widening its
    /// reads holds the extension until someone accepts it.
    #[test]
    fn a_program_is_pinned_at_consent_and_a_wider_read_holds_the_update() {
        let world = World::new("consent-program");
        sh_reads(&world.band, "\"/etc/gsd\"");
        let extensions = world.app.extensions();
        let to_pin = extensions.programs_to_pin(&world.band()).unwrap();
        let listed: Vec<_> = to_pin
            .iter()
            .map(|pin| {
                (
                    pin.name.as_str(),
                    pin.reads.clone(),
                    pin.runs_anything,
                    pin.packaged,
                )
            })
            .collect();
        assert_eq!(listed, [("sh", vec!["/etc/gsd".to_owned()], true, false)]);
        assert!(
            extensions
                .resolve_again(&world.band(), Asker::Operator)
                .is_err(),
            "nothing is pinned before consent"
        );

        accept(&world, &world.band()).unwrap();
        let recorded = extensions.consent_record(&world.band()).unwrap();
        assert!(recorded.exec_paths.is_some(), "{recorded:?}");
        assert!(recorded.enforced_at_accept.is_some());
        let pinned = extensions.pinned_programs(&world.band()).unwrap();
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].name, "sh");
        assert_eq!(pinned[0].path, to_pin[0].path, "pinned where it was found");
        assert_eq!(
            extensions
                .resolve_again(&world.band(), Asker::Operator)
                .unwrap(),
            to_pin
        );

        extensions
            .enable(&world.band(), &world.checkout, Asker::Operator)
            .unwrap();
        assert_eq!(
            extensions.state(&world.band(), &world.checkout).unwrap(),
            ExtensionState::Enabled
        );

        sh_reads(&world.band, "\"/etc/gsd\" \"/home/op/.ssh\"");
        assert_eq!(
            extensions.state(&world.band(), &world.checkout).unwrap(),
            ExtensionState::Held {
                new_grants: BTreeSet::new(),
                widened: vec!["exec:sh reads /home/op/.ssh".to_owned()],
            }
        );
        let refused = extensions
            .enable(&world.band(), &world.checkout, Asker::Operator)
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("wants `exec:sh reads /home/op/.ssh`, which nobody accepted"),
            "{refused}"
        );
    }

    /// A program packaged with the extension is listed for consent by its
    /// file and makes the consent record whether it ran confined; an update
    /// adding another holds the extension.
    #[test]
    fn a_packaged_program_is_listed_and_one_added_holds_the_update() {
        let world = World::new("consent-packaged");
        packaged_program(&world.band, "meter", "bin/meter");
        let extensions = world.app.extensions();
        let listed: Vec<_> = extensions
            .programs_to_pin(&world.band())
            .unwrap()
            .into_iter()
            .map(|pin| (pin.name, pin.packaged))
            .collect();
        assert_eq!(listed, [("./bin/meter".to_owned(), true)]);

        accept(&world, &world.band()).unwrap();
        let recorded = extensions.consent_record(&world.band()).unwrap();
        assert!(recorded.enforced_at_accept.is_some(), "{recorded:?}");
        assert_eq!(recorded.exec_paths, Some(BTreeMap::new()), "never pinned");
        extensions
            .enable(&world.band(), &world.checkout, Asker::Operator)
            .unwrap();

        packaged_program(&world.band, "upload", "bin/upload");
        assert_eq!(
            extensions.state(&world.band(), &world.checkout).unwrap(),
            ExtensionState::Held {
                new_grants: BTreeSet::new(),
                widened: vec!["run ./bin/upload".to_owned()],
            }
        );
    }

    /// A preference is set to a value it may hold and read back as the
    /// extension reads it; one it does not declare is refused.
    #[test]
    fn a_declared_preference_is_set_and_read_back() {
        let world = World::new("consent-preferences");
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            format!(
                "{text}\npreferences {{\n    number \"warn-at\" label=\"Warn at\" default=70 min=1 max=100\n}}\n"
            ),
        )
        .unwrap();
        let extensions = world.app.extensions();
        assert_eq!(
            extensions.preferences(&world.band()).unwrap(),
            serde_json::json!({ "warn-at": 70 })
        );
        assert_eq!(
            extensions
                .set_preference(&world.band(), "warn-at", 40.into())
                .unwrap(),
            serde_json::json!({ "warn-at": 40 })
        );
        assert_eq!(
            extensions.preferences(&world.band()).unwrap(),
            serde_json::json!({ "warn-at": 40 })
        );
        assert!(
            extensions
                .set_preference(&world.band(), "absent", 1.into())
                .is_err()
        );
    }

    /// A secret is kept when it is one line of text, forgotten when nothing
    /// was typed, and anything else is refused.
    // Only where a secret is kept in a file: on macOS storing one writes the
    // operator's own Keychain, and on Windows the Credential Manager, each
    // the machine's own.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_secret_is_kept_as_one_line_and_forgotten_when_emptied() {
        let world = World::new("consent-secret");
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            format!("{text}\npreferences {{\n    secret \"token\" label=\"Token\"\n}}\n"),
        )
        .unwrap();
        wants(&world.band, "\"read:checkout\" \"net:api.github.com\"");
        let extensions = world.app.extensions();
        let set = || {
            uze_workspace::extensions::secrets::set_names(
                &world.app.home,
                &plugin_key(&world.band()),
                "gsd-band",
            )
        };

        extensions
            .set_preference(&world.band(), "token", "ghp_x".into())
            .unwrap();
        assert!(set().contains("token"));
        for refused in [
            serde_json::json!("two\nlines"),
            serde_json::json!("a".repeat(4097)),
            serde_json::json!(5),
        ] {
            assert!(
                extensions
                    .set_preference(&world.band(), "token", refused.clone())
                    .is_err(),
                "{refused}"
            );
        }
        assert!(set().contains("token"), "a refused value keeps the secret");
        extensions
            .set_preference(&world.band(), "token", "".into())
            .unwrap();
        assert!(set().is_empty());
    }

    /// A plugin leaving the machine forgets what was accepted for each of
    /// its extensions, and names them.
    #[test]
    fn a_plugin_leaving_forgets_its_extensions_consent() {
        let world = World::new("consent-forget");
        let extensions = world.app.extensions();
        accept(&world, &world.band()).unwrap();

        assert_eq!(
            extensions.forget_plugin("gsd", "studio").unwrap(),
            [consent_key(&world.band())]
        );
        assert_eq!(extensions.consent_record(&world.band()), None);
        assert!(
            extensions
                .forget_plugin("gsd", "studio")
                .unwrap()
                .is_empty()
        );
    }
}
