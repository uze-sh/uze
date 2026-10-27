//! A resident script: one that keeps its state for as long as a surface
//! showing it is open (`extension-platform` D19,
//! `extension-media-and-resident` M4).
//!
//! A session lives on a thread of its own, holding the engine, the
//! script's top-level scope and its `this`: a script function sees no
//! variable outside itself, so what a resident script keeps between calls
//! it keeps on `this`, a map the session hands every entry point.
//!
//! The entry points: `start(context)`, `event(event)` and `input(input)`
//! answer whether to redraw (a bool, or a map whose `redraw` says it beside
//! the envelope's other members); `render(size)` answers a catalog tree;
//! `stop()`. Only `render` is required. Every call is held to the
//! per-question engine's bounds, and the first that fails stops the
//! session: what it says is why, and every later call answers it.
//!
//! `timer(ms)` asks for one tick, delivered by the host as
//! `event(#{ name: "timer" })`, never sooner than [`TIMER_FLOOR`].

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use rhai::{AST, CallFnOptions, Dynamic, Engine, Map, Scope};
use serde_json::Value;

use crate::engine::{self, CallClock, LIMITS};
use crate::envelope::{self, Envelope};
use crate::error::{Limit, ScriptError};
use crate::reach::{Grants, Reach};
use crate::script::Answered;
use crate::trace::Trace;
use crate::value;

/// The soonest a tick may follow the call that asked for it: 30 a second.
pub const TIMER_FLOOR: Duration = Duration::from_millis(34);

/// The one entry point every resident script defines.
const RENDER: &str = "render";

/// A resident script's text, compiled: it defines `fn render(size)`, and
/// whichever of the other entry points it needs.
#[derive(Debug, Clone)]
pub struct ResidentScript {
    ast: AST,
    entries: Entries,
}

/// Which optional entry points the script defines.
#[derive(Debug, Clone, Copy, Default)]
struct Entries {
    start: bool,
    event: bool,
    input: bool,
    stop: bool,
}

impl ResidentScript {
    /// Compile a resident script's bytes, refusing one that is not UTF-8,
    /// is larger than the engine takes, or defines no `render(size)`.
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
        let defines = |name: &str, arity: usize| {
            ast.iter_functions()
                .any(|function| function.name == name && function.params.len() == arity)
        };
        if !defines(RENDER, 1) {
            return Err(ScriptError::Compile {
                message: format!("it defines no `fn {RENDER}(size)`"),
                at: None,
            });
        }
        let entries = Entries {
            start: defines("start", 1),
            event: defines("event", 1),
            input: defines("input", 1),
            stop: defines("stop", 0),
        };
        Ok(Self { ast, entries })
    }
}

/// A call into the session, with where its answer goes.
type Asked = (Call, Sender<Answered>);

/// What the session is asked to do.
enum Call {
    Start(Value),
    Event(Value),
    Input(Value),
    Render { width: u16, height: u16 },
    Stop,
}

/// A running resident script. Calls are answered in the order they are
/// made, each on the session's own thread; dropping it ends the thread
/// without calling `stop`.
#[derive(Debug)]
pub struct Resident {
    calls: Mutex<Option<Sender<Asked>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    /// Why the session stopped, once it has.
    stopped: Arc<Mutex<Option<ScriptError>>>,
}

impl Resident {
    /// Starts `script` with the host functions `grants` register, answered
    /// by `reach`: runs its top level once, then `start(context)`. The
    /// answer says whether to render; when it failed, the session is
    /// already stopped.
    pub fn start(
        script: Arc<ResidentScript>,
        grants: &Grants,
        reach: Arc<dyn Reach>,
        context: &Value,
    ) -> (Self, Answered) {
        let (calls, received) = mpsc::channel::<Asked>();
        let stopped = Arc::new(Mutex::new(None));
        let grants = grants.clone();
        let why = Arc::clone(&stopped);
        let thread = std::thread::Builder::new()
            .name("uze-resident".to_owned())
            .spawn(move || Session::new(&grants, reach).serve(&script, &received, &why));
        let resident = Self {
            calls: Mutex::new(Some(calls)),
            thread: Mutex::new(thread.ok()),
            stopped,
        };
        let answered = resident.call(Call::Start(context.clone()));
        (resident, answered)
    }

