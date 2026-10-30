//! Keeping the binary the installer placed current, and saying so.
//!
//! Only a binary `install.sh` placed is ever replaced. The installer leaves
//! a receipt (`state/install.json`) naming the file it wrote, and a binary
//! running from anywhere else — `cargo install`, `make install`, a package
//! manager, `target/debug` — belongs to whatever put it there, and is left
//! to it: `uze upgrade` says why when asked. That receipt is
//! what ADR-034 was guarding when it kept a self-update out of scope; a
//! replacement that cannot tell whose file it is replacing is the thing to
//! refuse, and the receipt answers the question.
//!
//! The replacement is a rename over the old file, so a process already
//! running from it — the terminal server, a pane's shim, this very client —
//! keeps the inode it started from, and the next launch is the first to run
//! the new release. Nothing here restarts anything.
//!
//! `uze upgrade` is the same replacement asked for by a person: it runs in
//! the foreground, and when it replaces nothing it says why — the receipt
//! missing, or naming a different file than the `uze` that is running.
//!
//! `UZE_AUTOUPDATE=off` stops the check entirely, `notify` checks without
//! ever replacing — leaving it to `uze upgrade` — and `CI` being set means off unless the variable says
//! otherwise: a disposable machine has no use for a newer binary than the
//! one it was handed.
//!
//! `UZE_BASE_URL` is `install.sh`'s testing override and stops there. A
//! shipped `uze` fetches from `RELEASES` and nowhere else, because this
//! module downloads a binary and renames it over the one in `PATH`: were
//! the download root an environment variable, anything that can set one —
//! a cloned repository's `.envrc`, a `Makefile`, a parent process — would
//! choose which binary a person runs from then on, and the checksum could
//! not tell, since `SHASUMS256.txt` comes from that same root. The
//! integrity check proves the bytes arrived whole from the release page;
//! only the fixed origin makes that page the right one. A debug build
//! still honours the variable so the offline fixture suite can drive a
//! whole pass without a network — a developer's own build is not the
//! threat this closes.
//!
//! The fixed origin is not the whole answer either: whoever can publish to
//! that page — a stolen token, a compromised workflow — publishes the
//! checksums too. So `SHASUMS256.txt` carries a signature,
//! `SHASUMS256.txt.sig`, made with a key that never sits in CI unguarded,
//! and nothing is unpacked until `ssh-keygen -Y verify` (the system's own,
//! by path) accepts it against `RELEASE_KEY`, the public key this binary
//! was built with. No signature, a bad one, or no `ssh-keygen` all end the
//! same way: nothing is installed.
//!
//! A release is installed in the background as soon as a check finds it,
//! unless `UZE_RELEASE_CHANNEL` is `stable`: then only once it has been the
//! latest for `SETTLE`, so a release found to be bad and pulled within
//! that window never reaches a machine that was only waiting. The window is
//! counted from when the release was published, and one that saw it out
//! before a newer one replaced it is still installed while that one waits:
//! counting from when a machine first heard, and forgetting the release a
//! newer one replaced, left a machine that ran nothing for a day a release
//! behind. `uze upgrade` is a person asking, and does not wait.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest as _, Sha256};
use uze_application::UzeHome;

/// Where releases are published, and where a notice's link points.
const RELEASES: &str = "https://github.com/uze-sh/uze/releases";

/// Where a CLI command points the reader after an update: the site's
/// changelog, which opens on the newest release.
const CHANGELOG: &str = "https://uze.sh/docs/changelog";

/// Where a release's own `CHANGELOG.md` is read from: the file at its tag.
/// A constant for the same reason [`RELEASES`] is one, although what comes
/// from here is only ever shown.
const SOURCES: &str = "https://raw.githubusercontent.com/uze-sh/uze";

/// How long an answer about the latest release is trusted. Every client
/// that opens runs the check, and this is what keeps a second terminal from
/// asking again a minute after the first one did.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// How often an open client looks at the answer's age. Far shorter than
/// [`CHECK_EVERY`] because a client stays open for days: waking hourly
/// from whenever it started left an hour-old answer standing for up to
/// another hour. Looking costs a file read; only an answer past
/// [`CHECK_EVERY`] reaches the network.
pub(crate) const WAKE_EVERY: Duration = Duration::from_secs(15 * 60);

const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// How long a release must have been the latest before the background
/// pass installs it, on the [`Channel::Stable`] channel.
const SETTLE: Duration = Duration::from_secs(24 * 60 * 60);

/// The committed public key releases are signed with, as built into this
/// binary. `release-signing.pub` holds it; until it holds a key, every
/// release is refused.
const RELEASE_KEY: &str = include_str!("../release-signing.pub");

/// What a release signature is made for: `ssh-keygen -Y sign -n` binds a
/// signature to it, so one the same key made for anything else (a commit,
/// a file) is not a release signature.
const SIGNATURE_NAMESPACE: &str = "uze-release";

/// What the sidebar says about releases: that a newer release replaced
/// this binary on disk and the next launch runs it. It is the only thing
/// said. A release that is merely available is news the reader can do
/// nothing with while the updater is doing it for them, and one the updater
/// cannot install is `uze upgrade`'s to explain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Notice(pub(crate) String);

impl Notice {
    pub(crate) fn version(&self) -> &str {
        &self.0
    }

    /// What the row says. Clicking it opens the release's notes.
    pub(crate) fn action(&self) -> &'static str {
        "restart to use it"
    }
}

/// The release this binary is.
pub(crate) fn running() -> &'static str {
    RUNNING
}

/// The release's own page, where its notes are published.
pub(crate) fn release_page(version: &str) -> String {
    format!("{RELEASES}/tag/v{version}")
}

/// The notice as it stands, and a counter that moves whenever it does —
/// so a client can ask "anything new since?" every tick for the price of
/// one atomic load.
static NOTICE: Mutex<Option<Notice>> = Mutex::new(None);
static REVISION: AtomicU64 = AtomicU64::new(0);

/// The notice, when it changed since `seen`.
pub(crate) fn since(seen: u64) -> Option<(u64, Option<Notice>)> {
    let revision = REVISION.load(Ordering::Acquire);
    (revision != seen).then(|| {
        let notice = NOTICE.lock().map(|notice| notice.clone()).unwrap_or(None);
        (revision, notice)
    })
}

fn publish(notice: Option<Notice>) {
    if let Ok(mut current) = NOTICE.lock() {
        if *current == notice {
            return;
        }
        *current = notice;
    }
    REVISION.fetch_add(1, Ordering::AcqRel);
}

/// Starts the check for this process: now, and on every wake after, about
/// every [`WAKE_EVERY`], on a thread of its own. A wake asks the network
/// only once the answer is [`CHECK_EVERY`] old. Nothing a client draws
/// waits on it.
pub(crate) fn watch(home: UzeHome) {
    let policy = Policy::current();
    if policy == Policy::Off {
        return;
    }
    // Resolved once, before anything is replaced: on Linux a running
    // binary whose file was renamed over reports itself as `… (deleted)`,
    // and every pass after the first install would otherwise decide this
    // process was never the installer's.
    let this = this_binary();
    let releases = Published::current();
    let channel = Channel::current();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        loop {
            let notice =
                tracing::debug_span!(parent: &parent, "self_update.watch").in_scope(|| {
                    pass(
                        &home,
                        policy,
                        channel,
                        this.as_deref(),
                        &releases,
                        unix_now(),
                        false,
                    )
                });
            publish(notice);
            thread::sleep(jittered(WAKE_EVERY));
        }
    });
}

/// The check a CLI command handed off, run to the end in the process it
/// was handed to — `uze upgrade --background`, which nobody types.
pub fn check_now(home: &UzeHome) {
    let policy = Policy::current();
    if policy != Policy::Off {
        let this = this_binary();
        pass(
            home,
            policy,
            Channel::current(),
            this.as_deref(),
            &Published::current(),
            unix_now(),
            true,
        );
    }
}

/// The release a CLI command says it was updated to on its way out, if
/// any.
///
/// Nothing here touches the network: a command is budgeted in
/// milliseconds, and a download cannot outlive the process that started it
/// on a thread. When the last answer has gone stale the check is handed to
/// a detached `uze upgrade --background` instead, and what it finds is what the
/// *next* command says — the same trade `gh` and npm's notifier make.
pub fn after_command(home: &UzeHome) -> Option<String> {
    if Policy::current() == Policy::Off {
        return None;
    }
    let now = unix_now();
    let ledger = read_json::<Ledger>(&ledger_path(home)).unwrap_or_default();
    if now.saturating_sub(ledger.checked_at) >= CHECK_EVERY.as_secs() {
        // Claimed before it is handed off, so two commands a second apart
        // start one check rather than two.
        amend_ledger(home, |stored| stored.checked_at = now);
        hand_off_check();
    }
    updated_to(home, ledger)
}

/// Runs the installed release in this process's place, as a workspace
/// client landing where this one left off. Returns only when it could not
/// be started, saying why.
///
/// The file the installer's receipt names, not this process's own image:
/// that is the release the notice announced, and on Linux a binary renamed
/// over no longer has the path it ran from. Through
/// `uze_platform::process::run_in_place`: `exec` on Unix, and on Windows a
/// child on this console, waited for, whose exit is this process's.
pub fn restart(home: &UzeHome) -> String {
    let binary = read_json::<Receipt>(&receipt_path(home))
        .map(|receipt| receipt.binary)
        .or_else(|| env::current_exe().ok());
    let Some(binary) = binary else {
        return "the installed release could not be found".to_owned();
    };
    let mut command = Command::new(&binary);
    command.args(["workspace", "--resumed"]);
    let error = uze_platform::process::run_in_place(&mut command);
    format!("{} could not be started: {error}", binary.display())
}

