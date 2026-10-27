//! The engine, built raw (`add-extension-platform` D19): only the packages
//! chosen here, no module can be loaded, no `eval`, no undeclared variable,
//! and every bound set before a script runs.
//!
//! An engine is built per answer. It costs about 0.19 ms (D28), and in
//! exchange each answer gets its own trace, its own deadline and exactly
//! the host functions its grants register, with nothing shared between two
//! extensions on the pool.

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
}

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

/// The engine one answer runs in, its clock started.
pub(crate) fn answering(
    grants: &Grants,
    reach: Arc<dyn Reach>,
    trace: Arc<Mutex<Trace>>,
) -> Engine {
    let limits = LIMITS;
    let mut engine = raw(&limits);
    primitives::register(&mut engine, &limits);
    let waiting = Arc::new(std::sync::atomic::AtomicU64::new(0));
    register_host(&mut engine, grants, reach, Arc::clone(&waiting));

    let printed = Arc::clone(&trace);
    engine.on_print(move |text| record(&printed, TraceKind::Print, text, None));
    engine.on_debug(move |text, _, position| {
        record(&trace, TraceKind::Debug, text, position.line());
    });

    let started = Instant::now();
    engine.on_progress(move |operations| {
        // The clock is read every 256 operations: a few nanoseconds each,
        // and still within a fraction of a millisecond of the deadline.
        // What a granted program took is its own time limit's, not the
        // script's (P10).
        let waited = Duration::from_millis(waiting.load(std::sync::atomic::Ordering::Relaxed));
        (operations % 256 == 0 && started.elapsed().saturating_sub(waited) > limits.deadline)
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
    waiting: Arc<std::sync::atomic::AtomicU64>,
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
                let started = Instant::now();
                let reply = runner.run_program(name, &args);
                let took = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                waiting.fetch_add(took, std::sync::atomic::Ordering::Relaxed);
                let reply = reply.map_err(|error| failure(error.0))?;
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