    /// `event(event)`: whether to render.
    pub fn event(&self, event: &Value) -> Answered {
        self.call(Call::Event(event.clone()))
    }

    /// `input(input)`: a key or the pointer; whether to render.
    pub fn input(&self, input: &Value) -> Answered {
        self.call(Call::Input(input.clone()))
    }

    /// `render(#{ width, height })`: the catalog tree to draw.
    pub fn render(&self, width: u16, height: u16) -> Answered {
        self.call(Call::Render { width, height })
    }

    /// `stop()`, then the session ends. A session already stopped answers
    /// why.
    pub fn stop(&self) -> Answered {
        let answered = self.call(Call::Stop);
        lock(&self.calls).take();
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
        answered
    }

    /// Why the session stopped, once it has.
    pub fn stopped(&self) -> Option<ScriptError> {
        lock(&self.stopped).clone()
    }

    fn call(&self, call: Call) -> Answered {
        let (reply, answer) = mpsc::channel();
        let sent = lock(&self.calls)
            .as_ref()
            .is_some_and(|calls| calls.send((call, reply)).is_ok());
        let answered = sent.then(|| answer.recv().ok()).flatten();
        answered.unwrap_or_else(|| Answered {
            outcome: Err(self.stopped().unwrap_or_else(ended)),
            trace: Trace::default(),
        })
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        lock(&self.calls).take();
    }
}

/// The session's thread: the engine, the scope and `this`.
struct Session {
    engine: Engine,
    scope: Scope<'static>,
    this: Dynamic,
    clock: Arc<CallClock>,
    trace: Arc<Mutex<Trace>>,
    timer: Arc<Mutex<Option<rhai::INT>>>,
}

impl Session {
    fn new(grants: &Grants, reach: Arc<dyn Reach>) -> Self {
        let clock = CallClock::new();
        let trace = Arc::new(Mutex::new(Trace::default()));
        let mut engine = engine::running(grants, reach, Arc::clone(&trace), Arc::clone(&clock));
        let timer = Arc::new(Mutex::new(None));
        let asked = Arc::clone(&timer);
        engine.register_fn("timer", move |ms: rhai::INT| {
            *lock(&asked) = Some(ms);
        });
        Self {
            engine,
            scope: Scope::new(),
            this: Dynamic::from_map(Map::new()),
            clock,
            trace,
            timer,
        }
    }

    /// Answers calls until one fails, `stop` is called, or the handle is
    /// dropped.
    fn serve(
        mut self,
        script: &ResidentScript,
        calls: &Receiver<Asked>,
        stopped: &Mutex<Option<ScriptError>>,
    ) {
        while let Ok((call, reply)) = calls.recv() {
            let ending = matches!(call, Call::Stop);
            let answered = self.answer(script, call);
            if let Err(error) = &answered.outcome {
                *lock(stopped) = Some(error.clone());
            }
            let failed = answered.outcome.is_err();
            let _ = reply.send(answered);
            if ending || failed {
                break;
            }
        }
        let mut why = lock(stopped);
        if why.is_none() {
            *why = Some(ended());
        }
    }

    fn answer(&mut self, script: &ResidentScript, call: Call) -> Answered {
        self.clock.restart();
        lock(&self.timer).take();
        let outcome = self.run(script, call);
        let trace = std::mem::take(&mut *lock(&self.trace));
        let timer = lock(&self.timer).take().map(held_to_floor);
        Answered {
            outcome: outcome.map(|envelope| Envelope { timer, ..envelope }),
            trace,
        }
    }

