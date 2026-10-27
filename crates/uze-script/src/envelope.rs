//! The envelope a script answers with (`extension-platform` D4, change
//! 4's members): `data`, and optionally `refresh`, `toast` and `notice`.
//! Anything else fails the answer: a member a later change brings must not
//! be silently dropped by a build that does not know it yet.
//!
//! The strings are returned as the script wrote them. Making them drawable
//! is `uze_contract::Clean`'s, at the boundary where the host takes them.

use rhai::{Dynamic, Map};

use crate::error::ScriptError;
use crate::value;

pub use uze_contract::Envelope;

/// The envelope `answer` returned, refusing anything that is not a map of
/// its members.
pub(crate) fn from_answer(answer: &Dynamic) -> Result<Envelope, ScriptError> {
    let refuse = |why: String| Err(ScriptError::Envelope(why));
    let Some(map) = answer.read_lock::<Map>() else {
        return refuse(format!(
            "`answer` returned a `{}`, not a map with `data`",
            answer.type_name()
        ));
    };
    if let Some(unknown) = map
        .keys()
        .find(|key| !Envelope::MEMBERS.contains(&key.as_str()))
    {
        return refuse(format!(
            "`{unknown}` is not an envelope member (expected {})",
            Envelope::MEMBERS.join(", ")
        ));
    }
    let Some(data) = map.get("data") else {
        return refuse("it has no `data`".to_owned());
    };
    let data = value::to_json(data)
        .map_err(|crossing| ScriptError::Envelope(format!("`data` is {crossing}")))?;
    Ok(Envelope {
        data,
        refresh: strings(map.get("refresh"), "refresh")?,
        toast: text(map.get("toast"), "toast")?,
        notice: text(map.get("notice"), "notice")?,
        store: store(map.get("store"))?,
        notify: notify(map.get("notify"))?,
        sound: sound(map.get("sound"))?,
        timer: None,
    })
}

/// `notify`: `#{ title, body }`, both text.
fn notify(
    member: Option<&Dynamic>,
) -> Result<Option<Box<uze_contract::Notification>>, ScriptError> {
    let Some(member) = member else {
        return Ok(None);
    };
    let wrong = || ScriptError::Envelope("`notify` is not `#{ title, body }`".to_owned());
    let map = member.read_lock::<Map>().ok_or_else(wrong)?;
    let field = |name: &str| -> Result<String, ScriptError> {
        map.get(name)
            .and_then(|value| value.clone().into_string().ok())
            .ok_or_else(wrong)
    };
    Ok(Some(Box::new(uze_contract::Notification {
        title: field("title")?,
        body: field("body")?,
    })))
}

/// `sound`: the name of one of [`uze_contract::Sound::NAMES`].
fn sound(member: Option<&Dynamic>) -> Result<Option<uze_contract::Sound>, ScriptError> {
    match text(member, "sound")? {
        None => Ok(None),
        Some(cue) => uze_contract::Sound::parse(&cue).map(Some).ok_or_else(|| {
            ScriptError::Envelope(format!(
                "`sound` is `{cue}`, not one of {}",
                uze_contract::Sound::NAMES.join(", ")
            ))
        }),
    }
}

/// `store`: a map whose keys replace the store's, `()` removing one.
fn store(
    member: Option<&Dynamic>,
) -> Result<Option<Box<serde_json::Map<String, serde_json::Value>>>, ScriptError> {
    let Some(member) = member else {
        return Ok(None);
    };
    let wrong = || ScriptError::Envelope("`store` is not a map".to_owned());
    if member.read_lock::<Map>().is_none() {
        return Err(wrong());
    }
    match value::to_json(member)
        .map_err(|crossing| ScriptError::Envelope(format!("`store` is {crossing}")))?
    {
        serde_json::Value::Object(patch) => Ok(Some(Box::new(patch))),
        _ => Err(wrong()),
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
