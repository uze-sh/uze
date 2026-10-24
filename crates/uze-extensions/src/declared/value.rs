//! How a binding becomes a value: the scopes a reference reads, and the
//! one reading of a JSON value as each kind of property.
//!
//! Kept apart from the builder so the rules of D23 sit in one place: a
//! lone reference keeps its type, mixed text interpolates scalars only, and
//! a value of the wrong kind falls back to the property's default rather
//! than being coerced into something the view never said.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde_json::Value;
use uze_contract::compiled::{Lookup, look_up};
use uze_contract::{Binding, Path, RoleName, Scope, TextPart};

use crate::view::Role;

/// Everything a binding may read beside the items in scope.
pub(super) struct Env<'a> {
    /// The surface's UI state, with each `Tabs`' default filled in.
    pub ui: &'a BTreeMap<String, String>,
    pub context: &'a Value,
    pub lookups: &'a [Lookup],
    pub locale: &'a str,
}

/// The items in scope, outermost first: the whole data, then one per
/// enclosing `each`.
pub(super) type Scopes<'a> = Vec<&'a Value>;

impl<'a> Env<'a> {
    /// The value `path` names, or `None` when nothing is there.
    pub fn resolve(&self, scopes: &[&'a Value], path: &Path) -> Option<Cow<'a, Value>> {
        match path.scope {
            Scope::Item => walk(scopes.last().copied()?, &path.segments).map(Cow::Borrowed),
            Scope::Parent => {
                let parent = scopes.get(scopes.len().checked_sub(2)?).copied()?;
                walk(parent, &path.segments).map(Cow::Borrowed)
            }
            Scope::Context => walk(self.context, &path.segments).map(Cow::Borrowed),
            Scope::Ui => self
                .ui
                .get(&path.segments.join("."))
                .map(|value| Cow::Owned(Value::String(value.clone()))),
        }
    }

    /// What `binding` is bound to. `None` only for a lone reference whose
    /// field is missing, which hides the node it sits on (D23); text with a
    /// missing field in it draws that field empty.
    pub fn bind(&self, scopes: &[&'a Value], binding: &'a Binding) -> Option<Cow<'a, Value>> {
        match binding {
            Binding::Literal(value) => Some(Cow::Borrowed(value)),
            Binding::Ref(path) => self.resolve(scopes, path),
            Binding::Lookup { table, path } => {
                Some(Cow::Owned(Value::String(self.look_up(table, scopes, path))))
            }
            Binding::Text(parts) => {
                Some(Cow::Owned(Value::String(self.interpolate(scopes, parts))))
            }
        }
    }

    /// The field a lone reference names, when it is missing.
    pub fn missing(&self, scopes: &[&'a Value], binding: &Binding) -> Option<String> {
        let Binding::Ref(path) = binding else {
            return None;
        };
        self.resolve(scopes, path)
            .is_none()
            .then(|| path_text(path))
    }

    fn interpolate(&self, scopes: &[&'a Value], parts: &[TextPart]) -> String {
        let mut text = String::new();
        for part in parts {
            match part {
                TextPart::Literal(literal) => text.push_str(literal),
                TextPart::Ref(path) => {
                    if let Some(value) = self.resolve(scopes, path) {
                        text.push_str(&scalar_text(&value));
                    }
                }
                TextPart::Lookup { table, path } => {
                    text.push_str(&self.look_up(table, scopes, path));
                }
            }
        }
        text
    }

    /// The entry `table` gives the value at `path`, in the operator's
    /// locale, or empty when the table has neither the entry nor a default.
    ///
    /// When `path` holds no scalar, its own name is the key: that is how a
    /// label table reads, `{t:next}` meaning the label `next` rather than
    /// whatever `next` holds (the blueprint's board has an object there).
    /// A lookup therefore never hides its node for a missing field.
    fn look_up(&self, table: &str, scopes: &[&'a Value], path: &Path) -> String {
        let key = match self.resolve(scopes, path).as_deref() {
            Some(value @ (Value::String(_) | Value::Number(_) | Value::Bool(_))) => {
                scalar_text(value)
            }
            _ => path_text(path),
        };
        look_up(self.lookups, table, &key, self.locale)
            .unwrap_or_default()
            .to_owned()
    }
}

fn walk<'a>(mut value: &'a Value, segments: &[String]) -> Option<&'a Value> {
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    Some(value)
}

/// A reference as the view wrote it, for the inspector.
pub(super) fn path_text(path: &Path) -> String {
    let fields = path.segments.join(".");
    match path.scope {
        Scope::Item if fields.is_empty() => ".".to_owned(),
        Scope::Item => fields,
        Scope::Parent => format!("^.{fields}"),
        Scope::Ui => format!("ui.{fields}"),
        Scope::Context => format!("context.{fields}"),
    }
}

/// A scalar as text. A list or an object has no one reading as text, so
/// it reads as nothing rather than as JSON nobody asked to see.
pub(super) fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
    }
}

pub(super) fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number: &f64| number.is_finite())
}

/// A count of cells or things: a number rounded and held to `u16`.
pub(super) fn as_count(value: &Value) -> Option<u16> {
    as_number(value).map(|number| number.round().clamp(0.0, f64::from(u16::MAX)) as u16)
}

/// A flag: `true` and `false` as themselves, anything else by presence,
/// so `grow=1` grows and an empty string does not.
pub(super) fn as_flag(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::String(text) if text == "false" => false,
        other => uze_contract::binding::is_present(Some(other)),
    }
}

/// A role by its name. One the catalog does not know draws as the
/// default, since the check already refused every literal that is not one.
pub(super) fn as_role(value: &Value) -> Role {
    match value.as_str().and_then(RoleName::parse) {
        Some(name) => role(name),
        None => Role::Default,
    }
}

fn role(name: RoleName) -> Role {
    match name {
        RoleName::Default => Role::Default,
        RoleName::Muted => Role::Muted,
        RoleName::Secondary => Role::Secondary,
        RoleName::Bright => Role::Bright,
        RoleName::Inactive => Role::Inactive,
        RoleName::Accent => Role::Accent,
        RoleName::Dim => Role::Dim,
        RoleName::Faint => Role::Faint,
        RoleName::Info => Role::Info,
        RoleName::Success => Role::Success,
        RoleName::Warning => Role::Warning,
        RoleName::Danger => Role::Danger,
    }
}
