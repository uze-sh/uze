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
//! writes only its scratch directory, which is removed after it — save
//! the pictures it left there, kept for its views to show as
//! `scratch:<file>`.

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

/// What the objects a confinement makes for itself are named under (on
/// Windows, each run's container and the capability a grant writes).
const CONFINEMENT_NAMESPACE: &str = "uze";

static RUNS: AtomicU64 = AtomicU64::new(0);

/// How old a run's scratch directory must be before another process takes
/// it for one a killed run left, when the platform cannot say whether the
/// process that made it still runs: far past any program's time limit.
const STALE_SCRATCH: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// How long a run's scratch may live even while a process with its pid
/// answers alive: a pid is reused, and no program a view waits on runs for
/// a week, so past this the process answering is another one.
const SCRATCH_AGE_CAP: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// The scratch directories a killed process left are swept once, by its
/// successor's first run: a process that lives removes its own runs'.
static SWEPT: std::sync::Once = std::sync::Once::new();

/// A fresh scratch directory for one run, named apart from every other
/// run of every process, after this process's first one swept what killed
/// runs left.
fn scratch_for_run(home: &UzeHome) -> PathBuf {
    SWEPT.call_once(|| {
        crate::application::Collectable::collect(&ExtensionScratch, home);
    });
    let run = format!(
        "{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    );
    home.extension_scratch_dir(&run)
}

/// Every run's scratch directory: alive while the process whose id names
/// it (`{pid}-{run}`) is alive, so a program that runs for hours keeps its
/// scratch however old it grows. A directory whose process the platform
/// cannot answer for falls back to its age ([`STALE_SCRATCH`]), and one
/// older than [`SCRATCH_AGE_CAP`] goes whatever its pid answers, since the
/// pid may have been reused. Takes no lock: only a process that is gone,
/// or one too old to be running, loses its scratch.
pub(crate) struct ExtensionScratch;

impl crate::application::Collectable for ExtensionScratch {
    fn name(&self) -> &'static str {
        "extension scratch"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["extension_scratch_dir"]
    }

    fn collect(&self, home: &UzeHome) -> crate::application::Collected {
        crate::application::Collected::removed(
            self.name(),
            sweep_scratch(home, STALE_SCRATCH, uze_platform::process::alive),
        )
    }
}

/// Removes every run's scratch directory whose process `alive` says is
/// gone, or, where it cannot say, last changed more than `older_than` ago;
/// and any last changed more than [`SCRATCH_AGE_CAP`] ago.
fn sweep_scratch(
    home: &UzeHome,
    older_than: std::time::Duration,
    alive: impl Fn(u32) -> Option<bool>,
) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home.extension_scratch_root()) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let process = entry
            .file_name()
            .to_str()
            .and_then(|name| name.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<u32>().ok());
        let older = |than: std::time::Duration| {
            entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age >= than))
        };
        let gone = match process.and_then(&alive) {
            Some(alive) => !alive || older(SCRATCH_AGE_CAP),
            None => older(older_than),
        };
        if gone && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed.push(entry.path());
        }
    }
    removed
}

/// Removes every run's scratch directory last changed more than
/// `older_than` ago, whatever its process: the rule where liveness cannot
/// be told.
#[cfg(test)]
fn sweep_stale_scratch(home: &UzeHome, older_than: std::time::Duration) {
    sweep_scratch(home, older_than, |_| None);
}

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
    /// Where the pictures the run leaves in its scratch are kept, when its
    /// views may show them; a check keeps none.
    pub pictures: Option<&'a Path>,
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
    let dir = scratch_for_run(asked.home);
    let ended = prepare(asked, &dir).map(|prepared| extension_program::run(&prepared));
    if let (Ok(_), Some(kept)) = (&ended, asked.pictures) {
        keep_pictures(&dir.join(SCRATCH), kept);
    }
    let _ = std::fs::remove_dir_all(&dir);
    ended
}

/// The run's own scratch, inside its directory: its `TMPDIR`.
const SCRATCH: &str = "tmp";

/// The most pictures one run keeps, and one extension's pictures hold.
const KEPT_PICTURES: usize = 16;

/// The pictures extension programs left, by the bound each run keeps them
/// to: the newest [`KEPT_PICTURES`] per extension. Takes no lock: a picture
/// lands whole by a rename, and one taken is written again by its
/// program's next run.
pub(crate) struct ExtensionPictures;

