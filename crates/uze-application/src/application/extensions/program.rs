//! A declared extension's program, from the declaration the operator
//! accepted to the answer it gave (`extension-programs-and-confinement`).
//!
//! What runs is rebuilt from the compile whose digest was consented to:
//! the program the declaration names, pinned when it was consented to or
//! packaged in those very bytes, its arguments substituted whole from the
//! question's input. A packaged program is written out of the loaded bytes
//! into the run's own directory and executed from there, so what runs is
//! what was digested even if the files on disk change in between. Where
//! this machine confines, the run reads the checkout, its Git directory,
//! the extension's own files and the paths its `exec` declares, and
//! writes only its scratch directory, which is removed after it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use uze_contract::{Answer, Asking, Envelope, Output, Program};
use uze_core::UzeHome;
use uze_core::capability::extension::Loaded;
use uze_core::capability::extension::program::{Packaged, answer_of, packaged};
use uze_core::machine::extension_program::{
    self, Confine, ProgramOutcome, ProgramRun, environment, substitute_all,
};
use uze_platform::confine::{Confinement, Limits};

use super::{ExtensionCheckout, ExtensionConsent};

/// The prefixes a program may read and execute beneath: where a system
/// keeps its programs and the libraries they load. Only those that exist
/// are granted, since a grant of a missing path fails the run.
const SYSTEM_PREFIXES: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/etc",
    "/opt",
    "/nix/store",
    "/snap",
];

/// What a program reads that is no file of anybody's: randomness and the
/// empty devices every runtime opens.
const SYSTEM_READS: &[&str] = &[
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/proc/self",
    "/proc/sys/kernel/random",
    "/proc/sys/kernel/osrelease",
];

/// Where a declared path beneath it is a device: spoken to, not read.
const DEVICES: &str = "/dev/";

static RUNS: AtomicU64 = AtomicU64::new(0);

/// One question for a program, with everything it is answered from.
pub(super) struct Asked<'a> {
    pub home: &'a UzeHome,
    pub extension_dir: &'a Path,
    /// The consented compile's bytes, for a packaged program; a script's
    /// `run_program` names only programs an `exec:` grant covers.
    pub files: Option<&'a Loaded>,
    pub program: &'a Program,
    pub output: &'a Output,
    pub reads: &'a [String],
    pub asking: Asking,
    pub declaration: &'a str,
    pub extension: &'a str,
    pub checkout: &'a ExtensionCheckout,
    pub input: &'a serde_json::Value,
    pub store: Option<&'a serde_json::Value>,
    pub consent: Option<&'a ExtensionConsent>,
    /// The confinement helper, when this runner confines.
    pub helper: Option<&'a Path>,
    /// Whether the arguments are a script's own, taken as they are, rather
    /// than a declaration's, with `{field}` substituted.
    pub literal: bool,
}

/// Runs what `asked` declares and reads its answer as its output mode
/// says.
pub(super) fn answer(asked: &Asked<'_>) -> Answer {
    match ran(asked) {
        Ok(ended) => outcome(asked, ended),
        Err(not_run) => not_run.into(),
    }
}

/// What a script's `run_program` call gets back: the program's code and
/// both streams, or why it did not run.
pub(super) fn reply(asked: &Asked<'_>) -> Result<uze_script::ProgramReply, uze_script::ReachError> {
    let name = asked.program.program();
    let refused = |reason: String| uze_script::ReachError(format!("`{name}`: {reason}"));
    let ended = ran(asked).map_err(|not_run| match not_run {
        NotRun::Withheld(reason) | NotRun::Failed(reason) => refused(reason),
    })?;
    let reply = |code: Option<i64>, stdout: String, stderr: String| uze_script::ProgramReply {
        code,
        stdout,
        stderr,
    };
    match ended {
        ProgramOutcome::Answered { stdout } => Ok(reply(
            Some(0),
            String::from_utf8_lossy(&stdout).into_owned(),
            String::new(),
        )),
        ProgramOutcome::Refused { reason } => Ok(reply(
            Some(i64::from(extension_program::REFUSED_EXIT)),
            String::new(),
            reason,
        )),
        ProgramOutcome::Failed { reason } => Ok(reply(None, String::new(), reason)),
        ProgramOutcome::TimedOut { limit } => {
            Err(refused(format!("timed out after {}s", limit.as_secs_f32())))
        }
        ProgramOutcome::Flooded { stream, bound } => Err(refused(format!(
            "wrote more than {} KiB on {stream}",
            bound / 1024
        ))),
        ProgramOutcome::Unenforced { reason } => Err(refused(format!(
            "its confinement did not apply ({reason}); nothing ran"
        ))),
        ProgramOutcome::NotStarted { reason } => Err(refused(format!("did not start: {reason}"))),
    }
}

