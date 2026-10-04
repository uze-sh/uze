//! Core-owned Git materialization.
//!
//! The mechanism is Git and nothing else: a URL is a URL, so no host is named
//! here and no host API is called. Everything runs through the `git`
//! executable with a deliberately hostile-input posture.
//!
//! Acquisition never executes package code. It also takes care not to let the
//! *repository* execute anything on its behalf, which is a separate problem:
//! a clone can carry hooks and submodule declarations, and Git will honour
//! configuration it finds unless told not to.

use crate::path::Canonical as _;
use std::{
    fs,
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};

use super::forge::{self, Access, Transport};
use crate::{
    error::{Result, UzeError},
    subprocess::{Seat, read_bounded, spawn_tree, wait_with_timeout},
};

/// SSH that neither waits on a prompt nor offers a key to a host the
/// operator never accepted. A passphrase or host-key question would stop
/// the process on the terminal it shares with UZE, and an unknown host
/// collecting the operator's public keys could identify them from a host
/// named in somebody else's `agents.yaml`.
const SSH_OPTIONS: &str = "-o BatchMode=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=15";

/// How this machine reaches the network, which a repository cannot
/// influence. libcurl reads `http_proxy` only in lowercase, the others in
/// both cases.
const NETWORK_ENVIRONMENT: &[&str] = &[
    "http_proxy",
    "https_proxy",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
];

/// Where an operation says what it is doing, for whoever shows a person.
pub const STEP: &str = "uze::step";

/// The operator's network settings, with their per-URL scopes.
const NETWORK_KEYS: &str = r"^http\.(.+\.)?(proxy|sslcainfo|sslcapath)$";

/// The operator's credential settings, with their per-URL scopes.
const CREDENTIAL_KEYS: &str = r"^credential\.";

/// Wall-clock budget for any single Git invocation. A remote that never
/// answers must fail rather than hang a `uze add` forever.
const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Per-stream cap for a Git invocation's captured output. Every caller in
/// this module only ever reads small plumbing output (a ref name, a commit
/// hash, a short failure line) — this exists purely as a defense-in-depth
/// backstop against a hostile or misbehaving remote, not a real limit any
/// legitimate invocation should approach.
const GIT_OUTPUT_CAP: usize = 8 * 1024 * 1024;

/// Upper bound on a materialized checkout. Crude on purpose: it exists so a
/// hostile or accidentally enormous repository cannot fill the disk, not to
/// enforce a package-size policy.
const MAX_MATERIALIZED_BYTES: u64 = 512 * 1024 * 1024;

/// Rejects a URL carrying inline credentials before it is ever used, logged
/// or persisted.
///
/// Storing a sanitized copy was the alternative and is worse: the secret
/// would still have existed in process memory, in argv, and in whatever Git
/// wrote to its own error output. Refusing the input keeps the secret from
/// entering UZE at all. Authenticated Git is a separate mechanism to design
/// deliberately, not a side effect of URL parsing.
///
/// Which userinfo is a secret depends on the transport, so this asks per
/// transport rather than refusing the `@` character:
///
/// - **`http`/`https`** — userinfo is only ever a credential there
///   (`user:token@host`, and a bare `token@host` for a forge that accepts
///   one), so any of it is refused.
/// - **SSH**, in either spelling (`ssh://git@host/repo`,
///   `git@host:org/repo`) — the userinfo is a *user name*, and the secret
///   is a key on disk that never appears in the URL. Refusing it made every
///   private repository unusable to protect against a secret that is not
///   there.
pub fn reject_inline_credentials(url: &str) -> Result<()> {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        // No scheme is `scp`-style, which is SSH.
        None => return Ok(()),
    };
    if !matches!(scheme, "http" | "https") {
        return Ok(());
    }
    // Only the authority carries userinfo, and only before the path.
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.contains('@') {
        return Err(UzeError::CredentialBearingUrl);
    }
    Ok(())
}

/// Refuses a value Git would read as an option instead of as the URL or
/// reference it is meant to be.
///
/// The input is not only operator-typed: `git:` comes out of a project's
/// checked-in `agents.yaml` and out of `agents.lock`, so cloning a repository
/// and running `uze install` is a delivery path. A leading `-` turns the URL
/// into `--upload-pack=<cmd>` (a command Git spawns), `--template=<dir>` (a
/// directory Git copies into the new repository) or `--separate-git-dir=<p>`
/// (a write outside the scratch directory). The end-of-options markers at
/// each call site are the second half of the same rule; this half is what
/// holds for `git checkout`, which offers no such marker, and it refuses
/// before any process is spawned.
pub(super) fn reject_option_shaped(value: &str, what: &str) -> Result<()> {
    if value.starts_with('-') {
        return Err(UzeError::AcquisitionFailed(format!(
            "{what} `{value}` starts with `-`, which git reads as an option"
        )));
    }
    Ok(())
}

/// Clones `url` into `destination` and checks out `reference`, returning the
/// resolved commit.
///
/// `destination` must not exist; the caller owns it and its cleanup.
/// What a checkout of a commit produced: the commit, and the symbolic links
/// its tree holds that the checkout could not make (see [`unmade_links`]).
pub struct Checkout {
    pub commit: String,
    pub links: crate::digest::Links,
}

pub fn materialize(url: &str, reference: Option<&str>, destination: &Path) -> Result<Checkout> {
    reject_inline_credentials(url)?;
    reject_option_shaped(url, "repository url")?;
    if let Some(reference) = reference {
        reject_option_shaped(reference, "reference")?;
    }

    // `--no-checkout` first, so nothing from the repository lands on disk
    // before the requested revision is chosen. Full history, not `--depth 1`:
    // a shallow clone cannot check out an arbitrary commit the caller pinned,
    // and correctness comes before the transfer saving.
    //
    // `--` is where `git clone [<options>] [--] <repo> [<dir>]` stops reading
    // options, so neither positional can be taken for one.
    through(url, None, |transport| {
        // A failed attempt may leave a partial clone behind, and the next
        // one needs the directory absent.
        let _ = fs::remove_dir_all(destination);
        run_as(
            Reach::of(transport),
            &[
                "clone",
                "--no-checkout",
                "--no-recurse-submodules",
                "--",
                &transport.url,
                &destination.to_string_lossy(),
            ],
            None,
        )?;
        holds_a_commit(destination, transport)
    })?;

    // Resolve to a commit *before* checking anything out. Detaching by SHA
    // removes every ambiguity a ref name carries — a branch that exists only
    // as a remote-tracking ref after clone, a tag and branch sharing a name,
    // or `HEAD` being read as a path — and it yields the resolved revision as
    // a side effect rather than as a second question.
    let commit = resolve_commit(reference, destination)?;
    // `git checkout` accepts no end-of-options marker (`--` there introduces
    // pathspecs), so the guard is the only thing standing between a value
    // Git printed and a value Git parses as an option.
    reject_option_shaped(&commit, "resolved commit")?;
    assert_tree_within_size_budget(Reach::LOCAL, destination, &commit, None)?;
    run(&["checkout", "--detach", &commit], Some(destination))?;

    // What the tree declared is what a checkout writes, so this only
    // answers for whatever the listing could not see.
    let checked_out = size_within(destination, MAX_MATERIALIZED_BYTES)?;
    let links = unmade_links(destination)?;
    stand_in_for_links(destination, &links, MAX_MATERIALIZED_BYTES - checked_out)?;

    // The repository's own metadata is not package content, and leaving it in
    // place would let a `.git` directory travel into the Store.
    let git_dir = destination.join(".git");
    if git_dir.exists() {
        fs::remove_dir_all(&git_dir).map_err(UzeError::write(git_dir))?;
    }
    Ok(Checkout { commit, links })
}

