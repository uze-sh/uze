//! The envelope a script answers with (`add-extension-platform` D4, change
//! 4's members): `data`, and optionally `refresh`, `toast` and `notice`.
//! Anything else fails the answer: a member a later change brings must not
//! be silently dropped by a build that does not know it yet.
//!
//! The strings are returned as the script wrote them. Making them drawable
//! is `uze_contract::Clean`'s, at the boundary where the host takes them.

use rhai::{Dynamic, Map};
use serde_json::Value;

use crate::error::ScriptError;
use crate::value;

/// What a script answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    /// The answer the source's view binds.
    pub data: Value,
    /// Source ids to ask again.
    pub refresh: Vec<String>,
    pub toast: Option<String>,
    pub notice: Option<String>,
}

const MEMBERS: &[&str] = &["data", "refresh", "toast", "notice"];

impl Envelope {
    pub(crate) fn from_answer(answer: &Dynamic) -> Result<Self, ScriptError> {
        let refuse = |why: String| Err(ScriptError::Envelope(why));
        let Some(map) = answer.read_lock::<Map>() else {
            return refuse(format!(
                "`answer` returned a `{}`, not a map with `data`",
                answer.type_name()
            ));
        };
        if let Some(unknown) = map.keys().find(|key| !MEMBERS.contains(&key.as_str())) {
            return refuse(format!(
                "`{unknown}` is not an envelope member (expected {})",
                MEMBERS.join(", ")
            ));
        }
        let Some(data) = map.get("data") else {
            return refuse("it has no `data`".to_owned());
        };
        let data = value::to_json(data)
            .map_err(|crossing| ScriptError::Envelope(format!("`data` is {crossing}")))?;
        Ok(Self {
            data,
            refresh: strings(map.get("refresh"), "refresh")?,
            toast: text(map.get("toast"), "toast")?,
            notice: text(map.get("notice"), "notice")?,
        })
    }
}

fn text(member: Option<&Dynamic>, name: &str) -> Result<Option<String>, ScriptError> {
    match member {
        None => Ok(None),
        Some(value) if value.is_unit() => Ok(None),
        Some(value) => {
            value.clone().into_string().map(Some).map_err(|kind| {
                ScriptError::Envelope(format!("`{name}` is a `{kind}`, not a string"))
            })
        }
    }
}

fn strings(member: Option<&Dynamic>, name: &str) -> Result<Vec<String>, ScriptError> {
    let Some(member) = member else {
        return Ok(Vec::new());
    };
    let wrong = || ScriptError::Envelope(format!("`{name}` is not a list of strings"));
    let list = member.read_lock::<rhai::Array>().ok_or_else(wrong)?;
    list.iter()
        .map(|item| item.clone().into_string().map_err(|_| wrong()))
        .collect()
}
