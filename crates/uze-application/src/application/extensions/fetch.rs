//! `fetch` answered for a script (`extension-growth` G1, G2).
//!
//! `uze-script` has already refused anything but `https` to a host the
//! grants name; the host checks it again, then asks the system's `curl`
//! through the program runner. The URL is the one argument a script
//! chose; the headers, the body and the secret go in `curl`'s
//! configuration on standard input, so the secret is in no argument and
//! no environment. No redirect is followed: a script that wants the
//! `Location` asks for it, checked like any other URL. Every reply is
//! scrubbed of the secret's value before the script sees it, and a run
//! that placed one is marked, so what it answers keeps nothing and
//! notifies nobody.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use uze_core::UzeHome;
use uze_core::machine::extension_program::ProgramOutcome;
use uze_script::{FetchReply, FetchRequest, ReachError};

/// The most a request may take, whatever the program bound: one source's
/// answer is waited on by its surface.
pub const FETCH_TIME_LIMIT: Duration = uze_contract::PROGRAM_TIME_LIMIT;

/// What replaces a secret's value wherever a reply carried it.
pub const SCRUBBED: &str = "[secret]";

/// What `curl` prints after the body, so the status is told from it
/// whatever the body ends with.
const STATUS_MARK: &str = "\n%{http_code}";

/// `fetch` for one extension: the hosts it was granted, the secrets it
/// declares and where they are kept, and the runner's confinement.
#[derive(Clone, Debug)]
pub(super) struct Fetcher {
    pub home: UzeHome,
    pub plugin: String,
    pub id: String,
    pub hosts: Vec<String>,
    pub secrets: Vec<String>,
    pub helper: Option<PathBuf>,
    /// The `curl` run: the system's own, by path.
    pub curl: PathBuf,
    /// Set once a request carried a secret.
    pub tainted: Arc<AtomicBool>,
}

impl Fetcher {
    /// Makes `request`, or says why it was not made.
    pub fn fetch(&self, request: &FetchRequest) -> Result<FetchReply, ReachError> {
        let host = uze_script::host_of(&request.url)?;
        if host != request.host || !self.hosts.contains(&host) {
            return Err(refused(format!(
                "`{host}` is a host this extension may not reach"
            )));
        }
        let secret = match &request.secret {
            None => None,
            Some(used) => Some((used, self.secret(&used.preference)?)),
        };
        let config = config(
            request,
            secret.as_ref().map(|(used, value)| {
                let value = match &used.scheme {
                    Some(scheme) => format!("{scheme} {value}"),
                    None => value.clone(),
                };
                (used.header.as_str(), value)
            }),
        );
        if secret.is_some() {
            self.tainted.store(true, Ordering::Relaxed);
        }
        let values: Vec<&str> = secret.iter().map(|(_, value)| value.as_str()).collect();
        let ended = super::program::curl(&super::program::CurlRun {
            home: &self.home,
            curl: &self.curl,
            args: arguments(request),
            config: config.into_bytes(),
            time_limit: FETCH_TIME_LIMIT,
            helper: self.helper.as_deref(),
        });
        reply_of(ended, &values)
    }

    /// The value of the secret preference `name`, which the extension must
    /// declare and the operator must have set.
    fn secret(&self, name: &str) -> Result<String, ReachError> {
        if !self.secrets.iter().any(|declared| declared == name) {
            return Err(refused(format!(
                "`{name}` is not a secret this extension declares"
            )));
        }
        uze_workspace::extensions::secrets::read(&self.home, &self.plugin, &self.id, name)
            .map_err(|_| refused(format!("the secret `{name}` could not be read")))?
            .ok_or_else(|| {
                refused(format!(
                    "the secret `{name}` is not set; set it on the Extensions screen"
                ))
            })
    }
}