/// The symbolic links the checked-out tree holds that the checkout could not
/// make. Where links cannot be made (`core.symlinks=false`, Windows' case
/// for an ordinary account), Git writes each as a file holding its target;
/// its index still says it is a link (mode `120000`). Empty wherever the
/// checkout made them, since those are links on disk the digest reads.
fn unmade_links(checkout: &Path) -> Result<crate::digest::Links> {
    let listing = run(&["ls-files", "--stage", "-z"], Some(checkout))?;
    let mut links = crate::digest::Links::new();
    for record in listing.split('\0') {
        let Some((stage, path)) = record.split_once('\t') else {
            continue;
        };
        if !stage.starts_with("120000 ") {
            continue;
        }
        let on_disk = checkout.join(path);
        let metadata = fs::symlink_metadata(&on_disk).map_err(UzeError::read(&on_disk))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let target = fs::read_to_string(&on_disk).map_err(UzeError::read(&on_disk))?;
        links.insert(PathBuf::from(path), PathBuf::from(target));
    }
    Ok(links)
}

/// Puts a copy of each unmade link's target where the link would be, so a
/// harness reading the package finds what the link names rather than a file
/// holding its path; one that names nothing leaves the file Git wrote.
///
/// The copies are bytes the repository did not carry, so each is weighed
/// against what is left of the size budget before it is made. Refused, as
/// the repository is: a target outside the package, as a link is; one that
/// holds the link itself, which would copy a directory into itself without
/// end; one in Git's own metadata, which never travels into the Store; and
/// one holding another unmade link, whose copy would depend on the order
/// the two were made in.
fn stand_in_for_links(
    checkout: &Path,
    links: &crate::digest::Links,
    mut budget: u64,
) -> Result<()> {
    let refuse = |path: &Path, target: &Path, why: &str| {
        UzeError::AcquisitionFailed(format!(
            "the link `{}` (`{}`) {why}",
            path.display(),
            target.display()
        ))
    };
    let unmade: Vec<PathBuf> = links.keys().map(|path| checkout.join(path)).collect();
    for (path, target) in links {
        let link = checkout.join(path);
        let resolved = lexically_within(checkout, &link.parent().unwrap_or(checkout).join(target))
            .ok_or_else(|| refuse(path, target, "points outside the package"))?;
        if !resolved.exists() {
            continue;
        }
        if link.starts_with(&resolved) {
            return Err(refuse(path, target, "points at a directory that holds it"));
        }
        let in_git = resolved
            .strip_prefix(checkout)
            .is_ok_and(|inside| inside.components().any(|part| part.as_os_str() == ".git"))
            || checkout.join(".git").starts_with(&resolved);
        if in_git {
            return Err(refuse(path, target, "points at Git's own metadata"));
        }
        if unmade.iter().any(|other| other.starts_with(&resolved)) {
            return Err(refuse(path, target, "points at another link"));
        }
        budget = budget
            .checked_sub(size_within(&resolved, budget)?)
            .ok_or_else(over_budget)?;
        fs::remove_file(&link).map_err(UzeError::write(&link))?;
        if resolved.is_dir() {
            crate::store::copy_tree(&resolved, &link)?;
        } else {
            fs::copy(&resolved, &link).map_err(UzeError::write(&link))?;
        }
    }
    Ok(())
}

/// `path` with `.` and `..` resolved on paper, when it stays inside `root`.
fn lexically_within(root: &Path, path: &Path) -> Option<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.strip_prefix(root).ok()?.components() {
        match component {
            std::path::Component::Normal(part) => resolved.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !resolved.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(root.join(resolved))
}

/// Turns a request into an immutable commit.
///
/// `None` means the repository's own default branch: after a clone, `HEAD`
/// already points at whatever the remote chose, so it is read rather than
/// guessed. A hardcoded `main` or `master` would fail on a repository using
/// neither, which is exactly the assumption this avoids.
///
/// A named reference is tried as given first, then as a tag, then as a
/// remote-tracking branch — a plain `clone` creates a local branch only for
/// the default, so `feature` exists solely as `origin/feature`.
fn resolve_commit(reference: Option<&str>, checkout: &Path) -> Result<String> {
    let candidates: Vec<String> = match reference {
        // A `--no-checkout` clone creates no local branch, so the local
        // `HEAD` may point at an unborn ref. The remote's own default is read
        // from the tracking ref the clone did create, and if the remote never
        // advertised one, from its sole branch. Never from a guessed name.
        None => {
            let mut candidates = vec!["refs/remotes/origin/HEAD^{commit}".to_owned()];
            if let Some(single) = sole_remote_branch(checkout) {
                candidates.push(single);
            }
            candidates
        }
        Some(reference) => vec![
            format!("{reference}^{{commit}}"),
            format!("refs/tags/{reference}^{{commit}}"),
            format!("refs/remotes/origin/{reference}^{{commit}}"),
        ],
    };
    for candidate in &candidates {
        // `--end-of-options`, not `--`: in `rev-parse` the latter separates
        // revisions from *paths*, so it would stop the candidate being read
        // as a revision at all.
        if let Ok(output) = run(
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                candidate,
            ],
            Some(checkout),
        ) {
            let commit = output.trim().to_owned();
            if !commit.is_empty() {
                return Ok(commit);
            }
        }
    }
    Err(UzeError::AcquisitionFailed(format!(
        "no commit matches reference `{}`",
        reference.unwrap_or("HEAD")
    )))
}

/// The single remote branch, when a repository has exactly one.
///
/// Only consulted when the remote advertised no default. With more than one
/// branch and no advertised default there is no honest answer, so this
/// returns `None` and the caller reports that rather than picking.
fn sole_remote_branch(checkout: &Path) -> Option<String> {
    let listing = run(
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/remotes/origin/",
        ],
        Some(checkout),
    )
    .ok()?;
    let branches: Vec<&str> = listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "refs/remotes/origin/HEAD")
        .collect();
    match branches.as_slice() {
        [single] => Some(format!("{single}^{{commit}}")),
        _ => None,
    }
}

/// Refuses a checkout of `commit` — or of `subdirectory` within it — whose
/// files add up past [`MAX_MATERIALIZED_BYTES`], before a byte of it is
/// written: the tree already says how large every blob is.
///
/// `reach` is how a partial repository fetches a blob the listing has to
/// size, the same access its checkout would use.
pub(super) fn assert_tree_within_size_budget(
    reach: Reach,
    repository: &Path,
    commit: &str,
    subdirectory: Option<&str>,
) -> Result<()> {
    if tree_bytes(reach, repository, commit, subdirectory)? > MAX_MATERIALIZED_BYTES {
        return Err(UzeError::AcquisitionFailed(format!(
            "materialized repository exceeds {MAX_MATERIALIZED_BYTES} bytes"
        )));
    }
    Ok(())
}

