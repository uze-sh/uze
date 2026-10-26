//! The operator's consent to the extensions plugins declare: which grants
//! they accepted for which code, and the projects each extension is
//! enabled in (`add-extension-platform` D5).
//!
//! One record for the machine, `state/extensions.json`, keyed by the
//! extension's identity `plugin@marketplace/id`: consent is given once per
//! machine, because the code is the same in every project, and enabling
//! is per project. A same-named extension of another plugin, or the same
//! plugin from another marketplace, is a different key and shares nothing.
//!
//! Grants are kept as the words a manifest spells them with, so a grant a
//! newer build knows reads here as one this build does not, never as none.
//! Nothing here runs anything, or reads an extension: what an extension
//! wants and what its files digest to are the caller's to say.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uze_core::path::Canonical as _;

use crate::{Result, UzeError, home::UzeHome, persistence::write_atomic};

const LOCK_PATIENCE: Duration = Duration::from_secs(2);

/// What the operator accepted for one extension.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Consent {
    /// The grants accepted, as a manifest spells them.
    pub accepted_grants: BTreeSet<String>,
    /// `sha256:<hex>` of the extension's files the grants were accepted
    /// for, or the update applied since under the same or fewer grants.
    pub accepted_digest: String,
    /// When that digest was accepted, in seconds since the epoch.
    pub accepted_at: u64,
    /// The projects it runs in, each by its canonical root, as a
    /// project's approved commands name it (ADR-055).
    pub enabled_in: BTreeSet<PathBuf>,
    /// Each `exec:` program as it was resolved when accepted. Filled by
    /// the change that brings programs; absent until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec_paths: Option<BTreeMap<String, PathBuf>>,
    /// Whether the machine confined programs when they were accepted.
    /// Filled with `exec_paths`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforced_at_accept: Option<bool>,
}

impl Consent {
    pub fn enabled_in(&self, project: &Path) -> bool {
        self.enabled_in.contains(&canonical(project))
    }
}

/// The key an extension's consent is recorded under.
pub fn identity_key(plugin: &str, marketplace: &str, id: &str) -> String {
    format!("{plugin}@{marketplace}/{id}")
}

/// What an extension's current files and grants are, against its record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Judgement {
    /// Nothing was accepted for it.
    Unconsented,
    /// The record covers exactly these files.
    Current,
    /// The files changed and want no grant beyond what was accepted: the
    /// update applies, and the record owes it the new digest.
    Updated,
    /// The files want grants nobody accepted; nothing of it runs until
    /// someone does.
    Held { new_grants: BTreeSet<String> },
}

/// Judges an extension wanting `grants` with files digesting to `digest`
/// against what `consent` accepted. Compared by grant set: a wider set
/// holds it, the same set or a narrower one applies.
pub fn judge(consent: Option<&Consent>, grants: &BTreeSet<String>, digest: &str) -> Judgement {
    let Some(consent) = consent else {
        return Judgement::Unconsented;
    };
    let new_grants: BTreeSet<String> = grants
        .difference(&consent.accepted_grants)
        .cloned()
        .collect();
    if !new_grants.is_empty() {
        Judgement::Held { new_grants }
    } else if consent.accepted_digest == digest {
        Judgement::Current
    } else {
        Judgement::Updated
    }
}

/// Every consent on this machine, by identity key. Fails closed: a record
/// that cannot be read, or that a newer build wrote, consents to nothing.
pub fn consents(home: &UzeHome) -> BTreeMap<String, Consent> {
    uze_document::read::<ExtensionsRecord>(&home.extensions_record_path())
        .ok()
        .and_then(uze_document::Carried::record)
        .map(|record| record.extensions)
        .unwrap_or_default()
}

pub fn consent(home: &UzeHome, key: &str) -> Option<Consent> {
    consents(home).remove(key)
}

