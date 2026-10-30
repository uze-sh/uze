//! The engine, built raw (`extension-platform` D19): only the packages
//! chosen here, no module can be loaded, no `eval`, no undeclared variable,
//! and every bound set before a script runs.
//!
//! An engine is built per answer. It costs about 0.19 ms (D28), and in
//! exchange each answer gets its own trace, its own deadline and exactly
//! the host functions its grants register, with nothing shared between two
//! extensions on the pool.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use rhai::module_resolvers::DummyModuleResolver;
use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicIteratorPackage, BasicMapPackage, BasicMathPackage,
    BasicStringPackage, LogicPackage, MoreStringPackage, Package,
};
use rhai::{Array, Dynamic, Engine, Map, Module};

use crate::primitives::{self, failure};
use crate::reach::{CheckoutPath, Grant, Grants, Reach};
use crate::trace::{Trace, TraceKind};

/// The bounds of one answer, from D28's measures. The gallery's heaviest
/// board answers in 0.45 ms with 2 reads and 6 listings; the targets are
/// an answer under 20 ms at p95 and under 1 MiB per loaded extension.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    /// Some 20–50 ms of interpreter work: well past the measured board,
    /// and stopped before the deadline when the script only computes.
    pub operations: u64,
    /// Five times the p95 target. A lane of the two-thread pool is never
    /// held longer by one answer, however slow the host's reads.
    pub deadline: Duration,
    /// Recursion uses the pool thread's stack; 32 levels keep well inside
    /// it in a debug build too.
    pub call_levels: usize,
    /// Rhai's own release defaults, which keep the parser off the stack's end.
    pub expression_depth: usize,
    pub function_expression_depth: usize,
    /// One file of a checkout read whole: about the 1 MiB per extension.
    pub string_size: usize,
    /// A board of thousands of items, a few hundred KiB of values.
    pub array_size: usize,
    pub map_size: usize,
    /// Scripts are written by hand; 256 KiB is far beyond one.
    pub script_size: usize,
    /// The trace is for the inspector, not a log: bounded in both ways.
    pub trace_lines: usize,
    pub trace_line_bytes: usize,
    /// The host's programs and requests are timed on their own limits, not
    /// the deadline; these keep one answer from holding a pool lane for
    /// many of them in turn.
    pub host_calls: u64,
    pub host_waiting: Duration,
}

/// The stack every script runs on: a pool answer on a thread of its own,
/// a resident session on its thread. Rhai clones, drops, compares, prints
/// and size-checks a value by recursing into it, and its own bounds let a
/// value nest as deep as its array and map bounds together (some 20 000
/// levels, alternating). Measured on rhai 1.27: a release build compares
/// and prints a value that deep within 64 MiB; a debug build some 5 000
/// levels, several times what one answer builds before its deadline. A
/// stack is reserved, not committed, so the size costs address space only.
pub(crate) const STACK: usize = 64 << 20;

pub(crate) const LIMITS: Limits = Limits {
    operations: 1_000_000,
    deadline: Duration::from_millis(100),
    call_levels: 32,
    expression_depth: 64,
    function_expression_depth: 32,
    string_size: 1 << 20,
    array_size: 10_000,
    map_size: 10_000,
    script_size: 256 << 10,
    trace_lines: 200,
    trace_line_bytes: 1024,
    host_calls: 16,
    host_waiting: Duration::from_secs(60),
};

/// Arithmetic, logic, strings, arrays, maps, ranges and maths. Left out:
/// `LanguageCorePackage` (its `sleep` would hold a pool lane past any
/// operation count), time (the host's `clock` answers it), BLOBs, bit
/// fields, function-pointer metadata and debugging.
static PACKAGES: LazyLock<Vec<Arc<Module>>> = LazyLock::new(|| {
    vec![
        ArithmeticPackage::new().as_shared_module(),
        LogicPackage::new().as_shared_module(),
        BasicStringPackage::new().as_shared_module(),
        MoreStringPackage::new().as_shared_module(),
        BasicIteratorPackage::new().as_shared_module(),
        BasicArrayPackage::new().as_shared_module(),
        BasicMapPackage::new().as_shared_module(),
        BasicMathPackage::new().as_shared_module(),
    ]
});