/// The bytes a checkout of `commit`, or of `subdirectory` within it, would
/// write — counted only until they pass the budget, which is all the
/// answer needs.
///
/// Streamed: a tree listing grows with the file count, not the byte count,
/// so a repository well within budget can still list past any output cap.
fn tree_bytes(
    reach: Reach,
    repository: &Path,
    commit: &str,
    subdirectory: Option<&str>,
) -> Result<u64> {
    reject_option_shaped(commit, "commit")?;
    let mut arguments = vec!["--literal-pathspecs", "ls-tree", "-r", "-l", "-z", commit];
    if let Some(subdirectory) = subdirectory {
        arguments.extend(["--", subdirectory]);
    }
    run_records(
        reach,
        &arguments,
        Some(repository),
        0u64,
        |total, record| {
            *total = total.saturating_add(record_size(record));
            if *total > MAX_MATERIALIZED_BYTES {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
}

/// The size an `ls-tree -l` record declares — `<mode> <type> <object>
/// <size>\t<path>` — with `-` (what is no blob) counted as nothing.
fn record_size(record: &[u8]) -> u64 {
    let described = record
        .split(|byte| *byte == b'\t')
        .next()
        .unwrap_or_default();
    std::str::from_utf8(described)
        .ok()
        .and_then(|described| described.split_whitespace().nth(3))
        .and_then(|size| size.parse().ok())
        .unwrap_or(0)
}

pub(super) fn assert_within_size_budget(root: &Path) -> Result<()> {
    size_within(root, MAX_MATERIALIZED_BYTES).map(|_| ())
}

/// The bytes under `path` (a file or a directory), refused the moment they
/// pass `budget` rather than counted to the end. A link counts as itself and
/// is never followed: `skills/up -> ..` would otherwise be counted forever.
fn size_within(path: &Path, budget: u64) -> Result<u64> {
    fn total(path: &Path, accumulated: &mut u64, budget: u64) -> Result<()> {
        let metadata = fs::symlink_metadata(path).map_err(UzeError::read(path))?;
        if !metadata.is_dir() {
            *accumulated += metadata.len();
        } else {
            for entry in fs::read_dir(path).map_err(UzeError::read(path))? {
                let entry = entry.map_err(UzeError::read(path))?;
                total(&entry.path(), accumulated, budget)?;
            }
        }
        if *accumulated > budget {
            return Err(over_budget());
        }
        Ok(())
    }
    let mut accumulated = 0;
    total(path, &mut accumulated, budget)?;
    Ok(accumulated)
}

fn over_budget() -> UzeError {
    UzeError::AcquisitionFailed(format!(
        "materialized repository exceeds {MAX_MATERIALIZED_BYTES} bytes"
    ))
}

/// What one Git invocation may carry beyond the stripped environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Reach {
    access: Access,
    plain_http: bool,
}

impl Reach {
    /// A repository on this disk, or a question about one: nothing to
    /// authenticate and no network to reach.
    pub(super) const LOCAL: Self = Self {
        access: Access::Local,
        plain_http: false,
    };

    pub(super) fn of(transport: &Transport) -> Self {
        Self {
            access: transport.access,
            plain_http: transport.plain_http(),
        }
    }
}

/// Runs one `git` invocation under a deliberately minimal, non-interactive
/// environment.
///
/// Every flag here closes a way the repository or the ambient machine could
/// influence the run:
///
/// - a cleared environment keeping only `PATH` and what the platform cannot
///   run without (`uze_platform::process::clear_environment`): no inherited
///   Git configuration, no proxy or credential helper picked up from the
///   operator's shell.
/// - `GIT_CONFIG_NOSYSTEM` / `GIT_CONFIG_GLOBAL=/dev/null`: system and user
///   config cannot introduce a helper, alias or `filter` that runs a command.
/// - `core.hooksPath=/dev/null`: a repository's own hooks are never run.
/// - `GIT_TERMINAL_PROMPT=0` and empty `GIT_ASKPASS`: a private repository
///   fails immediately rather than blocking on a credential prompt.
/// - `protocol.file.allow=always`: needed so a local bare repository — the
///   only kind the deterministic tests use — remains reachable.
pub(super) fn run(arguments: &[&str], working_directory: Option<&Path>) -> Result<String> {
    run_as(Reach::LOCAL, arguments, working_directory)
}

/// [`run`], for an attempt that reaches the network.
///
/// Every attempt that leaves this machine also takes the operator's network
/// — proxy and certificate authority, from the environment and from their
/// Git config — because a company network is where a stripped environment
/// fails first. A credentialed one takes the operator's environment minus
/// every `GIT_*` variable, and their `credential.*` settings with the URL
/// scopes `gh auth setup-git` writes: a helper needs whatever its author
/// needed, and a whitelist would be a list of helpers that happen to work.
/// What stays excluded is *configuration* — aliases, filters, `includeIf`,
/// `core.sshCommand` — which is what the stripped environment is for.
///
/// Configuration is handed over through `GIT_CONFIG_COUNT`, never `-c`: an
/// inline helper can carry a secret, and argv is visible in `ps` and
/// recorded in the span below.
pub(super) fn run_as(
    reach: Reach,
    arguments: &[&str],
    working_directory: Option<&Path>,
) -> Result<String> {
    let span = tracing::info_span!(
        "acquisition.git",
        args = %arguments.join(" "),
        exit = tracing::field::Empty
    );
    let _entered = span.enter();
    let mut command = git_command(reach, arguments, working_directory);
    let (mut child, tree) = spawn_tree(&mut command, Seat::OwnGroup)
        .map_err(|error| UzeError::AcquisitionFailed(format!("could not run `git`: {error}")))?;
    let Some(mut stdout) = child.stdout.take() else {
        return Err(UzeError::AcquisitionFailed(
            "captured git stdout was not piped".to_owned(),
        ));
    };
    let Some(mut stderr) = child.stderr.take() else {
        return Err(UzeError::AcquisitionFailed(
            "captured git stderr was not piped".to_owned(),
        ));
    };
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let timed_out_error = || {
        UzeError::AcquisitionFailed(format!(
            "`git {}` timed out",
            arguments.first().copied().unwrap_or("command")
        ))
    };
    // Readers report through a channel rather than a bare `wait_with_output`
    // call: `git` itself can write more than the OS pipe buffer before
    // exiting (a large `for-each-ref`/`log` listing, a verbose failure), and
    // nothing was draining the pipes while `wait_with_timeout` below polls
    // — `git`'s own write() would then block on the full buffer, so it never
    // reaches exit, and the stall gets misreported as a timeout rather than
    // what it actually is: backpressure from an unread pipe.
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = stdout_tx.send(read_bounded(&mut stdout, GIT_OUTPUT_CAP));
    });
    thread::spawn(move || {
        let _ = stderr_tx.send(read_bounded(&mut stderr, GIT_OUTPUT_CAP));
    });
    let (status, timed_out) =
        wait_with_timeout(&mut child, &tree, COMMAND_TIMEOUT).map_err(|error| {
            UzeError::AcquisitionFailed(format!("could not wait for `git`: {error}"))
        })?;
    span.record("exit", status.code().unwrap_or(-1));
    if timed_out {
        tracing::warn!("git timed out");
        return Err(timed_out_error());
    }
    // `git` itself exited, but the same reasoning as above applies once more
    // if it forked a helper (a credential helper, a submodule hook despite
    // `core.hooksPath=/dev/null`) that inherited and still holds a pipe
    // open: bound the wait for the readers by what remains of the deadline
    // instead of joining unconditionally.
    let remaining = || deadline.saturating_duration_since(Instant::now());
    let (stdout_bytes, stdout_dropped) = match stdout_rx.recv_timeout(remaining()) {
        Ok(result) => result,
        Err(_) => {
            tree.end_survivors();
            return Err(timed_out_error());
        }
    };
    let (stderr_bytes, _) = match stderr_rx.recv_timeout(remaining()) {
        Ok(result) => result,
        Err(_) => {
            tree.end_survivors();
            return Err(timed_out_error());
        }
    };
    if !status.success() {
        return Err(failure(arguments, &stderr_bytes));
    }

    // `read_bounded` keeps the tail, and the tail of a listing read as the
    // whole of it is an answer that is wrong without looking wrong.
    if stdout_dropped > 0 {
        return Err(UzeError::AcquisitionFailed(format!(
            "`git {}` wrote more than {GIT_OUTPUT_CAP} bytes",
            arguments.first().copied().unwrap_or("command")
        )));
    }
    Ok(String::from_utf8_lossy(&stdout_bytes).into_owned())
}