/// Records that the operator accepted `grants` for the files digesting to
/// `digest`, at `now`. The projects it was enabled in stay enabled: they
/// are the operator's earlier answer, about the same extension.
pub fn accept(
    home: &UzeHome,
    key: &str,
    grants: &BTreeSet<String>,
    digest: &str,
    now: u64,
) -> Result<Consent> {
    rewrite(home, |extensions| {
        let consent = extensions.entry(key.to_owned()).or_default();
        consent.accepted_grants = grants.clone();
        consent.accepted_digest = digest.to_owned();
        consent.accepted_at = now;
        Ok(consent.clone())
    })
}

/// Applies an update that wants no new grant: the record takes the new
/// digest, the date and the grants as they are now, so a grant the update
/// dropped is asked for again if it comes back. `None` when the record does
/// not judge it [`Judgement::Updated`] any more (another process applied
/// it, or it widened in between): judged again under the lock.
pub fn apply_update(
    home: &UzeHome,
    key: &str,
    grants: &BTreeSet<String>,
    digest: &str,
    now: u64,
) -> Result<Option<Consent>> {
    rewrite(home, |extensions| {
        let Some(consent) = extensions.get_mut(key) else {
            return Ok(None);
        };
        if judge(Some(consent), grants, digest) != Judgement::Updated {
            return Ok(None);
        }
        consent.accepted_grants = grants.clone();
        consent.accepted_digest = digest.to_owned();
        consent.accepted_at = now;
        Ok(Some(consent.clone()))
    })
}

/// Enables a consented extension in the project rooted at `project`, and
/// says whether it was not enabled there already. Refused for an extension
/// nobody consented to: enabling is never the first answer.
pub fn enable(home: &UzeHome, key: &str, project: &Path) -> Result<bool> {
    let project = canonical(project);
    rewrite(home, |extensions| {
        let consent = extensions.get_mut(key).ok_or_else(|| {
            UzeError::ExtensionConsent(format!(
                "{key} has not been consented to on this machine; consent comes before enabling"
            ))
        })?;
        Ok(consent.enabled_in.insert(project))
    })
}

/// Disables it in that project, and says whether it was enabled there.
pub fn disable(home: &UzeHome, key: &str, project: &Path) -> Result<bool> {
    let project = canonical(project);
    if !consent(home, key).is_some_and(|consent| consent.enabled_in.contains(&project)) {
        return Ok(false);
    }
    rewrite(home, |extensions| {
        Ok(extensions
            .get_mut(key)
            .is_some_and(|consent| consent.enabled_in.remove(&project)))
    })
}

/// Forgets every extension of `plugin` from `marketplace`, as the plugin
/// leaves the machine, and names the keys forgotten.
pub fn forget_plugin(home: &UzeHome, plugin: &str, marketplace: &str) -> Result<Vec<String>> {
    let prefix = identity_key(plugin, marketplace, "");
    if !consents(home).keys().any(|key| key.starts_with(&prefix)) {
        return Ok(Vec::new());
    }
    rewrite(home, |extensions| {
        let forgotten: Vec<String> = extensions
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in &forgotten {
            extensions.remove(key);
        }
        Ok(forgotten)
    })
}

/// The record as `state/extensions.json` holds it.
#[derive(Debug, Deserialize, Serialize)]
struct ExtensionsRecord {
    schema_version: u32,
    extensions: BTreeMap<String, Consent>,
}

impl Default for ExtensionsRecord {
    fn default() -> Self {
        Self {
            schema_version: <Self as uze_document::Shaped>::SHAPE,
            extensions: BTreeMap::new(),
        }
    }
}

/// A record: only the operator's answers say it, and nothing on the
/// machine can derive them again.
impl uze_document::Shaped for ExtensionsRecord {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "extension consents";
}