/// The site's changelog, where a command points after an update.
pub fn changelog() -> &'static str {
    CHANGELOG
}

/// A command is a process of its own, so the one after an update is
/// already the new release: what it says is that it was updated, once.
fn updated_to(home: &UzeHome, ledger: Ledger) -> Option<String> {
    let version = ledger.installed.filter(|installed| installed == RUNNING)?;
    if ledger.told.as_deref() == Some(version.as_str()) {
        return None;
    }
    amend_ledger(home, |stored| stored.told = Some(version.clone()));
    Some(version)
}

/// Starts `uze upgrade --background` in a process group of its own, so the Ctrl+C
/// that ends the next command in this terminal cannot end it too.
fn hand_off_check() {
    let Ok(binary) = env::current_exe() else {
        return;
    };
    let mut command = Command::new(binary);
    command
        .args(["upgrade", "--background"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _ = uze_platform::process::spawn_detached(&mut command);
}

/// `period` moved by up to a fifth either way, so clients started
/// together, or woken together from sleep, do not ask together. Not
/// cryptographic: `RandomState`'s keys are seeded per process.
pub(crate) fn jittered(period: Duration) -> Duration {
    use std::hash::{BuildHasher as _, Hasher as _};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    let spread = period / 5;
    let offset = Duration::from_nanos(hasher.finish() % (2 * spread.as_nanos().max(1)) as u64);
    period - spread + offset
}

fn this_binary() -> Option<PathBuf> {
    env::current_exe().and_then(fs::canonicalize).ok()
}

/// Puts the notice about `version` away, here and in every client after.
pub(crate) fn acknowledge(home: &UzeHome, version: &str) {
    publish(None);
    let (home, version) = (home.clone(), version.to_owned());
    thread::spawn(move || {
        amend_ledger(&home, |ledger| ledger.acknowledged = Some(version));
    });
}

/// When a release found by a check is installed: at once on `latest`,
/// once it has been the latest for [`SETTLE`] on `stable`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Channel {
    Latest,
    Stable,
}

impl Channel {
    fn current() -> Self {
        Self::from_env(env::var("UZE_RELEASE_CHANNEL").ok().as_deref())
    }

    fn from_env(setting: Option<&str>) -> Self {
        match setting.map(str::trim) {
            Some("stable") => Self::Stable,
            _ => Self::Latest,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Policy {
    Off,
    Notify,
    Install,
}

impl Policy {
    fn current() -> Self {
        Self::from_env(
            env::var("UZE_AUTOUPDATE").ok().as_deref(),
            env::var_os("CI").is_some(),
        )
    }

    fn from_env(setting: Option<&str>, ci: bool) -> Self {
        match setting.map(str::trim) {
            Some("off" | "0" | "false" | "no") => Self::Off,
            Some("notify") => Self::Notify,
            Some("on" | "1" | "true" | "yes") => Self::Install,
            _ if ci => Self::Off,
            _ => Self::Install,
        }
    }
}

/// What `install.sh` wrote: the file it placed, and the release it was.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Receipt {
    binary: PathBuf,
    version: String,
}

/// What the updater itself remembers between runs.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Ledger {
    /// Seconds since the epoch of the last time the latest release was asked for.
    #[serde(default)]
    checked_at: u64,
    #[serde(default)]
    latest: Option<String>,
    /// When `latest` was published, or first answered while the release
    /// page will not say.
    #[serde(default)]
    latest_since: u64,
    /// Whether `latest_since` is when `latest` was published. A release
    /// first answered undated is dated again at every check until it is.
    #[serde(default)]
    latest_dated: bool,
    /// The release an undated `latest` replaced, judged once `latest` is
    /// dated.
    #[serde(default)]
    replaced: Option<Replaced>,
    /// The newest release that stayed the latest for [`SETTLE`] before a
    /// newer one replaced it: still worth installing while its successor
    /// settles, so a machine that missed the window does not skip it.
    #[serde(default)]
    settled: Option<String>,
    /// The last release the updater put in place.
    #[serde(default)]
    installed: Option<String>,
    /// The last release whose notice was put away.
    #[serde(default)]
    acknowledged: Option<String>,
    /// The last release a CLI command mentioned. A line printed after every
    /// command is a line nobody reads, so each release is mentioned once.
    #[serde(default)]
    told: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Replaced {
    version: String,
    since: u64,
}

/// Where releases come from. A trait so a pass can be exercised end to end
/// without a network.
trait Releases {
    fn latest(&self) -> Option<String>;
    /// How long ago `version` was published, in seconds, by the release
    /// host's own clock: an age rather than a date, so a machine whose
    /// clock is wrong when it asks does not shorten the wait.
    fn age(&self, version: &str) -> Option<u64>;
    fn changelog(&self, version: &str) -> Option<String>;
    fn install(&self, version: &str, target: &Path, scratch: &Path) -> Result<(), String>;
}

/// One check: ask what the latest release is (when told to, or when the
/// last answer has gone stale), replace the installer's binary when it is behind, and say
/// what the reader should be told.
fn pass(
    home: &UzeHome,
    policy: Policy,
    channel: Channel,
    this: Option<&Path>,
    releases: &dyn Releases,
    now: u64,
    ask: bool,
) -> Option<Notice> {
    let _span = tracing::info_span!("self_update.pass", running = RUNNING).entered();
    let stale = |ledger: &Ledger| now.saturating_sub(ledger.checked_at) >= CHECK_EVERY.as_secs();
    let mut ledger = read_json::<Ledger>(&ledger_path(home)).unwrap_or_default();
    if (ask || stale(&ledger))
        && let Some(_claim) = claim_check(home)
    {
        // Read again under the claim: whoever held it last may have just
        // answered, and asking again would be the duplicate it prevents.
        ledger = read_json::<Ledger>(&ledger_path(home)).unwrap_or_default();
        if ask || stale(&ledger) {
            // Stamped before asking, and kept when the question goes
            // unanswered: offline is a state that lasts, and asking again
            // on every wake changes nothing about it.
            amend_ledger(home, |stored| stored.checked_at = now);
            let latest = releases.latest().or(ledger.latest.clone());
            ledger = observe(home, latest, releases, now);
        }
    }

    let mut receipt = read_json::<Receipt>(&receipt_path(home))
        .filter(|receipt| this.is_some_and(|this| is_same_file(this, &receipt.binary)));
    if let (Some(owned), Some(target), Policy::Install) =
        (&mut receipt, settled(&ledger, now, channel), policy)
        && newer(&target, &owned.version)
    {
        match install_over(home, owned, &target, releases) {
            Ok(()) => ledger.installed = Some(target),
            Err(error) => tracing::warn!(%error, "the release could not be installed"),
        }
    }
    decide(
        RUNNING,
        receipt.as_ref().map(|receipt| receipt.version.as_str()),
        &ledger,
    )
}

/// Records `latest` as the answer given at `now`, and returns the ledger
/// as it then stands. A release's wait is counted from when it was
/// published rather than from when this machine first heard of it, so a
/// machine that was off does not start the day over; and the release a
/// newer one replaces is remembered as settled when it had been the latest
/// for [`SETTLE`] by the time its successor was published.
///
/// A release the host will not date is taken in all the same, as published
/// when first answered: the wait it gets is never shorter than the real
/// one, and a release replaced in a hurry is never judged settled on a
/// guess. It is dated again at every check, and the release it replaced
/// is judged then.
fn observe(home: &UzeHome, latest: Option<String>, releases: &dyn Releases, now: u64) -> Ledger {
    let stored = read_json::<Ledger>(&ledger_path(home)).unwrap_or_default();
    let to_date = latest
        .as_ref()
        .filter(|version| stored.latest.as_ref() != Some(*version) || !stored.latest_dated);
    let age = to_date.map(|version| releases.age(version));
    if let (Some(version), Some(None)) = (to_date, age) {
        tracing::warn!(%version, "the release page did not say when this release was published");
    }
    let mut observed = None;
    amend_ledger(home, |ledger| {
        ledger.checked_at = now;
        let (Some(version), Some(age)) = (latest, age) else {
            observed = Some(ledger.clone());
            return;
        };
        if ledger.latest.as_ref() != Some(&version) {
            // An undated `since` is when this machine first heard, never
            // before the real date: judging by it can only be stricter.
            ledger.replaced = ledger.latest.take().map(|previous| Replaced {
                version: previous,
                since: ledger.latest_since,
            });
            ledger.latest = Some(version);
            ledger.latest_since = now;
            ledger.latest_dated = false;
        }
        if let Some(age) = age {
            ledger.latest_since = now.saturating_sub(age);
            ledger.latest_dated = true;
            if let Some(previous) = ledger.replaced.take()
                && ledger.latest_since.saturating_sub(previous.since) >= SETTLE.as_secs()
            {
                ledger.settled = Some(previous.version);
            }
        }
        observed = Some(ledger.clone());
    });
    observed.unwrap_or_default()
}

/// The newest release the background pass may install at `now`: on
/// `latest` the latest, and on `stable` the latest once it has been out
/// for [`SETTLE`], otherwise the last one that was.
fn settled(ledger: &Ledger, now: u64, channel: Channel) -> Option<String> {
    if channel == Channel::Latest || now.saturating_sub(ledger.latest_since) >= SETTLE.as_secs() {
        ledger.latest.clone()
    } else {
        ledger.settled.clone()
    }
}

/// Replaces the installer's file with `latest` and moves the receipt and
/// the ledger with it — the one step the background pass and
/// `uze upgrade` share.
fn install_over(
    home: &UzeHome,
    owned: &mut Receipt,
    latest: &str,
    releases: &dyn Releases,
) -> Result<(), String> {
    let _span = tracing::info_span!("self_update.install", version = %latest).entered();
    releases.install(latest, &owned.binary, &home.cache_dir())?;
    // The harness launchers run this binary too; where they are copies of
    // it they would go on running the one just replaced. One that cannot be
    // placed now is placed again by the next setup.
    if let Err(error) =
        uze_platform::executable::refresh_launchers(&owned.binary, &home.shims_dir())
    {
        tracing::warn!(%error, "the harness launchers were not refreshed");
    }
    owned.version = latest.to_owned();
    let _ = write_json(&receipt_path(home), owned);
    amend_ledger(home, |stored| stored.installed = Some(latest.to_owned()));
    // Fetched now, while this is already online and off anyone's frame, so
    // the notice's notes open without a wait.
    notes_with(home, latest, releases);
    Ok(())
}

/// One release's section of the changelog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReleaseNotes {
    pub(crate) version: String,
    pub(crate) date: Option<String>,
    /// The section's Markdown, below its heading.
    pub(crate) body: String,
}

/// `version`'s own section of the changelog published at its tag: from the
/// cache when it already carries it, fetched and kept otherwise. It may wait
/// on the network, so nothing that draws calls it.
pub(crate) fn release_notes(home: &UzeHome, version: &str) -> Option<ReleaseNotes> {
    notes_with(home, version, &Published::current())
}

fn notes_with(home: &UzeHome, version: &str, releases: &dyn Releases) -> Option<ReleaseNotes> {
    let path = home.release_notes_cache_path();
    let section = |text: &str| {
        sections(text)
            .into_iter()
            .find(|notes| notes.version == version)
    };
    if let Some(cached) = fs::read_to_string(&path)
        .ok()
        .and_then(|text| section(&text))
    {
        return Some(cached);
    }
    let text = releases.changelog(version)?;
    let fetched = section(&text)?;
    let _ = uze_application::write_atomic(&path, text.as_bytes());
    Some(fetched)
}

/// Splits a `git-cliff` changelog into its releases. A section starts at a
/// `## [version](…) - date` heading and runs to the next one; the preamble
/// above the first belongs to none.
fn sections(changelog: &str) -> Vec<ReleaseNotes> {
    let mut notes: Vec<ReleaseNotes> = Vec::new();
    for line in changelog.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            let version = heading
                .strip_prefix('[')
                .and_then(|rest| rest.split_once(']'))
                .map_or(heading, |(version, _)| version)
                .trim_start_matches('v')
                .to_owned();
            let date = heading
                .rsplit_once(" - ")
                .map(|(_, date)| date.trim().to_owned());
            notes.push(ReleaseNotes {
                version,
                date,
                body: String::new(),
            });
        } else if let Some(current) = notes.last_mut() {
            current.body.push_str(line);
            current.body.push('\n');
        }
    }
    for release in &mut notes {
        release.body = release.body.trim().to_owned();
    }
    notes
}

