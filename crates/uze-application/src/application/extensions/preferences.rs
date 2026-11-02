//! Every extension's preferences, built-in or declared, read and set the
//! one way: the values from `config.toml`, each admitted against its
//! declaration, and the drawer's rows made of both.
//!
//! A declared extension's declarations come from its manifest; a
//! built-in's are handed in by the client, the one side that knows the
//! built-in set. Either is keyed by the id its switch uses.

use std::collections::BTreeSet;

use uze_contract::{PreferenceDecl, PreferenceKind};
use uze_core::{Result, UzeHome};

use super::{Extensions, PreferenceRow};

/// Each preference `declared` that is not a secret, at the operator's
/// value where it may hold it and at its default otherwise. A
/// `config.toml` that does not parse reads as nothing set: what is drawn
/// keeps working, and every write says why the file is refused.
pub(super) fn admitted(home: &UzeHome, id: &str, declared: &[PreferenceDecl]) -> serde_json::Value {
    let set = uze_workspace::extensions::preferences::read(home, id).unwrap_or_default();
    serde_json::Value::Object(
        declared
            .iter()
            .filter(|decl| !decl.is_secret())
            .map(|decl| (decl.name.clone(), decl.admit(set.get(&decl.name))))
            .collect(),
    )
}

/// The drawer's row for each preference `declared`: a secret said set or
/// not from `secrets_set`, never read for its value.
pub(super) fn rows(
    home: &UzeHome,
    id: &str,
    declared: &[PreferenceDecl],
    secrets_set: &BTreeSet<String>,
) -> Vec<PreferenceRow> {
    let admitted = admitted(home, id, declared);
    declared
        .iter()
        .map(|decl| PreferenceRow {
            name: decl.name.clone(),
            label: decl.label.clone(),
            kind: match decl.kind {
                PreferenceKind::Text => "text",
                PreferenceKind::Number { .. } => "number",
                PreferenceKind::Toggle => "toggle",
                PreferenceKind::Choice { .. } => "choice",
                PreferenceKind::Secret => "secret",
            },
            value: if decl.is_secret() {
                secrets_set.contains(&decl.name).into()
            } else {
                admitted.get(&decl.name).cloned().unwrap_or_default()
            },
            values: match &decl.kind {
                PreferenceKind::Choice { values } => values.clone(),
                _ => Vec::new(),
            },
            default: if decl.is_secret() {
                serde_json::Value::Null
            } else {
                decl.default.clone()
            },
            min: match decl.kind {
                PreferenceKind::Number { min, .. } => min,
                _ => None,
            },
            max: match decl.kind {
                PreferenceKind::Number { max, .. } => max,
                _ => None,
            },
        })
        .collect()
}

/// Sets the preference `name` of the extension `id`, refused when it
/// declares no such preference, when it is a secret (kept elsewhere, by
/// whoever may keep one) or when `value` is not one it may hold.
pub(super) fn set(
    home: &UzeHome,
    id: &str,
    declared: &[PreferenceDecl],
    name: &str,
    value: &serde_json::Value,
) -> Result<serde_json::Value> {
    let Some(decl) = declared.iter().find(|decl| decl.name == name) else {
        return Err(super::consent::refusal(format!(
            "{id} declares no preference `{name}`"
        )));
    };
    if decl.is_secret() {
        return Err(super::consent::refusal(format!(
            "`{name}` is a secret, kept apart from the other preferences"
        )));
    }
    if !decl.holds(value) {
        return Err(super::consent::refusal(format!(
            "`{value}` is not a value the preference `{name}` may hold"
        )));
    }
    uze_workspace::extensions::preferences::set(home, id, name, value)?;
    Ok(admitted(home, id, declared))
}

impl Extensions<'_> {
    /// The built-in extension `id`'s preferences as it reads them, from
    /// the declarations the client hands in.
    pub fn builtin_preferences(&self, id: &str, declared: &[PreferenceDecl]) -> serde_json::Value {
        admitted(&self.0.home, id, declared)
    }

    /// The built-in extension `id`'s preferences as its drawer lists them.
    /// A built-in keeps no secret.
    pub fn builtin_preference_rows(
        &self,
        id: &str,
        declared: &[PreferenceDecl],
    ) -> Vec<PreferenceRow> {
        rows(&self.0.home, id, declared, &BTreeSet::new())
    }

    /// Sets the built-in extension `id`'s preference `name` to `value`,
    /// refused as a declared extension's would be.
    #[tracing::instrument(name = "extensions.set_builtin_preference", skip_all, fields(extension = id, preference = name), err)]
    pub fn set_builtin_preference(
        &self,
        id: &str,
        declared: &[PreferenceDecl],
        name: &str,
        value: serde_json::Value,
    ) -> Result<serde_json::Value> {
        set(&self.0.home, id, declared, name, &value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Vec<PreferenceDecl> {
        vec![
            PreferenceDecl {
                name: "auto".to_owned(),
                label: "Auto".to_owned(),
                kind: PreferenceKind::Toggle,
                default: true.into(),
            },
            PreferenceDecl {
                name: "token".to_owned(),
                label: "Token".to_owned(),
                kind: PreferenceKind::Secret,
                default: serde_json::Value::Null,
            },
        ]
    }

    #[test]
    fn a_value_the_declaration_does_not_admit_reads_as_its_default() {
        let home = UzeHome::at(uze_testkit::temp::scratch("preferences-admitted"));
        std::fs::create_dir_all(home.root()).unwrap();
        std::fs::write(home.config_path(), "[preferences.review]\nauto = \"yes\"\n").unwrap();
        assert_eq!(
            admitted(&home, "review", &declared()),
            serde_json::json!({ "auto": true })
        );
        set(&home, "review", &declared(), "auto", &false.into()).unwrap();
        assert_eq!(
            admitted(&home, "review", &declared()),
            serde_json::json!({ "auto": false })
        );
    }

    #[test]
    fn a_secret_or_a_value_out_of_bounds_is_never_written() {
        let home = UzeHome::at(uze_testkit::temp::scratch("preferences-refused"));
        assert!(set(&home, "review", &declared(), "token", &"ghp".into()).is_err());
        assert!(set(&home, "review", &declared(), "auto", &"yes".into()).is_err());
        assert!(set(&home, "review", &declared(), "missing", &true.into()).is_err());
        assert!(!home.config_path().exists());
    }
}
