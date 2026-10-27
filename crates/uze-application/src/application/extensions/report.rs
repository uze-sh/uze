//! Each declared extension as the operator's surfaces report it: whose it
//! is, what it wants, and where it stands, on the machine or in one
//! project (`extension-consent`, "The operator can see each extension's
//! state"). The Extensions screen, `uze status`, `uze inspect` and the
//! workspace's offer all read this one shape, so they say the same thing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uze_contract::{Clean, Identity};
use uze_workspace::extensions::consent;

use super::{ExtensionCheckout, ExtensionState, NotACheckout, ProgramPin, consent_key};
use crate::application::services::Extensions;

/// Where an extension stands, as one word a report prints.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionCondition {
    /// Runs nothing and reaches nothing: draws from its fixtures.
    FixturesOnly,
    /// Runs code nobody on this machine accepted.
    NeedsConsent,
    /// Consented to, and not switched on in the project asked about.
    NeedsEnabling,
    /// Consented to and switched on in the project asked about.
    Enabled,
    /// Consented to: the machine's answer, with no project asked about.
    Consented,
    /// An update wants grants nobody accepted.
    Held,
    /// Does not compile, so wants nothing anybody could accept.
    Broken,
}

impl ExtensionCondition {
    /// What a person reads for it.
    pub fn said(self) -> &'static str {
        match self {
            Self::FixturesOnly => "runs nothing",
            Self::NeedsConsent => "needs consent",
            Self::NeedsEnabling => "needs enabling here",
            Self::Enabled => "enabled",
            Self::Consented => "consented",
            Self::Held => "held",
            Self::Broken => "does not compile",
        }
    }

    /// Whether the operator has something to answer for it here.
    pub fn awaits_the_operator(self) -> bool {
        matches!(self, Self::NeedsConsent | Self::NeedsEnabling | Self::Held)
    }
}

/// One declared extension, reported. Every text is the plugin's, cleaned
/// to be printed or drawn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExtensionReport {
    pub id: String,
    pub plugin: String,
    pub marketplace: String,
    /// What its manifest calls it, or its id when it does not compile.
    pub name: String,
    /// What it wants, as a manifest spells each grant.
    pub grants: Vec<String>,
    pub condition: ExtensionCondition,
    /// What a held update wants beyond what was accepted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_grants: Vec<String>,
    /// The projects it is enabled in, by canonical root.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enabled_in: Vec<PathBuf>,
    /// When an update that wanted no new grant was last applied, in
    /// seconds since the epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
    /// Why the project asked about is no checkout it may read, when it
    /// would run there: enabling it changes nothing until this is fixed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_refused: Option<String>,
    /// The operator put away the workspace's offer to enable it in the
    /// project asked about.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub offer_declined: bool,
    /// What its last compile said was wrong, one problem a line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Each program its `exec:` grants name: pinned where it was
    /// consented to, as it would be pinned where it was not.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<ProgramPin>,
    /// Whether those programs run confined, when it has any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confined: Option<bool>,
}

impl ExtensionReport {
    pub fn identity(&self) -> Identity {
        Identity {
            marketplace: self.marketplace.clone(),
            plugin: self.plugin.clone(),
            id: self.id.clone(),
        }
    }

    /// `id · plugin@marketplace`, as every surface attributes it.
    pub fn attribution(&self) -> String {
        self.identity().to_string()
    }

    /// `plugin@marketplace`.
    pub fn plugin_id(&self) -> String {
        format!("{}@{}", self.plugin, self.marketplace)
    }

    /// The day an update that wanted no new grant was applied, as
    /// `2026-10-26` in UTC.
    pub fn updated_on(&self) -> Option<String> {
        self.updated_at.map(utc_day)
    }
}