/// The one `git` invocation [`run_as`] and [`run_records`] both spawn: the
/// stripped environment, the pushed configuration and piped output.
fn git_command(reach: Reach, arguments: &[&str], working_directory: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    match reach.access {
        Access::Local | Access::Anonymous => {
            uze_platform::process::clear_environment(&mut command);
        }
        Access::Credentialed => {
            without_git_environment(&mut command, &[]);
            // A helper that would open a browser or a dialog for a host a
            // project named is a login page somebody else chose.
            command.env("GCM_INTERACTIVE", "never");
        }
    }
    let protocols = if reach.plain_http {
        "file:https:ssh:git:http"
    } else {
        "file:https:ssh:git"
    };
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        // Git's own spelling of "no file", on every platform it runs on:
        // Git for Windows reads `/dev/null` as the null device and cannot
        // open `NUL` as a configuration file.
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("GIT_ALLOW_PROTOCOL", protocols);
    if reach.access != Access::Local {
        for key in NETWORK_ENVIRONMENT {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
    }
    let config = pushed_config(reach);
    command.env("GIT_CONFIG_COUNT", config.len().to_string());
    for (index, (key, value)) in config.iter().enumerate() {
        command
            .env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    command
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(["-c", "protocol.file.allow=always"])
        .args(["-c", "submodule.recurse=false"])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = working_directory {
        command.current_dir(directory);
    }
    command
}

/// What a failed invocation reports, from what it said on stderr.
///
/// Git echoes the URL it was given. A rejected credential-bearing URL never
/// reaches this far, but a redirect or an embedded token in some other
/// position still must not survive into an error a user pastes into an
/// issue.
fn failure(arguments: &[&str], stderr_bytes: &[u8]) -> UzeError {
    let complaint = String::from_utf8_lossy(stderr_bytes);
    let complaint = complaint.trim();
    if cannot_resolve_host(complaint) {
        return UzeError::RepositoryOffline {
            detail: redact(complaint),
        };
    }
    if refused_for_access(complaint) {
        return UzeError::RepositoryAccessRefused {
            detail: redact(complaint),
        };
    }
    UzeError::AcquisitionFailed(redact(&format!(
        "`git {}` failed: {}",
        arguments.first().copied().unwrap_or("command"),
        complaint
    )))
}

/// [`run_as`] for a `-z` listing that may be longer than any cap: each
/// NUL-terminated record is handed to `fold` as it arrives, and nothing but
/// `state` is kept. `fold` answering [`ControlFlow::Break`] stops the
/// reading — the pipe is dropped and `git` ends on its next write — and the
/// state so far is the answer, whatever `git` exits with.
fn run_records<T: Send + 'static>(
    reach: Reach,
    arguments: &[&str],
    working_directory: Option<&Path>,
    mut state: T,
    mut fold: impl FnMut(&mut T, &[u8]) -> ControlFlow<()> + Send + 'static,
) -> Result<T> {
    let span = tracing::info_span!(
        "acquisition.git",
        args = %arguments.join(" "),
        exit = tracing::field::Empty
    );
    let _entered = span.enter();
    let mut command = git_command(reach, arguments, working_directory);
    let (mut child, tree) = spawn_tree(&mut command, Seat::OwnGroup)
        .map_err(|error| UzeError::AcquisitionFailed(format!("could not run `git`: {error}")))?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(UzeError::AcquisitionFailed(
            "captured git output was not piped".to_owned(),
        ));
    };
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let timed_out_error = || {
        UzeError::AcquisitionFailed(format!(
            "`git {}` timed out",
            arguments.first().copied().unwrap_or("command")
        ))
    };
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        use std::io::Read;
        let mut buffer = [0u8; 64 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        let stopped = 'reading: loop {
            let read = match stdout.read(&mut buffer) {
                Ok(0) => break false,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break false,
            };
            pending.extend_from_slice(&buffer[..read]);
            let mut consumed = 0;
            while let Some(end) = pending[consumed..].iter().position(|byte| *byte == 0) {
                let record = &pending[consumed..consumed + end];
                consumed += end + 1;
                if fold(&mut state, record).is_break() {
                    break 'reading true;
                }
            }
            pending.drain(..consumed);
        };
        let stopped = stopped || (!pending.is_empty() && fold(&mut state, &pending).is_break());
        let _ = stdout_tx.send((state, stopped));
    });
    thread::spawn(move || {
        let _ = stderr_tx.send(read_bounded(&mut stderr, GIT_OUTPUT_CAP));
    });
    let (status, timed_out) =
        wait_with_timeout(&mut child, &tree, COMMAND_TIMEOUT).map_err(|error| {
            UzeError::AcquisitionFailed(format!("could not wait for `git`: {error}"))
        })?;
    span.record("exit", status.code().unwrap_or(-1));
    if timed_out {
        tracing::warn!("git timed out");
        return Err(timed_out_error());
    }
    let remaining = || deadline.saturating_duration_since(Instant::now());
    let Ok((state, stopped)) = stdout_rx.recv_timeout(remaining()) else {
        tree.end_survivors();
        return Err(timed_out_error());
    };
    if stopped {
        return Ok(state);
    }
    let Ok((stderr_bytes, _)) = stderr_rx.recv_timeout(remaining()) else {
        tree.end_survivors();
        return Err(timed_out_error());
    };
    if !status.success() {
        return Err(failure(arguments, &stderr_bytes));
    }
    Ok(state)
}

/// [`SSH_COMMAND`], sharing one connection per host for a minute when this
/// machine has a directory only this user can reach to keep its socket in.
///
/// An SSH handshake to a forge is a second or more of round trips, paid
/// again by every Git operation that opens one: a catalogue refresh over
/// three marketplaces on one host paid it three times. A master connection
/// kept for sixty seconds turns the second and later into a few
/// milliseconds. The socket is the operator's connection, so it lives where
/// nobody else can reach it — `$XDG_RUNTIME_DIR`, or a directory under the
/// temporary one that this user owns with no access for anybody else — and
/// when neither can be had, every operation opens its own.
fn ssh_command() -> String {
    let command = format!("{} {SSH_OPTIONS}", ssh_program());
    match multiplexing_directory() {
        Some(directory) => format!(
            "{command} -o ControlMaster=auto -o ControlPersist=60 -o \"ControlPath={}/%C\"",
            directory.display()
        ),
        None => command,
    }
}

/// The `ssh` the operator's `PATH` names, by its full path. Left as a bare
/// name, Git looks it up on a `PATH` of its own, and Git for Windows puts
/// the SSH it bundles first: one that never asks the Windows OpenSSH agent,
/// where a person's keys are loaded, so a key with a passphrase failed.
/// Forward slashes and quotes, as the shell Git runs the command in reads.
fn ssh_program() -> String {
    match uze_platform::executable::on_path("ssh") {
        Some(program) => format!("\"{}\"", program.display().to_string().replace('\\', "/")),
        None => "ssh".to_owned(),
    }
}