/// What `uze upgrade` did.
#[derive(Debug, Eq, PartialEq)]
pub enum Upgrade {
    /// The installer's binary already is the latest release.
    Current(String),
    /// The installer's binary was replaced; processes already running keep
    /// the release they started from.
    Replaced { from: String, to: String },
    /// This binary is not the one `install.sh` placed, so it is not this
    /// command's to replace. `placed` is the file the installer did place,
    /// when there is one — a different `uze` earlier on `PATH` is the usual
    /// reason a curl install never seems to update.
    NotInstalled {
        running: Option<PathBuf>,
        placed: Option<PathBuf>,
        latest: String,
    },
}

/// The update a person asked for, run to the end in the foreground. Unlike
/// the background pass it ignores `UZE_AUTOUPDATE` — asking is the consent
/// the setting withholds — and it says why whenever it does nothing.
pub fn upgrade(home: &UzeHome) -> Result<Upgrade, String> {
    upgrade_with(
        home,
        this_binary().as_deref(),
        &Published::current(),
        unix_now(),
    )
}

fn upgrade_with(
    home: &UzeHome,
    this: Option<&Path>,
    releases: &dyn Releases,
    now: u64,
) -> Result<Upgrade, String> {
    let _span = tracing::info_span!("self_update.upgrade", running = RUNNING).entered();
    let latest = releases
        .latest()
        .ok_or_else(|| format!("cannot reach {RELEASES} to ask for the latest release"))?;
    observe(home, Some(latest.clone()), releases, now);
    let receipt = read_json::<Receipt>(&receipt_path(home));
    let Some(mut owned) = receipt
        .clone()
        .filter(|receipt| this.is_some_and(|this| is_same_file(this, &receipt.binary)))
    else {
        return Ok(Upgrade::NotInstalled {
            running: this.map(Path::to_owned),
            placed: receipt.map(|receipt| receipt.binary),
            latest,
        });
    };
    if !newer(&latest, &owned.version) {
        return Ok(Upgrade::Current(owned.version));
    }
    let from = owned.version.clone();
    install_over(home, &mut owned, &latest, releases)?;
    Ok(Upgrade::Replaced { from, to: latest })
}

/// What to say, from what is running and what the installer's file now
/// is (when this is the installer's binary at all).
fn decide(running: &str, on_disk: Option<&str>, ledger: &Ledger) -> Option<Notice> {
    let on_disk = on_disk?;
    (newer(on_disk, running) && ledger.acknowledged.as_deref() != Some(on_disk))
        .then(|| Notice(on_disk.to_owned()))
}

/// The releases `install.sh` downloads, fetched the way it fetches them.
struct Published {
    base: String,
}

impl Published {
    fn current() -> Self {
        Self {
            base: base(
                env::var("UZE_BASE_URL").ok().as_deref(),
                cfg!(debug_assertions),
            ),
        }
    }
}

/// Where a release is fetched from. A shipped binary answers [`RELEASES`]
/// and nothing else — see the module doc for why `UZE_BASE_URL` stops at
/// the installer script. `debug` is `cfg!(debug_assertions)`, passed in so
/// both answers can be tested from one build.
fn base(override_url: Option<&str>, debug: bool) -> String {
    match override_url {
        Some(url) if debug => url.to_owned(),
        _ => RELEASES.to_owned(),
    }
}

impl Releases for Published {
    /// Read from where `releases/latest` redirects rather than from the
    /// API: the redirect carries no rate limit and no JSON, and it is the
    /// same "latest" the installer resolves. Not followed: following it
    /// downloaded the tag's whole page, about 170 KB, to read one version.
    fn latest(&self) -> Option<String> {
        let output = uze_platform::tools::curl()
            .args([
                "-fsS",
                "--max-time",
                "15",
                "-o",
                uze_platform::fs::NULL_DEVICE,
                "-w",
            ])
            .arg("%{redirect_url}")
            .arg(format!("{}/latest", self.base))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        tag_version(&String::from_utf8_lossy(&output.stdout))
    }

    /// Read off the release's checksums, which are uploaded with the rest
    /// of its assets: the host's `Date` less the file's `Last-Modified`,
    /// from the same origin as everything else here and without the API's
    /// rate limit.
    fn age(&self, version: &str) -> Option<u64> {
        let version = release_version(version)?;
        let output = uze_platform::tools::curl()
            .args(["-fsSL", "--max-time", "15", "-D", "-", "-o"])
            .arg(uze_platform::fs::NULL_DEVICE)
            .arg(format!("{}/download/v{version}/{SUMS}", self.base))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        served_age(&String::from_utf8_lossy(&output.stdout))
    }

    fn changelog(&self, version: &str) -> Option<String> {
        let version = release_version(version)?;
        let output = uze_platform::tools::curl()
            .args(["-fsSL", "--max-time", "15"])
            .arg(format!("{SOURCES}/v{version}/CHANGELOG.md"))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        String::from_utf8(output.stdout).ok()
    }

    fn install(&self, version: &str, target: &Path, scratch: &Path) -> Result<(), String> {
        let version =
            release_version(version).ok_or_else(|| format!("{version:?} is not a release"))?;
        let archive = asset().ok_or("no release is built for this platform")?;
        // One directory per process: two clients installing the same
        // release at once would otherwise unpack into each other's files.
        let scratch = scratch
            .join("release")
            .join(format!("{version}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
        let result = (|| {
            let download = format!("{}/download/v{version}", self.base);
            for file in [archive.as_str(), SUMS, SIGNATURE] {
                fetch(&format!("{download}/{file}"), &scratch.join(file))?;
            }
            install_downloaded(&scratch, &archive, version, target, RELEASE_KEY)
        })();
        let _ = fs::remove_dir_all(&scratch);
        result
    }
}

/// How old the file at the end of a redirect chain is, from the headers
/// `curl -D` wrote for every response in it: the last response's `Date`
/// less its `Last-Modified`, both the host's clock.
fn served_age(headers: &str) -> Option<u64> {
    let (mut date, mut modified) = (None, None);
    for line in headers.lines() {
        if line.starts_with("HTTP/") {
            (date, modified) = (None, None);
        } else if let Some((name, value)) = line.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "date" => date = http_date(value),
                "last-modified" => modified = http_date(value),
                _ => {}
            }
        }
    }
    Some(date?.saturating_sub(modified?))
}