/// What a grant lets an extension do, as the operator reads it before
/// accepting it.
pub fn grant_meaning(grant: &str) -> &'static str {
    match uze_contract::Grant::parse(grant) {
        Some(uze_contract::Grant::ReadCheckout) => "reads the files of the checkout in front",
        Some(uze_contract::Grant::Clipboard) => "copies text you choose to the clipboard",
        Some(uze_contract::Grant::OpenPath) => "opens a file of the checkout in its program",
        Some(uze_contract::Grant::Store) => {
            "keeps what it remembers for this project in UZE's state"
        }
        Some(uze_contract::Grant::StoreGlobal) => {
            "keeps what it remembers in UZE's state, shared by every project"
        }
        Some(uze_contract::Grant::Exec(_)) => "runs this program on your machine",
        Some(uze_contract::Grant::Notify) => {
            "notifies you and sounds a cue, a few times a minute at most"
        }
        None => "a grant this build does not know",
    }
}

/// `seconds` since the epoch as its UTC day, `YYYY-MM-DD`.
fn utc_day(seconds: u64) -> String {
    // Howard Hinnant's civil-from-days, March-based so the leap day ends
    // the year.
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let day = of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

impl Extensions<'_> {
    /// The project `cwd` is inside, by the root its extensions are enabled
    /// under, or `None` outside every project.
    pub fn project_here(&self, cwd: &Path) -> Option<PathBuf> {
        let canonical = uze_platform::path::canonical(cwd).ok()?;
        uze_core::project_root::resolve_project_root(&canonical)
            .ok()
            .flatten()
            .map(|root| crate::project_root_of(&root))
    }

    /// Every declared extension the installed plugins carry, reported for
    /// the project rooted at `project`, or for the machine with `None`.
    /// Applies an update that wants no new grant as it finds one, as
    /// [`Self::standings`] does.
    #[tracing::instrument(name = "extensions.reports", skip_all)]
    pub fn reports(&self, project: Option<&Path>) -> Vec<ExtensionReport> {
        self.reports_over(project, |path| self.checkout(path))
    }

    /// [`Self::reports`], with the checkout resolved by `checkout`.
    pub(crate) fn reports_over(
        &self,
        project: Option<&Path>,
        checkout: impl FnOnce(&Path) -> Result<ExtensionCheckout, NotACheckout>,
    ) -> Vec<ExtensionReport> {
        let refused = project.and_then(|project| {
            checkout(project)
                .err()
                .map(|refusal| refusal.reason().to_owned())
        });
        self.installed()
            .into_iter()
            .map(|installed| {
                let identity = installed.identity.clone();
                let key = consent_key(&identity);
                let mut report = ExtensionReport {
                    name: Clean::label(&identity.id).into_string(),
                    id: identity.id.clone(),
                    plugin: identity.plugin.clone(),
                    marketplace: identity.marketplace.clone(),
                    grants: Vec::new(),
                    condition: ExtensionCondition::Broken,
                    new_grants: Vec::new(),
                    enabled_in: Vec::new(),
                    updated_at: None,
                    checkout_refused: None,
                    offer_declined: project.is_some_and(|project| {
                        consent::offer_declined(&self.0.home, &key, project)
                    }),
                    errors: Vec::new(),
                    programs: Vec::new(),
                    confined: None,
                };
                let load = match self.load(&installed) {
                    Ok(load) => load,
                    Err(diagnostics) => {
                        report.errors = diagnostics
                            .iter()
                            .map(|diagnostic| Clean::text(&diagnostic.to_string()).into_string())
                            .collect();
                        return report;
                    }
                };
                let manifest = &load.compiled.manifest;
                if !manifest.name.trim().is_empty() {
                    report.name = Clean::label(&manifest.name).into_string();
                }
                report.grants = manifest
                    .wants
                    .iter()
                    .map(|grant| grant.name().into_owned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                // With no project asked about, a path that is no project
                // is judged: enabled nowhere, so consented is all it says.
                let judged_in = project.unwrap_or_else(|| Path::new(""));
                report.condition = match self.judged(&load, judged_in) {
                    Ok(ExtensionState::FixturesOnly) => ExtensionCondition::FixturesOnly,
                    Ok(ExtensionState::NeedsConsent { .. }) => ExtensionCondition::NeedsConsent,
                    Ok(ExtensionState::Held { new_grants }) => {
                        report.new_grants = new_grants
                            .iter()
                            .map(|grant| grant.name().into_owned())
                            .collect();
                        ExtensionCondition::Held
                    }
                    Ok(ExtensionState::NeedsEnabling | ExtensionState::Enabled)
                        if project.is_none() =>
                    {
                        ExtensionCondition::Consented
                    }
                    Ok(ExtensionState::NeedsEnabling) => ExtensionCondition::NeedsEnabling,
                    Ok(ExtensionState::Enabled) => ExtensionCondition::Enabled,
                    Err(error) => {
                        report.errors = vec![Clean::text(&error.to_string()).into_string()];
                        ExtensionCondition::Broken
                    }
                };
                let recorded = consent::consent(&self.0.home, &key);
                if manifest.programs().next().is_some() {
                    let consented = recorded
                        .as_ref()
                        .is_some_and(|consent| consent.exec_paths.is_some())
                        && report.condition != ExtensionCondition::NeedsConsent;
                    report.programs = if consented {
                        self.pinned_programs(&identity).unwrap_or_default()
                    } else {
                        super::consent::resolve_programs(manifest, &self.0.home)
                    };
                    let confines = super::confinement().is_available();
                    report.confined = Some(match &recorded {
                        Some(consent) if consented => {
                            consent.enforced_at_accept == Some(true) && confines
                        }
                        _ => confines,
                    });
                }
                if let Some(recorded) = recorded {
                    report.enabled_in = recorded.enabled_in.into_iter().collect();
                    report.updated_at = recorded.updated_at;
                }
                if matches!(
                    report.condition,
                    ExtensionCondition::Enabled | ExtensionCondition::NeedsEnabling
                ) {
                    report.checkout_refused = refused.clone();
                }
                report
            })
            .collect()
    }

    /// What the workspace offers to enable where the project rooted at
    /// `project` opens: each extension of a plugin the project declares
    /// that runs code and is not enabled there, unless the operator put
    /// the offer away there before.
    #[tracing::instrument(name = "extensions.offers", skip_all)]
    pub fn offers(&self, project: &Path) -> Vec<ExtensionReport> {
        self.offers_over(project, |path| self.checkout(path))
    }

    /// [`Self::offers`], with the checkout resolved by `checkout`.
    pub(crate) fn offers_over(
        &self,
        project: &Path,
        checkout: impl FnOnce(&Path) -> Result<ExtensionCheckout, NotACheckout>,
    ) -> Vec<ExtensionReport> {
        let Ok(Some(manifest)) = uze_core::manifest::load(project) else {
            return Vec::new();
        };
        let declared: BTreeSet<(String, String)> = manifest
            .declared_plugins()
            .map(|(plugin, marketplace)| (plugin.to_owned(), marketplace.to_owned()))
            .collect();
        self.reports_over(Some(project), checkout)
            .into_iter()
            .filter(|report| {
                declared.contains(&(report.plugin.clone(), report.marketplace.clone()))
                    && report.condition.awaits_the_operator()
                    && !report.offer_declined
                    && report.checkout_refused.is_none()
            })
            .collect()
    }

    /// Records that the operator put away the offer to enable `identity`
    /// in the project rooted at `project`: the workspace does not make it
    /// there again, and the Extensions screen still does.
    #[tracing::instrument(name = "extensions.decline_offer", skip_all, fields(extension = %identity), err)]
    pub fn decline_offer(&self, identity: &Identity, project: &Path) -> uze_core::Result<bool> {
        consent::decline_offer(&self.0.home, &consent_key(identity), project)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::Asker;
    use crate::application::extensions::checkout::extension_checkout;
    use crate::application::extensions::runner::tests::World;

    fn report_of(world: &World, project: Option<&Path>) -> ExtensionReport {
        world
            .app
            .extensions()
            .reports_over(project, |path| extension_checkout(path, &world.around))
            .into_iter()
            .find(|report| report.id == "gsd-band")
            .unwrap()
    }

    fn offered(world: &World) -> Vec<String> {
        world
            .app
            .extensions()
            .offers_over(&world.checkout, |path| {
                extension_checkout(path, &world.around)
            })
            .iter()
            .map(ExtensionReport::attribution)
            .collect()
    }

    fn declare_the_plugin(world: &World) {
        fs::write(
            world.checkout.join("agents.yaml"),
            "marketplaces:\n  studio:\n    path: ../market\n    plugins: [gsd]\n",
        )
        .unwrap();
    }

    #[test]
    fn an_update_is_dated_by_its_utc_day() {
        assert_eq!(utc_day(0), "1970-01-01");
        assert_eq!(utc_day(1_830_297_600), "2028-01-01");
        assert_eq!(utc_day(1_835_395_200), "2028-02-29");
        assert_eq!(utc_day(1_792_972_800), "2026-10-26");
    }

    #[test]
    fn a_report_follows_the_extension_from_consent_to_enabled() {
        let world = World::new("report-states");
        let here = report_of(&world, Some(&world.checkout));
        assert_eq!(here.condition, ExtensionCondition::NeedsConsent);
        assert_eq!(here.grants, ["read:checkout"]);
        assert_eq!(here.attribution(), "gsd-band · gsd@studio");
        assert_eq!(here.name, "GSD");

        world
            .app
            .extensions()
            .consent(&world.band(), Asker::Operator)
            .unwrap();
        assert_eq!(
            report_of(&world, Some(&world.checkout)).condition,
            ExtensionCondition::NeedsEnabling
        );
        assert_eq!(
            report_of(&world, None).condition,
            ExtensionCondition::Consented,
            "the machine has no `here`"
        );

        world
            .app
            .extensions()
            .enable(&world.band(), &world.checkout, Asker::Operator)
            .unwrap();
        let enabled = report_of(&world, Some(&world.checkout));
        assert_eq!(enabled.condition, ExtensionCondition::Enabled);
        assert_eq!(enabled.enabled_in.len(), 1);
        assert_eq!(enabled.checkout_refused, None);
        let fixtures_only = world
            .app
            .extensions()
            .reports(None)
            .into_iter()
            .find(|report| report.id == "gsd-panel")
            .unwrap();
        assert_eq!(fixtures_only.condition, ExtensionCondition::FixturesOnly);
    }

    #[test]
    fn a_widened_update_is_held_naming_the_new_grant_and_a_same_set_one_says_when() {
        let world = World::new("report-held");
        world.enabled();
        let manifest = world.band.join("extension.kdl");
        let text = fs::read_to_string(&manifest).unwrap();

        fs::write(
            world.band.join("state.rhai"),
            "fn answer(i, c) { #{ data: #{} } }",
        )
        .unwrap();
        let updated = report_of(&world, Some(&world.checkout));
        assert_eq!(updated.condition, ExtensionCondition::Enabled);
        assert!(updated.updated_at.is_some(), "{updated:?}");

        fs::write(
            &manifest,
            text.replace(
                "wants \"read:checkout\"",
                "wants \"read:checkout\" \"clipboard\"",
            ),
        )
        .unwrap();
        let held = report_of(&world, Some(&world.checkout));
        assert_eq!(held.condition, ExtensionCondition::Held);
        assert_eq!(held.new_grants, ["clipboard"]);
    }

    #[test]
    fn a_home_directory_is_no_checkout_and_the_report_says_so() {
        let world = World::new("report-home");
        world.enabled();
        let mut around = world.around.clone();
        around.home = Some(world.checkout.clone());
        let report = world
            .app
            .extensions()
            .reports_over(Some(&world.checkout), |path| {
                extension_checkout(path, &around)
            })
            .into_iter()
            .find(|report| report.id == "gsd-band")
            .unwrap();
        assert_eq!(
            report.checkout_refused.as_deref(),
            Some("your home directory is not a checkout")
        );
    }

    #[test]
    fn the_offer_is_made_where_the_plugin_is_declared_until_it_is_put_away() {
        let world = World::new("report-offers");
        assert!(offered(&world).is_empty(), "the project declares nothing");

        declare_the_plugin(&world);
        assert_eq!(offered(&world), ["gsd-band · gsd@studio"]);

        assert!(
            world
                .app
                .extensions()
                .decline_offer(&world.band(), &world.checkout)
                .unwrap()
        );
        assert!(
            offered(&world).is_empty(),
            "not again after it was put away"
        );
        assert!(report_of(&world, Some(&world.checkout)).offer_declined);
    }

    #[test]
    fn an_enabled_extension_is_not_offered() {
        let world = World::new("report-offer-enabled");
        declare_the_plugin(&world);
        world.enabled();
        assert!(offered(&world).is_empty());
    }
}
