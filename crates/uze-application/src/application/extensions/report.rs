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
    /// What a held update wants beyond what was accepted: its grants, then
    /// the rest of its reach (`exec:sh reads /etc/gsd`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_grants: Vec<String>,
    /// The projects it is enabled in, by canonical root.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enabled_in: Vec<PathBuf>,
    /// Whether it is enabled in every project, `enabled_in` or not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub enabled_everywhere: bool,
    /// Whether it wants a grant that reaches agents, which keeps it to the
    /// projects named: it is never enabled in every project.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reaches_agents: bool,
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
    /// Each program its `exec:` grants name, pinned where it was consented
    /// to and as it would be pinned where it was not, then each program
    /// packaged with it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<ProgramPin>,
    /// Whether those programs run confined, when it has any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confined: Option<bool>,
    /// Each preference it declares, at its value now.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<PreferenceRow>,
    /// The rows its bands take over each agent's pane, together, when it
    /// declares any: what enabling it costs every agent of the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band_rows: Option<u8>,
    /// The digest of the files this report was read from, when they
    /// compile: what a surface showing it hands to
    /// [`Extensions::consent`], so consent covers what was shown.
    #[serde(skip)]
    pub digest: Option<String>,
}

/// One preference as the Extensions screen lists and edits it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreferenceRow {
    pub name: String,
    pub label: String,
    /// `text`, `number`, `toggle`, `choice` or `secret`, whose value is
    /// only whether it is set.
    pub kind: &'static str,
    pub value: serde_json::Value,
    /// A choice's values, in order; empty for any other kind.
    pub values: Vec<String>,
    /// What it holds until the operator says otherwise; nothing for a
    /// secret, which has no default to show.
    pub default: serde_json::Value,
    /// A number's bounds, when it declares them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
}

impl PreferenceRow {
    /// The value as the screen says it.
    pub fn said(&self) -> String {
        if self.is_secret() {
            return if self.value == serde_json::Value::Bool(true) {
                "set".to_owned()
            } else {
                "not set".to_owned()
            };
        }
        match &self.value {
            serde_json::Value::Bool(true) => "on".to_owned(),
            serde_json::Value::Bool(false) => "off".to_owned(),
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        }
    }

    /// What the next press makes it, when one press is the whole answer:
    /// a toggle flipped, a choice moved on to its next value. `None` for a
    /// text or a number, which is typed.
    pub fn pressed(&self) -> Option<serde_json::Value> {
        match self.kind {
            "toggle" => Some(serde_json::Value::Bool(
                !self.value.as_bool().unwrap_or(false),
            )),
            "choice" => {
                let at = self
                    .values
                    .iter()
                    .position(|value| Some(value.as_str()) == self.value.as_str())
                    .map_or(0, |at| (at + 1) % self.values.len().max(1));
                self.values.get(at).cloned().map(serde_json::Value::String)
            }
            _ => None,
        }
    }

    /// Whether it is a secret: typed masked, never shown again.
    pub fn is_secret(&self) -> bool {
        self.kind == "secret"
    }

    /// What one step makes it, `forward` or back: a number one up or down,
    /// held within its bounds, a choice its next or previous value, a
    /// toggle flipped. `None` when the step changes nothing — a number at
    /// its bound — or the kind is typed rather than stepped.
    pub fn stepped(&self, forward: bool) -> Option<serde_json::Value> {
        match self.kind {
            "toggle" => self.pressed(),
            "choice" => {
                let count = self.values.len();
                if count == 0 {
                    return None;
                }
                let at = self
                    .values
                    .iter()
                    .position(|value| Some(value.as_str()) == self.value.as_str());
                let next = match (at, forward) {
                    (None, _) => 0,
                    (Some(at), true) => (at + 1) % count,
                    (Some(at), false) => (at + count - 1) % count,
                };
                (Some(next) != at).then(|| serde_json::Value::String(self.values[next].clone()))
            }
            "number" => {
                let now = self
                    .value
                    .as_f64()
                    .or_else(|| self.default.as_f64())
                    .unwrap_or(0.0);
                let next = self.clamped(if forward { now + 1.0 } else { now - 1.0 });
                (next != now).then(|| as_number(next))
            }
            _ => None,
        }
    }

    /// `number` held within this preference's bounds.
    fn clamped(&self, number: f64) -> f64 {
        let number = self.min.map_or(number, |min| number.max(min as f64));
        self.max.map_or(number, |max| number.min(max as f64))
    }