/// Seconds since the epoch of an HTTP date in the one form a server sends
/// today, `Sun, 06 Nov 1994 08:49:37 GMT`.
fn http_date(text: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut words = text.split_whitespace().skip(1);
    let day: u64 = words.next()?.parse().ok()?;
    let month = words.next()?;
    let month = MONTHS.iter().position(|name| *name == month)? as u64 + 1;
    let year: u64 = words.next()?.parse().ok()?;
    let clock: Vec<u64> = words
        .next()?
        .split(':')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let [hours, minutes, seconds] = clock[..] else {
        return None;
    };
    let in_range = (1970..=9999).contains(&year)
        && (1..=31).contains(&day)
        && hours < 24
        && minutes < 60
        && seconds <= 60;
    (words.next()? == "GMT" && in_range).then_some(())?;
    // Days from the civil date, March-based so the leap day ends the year.
    let year = if month <= 2 { year - 1 } else { year };
    let (era, of_era) = (year / 400, year % 400);
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = (era * 146_097 + of_cycle).checked_sub(719_468)?;
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

/// The checksums a release publishes, and the signature over them.
const SUMS: &str = "SHASUMS256.txt";
const SIGNATURE: &str = "SHASUMS256.txt.sig";

/// Installs `archive`, already downloaded into `scratch` beside [`SUMS`]
/// and [`SIGNATURE`], over `target` — once the checksums are proven signed
/// by `key` and the archive proven to be the one they name. Nothing is
/// unpacked before both hold.
fn install_downloaded(
    scratch: &Path,
    archive: &str,
    version: &str,
    target: &Path,
    key: &str,
) -> Result<(), String> {
    // Read once: the bytes the signature is checked over are the bytes the
    // checksum is then read from.
    let sums = fs::read(scratch.join(SUMS)).map_err(|error| error.to_string())?;
    verify_signature(key, &sums, &scratch.join(SIGNATURE), scratch)?;
    let sums = String::from_utf8_lossy(&sums);
    let expected = expected_sum(&sums, archive).ok_or(format!("no checksum for {archive}"))?;
    let bytes = fs::read(scratch.join(archive)).map_err(|error| error.to_string())?;
    if sha256(&bytes) != expected {
        return Err(format!("checksum mismatch for {archive}"));
    }
    let unpacked = scratch.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|error| error.to_string())?;
    // Both GNU tar and the bsdtar Windows ships tell a gzip from a
    // zip by its bytes.
    let unpack = uze_platform::tools::system("tar")
        .arg("-xf")
        .arg(scratch.join(archive))
        .arg("-C")
        .arg(&unpacked)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if !unpack.success() {
        return Err(format!("cannot unpack {archive}"));
    }
    replace(
        &unpacked.join(uze_platform::executable::file_name("uze")),
        version,
        target,
    )
}

/// The one `ssh-ed25519` line of a public key file, comments and blank
/// lines aside. A file without one — the placeholder committed until a
/// release key exists — names no key, and every signature is refused.
fn release_key(file: &str) -> Option<&str> {
    let mut keys = file
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let key = keys.next()?;
    (keys.next().is_none() && key.starts_with("ssh-ed25519 ")).then_some(key)
}

/// Whether `signature` is `key`'s signature over `message` in the
/// [`SIGNATURE_NAMESPACE`], as the system's `ssh-keygen` judges it.
fn verify_signature(
    key: &str,
    message: &[u8],
    signature: &Path,
    scratch: &Path,
) -> Result<(), String> {
    let key = release_key(key).ok_or("this build carries no release signing key")?;
    if !signature.is_file() {
        return Err(format!("the release publishes no {SIGNATURE}"));
    }
    let signers = scratch.join("allowed_signers");
    fs::write(
        &signers,
        format!("{SIGNATURE_NAMESPACE} namespaces=\"{SIGNATURE_NAMESPACE}\" {key}\n"),
    )
    .map_err(|error| error.to_string())?;
    let verdict = uze_platform::tools::system("ssh-keygen")
        .args(["-Y", "verify", "-f"])
        .arg(&signers)
        .args(["-I", SIGNATURE_NAMESPACE, "-n", SIGNATURE_NAMESPACE, "-s"])
        .arg(signature)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut verifier| {
            if let Some(mut input) = verifier.stdin.take() {
                use std::io::Write as _;
                // A verifier that exits early closes the pipe; its status
                // is the answer either way.
                let _ = input.write_all(message);
            }
            verifier.wait()
        });
    let _ = fs::remove_file(&signers);
    match verdict {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("{SUMS} is not signed by the release key")),
        Err(error) => Err(format!(
            "cannot verify the release signature without OpenSSH's ssh-keygen: {error}"
        )),
    }
}

fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let status = uze_platform::tools::curl()
        .args(["-fsSL", "--max-time", "300", "-o"])
        .arg(to)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("cannot download {url}"))
}

/// Puts `staged` where `target` is, having first made it prove it is the
/// release it claims to be — the same last step `install.sh` takes, and for
/// the same reason: a file that does not run is worse than an old one.
fn replace(staged: &Path, version: &str, target: &Path) -> Result<(), String> {
    make_runnable(staged).map_err(|error| error.to_string())?;
    let reported = Command::new(staged)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("the downloaded binary does not run: {error}"))?;
    let reports_version = String::from_utf8_lossy(&reported.stdout)
        .split_whitespace()
        .any(|word| word == version);
    if !reports_version {
        return Err(format!("the downloaded binary is not {version}"));
    }
    // Beside the target, so the rename below never crosses a filesystem —
    // which is the only way it stays a rename rather than a copy that a
    // pane's shim could catch half-written.
    let beside = target.with_file_name(uze_platform::executable::file_name(&format!(
        ".uze-update-{}",
        std::process::id()
    )));
    let placed = (|| {
        fs::copy(staged, &beside)?;
        make_runnable(&beside)?;
        // Windows flushes only through a handle that may write.
        fs::OpenOptions::new()
            .write(true)
            .open(&beside)?
            .sync_all()?;
        uze_platform::executable::replace_running(&beside, target)
    })();
    if placed.is_err() {
        let _ = fs::remove_file(&beside);
    }
    placed.map_err(|error| format!("cannot replace {}: {error}", target.display()))
}

use uze_platform::executable::make_runnable;

/// Removes the images an upgrade set aside beside the running binary, once
/// nothing runs them.
pub fn sweep_set_aside() {
    if let Ok(running) = env::current_exe() {
        uze_platform::executable::sweep_replaced(&running);
    }
}

/// The asset `install.sh` would pick for this machine. Where the installer
/// has to ask `ldd` which C library the system uses, a running binary
/// already knows which one it was built against.
fn asset() -> Option<String> {
    use uze_platform::target;
    asset_for(target::OS, target::ARCH, target::MUSL)
}

/// The release archive built for `os` on `arch`: the table `release.yml`
/// publishes and the installers read.
fn asset_for(os: &str, arch: &str, musl: bool) -> Option<String> {
    let arch = match arch {
        arch @ ("x86_64" | "aarch64") => arch,
        _ => return None,
    };
    let (platform, extension) = match os {
        "macos" => (format!("{arch}-macos"), "tar.gz"),
        "linux" if musl => (format!("{arch}-linux-musl"), "tar.gz"),
        "linux" => (format!("{arch}-linux-gnu"), "tar.gz"),
        "windows" => (format!("{arch}-windows"), "zip"),
        _ => return None,
    };
    Some(format!("uze-{platform}.{extension}"))
}

/// The version a release page's address names — `…/releases/tag/v1.2.3`.
/// Anything else, including the `…/latest` a mirror answers without
/// redirecting, names none.
fn tag_version(url: &str) -> Option<String> {
    let version = url.trim().rsplit('/').next()?.strip_prefix('v')?;
    release_version(version).map(str::to_owned)
}

/// `version` when it is spelled as a release is: SemVer's
/// `MAJOR.MINOR.PATCH[-PRERELEASE]`, numbers without leading zeros and
/// identifiers of `[0-9A-Za-z-]`, and no build metadata. A release's
/// version becomes a directory and a URL, so anything else — a `/`, a
/// `..`, a `+` — is refused before it can name one.
fn release_version(version: &str) -> Option<&str> {
    let numeric = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let identifier = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && (!part.bytes().all(|byte| byte.is_ascii_digit()) || numeric(part))
    };
    if version.len() > 64 {
        return None;
    }
    let (core, prerelease) = match version.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (version, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let valid = parts.len() == 3
        && parts.iter().all(|part| numeric(part))
        && prerelease.is_none_or(|prerelease| prerelease.split('.').all(identifier));
    valid.then_some(version)
}

fn expected_sum(sums: &str, archive: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let sum = fields.next()?;
        (fields.next()?.trim_start_matches('*') == archive).then(|| sum.to_ascii_lowercase())
    })
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut spelled, byte| {
            let _ = write!(spelled, "{byte:02x}");
            spelled
        })
}

