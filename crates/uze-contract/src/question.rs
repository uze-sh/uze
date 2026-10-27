//! What a declared extension asks the host, and what the host answers
//! (`extension-platform` D3, D4).
//!
//! A question names a declaration of one extension and the input it is
//! asked with, never a program or a script's text: the host finds what
//! answers it in the compile the operator consented to. The answer is the
//! envelope the script returned, the reason nothing was answered, or why
//! nothing was even asked.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Identity;

/// Answer declaration `declaration` of the extension `identity`, with
/// `input`, about `checkout`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub identity: Identity,
    /// A source or an action id the manifest declares, as `asking` says.
    pub declaration: String,
    /// What kind of declaration it names, and whether a remembered answer
    /// may serve.
    #[serde(default)]
    pub asking: Asking,
    /// The declared input, as the engine resolved it.
    pub input: serde_json::Value,
    /// The checkout the surface asking is about. A script reads only
    /// beneath it; a fixture needs none.
    pub checkout: Option<PathBuf>,
}

/// What a [`Question`] asks of its declaration.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Asking {
    /// A source, which may be answered from what it answered this session.
    #[default]
    Source,
    /// A source asked again by a gesture or an answer: it runs, and its
    /// answer replaces the one remembered.
    Refresh,
    /// An action whose `does` is a script, run on a gesture and never
    /// remembered.
    Action,
    /// One call into a resident source's session (`extension-platform`
    /// D19): never remembered, and answered by the session the source's
    /// `Start` opened.
    Resident(ResidentCall),
}

/// The entry points of a resident script, as the engine calls them. The
/// question's input carries what each is called with: the context, the
/// event, the raw input or the size.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResidentCall {
    /// Open the session: `start(context)`.
    Start,
    /// `event(event)`: a timer's tick.
    Event,
    /// `input(input)`: a key or the pointer on its focused surface.
    Input,
    /// `render(size)`: the catalog tree to draw.
    Render,
    /// Close the session: `stop()`.
    Stop,
}

/// What a source answered: `data`, and optionally the sources to ask
/// again, a toast and a notice. The strings are the extension's own and
/// pass through [`crate::Clean`] where the host draws them.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// What the source's views bind.
    pub data: serde_json::Value,
    /// Source ids to ask again.
    #[serde(default)]
    pub refresh: Vec<String>,
    pub toast: Option<String>,
    pub notice: Option<String>,
    /// Keys of the extension's store to replace, `null` removing one.
    pub store: Option<Box<serde_json::Map<String, serde_json::Value>>>,
    /// A notification the operator is to get, under `notify`.
    pub notify: Option<Box<Notification>>,
    /// A cue to sound, under `notify`.
    pub sound: Option<Sound>,
    /// The milliseconds after which a resident script asked for its next
    /// tick, already held to the host's rate. Set by the engine from the
    /// script's `timer(ms)`, never a member an answer may hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub timer: Option<u64>,
}

/// What a notification says; the host puts the extension's attribution
/// before its title.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// The cues an answer may ask for. The host sounds each as the
/// terminal's bell.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sound {
    Done,
    Attention,
    Error,
}

impl Sound {
    pub const NAMES: &'static [&'static str] = &["done", "attention", "error"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "done" => Some(Self::Done),
            "attention" => Some(Self::Attention),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl Envelope {
    /// The members an answer may hold.
    pub const MEMBERS: &'static [&'static str] = &[
        "data", "refresh", "toast", "notice", "store", "notify", "sound",
    ];

    /// An envelope holding `data` alone, as a fixture answers.
    pub fn data(data: serde_json::Value) -> Self {
        Self {
            data,
            ..Self::default()
        }
    }
}

/// The host's answer to a [`Question`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Answer {
    /// The declaration answered. `trace` is what a script printed on the
    /// way, one line each.
    Answered {
        envelope: Envelope,
        trace: Vec<String>,
    },
    /// It ran and did not answer: it failed, or was stopped at a bound.
    /// `reason` is the sentence the operator reads.
    Failed { reason: String, trace: Vec<String> },
    /// Nothing ran, and `reason` says why: it needs consent, or enabling
    /// in this project, or the checkout is not one it may read.
    Withheld { reason: String },
}

impl Answer {
    pub fn withheld(reason: impl Into<String>) -> Self {
        Self::Withheld {
            reason: reason.into(),
        }
    }
}