/// Lays the run out, runs it and takes its directory away again.
fn ran(asked: &Asked<'_>) -> Result<ProgramOutcome, NotRun> {
    let run = format!(
        "{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    );
    let dir = asked.home.extension_scratch_dir(&run);
    let ended = prepare(asked, &dir).map(|prepared| extension_program::run(&prepared));
    let _ = std::fs::remove_dir_all(&dir);
    ended
}

fn failed(reason: impl Into<String>) -> Answer {
    Answer::Failed {
        reason: reason.into(),
        trace: Vec::new(),
    }
}

/// Why a program was not started: withheld when what is missing is the
/// operator's to give, failed when the extension or the machine is at
/// fault.
enum NotRun {
    Withheld(String),
    Failed(String),
}

impl From<NotRun> for Answer {
    fn from(not_run: NotRun) -> Self {
        match not_run {
            NotRun::Withheld(reason) => Answer::withheld(reason),
            NotRun::Failed(reason) => failed(reason),
        }
    }
}

fn withheld(reason: impl Into<String>) -> NotRun {
    NotRun::Withheld(reason.into())
}

fn not_started(reason: impl Into<String>) -> NotRun {
    NotRun::Failed(reason.into())
}

/// The program resolved, its arguments substituted, its directories made
/// and its confinement written: everything but starting it.
fn prepare(asked: &Asked<'_>, dir: &Path) -> Result<ProgramRun, NotRun> {
    let scratch = dir.join("tmp");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&scratch)
        .and_then(|()| std::fs::create_dir_all(&bin))
        .map_err(|error| {
            not_started(format!("its scratch directory could not be made: {error}"))
        })?;
    let pinned = asked
        .consent
        .and_then(|consent| consent.exec_paths.as_ref());
    let pin = |name: &str| -> Result<PathBuf, NotRun> {
        let path = pinned
            .and_then(|paths| paths.get(name))
            .cloned()
            .ok_or_else(|| {
                withheld(format!(
                    "`{name}` not found; resolve it again on the Extensions screen"
                ))
            })?;
        if path.starts_with(&asked.checkout.root) {
            return Err(withheld(format!(
                "`{name}` was pinned to {}, inside this checkout, where an extension could have put it",
                path.display()
            )));
        }
        Ok(path)
    };
    let scope = match asked.asking {
        Asking::Action => asked.input.get("item").cloned().unwrap_or_default(),
        Asking::Source | Asking::Refresh => asked.input.clone(),
    };
    let rest = if asked.literal {
        asked.program.argv[1..].to_vec()
    } else {
        substitute_all(&asked.program.argv[1..], &scope).map_err(withheld)?
    };
    let (executable, mut args, executes_from) = if asked.program.is_packaged() {
        let file = asked.program.program().trim_start_matches("./");
        let bytes = asked
            .files
            .and_then(|files| files.bytes(file))
            .ok_or_else(|| not_started(format!("the program `{file}` is not in the extension")))?;
        let copy = bin.join(file.rsplit('/').next().unwrap_or(file));
        std::fs::write(&copy, bytes)
            .and_then(|()| uze_platform::executable::make_runnable(&copy))
            .map_err(|error| {
                not_started(format!(
                    "the program `{file}` could not be laid out: {error}"
                ))
            })?;
        match packaged(bytes).map_err(|why| not_started(format!("the program `{file}`: {why}")))? {
            Packaged::Script { interpreter, args } => {
                let interpreter_path = pin(&interpreter)?;
                let mut argv = args;
                argv.push(copy.display().to_string());
                let parent = interpreter_path.parent().map(Path::to_path_buf);
                (interpreter_path, argv, parent)
            }
            Packaged::Native => (copy, Vec::new(), None),
        }
    } else {
        let path = pin(asked.program.program())?;
        let parent = path.parent().map(Path::to_path_buf);
        (path, Vec::new(), parent)
    };
    args.extend(rest);
    let inputs = flat_inputs(&scope);
    let confine = match asked.helper {
        Some(helper) => Some(Confine {
            helper: helper.to_path_buf(),
            rules: serde_json::to_string(&confinement(asked, &scratch, &bin, executes_from))
                .map_err(|error| {
                    not_started(format!("its confinement could not be written: {error}"))
                })?,
        }),
        None if asked
            .consent
            .is_some_and(|consent| consent.enforced_at_accept == Some(true)) =>
        {
            return Err(withheld(
                "it was allowed to run confined, and this machine does not confine programs now; \
                 nothing ran — allow it again on the Extensions screen",
            ));
        }
        None => None,
    };
    let mut stdin = serde_json::json!({
        "protocol": uze_contract::PROTOCOL,
        "context": {
            "checkout": asked.checkout.root.display().to_string(),
            "extension": asked.extension,
            declared_as(asked.asking): asked.declaration,
        },
        "input": asked.input,
    });
    if let Some(store) = asked.store {
        stdin["store"] = store.clone();
    }
    Ok(ProgramRun {
        executable,
        args,
        cwd: asked.checkout.root.clone(),
        environment: environment(std::env::vars_os(), &scratch, &asked.checkout.root, &inputs),
        stdin: serde_json::to_vec(&stdin).unwrap_or_default(),
        time_limit: asked.program.time_limit(),
        confine,
    })
}