/// One read-modify-write of the record under its lock. A record this build
/// cannot read is set aside rather than written over, and one a newer
/// build wrote is left alone and refused.
fn rewrite<T>(
    home: &UzeHome,
    change: impl FnOnce(&mut BTreeMap<String, Consent>) -> Result<T>,
) -> Result<T> {
    let _held =
        crate::file_lock::Held::acquire(&home.extensions_record_lock_path(), LOCK_PATIENCE)?;
    let path = home.extensions_record_path();
    let mut record = match uze_document::read::<ExtensionsRecord>(&path) {
        Ok(carried) => carried.or_default(),
        Err(reason) if uze_document::may_be_set_aside(&reason) => {
            uze_document::set_aside(
                &path,
                <ExtensionsRecord as uze_document::Shaped>::KIND,
                &reason,
            )?;
            ExtensionsRecord::default()
        }
        Err(reason) => return Err(reason.into()),
    };
    let answer = change(&mut record.extensions)?;
    record.schema_version = <ExtensionsRecord as uze_document::Shaped>::SHAPE;
    write_atomic(
        &path,
        &serde_json::to_vec_pretty(&record).expect("the extensions record serializes"),
    )?;
    Ok(answer)
}

fn canonical(root: &Path) -> PathBuf {
    root.canonical().unwrap_or_else(|_| root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(label: &str) -> (UzeHome, PathBuf) {
        let root = uze_testkit::temp::scratch(label);
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        (UzeHome::at(root.join("uze")), project)
    }

    fn grants(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    const BAND: &str = "gsd@acme/gsd-band";

    #[test]
    fn a_machine_that_never_answered_consents_to_nothing() {
        let (home, _) = home("consent-none");
        assert!(consents(&home).is_empty());
        assert_eq!(
            judge(None, &grants(&["read:checkout"]), "sha256:a"),
            Judgement::Unconsented
        );
    }

    #[test]
    fn consent_is_recorded_by_identity_with_its_grants_and_digest() {
        let (home, project) = home("consent-record");
        let accepted = accept(&home, BAND, &grants(&["read:checkout"]), "sha256:a", 7).unwrap();
        assert_eq!(accepted.accepted_at, 7);
        assert!(enable(&home, BAND, &project).unwrap());
        assert!(!enable(&home, BAND, &project).unwrap(), "already enabled");

        let recorded = consent(&home, BAND).unwrap();
        assert_eq!(recorded.accepted_grants, grants(&["read:checkout"]));
        assert_eq!(recorded.accepted_digest, "sha256:a");
        assert!(recorded.enabled_in(&project));
        assert_eq!(recorded.exec_paths, None);

        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(home.extensions_record_path()).unwrap()).unwrap();
        assert_eq!(written["schema_version"], 1);
        assert!(
            written["extensions"][BAND].get("exec_paths").is_none(),
            "a field this change does not fill is not written"
        );
    }

    #[test]
    fn a_same_named_extension_of_another_plugin_is_a_stranger() {
        let (home, project) = home("consent-stranger");
        accept(&home, BAND, &grants(&["read:checkout"]), "sha256:a", 1).unwrap();
        enable(&home, BAND, &project).unwrap();
        for stranger in [
            identity_key("evil", "acme", "gsd-band"),
            identity_key("gsd", "other", "gsd-band"),
        ] {
            assert_eq!(consent(&home, &stranger), None, "{stranger}");
            assert!(enable(&home, &stranger, &project).is_err(), "{stranger}");
        }
    }

    #[test]
    fn enabling_belongs_to_one_project() {
        let (home, project) = home("consent-per-project");
        let other = project.parent().unwrap().join("other");
        std::fs::create_dir_all(&other).unwrap();
        accept(&home, BAND, &grants(&[]), "sha256:a", 1).unwrap();
        enable(&home, BAND, &project).unwrap();

        let recorded = consent(&home, BAND).unwrap();
        assert!(recorded.enabled_in(&project));
        assert!(!recorded.enabled_in(&other));

        assert!(disable(&home, BAND, &project).unwrap());
        assert!(!disable(&home, BAND, &project).unwrap(), "nothing left");
        assert!(!consent(&home, BAND).unwrap().enabled_in(&project));
    }

    #[test]
    fn an_update_is_judged_by_its_grant_set() {
        let accepted = Consent {
            accepted_grants: grants(&["read:checkout", "clipboard"]),
            accepted_digest: "sha256:a".to_owned(),
            ..Consent::default()
        };
        let same = grants(&["read:checkout", "clipboard"]);
        assert_eq!(
            judge(Some(&accepted), &same, "sha256:a"),
            Judgement::Current
        );
        assert_eq!(
            judge(Some(&accepted), &same, "sha256:b"),
            Judgement::Updated
        );
        assert_eq!(
            judge(Some(&accepted), &grants(&["read:checkout"]), "sha256:b"),
            Judgement::Updated,
            "a narrowing applies"
        );
        assert_eq!(
            judge(
                Some(&accepted),
                &grants(&["read:checkout", "open-path"]),
                "sha256:b"
            ),
            Judgement::Held {
                new_grants: grants(&["open-path"])
            },
            "a widening is held, naming only what is new"
        );
    }

    #[test]
    fn an_applied_update_records_the_new_digest_and_date_and_keeps_the_projects() {
        let (home, project) = home("consent-update");
        accept(
            &home,
            BAND,
            &grants(&["read:checkout", "clipboard"]),
            "sha256:a",
            1,
        )
        .unwrap();
        enable(&home, BAND, &project).unwrap();

        let applied = apply_update(&home, BAND, &grants(&["read:checkout"]), "sha256:b", 9)
            .unwrap()
            .expect("a narrowing applies");
        assert_eq!(applied.accepted_digest, "sha256:b");
        assert_eq!(applied.accepted_at, 9);
        assert_eq!(applied.accepted_grants, grants(&["read:checkout"]));
        assert!(applied.enabled_in(&project));

        assert_eq!(
            apply_update(
                &home,
                BAND,
                &grants(&["read:checkout", "clipboard"]),
                "sha256:c",
                10
            )
            .unwrap(),
            None,
            "the dropped grant coming back is a widening, never applied silently"
        );
        assert_eq!(consent(&home, BAND).unwrap().accepted_digest, "sha256:b");
    }

    #[test]
    fn forgetting_a_plugin_forgets_its_extensions_only() {
        let (home, _) = home("consent-forget");
        let notes = identity_key("gsd", "acme", "notes");
        let other = identity_key("gsd-extra", "acme", "gsd-band");
        for key in [BAND, notes.as_str(), other.as_str()] {
            accept(&home, key, &grants(&[]), "sha256:a", 1).unwrap();
        }
        let forgotten = forget_plugin(&home, "gsd", "acme").unwrap();
        assert_eq!(forgotten, [BAND.to_owned(), notes.clone()]);
        assert_eq!(
            consents(&home).into_keys().collect::<Vec<_>>(),
            [other],
            "a plugin whose name starts the same is another plugin"
        );
        assert!(forget_plugin(&home, "gsd", "acme").unwrap().is_empty());
    }

    #[test]
    fn a_record_this_build_cannot_read_consents_to_nothing_and_is_set_aside_on_write() {
        let (home, _) = home("consent-unreadable");
        std::fs::create_dir_all(home.state_dir()).unwrap();
        std::fs::write(home.extensions_record_path(), b"{ not json").unwrap();
        assert!(consents(&home).is_empty());

        accept(&home, BAND, &grants(&[]), "sha256:a", 1).unwrap();
        assert!(consent(&home, BAND).is_some());
        let kept = std::fs::read_dir(home.state_dir())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(uze_document::SET_ASIDE_MARK)
            });
        assert!(
            kept,
            "the unreadable bytes were kept aside, not overwritten"
        );
    }

    #[test]
    fn a_newer_builds_record_is_left_alone() {
        let (home, _) = home("consent-newer");
        std::fs::create_dir_all(home.state_dir()).unwrap();
        let newer = br#"{"schema_version": 2, "extensions": {}}"#;
        std::fs::write(home.extensions_record_path(), newer).unwrap();

        assert!(accept(&home, BAND, &grants(&[]), "sha256:a", 1).is_err());
        assert_eq!(
            std::fs::read(home.extensions_record_path()).unwrap(),
            newer.to_vec()
        );
    }
}