/// A private directory for the control sockets, with room for the
/// 40-character hash `%C` expands to.
fn multiplexing_directory() -> Option<PathBuf> {
    uze_platform::fs::user_socket_directory("uze-ssh", 42)
}

/// The configuration an attempt carries, beyond the stripped environment's.
fn pushed_config(reach: Reach) -> Vec<(String, String)> {
    let mut config = vec![("core.sshCommand".to_owned(), ssh_command())];
    // A package's bytes are what its commit holds on every machine, or the
    // digest a lock records on one is never reproduced on another.
    config.extend(
        uze_platform::git::FAITHFUL_CHECKOUT
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
    );
    if reach.access != Access::Local {
        config.extend(operator_config(NETWORK_KEYS));
    }
    if reach.access == Access::Credentialed {
        config.extend(operator_config(CREDENTIAL_KEYS));
        // After the operator's own, so no helper setting of theirs turns it
        // back on: a helper asks nobody anything for a host a project named.
        config.push(("credential.interactive".to_owned(), "false".to_owned()));
        // A redirect must not carry the helper's answer to another host.
        config.push(("http.followRedirects".to_owned(), "false".to_owned()));
    }
    config
}

/// Removes every `GIT_*` variable from `command`'s inherited environment,
/// except those named in `keep`.
fn without_git_environment(command: &mut Command, keep: &[&str]) {
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("GIT_") && !keep.contains(&name.as_ref()) {
            command.env_remove(key);
        }
    }
}

/// The operator's own Git settings whose keys match `pattern`, in the order
/// Git reads them, across every scope and every `[include]`.
///
/// Read from a directory that is no repository, so nothing local answers,
/// and with the operator's environment minus `GIT_*` — except the three
/// that say which files their own Git reads (`GIT_CONFIG_NOSYSTEM`,
/// `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`), because what is read here has
/// to be what their Git would use. A path under
/// `~/` is expanded here: the attempt it is handed to may have no `HOME`.
///
/// Read once per process for each home it is asked under: a listing reads a
/// mirror's manifest with the access the mirror was last reached by, and a
/// `git config` spawn per read is what a budgeted command cannot pay.
fn operator_config(pattern: &str) -> Vec<(String, String)> {
    type Settings = Vec<(String, String)>;
    type Asked = (String, Vec<Option<std::ffi::OsString>>);
    static READ: std::sync::Mutex<Vec<(Asked, Settings)>> = std::sync::Mutex::new(Vec::new());
    let key = (
        pattern.to_owned(),
        uze_platform::home::VARIABLES
            .iter()
            .chain(&[
                "XDG_CONFIG_HOME",
                "GIT_CONFIG_NOSYSTEM",
                "GIT_CONFIG_GLOBAL",
                "GIT_CONFIG_SYSTEM",
            ])
            .map(std::env::var_os)
            .collect(),
    );
    if let Some((_, settings)) = READ
        .lock()
        .ok()
        .and_then(|read| read.iter().find(|(known, _)| *known == key).cloned())
    {
        return settings;
    }
    let settings = read_operator_config(pattern);
    if let Ok(mut read) = READ.lock() {
        read.push((key, settings.clone()));
    }
    settings
}

fn read_operator_config(pattern: &str) -> Vec<(String, String)> {
    const WHICH_FILES: &[&str] = &[
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
    ];
    // A device as the repository, so no repository's own config can answer.
    // Not a directory: one at a fixed path under the shared temporary
    // directory is one another local user can plant a `config` in first,
    // and an explicit `GIT_DIR` is read without Git's ownership check.
    let no_repository = Path::new("/dev/null");
    let mut command = Command::new("git");
    without_git_environment(&mut command, WHICH_FILES);
    let Ok(output) = command
        .args(["config", "--includes", "-z", "--get-regexp", pattern])
        .env("GIT_DIR", no_repository)
        .current_dir("/")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let home = crate::user_home();
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
            let value = match (value.strip_prefix("~/"), &home) {
                (Some(rest), Some(home))
                    if key.ends_with(".sslcainfo") || key.ends_with(".sslcapath") =>
                {
                    home.join(rest).to_string_lossy().into_owned()
                }
                _ => value.to_owned(),
            };
            (key.to_owned(), value)
        })
        .collect()
}

/// Reaches the repository `url` names by each of its transports in turn,
/// starting from `remembered` when it is one of them, and answers with the
/// first that works and the transport that did.
///
/// The HTTPS attempts share a host, so when one of them cannot resolve or
/// cannot connect to it, the other is skipped rather than paid for again —
/// but SSH is still asked, because the operator's `~/.ssh/config` may name
/// a host (`github-work`) that DNS has never heard of. The repository is
/// offline only when no transport resolved it. A failure to write locally
/// is not a question for another transport and is answered at once. When
/// none works, one error names every transport and what each said. A
/// repository with a single transport answers exactly as it always did.
pub(super) fn through<T>(
    url: &str,
    remembered: Option<&Transport>,
    mut attempt: impl FnMut(&Transport) -> Result<T>,
) -> Result<(T, Transport)> {
    let mut transports = forge::transports(url)?;
    if let Some(remembered) = remembered
        && let Some(position) = transports.iter().position(|t| t == remembered)
    {
        let first = transports.remove(position);
        transports.insert(0, first);
    }
    let identity = forge::canonical(url);
    let shown = forge::shown(&identity);
    let reach = |transport: &Transport| {
        tracing::info!(
            target: STEP,
            step = "reach",
            repository = %shown,
            via = transport.label()
        );
    };
    if let [only] = transports.as_slice() {
        reach(only);
        return attempt(only).map(|answer| (answer, only.clone()));
    }
    let mut failures = Vec::new();
    let mut https_unreachable: Option<&'static str> = None;
    let (mut unresolved, mut refused) = (0, 0);
    for transport in transports {
        let over_https = transport.url.contains("://") && !transport.url.starts_with("ssh://");
        if over_https && let Some(why) = https_unreachable {
            failures.push(format!("  {}: skipped, {why}", transport.label()));
            unresolved += usize::from(why == UNRESOLVED);
            continue;
        }
        // With no helper and no `.netrc`, HTTPS "with credentials" carries
        // none, and would only ask the anonymous question a second time.
        if over_https && transport.access == Access::Credentialed && !holds_https_credentials() {
            failures.push(format!(
                "  {}: skipped, no credential helper is configured",
                transport.label()
            ));
            continue;
        }
        reach(&transport);
        match attempt(&transport) {
            Ok(answer) => return Ok((answer, transport)),
            Err(error @ (UzeError::Write { .. } | UzeError::Read { .. })) => return Err(error),
            Err(error) => {
                let offline = matches!(error, UzeError::RepositoryOffline { .. });
                unresolved += usize::from(offline);
                refused += usize::from(matches!(error, UzeError::RepositoryAccessRefused { .. }));
                if over_https && offline {
                    https_unreachable = Some(UNRESOLVED);
                } else if over_https && cannot_connect(&error.to_string()) {
                    https_unreachable = Some("the host did not answer over HTTPS");
                }
                let reason = reason_of(&error, &transport);
                tracing::info!(
                    target: STEP,
                    step = "reach_failed",
                    repository = %shown,
                    via = transport.label(),
                    reason = %reason
                );
                failures.push(format!("  {}: {reason}", transport.label()));
            }
        }
    }
    let detail = format!("{identity}\n{}", failures.join("\n"));
    if unresolved == failures.len() {
        return Err(UzeError::RepositoryOffline { detail });
    }
    if refused > 0 {
        return Err(UzeError::RepositoryAccessRefused { detail });
    }
    Err(UzeError::AcquisitionFailed(detail))
}