    fn run(&mut self, script: &ResidentScript, call: Call) -> Result<Envelope, ScriptError> {
        let entries = script.entries;
        let crossing = |what: &'static str| {
            move |crossing: value::Crossing| {
                ScriptError::Question(format!("the {what} is {crossing}"))
            }
        };
        match call {
            Call::Start(context) => {
                self.engine
                    .run_ast_with_scope(&mut self.scope, &script.ast)
                    .map_err(|error| ScriptError::from_eval(&error))?;
                if !entries.start {
                    return Ok(redrawing(true));
                }
                let context = value::to_dynamic(&context).map_err(crossing("context"))?;
                let answer = self.entry(script, "start", vec![context])?;
                redraw_of("start", &answer)
            }
            Call::Event(event) if entries.event => {
                let event = value::to_dynamic(&event).map_err(crossing("event"))?;
                let answer = self.entry(script, "event", vec![event])?;
                redraw_of("event", &answer)
            }
            Call::Input(input) if entries.input => {
                let input = value::to_dynamic(&input).map_err(crossing("input"))?;
                let answer = self.entry(script, "input", vec![input])?;
                redraw_of("input", &answer)
            }
            Call::Event(_) | Call::Input(_) => Ok(redrawing(false)),
            Call::Render { width, height } => {
                let mut size = Map::new();
                size.insert("width".into(), Dynamic::from_int(width.into()));
                size.insert("height".into(), Dynamic::from_int(height.into()));
                let tree = self.entry(script, RENDER, vec![Dynamic::from_map(size)])?;
                if tree.is_unit() {
                    return Err(ScriptError::Envelope(
                        "`render` returned nothing, not a catalog tree".to_owned(),
                    ));
                }
                let tree = value::to_json(&tree)
                    .map_err(|crossing| ScriptError::Envelope(format!("the tree is {crossing}")))?;
                Ok(Envelope::data(tree))
            }
            Call::Stop => {
                if entries.stop {
                    drop(self.entry(script, "stop", Vec::new())?);
                }
                Ok(redrawing(false))
            }
        }
    }

    /// Calls `name` with `this` bound to what the script keeps, in the
    /// scope its top level left.
    fn entry(
        &mut self,
        script: &ResidentScript,
        name: &str,
        args: Vec<Dynamic>,
    ) -> Result<Dynamic, ScriptError> {
        let options = CallFnOptions::new()
            .eval_ast(false)
            .rewind_scope(true)
            .bind_this_ptr(&mut self.this);
        self.engine
            .call_fn_with_options::<Dynamic>(options, &mut self.scope, &script.ast, name, args)
            .map_err(|error| ScriptError::from_eval(&error))
    }
}

/// What `start`, `event` or `input` answered: `()` or a bool says whether
/// to redraw; a map says it as `redraw`, beside the envelope's members
/// (`store`, `toast`, …) the host applies as it does a source's.
fn redraw_of(entry: &str, answer: &Dynamic) -> Result<Envelope, ScriptError> {
    if answer.is_unit() {
        return Ok(redrawing(false));
    }
    if let Ok(redraw) = answer.as_bool() {
        return Ok(redrawing(redraw));
    }
    let Some(map) = answer.read_lock::<Map>().map(|map| map.clone()) else {
        return Err(ScriptError::Envelope(format!(
            "`{entry}` returned a `{}`, not whether to redraw",
            answer.type_name()
        )));
    };
    let mut map = map;
    let redraw = match map.remove("redraw") {
        None => false,
        Some(redraw) => redraw
            .as_bool()
            .map_err(|kind| ScriptError::Envelope(format!("`redraw` is a `{kind}`, not a bool")))?,
    };
    if map.contains_key("data") {
        return Err(ScriptError::Envelope(format!(
            "`{entry}` answers `redraw`, not `data`: `render` draws"
        )));
    }
    map.insert("data".into(), Dynamic::from_bool(redraw));
    envelope::from_answer(&Dynamic::from_map(map))
}

fn redrawing(redraw: bool) -> Envelope {
    Envelope::data(Value::Bool(redraw))
}

/// The delay a `timer(ms)` asked for, never sooner than [`TIMER_FLOOR`].
fn held_to_floor(ms: rhai::INT) -> u64 {
    let floor = u64::try_from(TIMER_FLOOR.as_millis()).unwrap_or(u64::MAX);
    u64::try_from(ms).unwrap_or(0).max(floor)
}

fn ended() -> ScriptError {
    ScriptError::Question("the resident script has stopped".to_owned())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
