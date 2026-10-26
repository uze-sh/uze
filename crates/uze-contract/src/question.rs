//! What a declared extension asks the host, and what the host answers
//! (`add-extension-platform` D3, D4).
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
    /// A source id the manifest declares.
    pub declaration: String,
    /// The declared input, as the engine resolved it.
    pub input: serde_json::Value,
    /// The checkout the surface asking is about. A script reads only
    /// beneath it; a fixture needs none.
    pub checkout: Option<PathBuf>,
}

/// What a source answered: `data`, and optionally the sources to ask
/// again, a toast and a notice. The strings are the extension's own and
/// pass through [`crate::Clean`] where the host draws them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// What the source's views bind.
    pub data: serde_json::Value,
    /// Source ids to ask again.
    pub refresh: Vec<String>,
    pub toast: Option<String>,
    pub notice: Option<String>,
}

impl Envelope {
    /// The members a script's answer may hold.
    pub const MEMBERS: &'static [&'static str] = &["data", "refresh", "toast", "notice"];

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