const UNRESOLVED: &str = "the host does not resolve";

/// Whether an authenticated HTTPS attempt could carry anything an anonymous
/// one does not: a credential helper in the operator's config, or a
/// `.netrc` curl would read.
fn holds_https_credentials() -> bool {
    let helper = operator_config(CREDENTIAL_KEYS)
        .iter()
        .any(|(key, value)| key.ends_with(".helper") && !value.is_empty());
    let netrc = crate::user_home()
        .is_some_and(|home| home.join(".netrc").is_file() || home.join("_netrc").is_file());
    helper || netrc
}

/// Whether an attempt failed before any HTTP was spoken — a port that
/// refuses or never answers, or TLS that did not complete — which the other
/// HTTPS attempt, on the same host and port, would meet again.
fn cannot_connect(complaint: &str) -> bool {
    const UNREACHABLE: &[&str] = &[
        "failed to connect",
        "couldn't connect",
        "connection refused",
        "connection timed out",
        "timed out",
        "ssl",
        "tls",
    ];
    let lowered = complaint.to_lowercase();
    UNREACHABLE.iter().any(|phrase| lowered.contains(phrase))
}

/// Whether what an attempt brought back is a repository at all.
///
/// A forge that answers a login page with status 200 is read by Git as a
/// dumb HTTP server holding nothing, and the clone "succeeds" empty. That
/// is not an answer, and the next transport must be asked.
pub(super) fn holds_a_commit(directory: &Path, transport: &Transport) -> Result<()> {
    let any = run(
        &["for-each-ref", "--count=1", "--format=%(objectname)"],
        Some(directory),
    )?;
    if any.trim().is_empty() {
        return Err(UzeError::AcquisitionFailed(format!(
            "{} answered with no repository",
            redact(&transport.url)
        )));
    }
    Ok(())
}

/// The one line of a failed attempt a person needs: Git's own complaint
/// rather than its advice, and how to accept an SSH host the operator has
/// never connected to.
fn reason_of(error: &UzeError, transport: &Transport) -> String {
    let message = match error {
        UzeError::RepositoryAccessRefused { detail } | UzeError::RepositoryOffline { detail } => {
            detail.clone()
        }
        other => other.to_string(),
    };
    // Git's own words, not its advice: the line that says what went wrong
    // rather than "Please make sure you have the correct access rights" or
    // "Cloning into …", which it prints around every failure alike.
    let lines: Vec<&str> = message.lines().map(str::trim).collect();
    let line = lines
        .iter()
        .find(|line| {
            let lowered = line.to_lowercase();
            [
                "denied",
                "fatal:",
                "error:",
                "timed out",
                "could not resolve",
            ]
            .iter()
            .any(|marker| lowered.contains(marker))
        })
        .or_else(|| lines.iter().rev().find(|line| !line.is_empty()))
        .copied()
        .unwrap_or("failed");
    let line = line.rsplit_once("fatal: ").map_or(line, |(_, rest)| rest);
    if line.to_lowercase().contains("host key verification failed")
        && let Some(destination) = forge::ssh_destination(&transport.url)
    {
        return format!("{line} (accept the host once with `ssh -T {destination}`)");
    }
    line.to_owned()
}

/// Whether Git's complaint is about *reaching* the repository rather than
/// about what is in it.
///
/// Matched on Git's own words, which is a real cost: they are not a stable
/// interface and a translated Git says something else. The alternative is
/// worse — a private marketplace surfacing four lines of Git's advice to
/// somebody who only needs to know which marketplace it was and that it is
/// a question of access. A phrase that stops matching degrades to the
/// ordinary message, never to a wrong one.
fn refused_for_access(complaint: &str) -> bool {
    const ACCESS: &[&str] = &[
        "could not read from remote repository",
        "authentication failed",
        "permission denied",
        "repository not found",
        "access denied",
        "please make sure you have the correct access rights",
        "terminal prompts disabled",
    ];
    let lowered = complaint.to_lowercase();
    ACCESS.iter().any(|phrase| lowered.contains(phrase))
}

/// Whether Git's complaint is that the host has no address from here —
/// the one failure no other transport can do better on. Checked before
/// [`refused_for_access`], because `ssh` follows a resolution failure with
/// the same "could not read from remote repository" it prints for a
/// refused key.
fn cannot_resolve_host(complaint: &str) -> bool {
    const UNRESOLVED: &[&str] = &[
        "could not resolve host",
        "could not resolve hostname",
        "name or service not known",
        "temporary failure in name resolution",
        "nodename nor servname provided",
    ];
    let lowered = complaint.to_lowercase();
    UNRESOLVED.iter().any(|phrase| lowered.contains(phrase))
}

/// Replaces anything shaped like inline credentials in a message.
pub fn redact(message: &str) -> String {
    let mut result = String::with_capacity(message.len());
    for token in message.split_inclusive(char::is_whitespace) {
        match (token.find("://"), token.find('@')) {
            (Some(scheme), Some(at)) if at > scheme => {
                result.push_str(&token[..scheme + 3]);
                result.push_str("<redacted>@");
                result.push_str(&token[at + 1..]);
            }
            _ => result.push_str(token),
        }
    }
    result
}