impl crate::application::Collectable for ExtensionPictures {
    fn name(&self) -> &'static str {
        "extension pictures"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["extension_pictures_dir"]
    }

    fn collect(&self, home: &UzeHome) -> crate::application::Collected {
        let pictures = home.extension_pictures_root();
        let mut removed = Vec::new();
        for plugin in std::fs::read_dir(&pictures).into_iter().flatten().flatten() {
            for extension in std::fs::read_dir(plugin.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                removed.extend(bound_pictures(&extension.path(), KEPT_PICTURES));
            }
        }
        crate::application::Collected::removed(self.name(), removed)
    }
}

/// Removes all but the newest `kept` pictures in `dir`.
fn bound_pictures(dir: &Path, kept: usize) -> Vec<PathBuf> {
    let mut pictures: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| entry.file_name().to_str().is_some_and(is_picture_name))
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (modified, entry.path())
        })
        .collect();
    pictures.sort_by(|left, right| right.cmp(left));
    pictures
        .into_iter()
        .skip(kept)
        .map(|(_, path)| path)
        .filter(|path| std::fs::remove_file(path).is_ok())
        .collect()
}

/// Copies the pictures a run left at the top of its scratch into `kept`,
/// each replacing the one of its name: a PNG, JPEG or GIF, a plain file
/// and no link, no larger than an `Image` reads. Each lands whole or not
/// at all, so a view never reads one half written.
fn keep_pictures(scratch: &Path, kept: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    let pictures = entries.flatten().filter(|entry| {
        let plain = entry.file_type().is_ok_and(|kind| kind.is_file());
        let small = entry
            .metadata()
            .is_ok_and(|meta| meta.len() <= uze_contract::IMAGE_FILE_MAX as u64);
        plain && small && entry.file_name().to_str().is_some_and(is_picture_name)
    });
    for entry in pictures.take(KEPT_PICTURES) {
        let name = entry.file_name();
        let partial = kept.join(format!(".{}.partial", name.to_string_lossy()));
        let kept_well = std::fs::create_dir_all(kept)
            .and_then(|()| std::fs::copy(entry.path(), &partial))
            .and_then(|_| std::fs::rename(&partial, kept.join(&name)));
        if let Err(error) = kept_well {
            let _ = std::fs::remove_file(&partial);
            tracing::debug!(%error, file = %name.to_string_lossy(), "a picture was not kept");
        }
    }
}

/// Whether `name` is a file an `Image` shows: a PNG, JPEG or GIF, named
/// plainly.
pub(super) fn is_picture_name(name: &str) -> bool {
    let plain = !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\']);
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    plain && matches!(extension.as_deref(), Some("png" | "jpg" | "jpeg" | "gif"))
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
    let scratch = dir.join(SCRATCH);
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
        Asking::Source | Asking::Refresh | Asking::Resident(_) => asked.input.clone(),
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
        Asking::Source | Asking::Refresh | Asking::Resident(_) => "source",
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
    let system = uze_platform::confine::system();
    let (devices, reads): (Vec<PathBuf>, Vec<PathBuf>) =
        asked.reads.iter().map(PathBuf::from).partition(|path| {
            system
                .device_roots
                .iter()
                .any(|root| path.starts_with(root))
        });
    let read_execute = system
        .read_execute
        .iter()
        .map(PathBuf::from)
        .chain([bin.to_path_buf()])
        .chain(executes_from)
        .collect();
    let read = system
        .read
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
        write: existing(
            [scratch.to_path_buf()]
                .into_iter()
                .chain(system.discard.iter().map(PathBuf::from))
                .collect(),
        ),
        devices: existing(devices),
        network: false,
        limits: Limits::with_cpu_seconds(asked.program.time_limit().as_secs().saturating_mul(2)),
        namespace: CONFINEMENT_NAMESPACE.to_owned(),
    }
}

/// One request `fetch` makes, as the system's `curl` runs it
/// (`extension-growth` G1): its arguments after the fixed ones and its
/// configuration on standard input, which is where a secret goes.
pub(super) struct CurlRun<'a> {
    pub home: &'a UzeHome,
    /// The `curl` executed: the system's own, by path.
    pub curl: &'a Path,
    pub args: Vec<String>,
    pub config: Vec<u8>,
    pub time_limit: std::time::Duration,
    /// The confinement helper, when this runner confines.
    pub helper: Option<&'a Path>,
}