    /// `typed` as this preference's value: a number read as one, held
    /// within its bounds.
    pub fn typed(&self, typed: &str) -> Option<serde_json::Value> {
        match self.kind {
            "number" => {
                let typed = typed.trim();
                typed
                    .parse::<i64>()
                    .ok()
                    .map(|whole| whole as f64)
                    .or_else(|| typed.parse::<f64>().ok())
                    .filter(|number| number.is_finite())
                    .map(|number| as_number(self.clamped(number)))
            }
            "text" | "secret" => Some(serde_json::Value::String(typed.to_owned())),
            _ => None,
        }
    }
}

/// `value` as JSON, a whole number when it has no fraction.
fn as_number(value: f64) -> serde_json::Value {
    if value.fract() == 0.0 && value.abs() < i64::MAX as f64 {
        serde_json::Value::from(value as i64)
    } else {
        serde_json::Value::from(value)
    }
}

/// The rows `manifest`'s bands take over each agent's pane, together;
/// `None` when it declares none.
fn band_rows(manifest: &uze_contract::Manifest) -> Option<u8> {
    let rows = manifest
        .surfaces
        .iter()
        .filter_map(|surface| surface.band)
        .fold(0u8, |rows, band| rows.saturating_add(band.rows));
    (rows > 0).then_some(rows)
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

/// Where this machine keeps an extension's secrets, as the operator reads
/// it where they type one.
pub fn secrets_kept_in() -> &'static str {
    uze_workspace::extensions::secrets::kept_in()
}

