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

/// The function a source's or a `script` action's script defines:
/// `fn answer(input, context)`.
const ENTRY: &str = uze_contract::SOURCE_ENTRY;

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
    /// Compile a script's bytes, refusing one that is not UTF-8 or is
    /// larger than the engine takes. Which functions it must define is its
    /// role's to say (`extension-components` D5): [`Script::answer`] asks
    /// for `answer`, [`Script::call`] for the function it names.
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
        Ok(Self { ast })
    }

    /// Whether it defines `function` taking `params` parameters.
    pub fn defines(&self, function: &str, params: usize) -> bool {
        self.ast
            .iter_functions()
            .any(|defined| defined.name == function && defined.params.len() == params)
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
        self.call(ENTRY, input, context, grants, reach)
    }

    /// Ask the script's `function(input, context)`, as [`Script::answer`]
    /// asks `answer`: what an action's `call` runs.
    pub fn call(
        &self,
        function: &str,
        input: &Value,
        context: &Value,
        grants: &Grants,
        reach: Arc<dyn Reach>,
    ) -> Answered {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let outcome = if self.defines(function, 2) {
            self.run(function, input, context, grants, reach, Arc::clone(&trace))
        } else {
            Err(ScriptError::Compile {
                message: format!("it defines no `fn {function}(input, context)`"),
                at: None,
            })
        };
        let trace = std::mem::take(
            &mut *trace
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        Answered { outcome, trace }
    }

    fn run(
        &self,
        function: &str,
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
            .call_fn(&mut Scope::new(), &self.ast, function, (input, context))
            .map_err(|error| ScriptError::from_eval(&error))?;
        envelope::from_answer(function, &answer)
    }
}

/// The script `file` holds: a component's `<script>` with the rest of the
/// file blanked, so every position is the file's own
/// (`extension-components` D2); any other file's bytes as they are.
pub fn script_of(file: &str, bytes: &[u8]) -> Result<Vec<u8>, ScriptError> {
    use uze_contract::component::{BlockKind, Component, blanked, is_component};
    if !is_component(file) {
        return Ok(bytes.to_vec());
    }
    let text = std::str::from_utf8(bytes).map_err(|error| ScriptError::Compile {
        message: format!("not UTF-8: {error}"),
        at: None,
    })?;
    let located = |offset: usize| {
        let before = &text[..offset.min(text.len())];
        Some(crate::Location {
            line: before.matches('\n').count() + 1,
            column: before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
        })
    };
    let component = Component::read(text).map_err(|errors| {
        let first = &errors[0];
        ScriptError::Compile {
            message: first.message.clone(),
            at: located(first.offset),
        }
    })?;
    let script = component
        .block(BlockKind::Script)
        .ok_or_else(|| ScriptError::Compile {
            message: "it holds no `<script>`".to_owned(),
            at: None,
        })?;
    Ok(blanked(text, script).into_bytes())
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