/// The arguments after the fixed ones: quiet but for errors, no URL
/// globbing, no redirect, the time bound, the status after the body, the
/// configuration on standard input, and the URL.
fn arguments(request: &FetchRequest) -> Vec<String> {
    [
        "--silent",
        "--show-error",
        "--globoff",
        "--max-redirs",
        "0",
        "--max-time",
        &FETCH_TIME_LIMIT
            .as_secs()
            .saturating_sub(1)
            .max(1)
            .to_string(),
        "--write-out",
        STATUS_MARK,
        "--config",
        "-",
        "--url",
        &request.url,
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect()
}

/// `curl`'s configuration for `request`: the method, each header, the
/// secret's header and the body, each a quoted value.
fn config(request: &FetchRequest, secret: Option<(&str, String)>) -> String {
    let mut lines = vec![format!("request = {}", quoted(request.method.name()))];
    for (name, value) in &request.headers {
        lines.push(format!("header = {}", quoted(&format!("{name}: {value}"))));
    }
    if let Some((header, value)) = secret {
        lines.push(format!(
            "header = {}",
            quoted(&format!("{header}: {value}"))
        ));
    }
    if let Some(body) = &request.body {
        lines.push(format!("data-binary = {}", quoted(body)));
    }
    lines.push(String::new());
    lines.join("\n")
}

/// A value of `curl`'s configuration file: in double quotes, with what
/// would end or bend it escaped.
fn quoted(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// How the request ended, as the script reads it, every secret value in
/// it replaced.
fn reply_of(ended: ProgramOutcome, secrets: &[&str]) -> Result<FetchReply, ReachError> {
    let failed = |reason: String| Err(refused(scrub(&reason, secrets)));
    match ended {
        ProgramOutcome::Answered { stdout } => {
            let text = String::from_utf8_lossy(&stdout);
            let Some((body, status)) = text.rsplit_once('\n') else {
                return failed("curl answered without a status".to_owned());
            };
            let Ok(status) = status.trim().parse::<u16>() else {
                return failed(format!("curl answered `{status}` for a status"));
            };
            if status == 0 {
                return failed("no answer came back".to_owned());
            }
            Ok(FetchReply {
                status,
                body: scrub(body, secrets),
            })
        }
        ProgramOutcome::Refused { reason } | ProgramOutcome::Failed { reason } => {
            failed(format!("the request failed: {reason}"))
        }
        ProgramOutcome::TimedOut { limit } => {
            failed(format!("no answer within {}s", limit.as_secs_f32()))
        }
        ProgramOutcome::Flooded { bound, .. } => {
            failed(format!("the answer is larger than {} KiB", bound / 1024))
        }
        ProgramOutcome::Unenforced { reason } => failed(format!(
            "its confinement did not apply ({reason}); nothing was requested"
        )),
        ProgramOutcome::NotStarted { reason } => failed(format!("curl did not start: {reason}")),
    }
}

/// `text` with every one of `secrets` replaced by [`SCRUBBED`].
pub fn scrub(text: &str, secrets: &[&str]) -> String {
    secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(text.to_owned(), |text, secret| {
            text.replace(secret, SCRUBBED)
        })
}

/// What a run that placed a secret answers: nothing kept, nobody
/// notified, since what it holds may carry what it was handed.
pub(super) fn untainted(
    tainted: &AtomicBool,
    mut envelope: uze_contract::Envelope,
) -> uze_contract::Envelope {
    if tainted.load(Ordering::Relaxed) && (envelope.store.is_some() || envelope.notify.is_some()) {
        envelope.store = None;
        envelope.notify = None;
        let why = "it used a secret, so it keeps nothing and notifies nobody".to_owned();
        envelope.notice = Some(match envelope.notice.take() {
            Some(notice) => format!("{notice} · {why}"),
            None => why,
        });
    }
    envelope
}

/// The system's `curl`, by path.
pub(super) fn system_curl() -> PathBuf {
    uze_platform::tools::system_program("curl")
}

fn refused(reason: String) -> ReachError {
    ReachError(format!("fetch: {reason}"))
}

impl Fetcher {
    /// A `fetch` for `manifest`'s hosts and secrets, or `None` when it
    /// wants no host.
    pub fn of(
        home: &UzeHome,
        identity: &uze_contract::Identity,
        manifest: &uze_contract::Manifest,
        helper: Option<&Path>,
    ) -> Option<Self> {
        let hosts: Vec<String> = manifest.hosts().map(str::to_owned).collect();
        if hosts.is_empty() {
            return None;
        }
        Some(Self {
            home: home.clone(),
            plugin: super::consent::plugin_key(identity),
            id: identity.id.clone(),
            hosts,
            secrets: manifest
                .preferences
                .iter()
                .filter(|decl| decl.is_secret())
                .map(|decl| decl.name.clone())
                .collect(),
            helper: helper.map(Path::to_path_buf),
            curl: system_curl(),
            tainted: Arc::default(),
        })
    }
}

// The stand-in `curl` is a `/bin/sh` script, which only a Unix runs.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    /// A world whose `curl` is a script that records its arguments and
    /// standard input, and answers a recorded reply that echoes the token.
    struct World {
        root: PathBuf,
        fetcher: Fetcher,
    }

    impl World {
        fn new(label: &str, reply: &str) -> Self {
            let root = uze_testkit::temp::scratch(label);
            let home = UzeHome::at(root.join("uze"));
            let curl = root.join("curl");
            std::fs::write(
                &curl,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\ncat > '{stdin}'\nenv > '{env}'\nprintf '%s' '{reply}'\n",
                    args = root.join("args").display(),
                    stdin = root.join("stdin").display(),
                    env = root.join("env").display(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
            let fetcher = Fetcher {
                home,
                plugin: "ci@studio".into(),
                id: "ci-status".into(),
                hosts: vec!["api.github.com".into()],
                secrets: vec!["token".into()],
                helper: None,
                curl,
                tainted: Arc::default(),
            };
            Self { root, fetcher }
        }

        fn read(&self, file: &str) -> String {
            std::fs::read_to_string(self.root.join(file)).unwrap_or_default()
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn request(url: &str, secret: bool) -> FetchRequest {
        FetchRequest {
            url: url.into(),
            host: uze_script::host_of(url).unwrap(),
            method: uze_script::FetchMethod::Get,
            headers: vec![("Accept".into(), "application/json".into())],
            body: None,
            secret: secret.then(|| uze_script::SecretUse {
                preference: "token".into(),
                header: "Authorization".into(),
                scheme: Some("Bearer".into()),
            }),
        }
    }

    #[test]
    fn the_secret_goes_on_stdin_and_comes_back_scrubbed() {
        let world = World::new("fetch-secret", "{\"echo\":\"Bearer ghp_TOKEN\"}\n200");
        uze_workspace::extensions::secrets::store(
            &world.fetcher.home,
            "ci@studio",
            "ci-status",
            "token",
            "ghp_TOKEN",
        )
        .unwrap();
        let reply = world
            .fetcher
            .fetch(&request(
                "https://api.github.com/repos/o/r/actions/runs",
                true,
            ))
            .unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, "{\"echo\":\"Bearer [secret]\"}");
        assert!(world.fetcher.tainted.load(Ordering::Relaxed));
        let args = world.read("args");
        assert!(!args.contains("ghp_TOKEN"), "{args}");
        assert!(!world.read("env").contains("ghp_TOKEN"));
        let stdin = world.read("stdin");
        assert!(
            stdin.contains("header = \"Authorization: Bearer ghp_TOKEN\""),
            "{stdin}"
        );
        assert!(
            stdin.contains("header = \"Accept: application/json\""),
            "{stdin}"
        );
        let args: Vec<&str> = args.lines().collect();
        assert_eq!(args.first(), Some(&"-q"));
        for pinned in [
            ["--max-redirs", "0"],
            ["--config", "-"],
            ["--proto-redir", "=https"],
        ] {
            assert!(
                args.windows(2).any(|pair| pair == pinned),
                "{pinned:?} in {args:?}"
            );
        }
        assert!(args.contains(&"--globoff"));
        assert_eq!(
            args.last(),
            Some(&"https://api.github.com/repos/o/r/actions/runs")
        );
    }

    #[test]
    fn an_unset_secret_or_an_ungranted_host_requests_nothing() {
        let world = World::new("fetch-refused", "{}\n200");
        let unset = world
            .fetcher
            .fetch(&request("https://api.github.com/user", true))
            .unwrap_err();
        assert!(unset.0.contains("is not set"), "{unset}");
        let elsewhere = world
            .fetcher
            .fetch(&request("https://example.com/", false))
            .unwrap_err();
        assert!(elsewhere.0.contains("may not reach"), "{elsewhere}");
        assert!(world.read("args").is_empty(), "curl ran");
        let reply = world
            .fetcher
            .fetch(&request("https://api.github.com/zen", false))
            .unwrap();
        assert_eq!((reply.status, reply.body.as_str()), (200, "{}"));
        assert!(!world.fetcher.tainted.load(Ordering::Relaxed));
    }

    #[test]
    fn a_value_is_quoted_so_it_cannot_add_an_option() {
        assert_eq!(quoted("a\"b\\c\nurl = x"), "\"a\\\"b\\\\c\\nurl = x\"");
    }

    #[test]
    fn a_tainted_answer_keeps_nothing() {
        let tainted = AtomicBool::new(true);
        let mut store = serde_json::Map::new();
        store.insert("last".into(), serde_json::json!("x"));
        let envelope = untainted(
            &tainted,
            uze_contract::Envelope {
                store: Some(Box::new(store)),
                ..uze_contract::Envelope::default()
            },
        );
        assert!(envelope.store.is_none());
        assert!(envelope.notice.unwrap().contains("used a secret"));
    }
}
