//! What a script that did not answer says about why: the bound it crossed,
//! or where in its text it went wrong.

use rhai::{EvalAltResult, ParseError, ParseErrorType, Position};

/// A bound the engine holds a script to (`add-extension-platform` D19, D28).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// More operations than one answer may take.
    Operations,
    /// Past the wall-clock deadline of one answer.
    Deadline,
    /// Functions calling functions deeper than allowed.
    CallDepth,
    /// An expression nested deeper than the parser accepts.
    ExpressionDepth,
    /// A string longer than allowed.
    StringSize,
    /// An array with more elements than allowed.
    ArraySize,
    /// A map with more entries than allowed.
    MapSize,
    /// A script larger than the engine compiles.
    ScriptSize,
}

impl Limit {
    /// The bound's name, as the surface and the inspector say it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Operations => "operations",
            Self::Deadline => "deadline",
            Self::CallDepth => "call depth",
            Self::ExpressionDepth => "expression depth",
            Self::StringSize => "string size",
            Self::ArraySize => "array size",
            Self::MapSize => "map size",
            Self::ScriptSize => "script size",
        }
    }
}

/// Where in the script's text something happened, 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

impl Location {
    fn of(position: Position) -> Option<Self> {
        Some(Self {
            line: position.line()?,
            column: position.position().unwrap_or(1),
        })
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Why a script did not compile or did not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("the script crossed its {} bound{}", .limit.name(), located(.at))]
    Limit { limit: Limit, at: Option<Location> },
    #[error("the script does not compile{}: {message}", located(.at))]
    Compile {
        message: String,
        at: Option<Location>,
    },
    #[error("the script failed{}: {message}", located(.at))]
    Runtime {
        message: String,
        at: Option<Location>,
    },
    #[error("the script calls `{name}`, which this engine does not have{}", located(.at))]
    MissingFunction { name: String, at: Option<Location> },
    #[error("the answer is not an envelope: {0}")]
    Envelope(String),
    #[error("the question cannot reach the script: {0}")]
    Question(String),
}

fn located(location: &Option<Location>) -> String {
    location
        .map(|location| format!(" at {location}"))
        .unwrap_or_default()
}

impl ScriptError {
    pub(crate) fn from_parse(error: &ParseError) -> Self {
        let at = Location::of(error.1);
        match error.0.as_ref() {
            ParseErrorType::ExprTooDeep => Self::Limit {
                limit: Limit::ExpressionDepth,
                at,
            },
            ParseErrorType::LiteralTooLarge(..) => Self::Limit {
                limit: Limit::StringSize,
                at,
            },
            other => Self::Compile {
                message: other.to_string(),
                at,
            },
        }
    }

    /// The innermost cause of a failed run: a bound reached three calls
    /// deep is reported as the bound, at the line where it was reached.
    pub(crate) fn from_eval(error: &EvalAltResult) -> Self {
        let at = Location::of(error.position());
        let limit = |limit| Self::Limit { limit, at };
        match error {
            EvalAltResult::ErrorInFunctionCall(_, _, inner, _)
            | EvalAltResult::ErrorInModule(_, inner, _) => Self::from_eval(inner),
            EvalAltResult::ErrorParsing(kind, position) => {
                Self::from_parse(&ParseError(Box::new(kind.clone()), *position))
            }
            EvalAltResult::ErrorTooManyOperations(_) => limit(Limit::Operations),
            EvalAltResult::ErrorTerminated(..) => limit(Limit::Deadline),
            EvalAltResult::ErrorStackOverflow(_) => limit(Limit::CallDepth),
            EvalAltResult::ErrorDataTooLarge(what, _) => limit(data_limit(what)),
            EvalAltResult::ErrorFunctionNotFound(signature, _) => Self::MissingFunction {
                name: function_name(signature).to_owned(),
                at,
            },
            other => Self::Runtime {
                message: runtime_message(other),
                at,
            },
        }
    }
}

fn data_limit(what: &str) -> Limit {
    if what.contains("string") {
        Limit::StringSize
    } else if what.contains("map") {
        Limit::MapSize
    } else {
        Limit::ArraySize
    }
}

/// `read_checkout (&str | ImmutableString)` → `read_checkout`.
fn function_name(signature: &str) -> &str {
    signature
        .split(|character: char| character == '(' || character.is_whitespace())
        .next()
        .unwrap_or(signature)
}

/// A thrown value reads as itself, not as Rhai's wrapper around it.
fn runtime_message(error: &EvalAltResult) -> String {
    match error {
        EvalAltResult::ErrorRuntime(value, _) => value.to_string(),
        other => {
            let text = other.to_string();
            match text.rsplit_once(" (line ") {
                Some((message, _)) => message.to_owned(),
                None => text,
            }
        }
    }
}