/// What a grant lets an extension do, as the operator reads it before
/// accepting it.
pub fn grant_meaning(grant: &str) -> &'static str {
    match uze_contract::Grant::parse(grant) {
        Some(uze_contract::Grant::ReadCheckout) => "reads the checkout's files",
        Some(uze_contract::Grant::Clipboard) => "copies what you choose",
        Some(uze_contract::Grant::OpenPath) => "opens a checkout's file in its program",
        Some(uze_contract::Grant::Store) => "keeps state for this project",
        Some(uze_contract::Grant::StoreGlobal) => "keeps state shared by every project",
        Some(uze_contract::Grant::Exec(_)) => "runs this program on your machine",
        Some(uze_contract::Grant::Net(_)) => "HTTPS to this host, and no other",
        Some(uze_contract::Grant::Resident) => "stays running while open, reads your keys",
        Some(uze_contract::Grant::AgentObserve) => "sees the agents and what they do",
        Some(uze_contract::Grant::AgentPrompt) => "prompts an agent when you press it, shown first",
        Some(uze_contract::Grant::AgentPromptAuto) => "prompts an agent alone, a few times an hour",
        Some(uze_contract::Grant::AgentRestart) => "restarts an agent, after asking you",
        Some(uze_contract::Grant::Notify) => "notifies you, a few times a minute",
        Some(uze_contract::Grant::Mcp { .. }) => "calls this tool of a plugin's MCP server",
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
                    enabled_everywhere: false,
                    reaches_agents: false,
                    updated_at: None,
                    checkout_refused: None,
                    offer_declined: project.is_some_and(|project| {
                        consent::offer_declined(&self.0.home, &key, project)
                    }),
                    errors: Vec::new(),
                    programs: Vec::new(),
                    confined: None,
                    preferences: Vec::new(),
                    band_rows: None,
                    digest: None,
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
                report.band_rows = band_rows(manifest);
                report.digest = Some(load.digest.clone());
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
                    Ok(held @ ExtensionState::Held { .. }) => {
                        report.new_grants = held.wanted_anew();
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
                let set = super::consent::admitted_preferences(&self.0.home, &identity, manifest);
                report.preferences = manifest
                    .preferences
                    .iter()
                    .map(|decl| PreferenceRow {
                        name: decl.name.clone(),
                        label: decl.label.clone(),
                        kind: match decl.kind {
                            uze_contract::PreferenceKind::Text => "text",
                            uze_contract::PreferenceKind::Number { .. } => "number",
                            uze_contract::PreferenceKind::Toggle => "toggle",
                            uze_contract::PreferenceKind::Choice { .. } => "choice",
                            uze_contract::PreferenceKind::Secret => "secret",
                        },
                        // A secret is said set or not, from the record that
                        // says so; its value is never read for a report.
                        value: if decl.is_secret() {
                            serde_json::Value::Bool(
                                uze_workspace::extensions::preferences::read(
                                    &self.0.home,
                                    &super::consent::plugin_key(&identity),
                                    &identity.id,
                                )
                                .get(&decl.name)
                                    == Some(&serde_json::Value::Bool(true)),
                            )
                        } else {
                            set.get(&decl.name).cloned().unwrap_or_default()
                        },
                        values: match &decl.kind {
                            uze_contract::PreferenceKind::Choice { values } => values.clone(),
                            _ => Vec::new(),
                        },
                        default: if decl.is_secret() {
                            serde_json::Value::Null
                        } else {
                            decl.default.clone()
                        },
                        min: match decl.kind {
                            uze_contract::PreferenceKind::Number { min, .. } => min,
                            _ => None,
                        },
                        max: match decl.kind {
                            uze_contract::PreferenceKind::Number { max, .. } => max,
                            _ => None,
                        },
                    })
                    .collect();
                report.reaches_agents = super::consent::reaches_agents(manifest);
                let recorded = consent::consent(&self.0.home, &key);
                if super::consent::runs_programs(manifest) {
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
                    report.enabled_everywhere = recorded.enabled_everywhere;
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
            .consent(
                &world.band(),
                &world.app.extensions().digest(&world.band()).unwrap(),
                Asker::Operator,
            )
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
            world.band.join("state.uze"),
            "<script>\nfn answer(i, c) { #{ data: #{} } }\n</script>\n",
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

    #[test]
    fn the_rows_a_manifests_bands_take_are_summed_and_none_without_a_band() {
        let surface = |slot, band| uze_contract::SurfaceDecl {
            id: "s".to_owned(),
            slot,
            view: "s.uze".to_owned(),
            source: "s".to_owned(),
            title: None,
            band,
            bar: None,
            drawer: None,
            applies_when: None,
        };
        let band = |rows| {
            Some(uze_contract::BandDecl {
                rows,
                place: uze_contract::BandPlace::Top,
            })
        };
        let mut manifest = uze_contract::Manifest {
            protocol: 1,
            id: "gsd-band".to_owned(),
            name: "GSD".to_owned(),
            description: String::new(),
            surfaces: vec![surface(uze_contract::Slot::View, None)],
            wants: Vec::new(),
            sources: Vec::new(),
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
        };
        assert_eq!(band_rows(&manifest), None);
        manifest
            .surfaces
            .push(surface(uze_contract::Slot::Band, band(2)));
        manifest
            .surfaces
            .push(surface(uze_contract::Slot::Band, band(4)));
        assert_eq!(band_rows(&manifest), Some(6));
    }

    fn minutes(value: i64) -> PreferenceRow {
        PreferenceRow {
            name: "minutes".to_owned(),
            label: "Minutes in a session".to_owned(),
            kind: "number",
            value: serde_json::json!(value),
            values: Vec::new(),
            default: serde_json::json!(25),
            min: Some(1),
            max: Some(90),
        }
    }

    #[test]
    fn a_number_steps_by_one_and_stops_at_its_bounds() {
        assert_eq!(minutes(25).stepped(true), Some(serde_json::json!(26)));
        assert_eq!(minutes(25).stepped(false), Some(serde_json::json!(24)));
        assert_eq!(minutes(90).stepped(true), None, "at its max");
        assert_eq!(minutes(1).stepped(false), None, "at its min");
    }

    #[test]
    fn a_typed_number_is_held_within_its_bounds() {
        assert_eq!(minutes(25).typed("120"), Some(serde_json::json!(90)));
        assert_eq!(minutes(25).typed("0"), Some(serde_json::json!(1)));
        assert_eq!(minutes(25).typed("2.5"), Some(serde_json::json!(2.5)));
        assert_eq!(minutes(25).typed("many"), None);
    }

    #[test]
    fn a_choice_steps_both_ways_round_its_values() {
        let row = PreferenceRow {
            name: "theme".to_owned(),
            label: "Theme".to_owned(),
            kind: "choice",
            value: serde_json::json!("light"),
            values: vec!["light".to_owned(), "dark".to_owned(), "auto".to_owned()],
            default: serde_json::json!("dark"),
            min: None,
            max: None,
        };
        assert_eq!(row.stepped(true), Some(serde_json::json!("dark")));
        assert_eq!(row.stepped(false), Some(serde_json::json!("auto")));
    }
}
