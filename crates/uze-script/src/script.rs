//! A script compiled once, and the answers it gives.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rhai::{AST, Dynamic, Scope};
use serde_json::Value;

use crate::engine::{self, LIMITS};
use crate::envelope::{self, Envelope};
use crate::error::{Limit, ScriptError};
use crate::reach::{Grants, Reach};
use crate::trace::Trace;
use crate::value;

/// The function every script defines: `fn answer(input, context)`.
const ENTRY: &str = "answer";

/// A script's text, compiled. Compiling checks syntax and bounds only;
/// which host functions it may call is decided per answer by the grants.
#[derive(Debug, Clone)]
pub struct Script {
    ast: AST,
}

/// One answer, and what the script printed on the way to it, kept whether
/// it answered or not: a failed run is when the trace matters most.
#[derive(Debug, Clone)]
pub struct Answered {
    pub outcome: Result<Envelope, ScriptError>,
    pub trace: Trace,
}

impl Script {
    /// Compile a script's bytes, refusing one that is not UTF-8, is larger
    /// than the engine takes, or defines no `answer(input, context)`.
    pub fn compile(bytes: &[u8]) -> Result<Self, ScriptError> {
        if bytes.len() > LIMITS.script_size {
            return Err(ScriptError::Limit {
                limit: Limit::ScriptSize,
                at: None,
            });
        }
        let source = std::str::from_utf8(bytes).map_err(|error| ScriptError::Compile {
            message: format!("not UTF-8: {error}"),
            at: None,
        })?;
        let ast = engine::compiling()
            .compile(source)
            .map_err(|error| ScriptError::from_parse(&error))?;
        let defines_entry = ast
            .iter_functions()
            .any(|function| function.name == ENTRY && function.params.len() == 2);
        if !defines_entry {
            return Err(ScriptError::Compile {
                message: format!("it defines no `fn {ENTRY}(input, context)`"),
                at: None,
            });
        }
        Ok(Self { ast })
    }

    /// Ask the script: `answer(input, context)`, in an engine holding the
    /// host functions `grants` register and answered by `reach`.
    ///
    /// `reach` is shared rather than borrowed because the engine keeps the
    /// functions it registers for as long as it lives, and Rhai holds them
    /// as `'static`.
    pub fn answer(
        &self,
        input: &Value,
        context: &Value,
        grants: &Grants,
        reach: Arc<dyn Reach>,
    ) -> Answered {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let outcome = self.run(input, context, grants, reach, Arc::clone(&trace));
        let trace = std::mem::take(
            &mut *trace
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        Answered { outcome, trace }
    }

    fn run(
        &self,
        input: &Value,
        context: &Value,
        grants: &Grants,
        reach: Arc<dyn Reach>,
        trace: Arc<Mutex<Trace>>,
    ) -> Result<Envelope, ScriptError> {
        let crossing = |what: &'static str| {
            move |crossing: value::Crossing| {
                ScriptError::Question(format!("the {what} is {crossing}"))
            }
        };
        let input = value::to_dynamic(input).map_err(crossing("input"))?;
        let context = value::to_dynamic(context).map_err(crossing("context"))?;
        let engine = engine::answering(grants, reach, trace);
        let answer: Dynamic = engine
            .call_fn(&mut Scope::new(), &self.ast, ENTRY, (input, context))
            .map_err(|error| ScriptError::from_eval(&error))?;
        envelope::from_answer(&answer)
    }
}

/// Scripts compiled on first use and kept by the digest of the bytes they
/// came from (D28: fifty enabled extensions do not cost fifty compiles at
/// startup). The digest is the caller's, the one consent recorded.
#[derive(Debug, Default)]
pub struct ScriptCache {
    compiled: HashMap<String, Arc<Script>>,
}

impl ScriptCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The script compiled from `bytes`, compiling it only the first time
    /// `digest` is asked for.
    pub fn get_or_compile(
        &mut self,
        digest: &str,
        bytes: &[u8],
    ) -> Result<Arc<Script>, ScriptError> {
        if let Some(found) = self.compiled.get(digest) {
            return Ok(Arc::clone(found));
        }
        let script = Arc::new(Script::compile(bytes)?);
        self.compiled.insert(digest.to_owned(), Arc::clone(&script));
        Ok(script)
    }

    /// Forget a digest, when the bytes it named are no longer consented to.
    pub fn forget(&mut self, digest: &str) {
        self.compiled.remove(digest);
    }
}
