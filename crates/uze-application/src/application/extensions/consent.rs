//! Where each declared extension stands with the operator in a project:
//! whether it needs consent, enabling, or nothing at all, and the three
//! answers that move it (`add-extension-platform` D5).
//!
//! What a state is judged against is what one load of the extension read:
//! its grants from that compile, its digest from those bytes. An update
//! that wants no new grant is applied as it is judged, recording the new
//! digest and its date; one that wants more holds the extension.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use uze_contract::{Does, Grant, Identity, Manifest, SourceKind};
use uze_core::{Result, UzeError};
use uze_workspace::extensions::consent::{self, Judgement};

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
            .any(|action| matches!(action.does, Does::Script(_)))
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
        let recorded = consent::accept(
            &self.0.home,
            &consent_key(identity),
            &spelled(&grants),
            &load.digest,
            now(),
        )?;
        Ok(Consented {
            identity: identity.clone(),
            grants,
            digest: recorded.accepted_digest,
            accepted_at: recorded.accepted_at,
        })
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
    /// `plugin@marketplace`, as it leaves the machine.
    pub(crate) fn forget_plugin(&self, plugin: &str, marketplace: &str) -> Result<Vec<String>> {
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