/// What every engine is, compiling or answering.
fn raw(limits: &Limits) -> Engine {
    let mut engine = Engine::new_raw();
    for package in PACKAGES.iter() {
        engine.register_global_module(package.clone());
    }
    engine
        .set_module_resolver(DummyModuleResolver::new())
        .set_max_modules(0)
        .disable_symbol("eval")
        .disable_symbol("import")
        .disable_symbol("export")
        .set_strict_variables(true)
        .set_max_operations(limits.operations)
        .set_max_call_levels(limits.call_levels)
        .set_max_expr_depths(limits.expression_depth, limits.function_expression_depth)
        .set_max_string_size(limits.string_size)
        .set_max_array_size(limits.array_size)
        .set_max_map_size(limits.map_size);
    engine
}

/// The engine a script is compiled by: the same syntax and bounds, and no
/// function a run would have.
pub(crate) fn compiling() -> Engine {
    raw(&LIMITS)
}

/// When the call running now began, and how long it has waited on the
/// host's programs: what the deadline is counted against. A per-question
/// engine starts it once; a resident session restarts it before each call.
#[derive(Debug)]
pub(crate) struct CallClock {
    started: Mutex<Instant>,
    /// Milliseconds a granted program took: its own time limit's, not the
    /// script's (P10).
    waiting: AtomicU64,
    /// Programs run and requests made by the call.
    host_calls: AtomicU64,
}

impl CallClock {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Mutex::new(Instant::now()),
            waiting: AtomicU64::new(0),
            host_calls: AtomicU64::new(0),
        })
    }

    /// A new call begins now.
    pub(crate) fn restart(&self) {
        *self
            .started
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
        self.waiting.store(0, Ordering::Relaxed);
        self.host_calls.store(0, Ordering::Relaxed);
    }

    /// Times `ask`, a program or a request the host answers, against the
    /// call's bounds: refused once the call has made [`Limits::host_calls`]
    /// of them or waited [`Limits::host_waiting`] on them.
    fn host<T>(
        &self,
        limits: &Limits,
        ask: impl FnOnce() -> T,
    ) -> Result<T, Box<rhai::EvalAltResult>> {
        if self.host_calls.fetch_add(1, Ordering::Relaxed) >= limits.host_calls {
            return Err(failure(format!(
                "an answer asks the host at most {} calls of `run_program`, `fetch` and `mcp`",
                limits.host_calls
            )));
        }
        let waited = Duration::from_millis(self.waiting.load(Ordering::Relaxed));
        if waited >= limits.host_waiting {
            return Err(failure(format!(
                "an answer waits on `run_program`, `fetch` and `mcp` at most {} s in all",
                limits.host_waiting.as_secs()
            )));
        }
        let started = Instant::now();
        let answer = ask();
        self.waited(started.elapsed());
        Ok(answer)
    }

    fn spent(&self) -> Duration {
        let started = *self
            .started
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let waited = Duration::from_millis(self.waiting.load(Ordering::Relaxed));
        started.elapsed().saturating_sub(waited)
    }

    fn waited(&self, took: Duration) {
        let took = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
        self.waiting.fetch_add(took, Ordering::Relaxed);
    }
}

/// The engine one answer runs in, its clock started.
pub(crate) fn answering(
    grants: &Grants,
    reach: Arc<dyn Reach>,
    trace: Arc<Mutex<Trace>>,
) -> Engine {
    running(grants, reach, trace, CallClock::new())
}

