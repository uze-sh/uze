//! Where each declared extension stands with the operator in a project:
//! whether it needs consent, enabling, or nothing at all, and the three
//! answers that move it (`add-extension-platform` D5).
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
use uze_workspace::extensions::consent::{self, Judgement, Pinned};

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
    /// An update wants grants nobody accepted; nothing of it runs until
    /// someone does.
    Held { new_grants: BTreeSet<Grant> },
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
            .any(|action| matches!(action.does, Does::Script(_) | Does::Run(_)))
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
}

/// `plugin@marketplace`, as the stores and preferences are kept under.
pub(super) fn plugin_key(identity: &Identity) -> String {
    format!("{}@{}", identity.plugin, identity.marketplace)
}

/// Each preference `manifest` declares, at the operator's value where it
/// may hold it and at its default otherwise.
pub(super) fn admitted_preferences(
    home: &uze_core::UzeHome,
    identity: &Identity,
    manifest: &Manifest,
) -> serde_json::Value {
    let set =
        uze_workspace::extensions::preferences::read(home, &plugin_key(identity), &identity.id);
    serde_json::Value::Object(
        manifest
            .preferences
            .iter()
            .map(|decl| (decl.name.clone(), decl.admit(set.get(&decl.name))))
            .collect(),
    )
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
            }
        })
        .collect()
}

fn pinned_paths(pins: &[ProgramPin]) -> BTreeMap<String, PathBuf> {
    pins.iter()
        .filter_map(|pin| Some((pin.name.clone(), pin.path.clone()?)))
        .collect()
}

fn wanted(manifest: &Manifest) -> BTreeSet<Grant> {
    manifest.wants.iter().cloned().collect()
}

fn spelled(grants: &BTreeSet<Grant>) -> BTreeSet<String> {
    grants
        .iter()
        .map(|grant| grant.name().into_owned())
        .collect()
}

fn refusal(reason: impl Into<String>) -> UzeError {
    UzeError::ExtensionConsent(reason.into())
}

fn refuse_an_agent(asker: Asker, what: &str) -> Result<()> {
    match asker {
        Asker::Operator => Ok(()),
        Asker::AgentPane => Err(refusal(format!(
            "an agent cannot {what} an extension; the operator does, on the workspace's \
             Extensions screen or from a terminal of their own"
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
    /// files it has now. Refused from an agent's pane, and for an
    /// extension with nothing to consent to.
    #[tracing::instrument(name = "extensions.consent", skip_all, fields(extension = %identity), err)]
    pub fn consent(&self, identity: &Identity, asker: Asker) -> Result<Consented> {
        refuse_an_agent(asker, "consent to")?;
        let load = self.load_identity(identity)?;
        let manifest = &load.compiled.manifest;
        if !needs_consent(manifest) {
            return Err(refusal(format!(
                "{identity} runs nothing and reaches nothing; it needs no consent"
            )));
        }
        let grants = wanted(manifest);
        let pinned = (manifest.programs().next().is_some()).then(|| Pinned {
            paths: pinned_paths(&resolve_programs(manifest, &self.0.home)),
            enforced: confinement().is_available(),
        });
        let recorded = consent::accept(
            &self.0.home,
            &consent_key(identity),
            &spelled(&grants),
            &load.digest,
            now(),
            pinned,
        )?;
        Ok(Consented {
            identity: identity.clone(),
            grants,
            digest: recorded.accepted_digest,
            accepted_at: recorded.accepted_at,
        })
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
        if !decl.holds(&value) {
            return Err(refusal(format!(
                "`{value}` is not a value the preference `{name}` may hold"
            )));
        }
        uze_workspace::extensions::preferences::set(
            &self.0.home,
            &plugin_key(identity),
            &identity.id,
            name,
            value,
        )?;
        Ok(admitted_preferences(&self.0.home, identity, manifest))
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
                }
            })
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
        refuse_an_agent(asker, "enable")?;
        match self.state(identity, project)? {
            ExtensionState::FixturesOnly => Err(refusal(format!(
                "{identity} runs nothing; it draws wherever its plugin is installed"
            ))),
            ExtensionState::NeedsConsent { .. } => Err(refusal(format!(
                "{identity} has not been consented to on this machine; consent comes before \
                 enabling"
            ))),
            ExtensionState::Held { new_grants } => Err(refusal(format!(
                "{identity} is held: its update wants {}, which nobody accepted",
                grant_list(&new_grants)
            ))),
            ExtensionState::NeedsEnabling | ExtensionState::Enabled => {
                let changed = consent::enable(&self.0.home, &consent_key(identity), project)?;
                Ok(Switched {
                    identity: identity.clone(),
                    project: project.to_path_buf(),
                    changed,
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
        })
    }

    /// Forgets what was accepted for every extension of the plugin
    /// `plugin@marketplace`, and what they kept, as it leaves the machine.
    pub(crate) fn forget_plugin(&self, plugin: &str, marketplace: &str) -> Result<Vec<String>> {
        uze_workspace::extensions::store::forget_plugin(
            &self.0.home,
            &format!("{plugin}@{marketplace}"),
        )?;
        uze_workspace::extensions::preferences::forget_plugin(
            &self.0.home,
            &format!("{plugin}@{marketplace}"),
        )?;
        consent::forget_plugin(&self.0.home, plugin, marketplace)
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
        let confinement_lost = manifest.programs().next().is_some()
            && recorded
                .as_ref()
                .is_some_and(|consent| consent.enforced_at_accept == Some(true))
            && !confinement().is_available();
        if confinement_lost {
            return Ok(ExtensionState::NeedsConsent { grants });
        }
        let enabled_here = match consent::judge(recorded.as_ref(), &spelled(&grants), &load.digest)
        {
            Judgement::Unconsented => return Ok(ExtensionState::NeedsConsent { grants }),
            Judgement::Held { new_grants } => {
                return Ok(ExtensionState::Held {
                    new_grants: new_grants
                        .iter()
                        .filter_map(|name| Grant::parse(name))
                        .collect(),
                });
            }
            Judgement::Current => recorded.is_some_and(|consent| consent.enabled_in(project)),
            Judgement::Updated => {
                consent::apply_update(&self.0.home, &key, &spelled(&grants), &load.digest, now())?
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

fn grant_list(grants: &BTreeSet<Grant>) -> String {
    grants
        .iter()
        .map(|grant| format!("`{grant}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