/// The variables `curl` keeps from UZE's environment beyond what any
/// program does: the proxy and the CA bundle a machine behind a TLS-
/// inspecting proxy needs (`uze_platform::tools::curl_reads`).
const CURL_KEEPS: &[&str] = &[
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CURL_CA_BUNDLE",
];

/// Runs `run` through the program runner: in its own scratch directory,
/// with network and nothing of the checkout, under its bounds.
pub(super) fn curl(run: &CurlRun<'_>) -> ProgramOutcome {
    let dir = scratch_for_run(run.home);
    let scratch = dir.join(SCRATCH);
    if let Err(error) = std::fs::create_dir_all(&scratch) {
        return ProgramOutcome::NotStarted {
            reason: format!("its scratch directory could not be made: {error}"),
        };
    }
    let mut environment = environment(std::env::vars_os(), &scratch, &scratch, &BTreeMap::new());
    let kept: Vec<(String, std::ffi::OsString)> = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value)))
        .filter(|(name, _)| CURL_KEEPS.contains(&name.as_str()))
        .collect();
    let certificates: Vec<PathBuf> = kept
        .iter()
        .filter(|(name, _)| name.starts_with("SSL_") || name.starts_with("CURL_"))
        .map(|(_, value)| PathBuf::from(value))
        .collect();
    environment.extend(kept);
    let confine = match run.helper {
        Some(helper) => {
            let rules = serde_json::to_string(&curl_confinement(run, &scratch, certificates));
            match rules {
                Ok(rules) => Some(Confine {
                    helper: helper.to_path_buf(),
                    rules,
                }),
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    return ProgramOutcome::NotStarted {
                        reason: format!("its confinement could not be written: {error}"),
                    };
                }
            }
        }
        None => None,
    };
    let args = uze_platform::tools::CURL_ARGUMENTS
        .iter()
        .map(|arg| (*arg).to_owned())
        .chain(run.args.iter().cloned())
        .collect();
    let ended = extension_program::run(&ProgramRun {
        executable: run.curl.to_path_buf(),
        args,
        cwd: scratch.clone(),
        environment,
        stdin: run.config.clone(),
        time_limit: run.time_limit,
        confine,
    });
    let _ = std::fs::remove_dir_all(&dir);
    ended
}