/// An engine whose every call is held to [`LIMITS`], counting its
/// deadline on `clock`.
pub(crate) fn running(
    grants: &Grants,
    reach: Arc<dyn Reach>,
    trace: Arc<Mutex<Trace>>,
    clock: Arc<CallClock>,
) -> Engine {
    let limits = LIMITS;
    let mut engine = raw(&limits);
    primitives::register(&mut engine, &limits);
    register_host(&mut engine, grants, reach, Arc::clone(&clock));

    let printed = Arc::clone(&trace);
    engine.on_print(move |text| record(&printed, TraceKind::Print, text, None));
    engine.on_debug(move |text, _, position| {
        record(&trace, TraceKind::Debug, text, position.line());
    });

    engine.on_progress(move |operations| {
        // The clock is read every 256 operations: a few nanoseconds each,
        // and still within a fraction of a millisecond of the deadline.
        (operations % 256 == 0 && clock.spent() > limits.deadline)
            .then(|| Dynamic::from("deadline"))
    });
    engine
}

fn record(trace: &Mutex<Trace>, kind: TraceKind, text: &str, line: Option<usize>) {
    trace
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(kind, text, line, &LIMITS);
}

/// Each host function exists only when its grant was given; `clock` is
/// always there.
fn register_host(
    engine: &mut Engine,
    grants: &Grants,
    reach: Arc<dyn Reach>,
    clock: Arc<CallClock>,
) {
    if crate::reach::holds(grants, &Grant::Store) {
        let keeper = Arc::clone(&reach);
        engine.register_fn(
            "store",
            move || -> Result<Dynamic, Box<rhai::EvalAltResult>> {
                let store = keeper.store().map_err(|error| failure(error.0))?;
                crate::value::to_dynamic(&store)
                    .map_err(|crossing| failure(format!("the store is {crossing}")))
            },
        );
    }
    if grants.iter().any(|grant| grant.program().is_some()) {
        let runner = Arc::clone(&reach);
        let clock = Arc::clone(&clock);
        let granted: Vec<String> = grants
            .iter()
            .filter_map(|grant| grant.program().map(str::to_owned))
            .collect();
        engine.register_fn(
            crate::reach::RUN_PROGRAM,
            move |name: &str, args: Array| -> Result<Map, Box<rhai::EvalAltResult>> {
                if !granted.iter().any(|program| program == name) {
                    return Err(failure(format!(
                        "`{name}` is not a program this extension holds `exec:{name}` for"
                    )));
                }
                let args = args
                    .into_iter()
                    .map(|arg| {
                        arg.into_string().map_err(|kind| {
                            failure(format!("an argument is a `{kind}`, not a string"))
                        })
                    })
                    .collect::<Result<Vec<String>, _>>()?;
                let reply = clock
                    .host(&LIMITS, || runner.run_program(name, &args))?
                    .map_err(|error| failure(error.0))?;
                let mut map = Map::new();
                map.insert(
                    "code".into(),
                    reply.code.map_or(Dynamic::UNIT, Dynamic::from),
                );
                map.insert("stdout".into(), Dynamic::from(reply.stdout));
                map.insert("stderr".into(), Dynamic::from(reply.stderr));
                Ok(map)
            },
        );
    }
    if crate::reach::holds(grants, &crate::reach::ANY_HOST) {
        let fetcher = Arc::clone(&reach);
        let granted = grants.clone();
        let waited = Arc::clone(&clock);
        let fetch = move |url: &str, options: Map| -> Result<Map, Box<rhai::EvalAltResult>> {
            let request = crate::fetch::FetchRequest::read(url, &options, &granted)
                .map_err(|error| failure(error.0))?;
            let reply = waited.host(&LIMITS, || fetcher.fetch(&request))?;
            Ok(crate::fetch::reply_map(
                reply.map_err(|error| failure(error.0))?,
            ))
        };
        let plain = fetch.clone();
        engine.register_fn("fetch", move |url: &str| plain(url, Map::new()));
        engine.register_fn("fetch", fetch);
    }
    if crate::reach::holds(grants, &crate::reach::ANY_TOOL) {
        let caller = Arc::clone(&reach);
        let granted = grants.clone();
        let waited = Arc::clone(&clock);
        let mcp = move |server: &str,
                        tool: &str,
                        arguments: Map|
              -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            let grant = Grant::Mcp {
                server: server.to_owned(),
                tool: tool.to_owned(),
            };
            if !granted.contains(&grant) {
                return Err(failure(format!(
                    "`{server}/{tool}` is not a tool this extension holds `{}` for",
                    grant.name()
                )));
            }
            let arguments = crate::value::to_json(&Dynamic::from_map(arguments))
                .map_err(|crossing| failure(format!("the arguments are {crossing}")))?;
            let answer = waited
                .host(&LIMITS, || caller.mcp(server, tool, &arguments))?
                .map_err(|error| failure(error.0))?;
            crate::value::to_dynamic(&answer)
                .map_err(|crossing| failure(format!("the result is {crossing}")))
        };
        let bare = mcp.clone();
        engine.register_fn("mcp", move |server: &str, tool: &str| {
            bare(server, tool, Map::new())
        });
        engine.register_fn("mcp", mcp);
    }
    if grants.contains(&Grant::ReadCheckout) {
        let reader = Arc::clone(&reach);
        engine.register_fn(
            "read_checkout",
            move |path: &str| -> Result<String, Box<rhai::EvalAltResult>> {
                let path = CheckoutPath::parse(path).map_err(|error| failure(error.0))?;
                reader
                    .read_checkout(&path)
                    .map_err(|error| failure(error.0))
            },
        );
        let lister = Arc::clone(&reach);
        engine.register_fn(
            "list_checkout",
            move |path: &str| -> Result<Array, Box<rhai::EvalAltResult>> {
                let path = CheckoutPath::parse(path).map_err(|error| failure(error.0))?;
                let entries = lister
                    .list_checkout(&path)
                    .map_err(|error| failure(error.0))?;
                Ok(entries
                    .into_iter()
                    .map(|entry| {
                        let mut map = Map::new();
                        map.insert("name".into(), Dynamic::from(entry.name));
                        map.insert("kind".into(), Dynamic::from(entry.kind.name()));
                        Dynamic::from_map(map)
                    })
                    .collect())
            },
        );
    }
    engine.register_fn("clock", move || -> rhai::INT {
        rhai::INT::try_from(reach.clock().as_millis()).unwrap_or(rhai::INT::MAX)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reach::{Entry, ReachError};

    struct Answering;

    impl Reach for Answering {
        fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
            Err(ReachError("no checkout".to_owned()))
        }

        fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
            Ok(Vec::new())
        }

        fn fetch(
            &self,
            _: &crate::fetch::FetchRequest,
        ) -> Result<crate::fetch::FetchReply, ReachError> {
            Ok(crate::fetch::FetchReply {
                status: 200,
                body: String::new(),
            })
        }
    }

    #[test]
    fn a_call_that_waited_its_ceiling_on_the_host_asks_no_more() {
        let grants = Grants::from([Grant::Net("api.github.com".into())]);
        let clock = CallClock::new();
        let engine = running(
            &grants,
            Arc::new(Answering),
            Arc::new(Mutex::new(Trace::default())),
            Arc::clone(&clock),
        );
        let fetch = r#"fetch("https://api.github.com/").status"#;
        assert_eq!(engine.eval::<rhai::INT>(fetch).unwrap(), 200);
        clock.waited(LIMITS.host_waiting);
        let refused = engine.eval::<rhai::INT>(fetch).unwrap_err().to_string();
        assert!(refused.contains("at most 60 s"), "{refused}");
        clock.restart();
        assert_eq!(engine.eval::<rhai::INT>(fetch).unwrap(), 200);
    }
}