/// Resolves a package root inside a materialized checkout.
///
/// Validated both lexically and physically: the lexical check rejects a
/// traversal before touching the filesystem, and the physical check catches a
/// path that only escapes once symlinks are followed.
pub fn resolve_subdirectory(root: &Path, subdirectory: &Path) -> Result<PathBuf> {
    if crate::path::is_anchored(subdirectory)
        || subdirectory
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(UzeError::PackageEscapesRoot {
            link: subdirectory.to_path_buf(),
            target: subdirectory.to_path_buf(),
        });
    }
    let candidate = root.join(subdirectory);
    if !candidate.is_dir() {
        return Err(UzeError::MissingPath(candidate));
    }
    let resolved = candidate.canonical().map_err(UzeError::read(&candidate))?;
    let root = root.canonical().map_err(UzeError::read(root))?;
    if !resolved.starts_with(&root) {
        return Err(UzeError::PackageEscapesRoot {
            link: candidate,
            target: resolved,
        });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree of files, written where the checkout would have left them.
    fn checkout_with(label: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = uze_testkit::temp::scratch(label);
        for (path, contents) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        root
    }

    fn links(pairs: &[(&str, &str)]) -> crate::digest::Links {
        pairs
            .iter()
            .map(|(path, target)| (PathBuf::from(path), PathBuf::from(target)))
            .collect()
    }

    #[test]
    fn an_unmade_link_is_replaced_by_what_it_names() {
        let root = checkout_with(
            "stand-in-copied",
            &[("skills/a/SKILL.md", "body"), ("skills/b", "a")],
        );
        stand_in_for_links(&root, &links(&[("skills/b", "a")]), u64::MAX).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("skills/b/SKILL.md")).unwrap(),
            "body"
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// Copying a directory into itself never ends: the disk fills, or the
    /// stack runs out, during an install of somebody else's plugin.
    #[test]
    fn a_link_to_a_directory_holding_it_is_refused() {
        for target in ["..", "."] {
            let root = checkout_with("stand-in-self", &[("a/l", target), ("a/f", "x")]);
            let error =
                stand_in_for_links(&root, &links(&[("a/l", target)]), u64::MAX).expect_err(target);
            assert!(error.to_string().contains("holds it"), "{error}");
            assert!(
                !root.join("a/l/a").exists(),
                "nothing was copied for {target}"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn a_link_into_git_s_metadata_is_refused() {
        let root = checkout_with(
            "stand-in-git",
            &[(".git/config", "[core]"), ("l", ".git/config")],
        );
        let error = stand_in_for_links(&root, &links(&[("l", ".git/config")]), u64::MAX)
            .expect_err("git metadata");
        assert!(error.to_string().contains("Git's own metadata"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_link_to_a_directory_holding_another_link_is_refused() {
        let root = checkout_with(
            "stand-in-chained",
            &[("a/inner", "../c"), ("c/f", "x"), ("b", "a")],
        );
        let error = stand_in_for_links(&root, &links(&[("a/inner", "../c"), ("b", "a")]), u64::MAX)
            .expect_err("chained");
        assert!(error.to_string().contains("another link"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    /// Weighed before it is written: a copy that would pass the budget
    /// leaves the checkout as it was.
    #[test]
    fn a_copy_past_the_budget_is_refused_before_it_is_made() {
        let root = checkout_with("stand-in-budget", &[("a/f", "0123456789"), ("b", "a")]);
        let error = stand_in_for_links(&root, &links(&[("b", "a")]), 5).expect_err("budget");
        assert!(error.to_string().contains("exceeds"), "{error}");
        assert_eq!(
            fs::read_to_string(root.join("b")).unwrap(),
            "a",
            "the file Git wrote stays"
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// The shared temporary directory is writable by every local user, so a
    /// repository planted where the old reader pointed `GIT_DIR` must not
    /// be able to name a credential helper for the operator.
    #[test]
    fn a_repository_planted_in_the_shared_temporary_directory_is_never_read() {
        let planted = std::env::temp_dir().join("uze-no-repository");
        if fs::create_dir_all(planted.join("objects")).is_err()
            || fs::create_dir_all(planted.join("refs")).is_err()
            || fs::write(planted.join("HEAD"), "ref: refs/heads/main\n").is_err()
            || fs::write(
                planted.join("config"),
                "[credential]\n\thelper = !planted-by-another-user\n",
            )
            .is_err()
        {
            return;
        }

        let credentials = read_operator_config(CREDENTIAL_KEYS);
        let _ = fs::remove_dir_all(&planted);

        assert!(
            credentials
                .iter()
                .all(|(_, value)| !value.contains("planted-by-another-user")),
            "{credentials:?}"
        );
    }

    /// A certificate authority the operator names under `~/` still reaches
    /// an attempt that has no `HOME`, and a credential helper keeps the URL
    /// scope `gh auth setup-git` writes it under.
    #[test]
    fn the_operators_network_and_credentials_are_read_with_their_scopes() {
        let home = uze_testkit::temp::scratch("operator-config");
        fs::write(
            home.join(".gitconfig"),
            "[http]\n\tsslCAInfo = ~/ca.pem\n\
             [http \"https://git.acme.io\"]\n\tproxy = http://proxy.acme.io:3128\n\
             [credential \"https://github.com\"]\n\thelper = !gh auth git-credential\n\
             [alias]\n\tco = checkout\n",
        )
        .unwrap();
        let mut environment = uze_testkit::env::scope();
        environment
            .home(&home)
            .set("XDG_CONFIG_HOME", home.join("xdg"))
            .set("GIT_CONFIG_NOSYSTEM", "1")
            .remove("GIT_CONFIG_GLOBAL")
            .remove("GIT_CONFIG_SYSTEM");

        let network = operator_config(NETWORK_KEYS);
        assert!(
            network.contains(&(
                "http.sslcainfo".to_owned(),
                home.join("ca.pem").to_string_lossy().into_owned()
            )),
            "{network:?}"
        );
        assert!(
            network.contains(&(
                "http.https://git.acme.io.proxy".to_owned(),
                "http://proxy.acme.io:3128".to_owned()
            )),
            "{network:?}"
        );
        let credentials = operator_config(CREDENTIAL_KEYS);
        assert_eq!(
            credentials,
            vec![(
                "credential.https://github.com.helper".to_owned(),
                "!gh auth git-credential".to_owned()
            )]
        );
        drop(environment);
        let _ = fs::remove_dir_all(home);
    }

    /// A helper is never allowed to ask anybody anything for a host a
    /// project named, whatever the operator configured.
    #[test]
    fn a_credentialed_attempt_never_lets_a_helper_prompt() {
        let home = uze_testkit::temp::scratch("no-interactive-helper");
        fs::write(
            home.join(".gitconfig"),
            "[credential]\n\thelper = manager\n\tinteractive = always\n",
        )
        .unwrap();
        let mut environment = uze_testkit::env::scope();
        environment
            .home(&home)
            .set("XDG_CONFIG_HOME", home.join("xdg"))
            .set("GIT_CONFIG_NOSYSTEM", "1")
            .remove("GIT_CONFIG_GLOBAL");
        let transport = Transport {
            url: "https://example.invalid/x".to_owned(),
            access: Access::Credentialed,
        };

        let config = pushed_config(Reach::of(&transport));

        let interactive: Vec<_> = config
            .iter()
            .filter(|(key, _)| key == "credential.interactive")
            .collect();
        assert_eq!(
            interactive.last().map(|(_, value)| value.as_str()),
            Some("false"),
            "{config:?}"
        );
        drop(environment);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn inline_credentials_are_rejected_rather_than_sanitized() {
        for url in [
            "https://user:secret@example.com/repo.git",
            "https://token@example.com/repo.git",
            "http://token@example.com/repo.git",
        ] {
            assert!(
                matches!(
                    reject_inline_credentials(url),
                    Err(UzeError::CredentialBearingUrl)
                ),
                "accepted {url}"
            );
        }
    }

    /// Over SSH the userinfo is a user name and the secret is a key on
    /// disk. Refusing `git@` there protected nothing and made every
    /// private repository unreachable.
    #[test]
    fn an_ssh_user_name_is_not_a_credential() {
        for url in [
            "git@example.com:org/repo.git",
            "ssh://git@example.com/org/repo.git",
            "ssh://example.com/org/repo.git",
        ] {
            assert!(reject_inline_credentials(url).is_ok(), "refused {url}");
        }
    }

    #[test]
    fn an_ordinary_url_is_accepted() {
        for url in [
            "https://example.com/org/repo.git",
            "file:///srv/repos/repo.git",
            "https://example.com/org/repo@weird.git",
        ] {
            assert!(reject_inline_credentials(url).is_ok(), "rejected {url}");
        }
    }

    #[test]
    fn redaction_removes_userinfo_from_a_message() {
        let redacted = redact("failed to clone https://user:secret@example.com/repo.git now");
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("<redacted>@example.com/repo.git"));
    }

    /// A `git:` line is project input, and a URL beginning with `-` is an
    /// option to Git — `--upload-pack=<cmd>` being the one that runs a
    /// command. The refusal has to happen before `git` is spawned, which is
    /// what a destination that is never created proves.
    #[test]
    fn an_option_shaped_url_or_reference_is_refused_before_git_is_spawned() {
        let root = uze_testkit::temp::scratch("option-shaped-url");
        let destination = root.join("checkout");

        for url in [
            "--upload-pack=touch /tmp/uze-injection",
            "--template=/tmp/uze-template",
            "-c",
        ] {
            assert!(
                matches!(
                    materialize(url, None, &destination),
                    Err(UzeError::AcquisitionFailed(_))
                ),
                "accepted {url}"
            );
            assert!(!destination.exists(), "{url} reached git");
        }

        assert!(
            matches!(
                materialize(
                    "file:///srv/repo.git",
                    Some("--output=/tmp/x"),
                    &destination
                ),
                Err(UzeError::AcquisitionFailed(_))
            ),
            "accepted an option-shaped reference"
        );
        assert!(
            !destination.exists(),
            "an option-shaped reference reached git"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_traversing_subdirectory_is_rejected_before_touching_the_filesystem() {
        let root = Path::new("/nonexistent");
        for subdirectory in ["/etc", "../escape", "packages/../../escape"] {
            assert!(
                matches!(
                    resolve_subdirectory(root, Path::new(subdirectory)),
                    Err(UzeError::PackageEscapesRoot { .. })
                ),
                "accepted {subdirectory}"
            );
        }
    }

    /// What the size budget is judged on, read off the tree before a
    /// checkout writes it — and a directory named `a*b` is that directory,
    /// never a pattern that also takes `ab` with it. NTFS holds no `*` in a
    /// name, so the fixture is a Unix one.
    #[cfg(unix)]
    #[test]
    fn a_tree_is_measured_literally_before_it_is_checked_out() {
        let _env = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("tree-bytes");
        fs::create_dir_all(root.join("plugins/a*b")).unwrap();
        fs::create_dir_all(root.join("plugins/ab")).unwrap();
        fs::write(root.join("plugins/a*b/x"), [0; 3]).unwrap();
        fs::write(root.join("plugins/ab/y"), [0; 5000]).unwrap();
        fs::write(root.join("top"), [0; 10]).unwrap();
        let git = |arguments: &[&str]| run(arguments, Some(&root)).unwrap();
        git(&["init", "--quiet"]);
        git(&["config", "user.email", "t@example.invalid"]);
        git(&["config", "user.name", "Test"]);
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "sizes"]);
        let commit = git(&["rev-parse", "HEAD"]).trim().to_owned();

        let measured = |subdirectory| tree_bytes(Reach::LOCAL, &root, &commit, subdirectory);
        assert_eq!(measured(None).unwrap(), 5013);
        assert_eq!(measured(Some("plugins/a*b")).unwrap(), 3);
        assert!(assert_tree_within_size_budget(Reach::LOCAL, &root, &commit, None).is_ok());

        let _ = fs::remove_dir_all(root);
    }

    /// A tree of `entries` names, each `name_length` long, every one of them
    /// the same blob of `blob_size` bytes — so the listing and the declared
    /// size can be as large as a test needs without writing either.
    fn repeated_tree(root: &Path, blob_size: usize, entries: usize, name_length: usize) -> String {
        use std::io::Write;
        fs::write(root.join("blob"), vec![b'x'; blob_size]).unwrap();
        let blob = run(&["hash-object", "-w", "blob"], Some(root)).unwrap();
        let mut listing = String::new();
        for index in 0..entries {
            listing.push_str(&format!(
                "100644 blob {}\t{index:0>name_length$}\n",
                blob.trim()
            ));
        }
        let mut mktree = Command::new("git")
            .arg("mktree")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        mktree
            .stdin
            .take()
            .unwrap()
            .write_all(listing.as_bytes())
            .unwrap();
        let output = mktree.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// A listing grows with the file count, so one past the output cap
    /// says nothing about the bytes: it is measured all the same.
    #[test]
    fn a_listing_longer_than_the_output_cap_is_still_measured() {
        let _env = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("tree-bytes-long");
        fs::create_dir_all(&root).unwrap();
        run(&["init", "--quiet"], Some(&root)).unwrap();
        let entries = 40_000;
        let tree = repeated_tree(&root, 1, entries, 200);

        assert_eq!(
            tree_bytes(Reach::LOCAL, &root, &tree, None).unwrap(),
            entries as u64
        );
        let listed = run(&["ls-tree", "-r", "-l", "-z", &tree], Some(&root));
        assert!(
            listed.is_err(),
            "the fixture must list past the output cap to prove anything"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_tree_declaring_more_than_the_budget_is_refused_before_checkout() {
        let _env = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("tree-bytes-over");
        fs::create_dir_all(&root).unwrap();
        run(&["init", "--quiet"], Some(&root)).unwrap();
        let mebibyte = 1024 * 1024;
        let entries = (MAX_MATERIALIZED_BYTES / mebibyte) as usize + 1;
        let tree = repeated_tree(&root, mebibyte as usize, entries, 8);

        assert!(matches!(
            assert_tree_within_size_budget(Reach::LOCAL, &root, &tree, None),
            Err(UzeError::AcquisitionFailed(_))
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn output_past_the_cap_is_refused_rather_than_read_as_its_tail() {
        let _env = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("output-cap");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("large"), vec![b'x'; GIT_OUTPUT_CAP + 1]).unwrap();
        run(&["init", "--quiet"], Some(&root)).unwrap();
        let blob = run(&["hash-object", "-w", "large"], Some(&root)).unwrap();

        assert!(matches!(
            run(&["cat-file", "-p", blob.trim()], Some(&root)),
            Err(UzeError::AcquisitionFailed(_))
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_drains_output_larger_than_the_pipe_buffer_instead_of_deadlocking() {
        // Regression test: `wait_with_timeout`'s poll loop does not itself
        // read the child's stdout/stderr, so if nothing drains those pipes
        // concurrently, `git` blocks on its own `write()` once a ~64KB OS
        // pipe buffer fills — it never reaches exit, and the stall gets
        // misreported as a timeout. Forcing `for-each-ref` output well past
        // that size reproduces the exact shape of hang this was fixed for.
        //
        // This test resolves `git` by name (via `PATH`), unlike its
        // absolute-path siblings elsewhere in this crate's suite, so it
        // must serialize against `harness_runtime`'s tests, which
        // temporarily narrow process-global `PATH` — otherwise this can
        // race into a spurious `NotFound` with no connection visible from
        // either test's own code.
        let _env = uze_testkit::env::scope();
        let root = uze_testkit::temp::scratch("large-output");
        fs::create_dir_all(&root).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(["commit", "-q", "--allow-empty", "-m", "root"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let commit = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&root)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let commit = commit.trim();
        // Written directly rather than via 5000 individual `git tag` calls:
        // this is setup for the test, not the behavior under test.
        let mut packed_refs = "# pack-refs with: peeled fully-peeled sorted\n".to_owned();
        for index in 0..5000 {
            packed_refs.push_str(&format!("{commit} refs/tags/tag-{index:05}\n"));
        }
        fs::write(root.join(".git/packed-refs"), packed_refs).unwrap();

        let started = Instant::now();
        let listing = run(
            &["for-each-ref", "--format=%(refname)", "refs/tags/"],
            Some(&root),
        )
        .expect("a large but well-formed listing must not be mistaken for a hang");
        assert!(
            started.elapsed() < COMMAND_TIMEOUT,
            "must finish well inside the timeout, not be saved only by eventually hitting it"
        );
        assert_eq!(listing.lines().count(), 5000, "no line may be lost");
        assert!(listing.contains("refs/tags/tag-04999"));

        let _ = fs::remove_dir_all(root);
    }
}