fn declared_as(asking: Asking) -> &'static str {
    match asking {
        Asking::Action => "action",
        Asking::Source | Asking::Refresh => "source",
    }
}

/// The input's top-level text and numbers, for `UZE_IN_<NAME>`.
fn flat_inputs(scope: &serde_json::Value) -> BTreeMap<String, String> {
    scope
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| {
            let text = match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Number(number) => number.to_string(),
                serde_json::Value::Bool(flag) => flag.to_string(),
                _ => return None,
            };
            uze_contract::binding::is_field_name(name).then(|| (name.clone(), text))
        })
        .collect()
}

/// What the run may reach (D6, P5): read and execute the system, the
/// pinned program's directory and the run's own copy of a packaged one;
/// read the checkout, its Git directory, the extension and what the
/// `exec` declares; speak to the devices it declares; write only the
/// scratch directory and `/dev/null`.
fn confinement(
    asked: &Asked<'_>,
    scratch: &Path,
    bin: &Path,
    executes_from: Option<PathBuf>,
) -> Confinement {
    let existing = |paths: Vec<PathBuf>| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = paths.into_iter().filter(|path| path.exists()).collect();
        paths.sort();
        paths.dedup();
        paths
    };
    let (devices, reads): (Vec<PathBuf>, Vec<PathBuf>) = asked
        .reads
        .iter()
        .map(PathBuf::from)
        .partition(|path| path.starts_with(DEVICES));
    let read_execute = SYSTEM_PREFIXES
        .iter()
        .map(PathBuf::from)
        .chain([bin.to_path_buf()])
        .chain(executes_from)
        .collect();
    let read = SYSTEM_READS
        .iter()
        .map(PathBuf::from)
        .chain([
            asked.checkout.root.clone(),
            asked.checkout.common_dir.clone(),
            asked.extension_dir.to_path_buf(),
        ])
        .chain(reads)
        .collect();
    Confinement {
        read: existing(read),
        read_execute: existing(read_execute),
        write: existing(vec![scratch.to_path_buf(), PathBuf::from("/dev/null")]),
        devices: existing(devices),
        network: false,
        limits: Limits::with_cpu_seconds(asked.program.time_limit().as_secs().saturating_mul(2)),
    }
}

/// How the run ended, as the surface that asked is told.
fn outcome(asked: &Asked<'_>, ended: ProgramOutcome) -> Answer {
    let name = asked.program.program();
    match ended {
        ProgramOutcome::Answered { stdout } => match answer_of(asked.output, &stdout) {
            Ok(envelope) => Answer::Answered {
                envelope,
                trace: Vec::new(),
            },
            Err(why) => failed(format!("`{name}`: {why}")),
        },
        // An action that refuses says why where the operator is looking; a
        // source that refuses has nothing to draw, which is a failure that
        // says so.
        ProgramOutcome::Refused { reason } if asked.asking == Asking::Action => Answer::Answered {
            envelope: Envelope {
                notice: Some(reason),
                ..Envelope::default()
            },
            trace: Vec::new(),
        },
        ProgramOutcome::Refused { reason } => failed(format!("`{name}` refused: {reason}")),
        ProgramOutcome::Failed { reason } => failed(format!("`{name}`: {reason}")),
        ProgramOutcome::TimedOut { limit } => {
            failed(format!("`{name}` timed out after {}s", limit.as_secs_f32()))
        }
        ProgramOutcome::Flooded { stream, bound } => failed(format!(
            "`{name}` wrote more than {} KiB on {stream}, and was stopped",
            bound / 1024
        )),
        ProgramOutcome::Unenforced { reason } => Answer::withheld(format!(
            "its confinement did not apply ({reason}); nothing ran"
        )),
        ProgramOutcome::NotStarted { reason } => {
            failed(format!("`{name}` did not start: {reason}"))
        }
    }
}