/// Whether `candidate` is a later release than `current`, by SemVer
/// precedence — which is what tells `0.0.0-alpha.10` from `alpha.9`, and a
/// build from `main` that is ahead of the latest release from one behind it.
fn newer(candidate: &str, current: &str) -> bool {
    match (precedence(candidate), precedence(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Identifier {
    // Declared first: SemVer ranks numeric identifiers below alphanumeric ones.
    Numeric(u64),
    Alphanumeric(String),
}

#[derive(Debug, Eq, PartialEq)]
struct Precedence {
    core: [u64; 3],
    prerelease: Vec<Identifier>,
}

impl Ord for Precedence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // A release outranks every pre-release of the same core; between
        // two pre-releases, identifiers compare left to right and a longer
        // list wins a tie — which is exactly `Vec`'s own ordering.
        self.core.cmp(&other.core).then_with(|| {
            match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => self.prerelease.cmp(&other.prerelease),
            }
        })
    }
}

impl PartialOrd for Precedence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn precedence(version: &str) -> Option<Precedence> {
    let version = version.trim().trim_start_matches('v');
    let version = version.split('+').next()?;
    let (core, prerelease) = version.split_once('-').unwrap_or((version, ""));
    let mut numbers = core.split('.').map(|part| part.parse::<u64>().ok());
    let core = [numbers.next()??, numbers.next()??, numbers.next()??];
    if numbers.next().is_some() {
        return None;
    }
    let prerelease = if prerelease.is_empty() {
        Vec::new()
    } else {
        prerelease
            .split('.')
            .map(|part| match part.parse::<u64>() {
                Ok(number) => Some(Identifier::Numeric(number)),
                Err(_) if !part.is_empty() => Some(Identifier::Alphanumeric(part.to_owned())),
                Err(_) => None,
            })
            .collect::<Option<Vec<_>>>()?
    };
    Some(Precedence { core, prerelease })
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The installer's own receipt, not a record of UZE's.
///
/// `install.sh` writes it, which is why it stays a document of its own
/// rather than folding into the ledger below: a different writer and a
/// different lifetime. UZE rewrites only its version, after placing a
/// release over the binary it names.
fn receipt_path(home: &UzeHome) -> PathBuf {
    home.install_receipt_path()
}

fn ledger_path(home: &UzeHome) -> PathBuf {
    home.binary_path()
}

/// The right to ask for the latest release, held by one process at a
/// time so clients that wake together ask once; `None` while another
/// process holds it. Released when dropped.
///
/// The wait is only long enough to outlast a descriptor a `fork` on
/// another thread still holds for a lock this process just let go of
/// (`MutationLock` explains it): a holder that is another process is not
/// waited for, and its answer is read instead.
fn claim_check(home: &UzeHome) -> Option<uze_application::FileLock> {
    uze_application::FileLock::acquire(
        &home.binary_check_lock_path(),
        Duration::from_millis(25),
        Duration::from_millis(1),
    )
    .ok()
}

/// Read, changed and written back in one step rather than from a copy held
/// across a download: the notice can be put away while a pass is running,
/// and a pass writing back what it read before would bring it back.
fn amend_ledger(home: &UzeHome, change: impl FnOnce(&mut Ledger)) {
    let path = ledger_path(home);
    let mut ledger = read_json::<Ledger>(&path).unwrap_or_default();
    change(&mut ledger);
    let _ = write_json(&path, &ledger);
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// The one writer, like everything else UZE owns: a reader afterwards sees
/// the previous content or the new one, never half of either. This module
/// used to carry its own atomic rename, which is how two conventions for
/// one thing start.
fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    uze_application::write_atomic(path, &serde_json::to_vec_pretty(value)?)
        .map_err(std::io::Error::other)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    // Unix file modes, which Windows does not keep.
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use uze_testkit::temp::TempDir;

    #[test]
    fn precedence_is_semvers() {
        for (candidate, current) in [
            ("0.0.0-alpha.5", "0.0.0-alpha.4"),
            ("0.0.0-alpha.10", "0.0.0-alpha.9"),
            ("0.0.0", "0.0.0-alpha.9"),
            ("0.0.0-beta", "0.0.0-alpha.9"),
            ("0.0.0-alpha.1.1", "0.0.0-alpha.1"),
            ("0.0.0-alpha.x", "0.0.0-alpha.9"),
            ("0.1.0-alpha.1", "0.0.9"),
            ("1.0.0", "0.99.99"),
            ("v1.0.1", "1.0.0"),
        ] {
            assert!(newer(candidate, current), "{candidate} > {current}");
            assert!(!newer(current, candidate), "{current} < {candidate}");
        }
        assert!(!newer("0.0.0-alpha.4", "0.0.0-alpha.4"));
        assert!(
            !newer("1.0.0+build", "1.0.0"),
            "build metadata does not rank"
        );
        assert!(
            !newer("garbage", "0.0.0"),
            "what does not parse is never newer"
        );
        assert!(!newer("1.0", "0.0.0"), "a core is three numbers");
    }

    #[test]
    fn the_version_is_read_off_the_release_page_the_redirect_lands_on() {
        assert_eq!(
            tag_version("https://github.com/uze-sh/uze/releases/tag/v0.0.0-alpha.4\n").as_deref(),
            Some("0.0.0-alpha.4")
        );
        assert_eq!(
            tag_version("https://mirror.test/uze/releases/latest"),
            None,
            "a mirror that did not redirect names no release"
        );
        assert_eq!(tag_version("https://x.test/releases/tag/vnext"), None);
    }

    #[test]
    fn a_version_becomes_a_path_only_when_it_is_spelled_as_a_release() {
        for release in [
            "1.0.0",
            "0.0.0-alpha.10",
            "1.0.0-beta.13",
            "10.20.30-rc-1.x",
        ] {
            assert_eq!(release_version(release), Some(release));
        }
        for hostile in [
            "1.0.0+build",
            "1.0.0-beta+evil",
            "1.0.0-../../x",
            "1.0.0-a/b",
            "1.0.0-a\\b",
            "1.0.0-",
            "1.0.0-a..b",
            "01.0.0",
            "1.0.0-01",
            "1.0",
            "1.0.0.0",
            "v1.0.0",
            " 1.0.0",
            "",
        ] {
            assert_eq!(release_version(hostile), None, "{hostile:?}");
        }
        assert_eq!(
            tag_version("https://github.com/uze-sh/uze/releases/tag/v1.0.0+evil"),
            None,
            "build metadata never becomes the latest release"
        );
        let refused = Published {
            base: RELEASES.to_owned(),
        }
        .install(
            "1.0.0-../../escape",
            Path::new("unused"),
            Path::new("unused"),
        );
        assert!(refused.is_err());
        let undated = Published {
            base: RELEASES.to_owned(),
        }
        .age("1.0.0-../../escape");
        assert_eq!(undated, None);
    }

    #[test]
    fn the_asset_is_the_one_the_installer_picks() {
        assert!(
            asset().is_some(),
            "every platform uze runs on has a release asset"
        );
        for (os, arch, musl, expected) in [
            ("linux", "x86_64", false, "uze-x86_64-linux-gnu.tar.gz"),
            ("linux", "aarch64", true, "uze-aarch64-linux-musl.tar.gz"),
            ("macos", "aarch64", false, "uze-aarch64-macos.tar.gz"),
            ("windows", "x86_64", false, "uze-x86_64-windows.zip"),
            ("windows", "aarch64", false, "uze-aarch64-windows.zip"),
        ] {
            assert_eq!(asset_for(os, arch, musl).as_deref(), Some(expected));
        }
        assert_eq!(asset_for("freebsd", "x86_64", false), None);
        assert_eq!(asset_for("linux", "riscv64", false), None);
    }

    #[test]
    fn a_checksum_is_found_by_the_archive_it_names() {
        let sums = "AB12  uze-x86_64-linux-gnu.tar.gz\ncd34 *uze-aarch64-macos.tar.gz\n";
        assert_eq!(
            expected_sum(sums, "uze-x86_64-linux-gnu.tar.gz").as_deref(),
            Some("ab12")
        );
        assert_eq!(
            expected_sum(sums, "uze-aarch64-macos.tar.gz").as_deref(),
            Some("cd34")
        );
        assert_eq!(expected_sum(sums, "uze-x86_64-linux-musl.tar.gz"), None);
    }

    #[test]
    fn an_untrusted_base_url_is_ignored() {
        for hostile in ["http://evil/", "file:///tmp/x", "https://evil.test/"] {
            assert_eq!(
                base(Some(hostile), false),
                RELEASES,
                "a shipped binary downloads from the release page alone"
            );
            assert_eq!(
                base(Some(hostile), true),
                hostile,
                "a debug build still drives the offline fixture"
            );
        }
        assert_eq!(base(None, true), RELEASES);
        assert_eq!(base(None, false), RELEASES);
    }

    #[test]
    fn the_policy_is_read_from_the_environment() {
        assert_eq!(Policy::from_env(None, false), Policy::Install);
        assert_eq!(Policy::from_env(None, true), Policy::Off, "CI means off");
        assert_eq!(
            Policy::from_env(Some("on"), true),
            Policy::Install,
            "unless told"
        );
        assert_eq!(Policy::from_env(Some("notify"), false), Policy::Notify);
        assert_eq!(Policy::from_env(Some("off"), false), Policy::Off);
        assert_eq!(Policy::from_env(Some("0"), false), Policy::Off);
    }

    fn ledger(latest: Option<&str>, installed: Option<&str>, seen: Option<&str>) -> Ledger {
        Ledger {
            checked_at: 0,
            latest: latest.map(str::to_owned),
            latest_since: 0,
            latest_dated: true,
            replaced: None,
            settled: None,
            installed: installed.map(str::to_owned),
            acknowledged: seen.map(str::to_owned),
            told: None,
        }
    }

    #[test]
    fn the_sidebar_speaks_only_of_an_update_waiting_on_a_restart() {
        let replaced = ledger(Some("1.1.0"), Some("1.1.0"), None);

        // The installer's binary was replaced; this process is the old one.
        assert_eq!(
            decide("1.0.0", Some("1.1.0"), &replaced),
            Some(Notice("1.1.0".to_owned()))
        );
        assert_eq!(
            decide(
                "1.0.0",
                Some("1.1.0"),
                &ledger(Some("1.1.0"), Some("1.1.0"), Some("1.1.0"))
            ),
            None,
            "until it is put away"
        );
        // The restart ran it: there is nothing left to do.
        assert_eq!(decide("1.1.0", Some("1.1.0"), &replaced), None);
        // A newer release that was not installed is not the sidebar's news.
        assert_eq!(
            decide("1.0.0", Some("1.0.0"), &ledger(Some("1.1.0"), None, None)),
            None
        );
        assert_eq!(
            decide("1.0.0", None, &ledger(Some("1.1.0"), None, None)),
            None,
            "nor is one for a binary the installer did not place"
        );
        assert_eq!(decide("1.0.0", Some("1.0.0"), &Ledger::default()), None);
    }

    #[test]
    fn a_command_says_once_that_it_is_the_updated_release() {
        let (_dir, home, _binary) = world();
        let stored = || read_json::<Ledger>(&ledger_path(&home)).unwrap_or_default();
        write_json(&ledger_path(&home), &ledger(None, Some(RUNNING), None)).unwrap();

        assert_eq!(
            updated_to(&home, stored()).as_deref(),
            Some(RUNNING),
            "the first command after it says so"
        );
        assert_eq!(updated_to(&home, stored()), None, "and only the first");
    }

    struct Fake {
        latest: Option<&'static str>,
        age: Option<u64>,
        changelogs: RefCell<Vec<String>>,
        installs: RefCell<Vec<(String, PathBuf)>>,
        fails: bool,
    }

    impl Releases for Fake {
        fn latest(&self) -> Option<String> {
            self.latest.map(str::to_owned)
        }
        fn age(&self, _: &str) -> Option<u64> {
            self.age
        }
        fn changelog(&self, version: &str) -> Option<String> {
            self.changelogs.borrow_mut().push(version.to_owned());
            self.latest.map(|latest| {
                format!("# Changelog\n\nIntro.\n\n## [{latest}](https://x/compare) - 2026-09-22\n\n### Fixes\n\n- one\n")
            })
        }
        fn install(&self, version: &str, target: &Path, _: &Path) -> Result<(), String> {
            self.installs
                .borrow_mut()
                .push((version.to_owned(), target.to_owned()));
            if self.fails {
                Err("offline".to_owned())
            } else {
                Ok(())
            }
        }
    }

    fn fake(latest: Option<&'static str>) -> Fake {
        Fake {
            latest,
            age: Some(0),
            changelogs: RefCell::default(),
            installs: RefCell::default(),
            fails: false,
        }
    }

    fn world() -> (TempDir, UzeHome, PathBuf) {
        let dir = TempDir::new("self-update");
        let home = UzeHome::at(dir.path().join(".uze"));
        let binary = dir.path().join("bin").join("uze");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, "#!/bin/sh\n").unwrap();
        (dir, home, binary)
    }