/// What a request may reach: the system's programs and libraries, the
/// resolver's files wherever they lead, the CA bundle it was pointed at,
/// and the network; it writes only its scratch.
fn curl_confinement(run: &CurlRun<'_>, scratch: &Path, certificates: Vec<PathBuf>) -> Confinement {
    let existing = |paths: Vec<PathBuf>| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = paths.into_iter().filter(|path| path.exists()).collect();
        paths.sort();
        paths.dedup();
        paths
    };
    let system = uze_platform::confine::system();
    let resolver = system
        .resolver
        .iter()
        .filter_map(|file| std::fs::canonicalize(file).ok());
    let read = system
        .read
        .iter()
        .map(PathBuf::from)
        .chain(resolver)
        .chain(certificates)
        .collect();
    let read_execute = system
        .read_execute
        .iter()
        .map(PathBuf::from)
        .chain(run.curl.parent().map(Path::to_path_buf))
        .collect();
    Confinement {
        read: existing(read),
        read_execute: existing(read_execute),
        write: existing(
            [scratch.to_path_buf()]
                .into_iter()
                .chain(system.discard.iter().map(PathBuf::from))
                .collect(),
        ),
        devices: Vec::new(),
        network: true,
        limits: Limits::with_cpu_seconds(run.time_limit.as_secs().saturating_mul(2)),
        namespace: CONFINEMENT_NAMESPACE.to_owned(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_keeps_the_plain_pictures_it_left_and_nothing_else() {
        let root = uze_testkit::temp::scratch("program-pictures");
        let (scratch, kept) = (root.join("tmp"), root.join("kept"));
        std::fs::create_dir_all(scratch.join("deeper")).unwrap();
        std::fs::write(scratch.join("shot.png"), b"png").unwrap();
        std::fs::write(scratch.join("notes.txt"), b"text").unwrap();
        std::fs::write(scratch.join("deeper/inner.png"), b"png").unwrap();
        let big = std::fs::File::create(scratch.join("big.png")).unwrap();
        big.set_len(uze_contract::IMAGE_FILE_MAX as u64 + 1)
            .unwrap();
        // A link is never kept, where the platform can make one at all.
        let _ = uze_platform::fs::symlink(&scratch.join("shot.png"), &scratch.join("link.png"));

        keep_pictures(&scratch, &kept);

        let mut names: Vec<String> = std::fs::read_dir(&kept)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["shot.png"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_scratch_a_killed_run_left_is_reclaimed_once_stale() {
        let root = uze_testkit::temp::scratch("program-stale-scratch");
        let home = UzeHome::at(root.clone());
        let left = home.extension_scratch_dir("1-0");
        std::fs::create_dir_all(left.join(SCRATCH)).unwrap();
        std::fs::write(left.join(SCRATCH).join("partial"), b"x").unwrap();

        sweep_stale_scratch(&home, STALE_SCRATCH);
        assert!(left.exists(), "a run that may still be going was swept");
        sweep_stale_scratch(&home, std::time::Duration::ZERO);
        assert!(!left.exists(), "a stale scratch was left");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A program that runs for hours keeps its scratch however old it
    /// grows, and loses it once its process is gone.
    // Unix only: `sleep` stands for the long-running program.
    #[cfg(unix)]
    #[test]
    fn a_long_running_programs_scratch_lives_while_its_process_does() {
        use crate::application::Collectable as _;
        let root = uze_testkit::temp::scratch("program-scratch-by-pid");
        let home = UzeHome::at(root.clone());
        let mut program = std::process::Command::new("sleep")
            .arg("300")
            .spawn()
            .unwrap();
        let running = home.extension_scratch_dir(&format!("{}-0", program.id()));
        std::fs::create_dir_all(running.join(SCRATCH)).unwrap();
        let hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 60 * 60);
        std::fs::File::open(&running)
            .unwrap()
            .set_modified(hours_ago)
            .unwrap();

        ExtensionScratch.collect(&home);
        assert!(
            running.exists(),
            "the scratch of a running program was swept"
        );

        program.kill().unwrap();
        program.wait().unwrap();
        let collected = ExtensionScratch.collect(&home);
        assert!(!running.exists(), "a gone program's scratch was left");
        assert_eq!(collected.removed, [running]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A pid reused by another process keeps answering alive: past the
    /// cap, the run's scratch goes anyway.
    #[test]
    fn scratch_past_the_age_cap_goes_though_its_pid_answers_alive() {
        let root = uze_testkit::temp::scratch("program-scratch-age-cap");
        let home = UzeHome::at(root.clone());
        let reused = home.extension_scratch_dir("4242-0");
        std::fs::create_dir_all(reused.join(SCRATCH)).unwrap();
        let fresh = home.extension_scratch_dir("4242-1");
        std::fs::create_dir_all(fresh.join(SCRATCH)).unwrap();
        let past_the_cap =
            std::time::SystemTime::now() - SCRATCH_AGE_CAP - std::time::Duration::from_secs(60);
        uze_platform::fs::set_modified(&reused, past_the_cap).unwrap();

        let removed = sweep_scratch(&home, STALE_SCRATCH, |_| Some(true));

        assert_eq!(removed, std::slice::from_ref(&reused));
        assert!(!reused.exists());
        assert!(fresh.exists(), "a live run's scratch was swept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_extension_keeps_its_newest_pictures_and_no_more() {
        let root = uze_testkit::temp::scratch("program-pictures-bound");
        let dir = root.join("pictures");
        std::fs::create_dir_all(&dir).unwrap();
        let start = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        for index in 0..KEPT_PICTURES + 2 {
            let path = dir.join(format!("shot-{index:02}.png"));
            std::fs::write(&path, b"png").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(start + std::time::Duration::from_secs(index as u64))
                .unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"text").unwrap();

        let removed = bound_pictures(&dir, KEPT_PICTURES);

        assert_eq!(removed, [dir.join("shot-01.png"), dir.join("shot-00.png")]);
        assert!(dir.join("notes.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_picture_name_is_a_plain_png_jpeg_or_gif() {
        for name in ["a.png", "b.JPG", "c.jpeg", "d.gif"] {
            assert!(is_picture_name(name), "{name}");
        }
        for name in ["", ".hidden.png", "a/b.png", "a.txt", "png"] {
            assert!(!is_picture_name(name), "{name}");
        }
    }
}