    fn receipt(home: &UzeHome, binary: &Path, version: &str) {
        write_json(
            &receipt_path(home),
            &Receipt {
                binary: binary.to_owned(),
                version: version.to_owned(),
            },
        )
        .unwrap();
    }

    /// `version` as the latest release since long before any pass a test
    /// runs, so the background pass treats it as settled.
    fn latest_since_long_ago(home: &UzeHome, version: &str) {
        write_json(&ledger_path(home), &ledger(Some(version), None, None)).unwrap();
    }

    const SETTLED: u64 = 10_000 + SETTLE.as_secs();

    #[test]
    fn the_installers_binary_is_replaced_and_the_next_launch_is_told() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        latest_since_long_ago(&home, "999.0.0");
        let releases = fake(Some("999.0.0"));

        let notice = pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &releases,
            SETTLED,
            true,
        );

        assert_eq!(
            releases.installs.borrow().as_slice(),
            [("999.0.0".to_owned(), binary.clone())]
        );
        assert_eq!(notice, Some(Notice("999.0.0".to_owned())));
        let written = read_json::<Receipt>(&receipt_path(&home)).unwrap();
        assert_eq!(written.version, "999.0.0", "the receipt follows the file");

        // Within the hour nothing is asked or installed again.
        let again = fake(Some("999.0.1"));
        pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &again,
            SETTLED + 1,
            false,
        );
        assert!(again.installs.borrow().is_empty());
    }

    #[test]
    fn the_channel_is_read_from_the_environment() {
        assert_eq!(Channel::from_env(None), Channel::Latest);
        assert_eq!(Channel::from_env(Some("latest")), Channel::Latest);
        assert_eq!(Channel::from_env(Some(" stable ")), Channel::Stable);
        assert_eq!(
            Channel::from_env(Some("nightly")),
            Channel::Latest,
            "an unknown channel is the default"
        );
    }

    #[test]
    fn on_the_latest_channel_a_release_is_installed_when_found() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let releases = fake(Some("999.0.0"));

        let notice = pass(
            &home,
            Policy::Install,
            Channel::Latest,
            Some(&binary),
            &releases,
            10_000,
            false,
        );

        assert_eq!(
            releases.installs.borrow().as_slice(),
            [("999.0.0".to_owned(), binary.clone())]
        );
        assert_eq!(notice, Some(Notice("999.0.0".to_owned())));
    }

    #[test]
    fn a_check_another_process_is_making_is_not_made_again() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let releases = fake(Some("999.0.0"));

        let other = claim_check(&home).expect("nobody holds it yet");
        pass(
            &home,
            Policy::Install,
            Channel::Latest,
            Some(&binary),
            &releases,
            10_000,
            false,
        );
        let ledger = read_json::<Ledger>(&ledger_path(&home)).unwrap_or_default();
        assert_eq!(ledger.latest, None, "nothing was asked");
        assert!(releases.installs.borrow().is_empty());

        drop(other);
        pass(
            &home,
            Policy::Install,
            Channel::Latest,
            Some(&binary),
            &releases,
            10_001,
            false,
        );
        let ledger = read_json::<Ledger>(&ledger_path(&home)).unwrap_or_default();
        assert_eq!(ledger.latest.as_deref(), Some("999.0.0"));
    }

    #[test]
    fn an_unanswered_check_waits_the_hour_like_an_answered_one() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let offline = fake(None);
        let ask = |now| {
            pass(
                &home,
                Policy::Install,
                Channel::Latest,
                Some(&binary),
                &offline,
                now,
                false,
            )
        };

        ask(10_000);
        let stamped = read_json::<Ledger>(&ledger_path(&home)).unwrap_or_default();
        assert_eq!(stamped.checked_at, 10_000);
        ask(10_000 + CHECK_EVERY.as_secs() - 1);
        let still = read_json::<Ledger>(&ledger_path(&home)).unwrap_or_default();
        assert_eq!(still.checked_at, 10_000, "not asked again within the hour");
    }

    /// The attempt is on record while the request is out, so a process
    /// waking meanwhile, or one that outlives a request that never ends,
    /// does not ask again.
    #[test]
    fn the_attempt_is_recorded_before_the_request_is_made() {
        struct Watching<'a> {
            home: &'a UzeHome,
            seen: RefCell<Option<u64>>,
        }
        impl Releases for Watching<'_> {
            fn latest(&self) -> Option<String> {
                let ledger = read_json::<Ledger>(&ledger_path(self.home)).unwrap_or_default();
                *self.seen.borrow_mut() = Some(ledger.checked_at);
                None
            }
            fn age(&self, _: &str) -> Option<u64> {
                None
            }
            fn changelog(&self, _: &str) -> Option<String> {
                None
            }
            fn install(&self, _: &str, _: &Path, _: &Path) -> Result<(), String> {
                Ok(())
            }
        }

        let (_dir, home, binary) = world();
        let watching = Watching {
            home: &home,
            seen: RefCell::default(),
        };
        pass(
            &home,
            Policy::Install,
            Channel::Latest,
            Some(&binary),
            &watching,
            10_000,
            false,
        );
        assert_eq!(*watching.seen.borrow(), Some(10_000));
    }

    #[test]
    fn a_wake_is_moved_by_at_most_a_fifth() {
        for _ in 0..64 {
            let wake = jittered(WAKE_EVERY);
            assert!(wake >= WAKE_EVERY - WAKE_EVERY / 5, "{wake:?}");
            assert!(wake <= WAKE_EVERY + WAKE_EVERY / 5, "{wake:?}");
        }
    }

    #[test]
    fn a_release_is_installed_in_the_background_only_once_it_has_settled() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let releases = fake(Some("999.0.0"));

        pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &releases,
            10_000,
            true,
        );
        assert!(
            releases.installs.borrow().is_empty(),
            "a release just published is not installed yet"
        );
        pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &releases,
            SETTLED - 1,
            true,
        );
        assert!(
            releases.installs.borrow().is_empty(),
            "nor a day less a second later"
        );

        // Published two seconds before the first had seen out its day.
        let newer_still = Fake {
            age: Some(2),
            ..fake(Some("999.0.1"))
        };
        pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &newer_still,
            SETTLED,
            true,
        );
        assert!(
            newer_still.installs.borrow().is_empty(),
            "a release that replaced the latest starts its own wait"
        );

        pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &newer_still,
            SETTLED + SETTLE.as_secs(),
            true,
        );
        assert_eq!(
            newer_still.installs.borrow().as_slice(),
            [("999.0.1".to_owned(), binary.clone())]
        );
    }

    const HOUR: u64 = 60 * 60;

    /// One background pass at `now`, the host answering that `latest` was
    /// published `age` seconds ago (or not saying): what it installed.
    fn check(
        home: &UzeHome,
        binary: &Path,
        latest: &'static str,
        age: Option<u64>,
        now: u64,
    ) -> Vec<String> {
        let releases = Fake {
            age,
            ..fake(Some(latest))
        };
        pass(
            home,
            Policy::Install,
            Channel::Stable,
            Some(binary),
            &releases,
            now,
            true,
        );
        let installs = releases.installs.borrow();
        installs
            .iter()
            .map(|(version, _)| version.clone())
            .collect()
    }

    #[test]
    fn the_wait_counts_from_publication_not_from_first_sight() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);

        assert_eq!(
            check(&home, &binary, "999.0.0", Some(SETTLE.as_secs()), SETTLED),
            ["999.0.0"],
            "a machine that was off while the release settled does not wait another day"
        );
    }

    /// The weekend of beta.16, .17 and .18: .17 was the latest for well
    /// over a day, but this machine heard of it early in its wait and ran
    /// nothing again until .18 was out.
    #[test]
    fn a_release_that_settled_before_it_was_replaced_is_still_installed() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, "999.0.16");
        let (seventeen, eighteen) = (10_000, 10_000 + 40 * HOUR);

        let early = check(
            &home,
            &binary,
            "999.0.17",
            Some(2 * HOUR),
            seventeen + 2 * HOUR,
        );
        assert!(early.is_empty(), "still within its wait");
        assert_eq!(
            check(&home, &binary, "999.0.18", Some(HOUR), eighteen + HOUR),
            ["999.0.17"],
            "while .18 settles, .17 is due"
        );
        assert_eq!(
            check(
                &home,
                &binary,
                "999.0.18",
                None,
                eighteen + SETTLE.as_secs()
            ),
            ["999.0.18"],
            "and .18 once it has settled too"
        );
    }

    #[test]
    fn a_release_replaced_within_the_window_is_never_installed() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, "999.0.16");
        let (pulled, fixed) = (10_000, 10_000 + 2 * HOUR);

        assert!(check(&home, &binary, "999.0.17", Some(HOUR), pulled + HOUR).is_empty());
        assert!(check(&home, &binary, "999.0.18", Some(HOUR), fixed + HOUR).is_empty());
        assert_eq!(
            check(&home, &binary, "999.0.18", None, fixed + SETTLE.as_secs()),
            ["999.0.18"]
        );
    }

    #[test]
    fn a_release_that_cannot_be_dated_is_dated_at_the_next_check() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, "999.0.16");
        let (seventeen, eighteen) = (10_000, 10_000 + 40 * HOUR);
        check(
            &home,
            &binary,
            "999.0.17",
            Some(2 * HOUR),
            seventeen + 2 * HOUR,
        );

        let undated = check(&home, &binary, "999.0.18", None, eighteen + HOUR);
        assert!(
            undated.is_empty(),
            ".17 cannot be judged settled on a guess"
        );
        assert_eq!(
            check(
                &home,
                &binary,
                "999.0.18",
                Some(2 * HOUR),
                eighteen + 2 * HOUR
            ),
            ["999.0.17"],
            "dated an hour later, .18 proves .17 was the latest for a day"
        );
    }

    #[test]
    fn a_pulled_release_is_not_installed_while_its_successor_is_undated() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, "999.0.16");
        let pulled = 10_000;
        check(&home, &binary, "999.0.17", Some(HOUR), pulled + HOUR);
        let undated = Fake {
            age: None,
            ..fake(Some("999.0.18"))
        };

        for (now, ask) in [(pulled + 30 * HOUR, true), (pulled + 30 * HOUR + 60, false)] {
            pass(
                &home,
                Policy::Install,
                Channel::Stable,
                Some(&binary),
                &undated,
                now,
                ask,
            );
        }
        assert!(
            undated.installs.borrow().is_empty(),
            ".17 has been out a day by its own date, but the host already answers .18"
        );
    }

    #[test]
    fn a_ledger_from_before_releases_were_dated_still_judges_its_release() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, "999.0.16");
        let seventeen_heard = 10_000;
        let undated = Ledger {
            latest_since: seventeen_heard,
            latest_dated: false,
            ..ledger(Some("999.0.17"), None, None)
        };
        write_json(&ledger_path(&home), &undated).unwrap();

        let eighteen = seventeen_heard + 40 * HOUR;
        assert_eq!(
            check(&home, &binary, "999.0.18", Some(HOUR), eighteen + HOUR),
            ["999.0.17"]
        );
    }

    #[test]
    fn a_release_that_is_never_dated_still_settles() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);

        assert!(check(&home, &binary, "999.0.0", None, 10_000).is_empty());
        assert_eq!(
            check(&home, &binary, "999.0.0", None, SETTLED),
            ["999.0.0"],
            "a day after this machine first heard of it"
        );
    }

    #[test]
    fn a_release_is_aged_by_the_last_response_of_its_redirects() {
        let headers = "HTTP/2 302\r\ndate: Wed, 21 Oct 2026 00:00:00 GMT\r\nlocation: https://x\r\n\r\n\
                       HTTP/2 200\r\nLast-Modified: Tue, 20 Oct 2026 14:38:30 GMT\r\nDate: Wed, 21 Oct 2026 00:24:26 GMT\r\n\r\n";
        assert_eq!(served_age(headers), Some(1_792_542_266 - 1_792_507_110));
        assert_eq!(
            served_age("HTTP/2 200\r\ndate: Wed, 21 Oct 2026 00:24:26 GMT\r\n"),
            None,
            "a file the host does not date has no age"
        );
        assert_eq!(
            served_age(
                "HTTP/2 302\r\nlast-modified: Tue, 20 Oct 2026 14:38:30 GMT\r\n\r\n\
                 HTTP/2 200\r\ndate: Wed, 21 Oct 2026 00:24:26 GMT\r\n\r\n"
            ),
            None,
            "a redirect's date is not the file's"
        );
    }

    #[test]
    fn an_http_date_is_read_in_the_form_hosts_send() {
        assert_eq!(
            http_date(" Wed, 21 Oct 2026 00:24:26 GMT"),
            Some(1_792_542_266)
        );
        assert_eq!(
            http_date("Tue, 29 Feb 2028 12:00:00 GMT"),
            Some(1_835_438_400)
        );
        assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        for malformed in [
            "Wed, 21 Okt 2026 00:24:26 GMT",
            "Wed, 21 Oct 2026 00:24 GMT",
            "Wed, 21 Oct 2026 00:24:26 +0000",
            "Wednesday, 21-Oct-26 00:24:26 GMT",
            "Wed, 21 Oct 2026 24:00:00 GMT",
            "Wed, 21 Oct 2026 00:60:00 GMT",
            "Sun, 01 Jan 99999999999999999 00:00:00 GMT",
            "Sun, 32 Jan 2026 00:00:00 GMT",
            "",
        ] {
            assert_eq!(http_date(malformed), None, "{malformed:?}");
        }
    }

    #[test]
    fn a_binary_the_installer_did_not_place_is_never_replaced() {
        let (dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let elsewhere = dir.path().join("target-debug-uze");
        fs::write(&elsewhere, "").unwrap();
        latest_since_long_ago(&home, "999.0.0");
        let releases = fake(Some("999.0.0"));

        let notice = pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&elsewhere),
            &releases,
            SETTLED,
            true,
        );

        assert!(releases.installs.borrow().is_empty());
        assert_eq!(notice, None);
    }

    #[test]
    fn notify_asks_but_never_replaces() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        latest_since_long_ago(&home, "999.0.0");
        let releases = fake(Some("999.0.0"));

        let notice = pass(
            &home,
            Policy::Notify,
            Channel::Stable,
            Some(&binary),
            &releases,
            SETTLED,
            true,
        );

        assert!(releases.installs.borrow().is_empty());
        assert_eq!(notice, None);
    }

    #[test]
    fn a_failed_replacement_leaves_the_receipt_and_offers_the_release() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        latest_since_long_ago(&home, "999.0.0");
        let releases = Fake {
            fails: true,
            ..fake(Some("999.0.0"))
        };

        let notice = pass(
            &home,
            Policy::Install,
            Channel::Stable,
            Some(&binary),
            &releases,
            SETTLED,
            true,
        );

        assert_eq!(releases.installs.borrow().len(), 1, "it was attempted");
        assert_eq!(notice, None);
        assert_eq!(
            read_json::<Receipt>(&receipt_path(&home)).unwrap().version,
            RUNNING
        );
    }

    #[test]
    fn upgrade_replaces_the_installers_binary_and_says_from_what() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let releases = fake(Some("999.0.0"));

        let outcome = upgrade_with(&home, Some(&binary), &releases, 10_000).unwrap();

        assert_eq!(
            outcome,
            Upgrade::Replaced {
                from: RUNNING.to_owned(),
                to: "999.0.0".to_owned()
            }
        );
        assert_eq!(
            read_json::<Receipt>(&receipt_path(&home)).unwrap().version,
            "999.0.0"
        );
        assert_eq!(
            upgrade_with(&home, Some(&binary), &releases, 10_001).unwrap(),
            Upgrade::Current("999.0.0".to_owned()),
            "asked again, there is nothing newer"
        );
    }

    #[test]
    fn upgrade_names_the_file_the_installer_placed_when_another_uze_ran() {
        let (dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let shadowing = dir.path().join("cargo-bin-uze");
        fs::write(&shadowing, "").unwrap();
        let releases = fake(Some("999.0.0"));

        let outcome = upgrade_with(&home, Some(&shadowing), &releases, 10_000).unwrap();

        assert_eq!(
            outcome,
            Upgrade::NotInstalled {
                running: Some(shadowing),
                placed: Some(binary),
                latest: "999.0.0".to_owned()
            }
        );
        assert!(releases.installs.borrow().is_empty());
    }

    #[test]
    fn upgrade_offline_is_an_error_rather_than_up_to_date() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        assert!(upgrade_with(&home, Some(&binary), &fake(None), 10_000).is_err());
    }

    #[test]
    fn a_changelog_splits_into_its_releases_newest_first() {
        let notes = sections(
            "# Changelog\n\nPreamble.\n\n\
             ## [0.0.0-alpha.8](https://x/compare/a...b) - 2026-09-22\n\n### Fixes\n\n- b\n\n\
             ## [0.0.0-alpha.7](https://x/compare/a...b) - 2026-09-21\n\n- a\n",
        );
        assert_eq!(
            notes,
            [
                ReleaseNotes {
                    version: "0.0.0-alpha.8".to_owned(),
                    date: Some("2026-09-22".to_owned()),
                    body: "### Fixes\n\n- b".to_owned(),
                },
                ReleaseNotes {
                    version: "0.0.0-alpha.7".to_owned(),
                    date: Some("2026-09-21".to_owned()),
                    body: "- a".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn notes_are_fetched_once_and_kept_for_the_release_they_cover() {
        let (_dir, home, _binary) = world();
        let releases = fake(Some("999.0.0"));

        let notes = notes_with(&home, "999.0.0", &releases).expect("fetched");
        assert_eq!(notes.version, "999.0.0");
        assert_eq!(notes.body, "### Fixes\n\n- one");
        notes_with(&home, "999.0.0", &releases).expect("kept");
        assert_eq!(releases.changelogs.borrow().len(), 1, "read from the cache");

        assert_eq!(
            notes_with(&home, "1000.0.0", &releases),
            None,
            "a changelog that does not cover the release is not its notes"
        );
    }

    #[test]
    fn installing_a_release_fetches_its_notes() {
        let (_dir, home, binary) = world();
        receipt(&home, &binary, RUNNING);
        let releases = fake(Some("999.0.0"));

        upgrade_with(&home, Some(&binary), &releases, 10_000).unwrap();

        assert_eq!(releases.changelogs.borrow().as_slice(), ["999.0.0"]);
        assert!(home.release_notes_cache_path().is_file());
    }

    #[test]
    fn offline_is_remembered_rather_than_retried_every_launch() {
        let (_dir, home, binary) = world();
        let offline = fake(None);
        assert_eq!(
            pass(
                &home,
                Policy::Install,
                Channel::Stable,
                Some(&binary),
                &offline,
                10_000,
                false
            ),
            None
        );
        let ledger = read_json::<Ledger>(&ledger_path(&home)).unwrap();
        assert_eq!(ledger.checked_at, 10_000);
    }

    /// The stand-in release is a shell script, which only Unix runs.
    #[cfg(unix)]
    #[test]
    fn a_replacement_that_does_not_run_as_the_release_is_refused() {
        let dir = TempDir::new("self-update-replace");
        let target = dir.path().join("uze");
        fs::write(&target, "old").unwrap();
        let staged = dir.path().join("staged");
        fs::write(&staged, "#!/bin/sh\necho uze 1.0.0\n").unwrap();

        assert!(replace(&staged, "2.0.0", &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old", "untouched");

        replace(&staged, "1.0.0", &target).unwrap();
        assert!(fs::read_to_string(&target).unwrap().contains("uze 1.0.0"));
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".uze-update")
            })
            .collect();
        assert!(leftovers.is_empty(), "nothing is left beside it");
    }

    /// A throwaway release key, made for this test alone.
    struct Signer {
        dir: TempDir,
    }

    impl Signer {
        fn new(name: &str) -> Self {
            let dir = TempDir::new(name);
            let made = uze_platform::tools::system("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-C", "test", "-f"])
                .arg(dir.path().join("key"))
                .stdin(Stdio::null())
                .status()
                .expect("ssh-keygen, which verifying a release needs too");
            assert!(made.success());
            Self { dir }
        }

        fn public(&self) -> String {
            format!(
                "# a comment, as the committed file carries\n{}",
                fs::read_to_string(self.dir.path().join("key.pub")).unwrap()
            )
        }

        fn sign(&self, message: &Path, namespace: &str) {
            let signed = uze_platform::tools::system("ssh-keygen")
                .args(["-Y", "sign", "-n", namespace, "-f"])
                .arg(self.dir.path().join("key"))
                .arg(message)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(signed.success());
        }
    }

    #[test]
    fn the_checksums_count_only_when_the_release_key_signed_them() {
        let release = Signer::new("release-key");
        let stranger = Signer::new("stranger-key");
        let dir = TempDir::new("release-signature");
        let sums = dir.path().join(SUMS);
        let signature = dir.path().join(SIGNATURE);
        fs::write(&sums, "ab12  uze-x86_64-linux-gnu.tar.gz\n").unwrap();

        let verify =
            |key: &str| verify_signature(key, &fs::read(&sums).unwrap(), &signature, dir.path());

        assert!(verify(&release.public()).is_err(), "no signature at all");

        release.sign(&sums, SIGNATURE_NAMESPACE);
        verify(&release.public()).expect("signed by the release key");
        assert!(verify(&stranger.public()).is_err(), "a different key");
        assert!(verify("").is_err(), "no key names no signer");
        assert!(
            verify(&format!("# PLACEHOLDER\n{}", release.public())).is_ok(),
            "comments around the key are only comments"
        );
        assert!(
            verify("# PLACEHOLDER: no release key has been generated yet.\n").is_err(),
            "a build carrying the placeholder trusts nothing"
        );

        fs::write(&sums, "00ff  uze-x86_64-linux-gnu.tar.gz\n").unwrap();
        assert!(
            verify(&release.public()).is_err(),
            "checksums changed after signing"
        );

        fs::write(&sums, "ab12  uze-x86_64-linux-gnu.tar.gz\n").unwrap();
        fs::remove_file(&signature).unwrap();
        release.sign(&sums, "git");
        assert!(
            verify(&release.public()).is_err(),
            "the same key signing for another purpose"
        );
    }

    /// The stand-in release is a shell script, which only Unix runs.
    #[cfg(unix)]
    #[test]
    fn an_unsigned_release_is_never_unpacked_or_installed() {
        let release = Signer::new("release-key-install");
        let dir = TempDir::new("self-update-signed");
        let scratch = dir.path().join("scratch");
        let stage = dir.path().join("stage");
        fs::create_dir_all(&scratch).unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("uze"), "#!/bin/sh\necho uze 2.0.0\n").unwrap();
        let archive = "uze-test.tar.gz";
        let packed = uze_platform::tools::system("tar")
            .arg("-czf")
            .arg(scratch.join(archive))
            .arg("-C")
            .arg(&stage)
            .arg("uze")
            .status()
            .unwrap();
        assert!(packed.success());
        let sum = sha256(&fs::read(scratch.join(archive)).unwrap());
        fs::write(scratch.join(SUMS), format!("{sum}  {archive}\n")).unwrap();
        let target = dir.path().join("installed-uze");
        fs::write(&target, "old").unwrap();

        let unsigned = install_downloaded(&scratch, archive, "2.0.0", &target, &release.public());
        assert!(unsigned.is_err(), "{unsigned:?}");
        assert_eq!(fs::read_to_string(&target).unwrap(), "old", "untouched");
        assert!(!scratch.join("unpacked").exists(), "nothing was unpacked");

        release.sign(&scratch.join(SUMS), SIGNATURE_NAMESPACE);
        install_downloaded(&scratch, archive, "2.0.0", &target, &release.public())
            .expect("a signed release installs");
        assert!(fs::read_to_string(&target).unwrap().contains("uze 2.0.0"));
    }

    #[test]
    fn the_installers_carry_the_key_the_binary_is_built_with() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let committed = release_key(RELEASE_KEY).unwrap_or("");
        let carried = |file: &str, prefix: &str| {
            let text = fs::read_to_string(root.join(file)).unwrap();
            let line = text
                .lines()
                .map(str::trim)
                .find_map(|line| line.strip_prefix(prefix))
                .unwrap_or_else(|| panic!("{file} carries no `{prefix}` line"))
                .to_owned();
            line.trim_matches('\'').to_owned()
        };
        assert_eq!(carried("install.sh", "UZE_RELEASE_KEY="), committed);
        assert_eq!(carried("install.ps1", "$ReleaseKey = "), committed);
    }
}
