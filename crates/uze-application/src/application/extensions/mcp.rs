//! The MCP servers extensions call, one live session per extension and
//! server (`extension-mcp` D3, D5, D7).
//!
//! A server is the one a harness is handed: the stdio launch the plugin's
//! `mcp.json` declares, read by `uze-integrations` as delivery reads it,
//! started with the operator's environment, unconfined, in the checkout.
//! It starts on the first call and stays for the next, its calls one at a
//! time; it is ended once idle for [`IDLE`], when a call times out or the
//! server stops answering, and when the runner holding it goes. A server
//! UZE did not end itself sees its standard input close when UZE exits,
//! which ends a stdio server.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use uze_contract::{Identity, MCP_TIMEOUT, Manifest};
use uze_core::UzeHome;
use uze_core::machine::mcp::{McpError, McpLaunch, McpSession};
use uze_core::store::{PackageId, UzeStore};
use uze_script::ReachError;

/// How long a session waits for its next call before it is ended.
pub const IDLE: Duration = Duration::from_secs(300);

/// How often idle sessions are looked for.
const SWEEP: Duration = Duration::from_secs(30);

/// One extension's session with one server: absent until the first call,
/// and after one that left it unusable.
#[derive(Default)]
struct Live {
    session: Option<McpSession>,
    used: Option<Instant>,
}

type Key = (Identity, String);

/// Every session the runner holds.
pub(super) struct McpSessions {
    live: Mutex<HashMap<Key, Arc<Mutex<Live>>>>,
    sweeping: std::sync::Once,
}

impl Default for McpSessions {
    fn default() -> Self {
        Self {
            live: Mutex::default(),
            sweeping: std::sync::Once::new(),
        }
    }
}

impl std::fmt::Debug for McpSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let live = self.live.lock().map(|live| live.len()).unwrap_or_default();
        f.debug_struct("McpSessions").field("live", &live).finish()
    }
}

/// What one call needs to find and start its server.
pub(super) struct Call<'a> {
    pub home: &'a UzeHome,
    pub identity: &'a Identity,
    pub manifest: &'a Manifest,
    pub checkout: &'a Path,
}

impl McpSessions {
    /// Calls `tool` of `server` for the extension `call` names, starting
    /// the server when no session of it is live.
    pub(super) fn call(
        self: &Arc<Self>,
        call: &Call<'_>,
        server: &str,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, ReachError> {
        self.sweep_idle_from_now_on();
        let within = call
            .manifest
            .mcp
            .iter()
            .find(|decl| decl.server == server)
            .and_then(|decl| decl.timeout)
            .unwrap_or(MCP_TIMEOUT);
        let live = Arc::clone(
            self.live
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry((call.identity.clone(), server.to_owned()))
                .or_default(),
        );
        let mut live = live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.used = Some(Instant::now());
        if live.session.is_none() {
            let launch = launch_of(call, server).map_err(ReachError)?;
            live.session =
                Some(McpSession::start(&launch, within).map_err(|error| said(server, &error))?);
        }
        let session = live.session.as_mut().expect("started above");
        let answered = session.call(tool, arguments.clone(), within);
        live.used = Some(Instant::now());
        answered.map_err(|error| {
            if !error.leaves_session() {
                live.session = None;
            }
            said(server, &error)
        })
    }

    /// Starts, once, the thread that ends sessions idle past [`IDLE`]. It
    /// holds the sessions weakly, so it ends with the runner.
    fn sweep_idle_from_now_on(self: &Arc<Self>) {
        self.sweeping.call_once(|| {
            let sessions: Weak<Self> = Arc::downgrade(self);
            thread::spawn(move || {
                loop {
                    thread::sleep(SWEEP);
                    let Some(sessions) = sessions.upgrade() else {
                        return;
                    };
                    sessions.end_idle(IDLE);
                }
            });
        });
    }

    /// Ends every session not called for `idle`; one in a call is busy and
    /// left alone.
    fn end_idle(&self, idle: Duration) {
        let live = self
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for session in live.values() {
            if let Ok(mut session) = session.try_lock()
                && session.used.is_some_and(|used| used.elapsed() >= idle)
            {
                session.session = None;
                session.used = None;
            }
        }
    }
}

/// `error` as the script reads it, naming the server.
fn said(server: &str, error: &McpError) -> ReachError {
    ReachError(format!("mcp `{server}`: {error}"))
}

/// The launch of `server`: the plugin an `mcp` node names, or the
/// extension's own, read as a harness is handed it.
fn launch_of(call: &Call<'_>, server: &str) -> Result<McpLaunch, String> {
    let named = call
        .manifest
        .mcp
        .iter()
        .find(|decl| decl.server == server)
        .and_then(|decl| decl.plugin.clone());
    let (plugin, marketplace) = match &named {
        Some(named) => named
            .split_once('@')
            .ok_or_else(|| format!("`{named}` is not a plugin as `<plugin>@<marketplace>`"))?,
        None => (
            call.identity.plugin.as_str(),
            call.identity.marketplace.as_str(),
        ),
    };
    let store = UzeStore::new(call.home.clone());
    let id = PackageId::from_marketplace_plugin(marketplace, plugin, &call.home.plugins_dir())
        .map_err(|error| error.to_string())?;
    let package = store
        .package(&id)
        .map_err(|_| format!("{plugin}@{marketplace} is not installed"))?;
    // `${PLUGIN_ROOT}` names the delivered copy, which a plugin no harness
    // was handed yet has not got.
    uze_core::delivered_root::materialize(call.home, &package)
        .map_err(|error| error.to_string())?;
    let (command, args) = uze_integrations::stdio_server_launch(&package, server)?;
    Ok(McpLaunch {
        command,
        args,
        environment: std::env::vars_os()
            .filter_map(|(key, value)| Some((key.into_string().ok()?, value)))
            .collect(),
        cwd: call.checkout.to_path_buf(),
    })
}

/// The copies UZE's extension MCP launcher hands a server it starts
/// (`UzeHome::delivered_package_dir`), kept while their package is
/// installed and declares an MCP server (`mcp.json`), and while a receipt
/// still names one, as an earlier release pointed every harness there; the
/// rest are copies of the earlier layout nothing reads. One is made again
/// on the next launch that needs it. Collects under the mutation lock,
/// which keeps it from an install replacing the packages it judges by.
pub(crate) struct DeliveredPackages;

impl crate::application::Collectable for DeliveredPackages {
    fn name(&self) -> &'static str {
        "delivered packages"
    }

    fn roots(&self) -> &'static [&'static str] {
        &["delivered_package_dir"]
    }

    fn collect(&self, home: &UzeHome) -> crate::application::Collected {
        let Ok(entries) = std::fs::read_dir(home.delivered_packages_dir()) else {
            return crate::application::Collected::removed(self.name(), Vec::new());
        };
        let _mutation = match uze_core::persistence::MutationLock::acquire(home) {
            Ok(lock) => lock,
            Err(error) => {
                return crate::application::Collected::skipped(self.name(), error.to_string());
            }
        };
        // Nothing is judged from a registry or a ledger that cannot be read.
        let Ok(packages) = UzeStore::new(home.clone()).packages() else {
            return crate::application::Collected::skipped(
                self.name(),
                "the installed packages could not be read",
            );
        };
        let Ok(receipts) = uze_core::state::receipts(home, None) else {
            return crate::application::Collected::skipped(
                self.name(),
                "the delivery ledger could not be read",
            );
        };
        let named = serde_json::to_string(&receipts).unwrap_or_default();
        let mut removed = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let serves = packages.iter().any(|package| {
                home.delivered_package_dir(&package.id) == path
                    && package.root.join("mcp.json").is_file()
            });
            if serves || named.contains(path.to_string_lossy().as_ref()) {
                continue;
            }
            let gone = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if gone.is_ok() {
                removed.push(path);
            }
        }
        crate::application::Collected::removed(self.name(), removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stdio MCP server in POSIX `sh`: it answers `initialize`, and every
    /// `tools/call` with its own process id as JSON text.
    #[cfg(unix)]
    const SERVER: &str = r#"while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{}}}\n' "$id" ;;
    *'"tools/call"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"{\\"pid\\":%s}"}]}}\n' "$id" "$$" ;;
  esac
done
"#;

    // The server is the POSIX `sh` stand-in above.
    #[cfg(unix)]
    #[test]
    fn a_plugins_server_is_started_once_kept_between_calls_and_ended_when_idle() {
        let root = uze_testkit::temp::scratch("mcp-session");
        let home = UzeHome::at(root.join("home"));
        let source = root.join("tracker");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("plugin.json"), r#"{"name":"tracker"}"#).unwrap();
        std::fs::write(source.join("server.sh"), SERVER).unwrap();
        std::fs::write(
            source.join("mcp.json"),
            r#"{"mcpServers":{"rtc":{"command":"/bin/sh","args":["${PLUGIN_ROOT}/server.sh"]}}}"#,
        )
        .unwrap();
        UzeStore::new(home.clone())
            .ingest(
                &uze_core::acquisition::acquire(&uze_core::PackageSource::local(source.clone()))
                    .unwrap(),
                "local",
                None,
            )
            .unwrap();

        let identity = Identity {
            marketplace: "local".into(),
            plugin: "tracker".into(),
            id: "timesheet".into(),
        };
        let manifest = Manifest {
            protocol: 1,
            id: "timesheet".into(),
            name: "Timesheet".into(),
            description: String::new(),
            surfaces: Vec::new(),
            wants: Vec::new(),
            sources: Vec::new(),
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
            icons: Vec::new(),
            mcp: Vec::new(),
        };
        let call = Call {
            home: &home,
            identity: &identity,
            manifest: &manifest,
            checkout: &root,
        };
        let sessions = Arc::new(McpSessions::default());
        let pid = |sessions: &Arc<McpSessions>| {
            sessions
                .call(
                    &call,
                    "rtc",
                    "rtc_timesheet",
                    &serde_json::json!({ "month": "2026-10" }),
                )
                .unwrap()["pid"]
                .as_u64()
                .unwrap()
        };
        let first = pid(&sessions);
        assert_eq!(pid(&sessions), first, "the second call reuses the server");
        sessions.end_idle(Duration::ZERO);
        assert_ne!(pid(&sessions), first, "an ended session starts again");

        let absent = sessions
            .call(&call, "jira", "search", &serde_json::json!({}))
            .unwrap_err();
        assert!(absent.0.contains("no MCP server `jira`"), "{absent:?}");
    }

    /// The launcher's copy of a plugin with an MCP server stays; the
    /// earlier layout's copies of one without, and of one gone, go.
    #[test]
    fn the_launchers_copies_are_kept_only_for_an_installed_mcp_server() {
        use crate::application::Collectable as _;
        let root = uze_testkit::temp::scratch("mcp-delivered-packages");
        let home = UzeHome::at(root.join("home"));
        let store = UzeStore::new(home.clone());
        let mut installed = Vec::new();
        for (name, serves) in [("served", true), ("plain", false)] {
            let source = root.join(name);
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(
                source.join("plugin.json"),
                format!(r#"{{"name":"{name}"}}"#),
            )
            .unwrap();
            if serves {
                std::fs::write(
                    source.join("mcp.json"),
                    r#"{"mcpServers":{"rtc":{"command":"/bin/sh"}}}"#,
                )
                .unwrap();
            }
            let package = store
                .ingest(
                    &uze_core::acquisition::acquire(&uze_core::PackageSource::local(source))
                        .unwrap(),
                    "local",
                    None,
                )
                .unwrap();
            installed.push(home.delivered_package_dir(&package.id));
        }
        let gone = home.delivered_packages_dir().join("removed@local");
        for copy in installed.iter().chain([&gone]) {
            std::fs::create_dir_all(copy).unwrap();
            std::fs::write(copy.join("plugin.json"), "{}").unwrap();
        }

        let collected = DeliveredPackages.collect(&home);

        let mut removed = collected.removed.clone();
        removed.sort();
        let mut expected = vec![installed[1].clone(), gone.clone()];
        expected.sort();
        assert_eq!(removed, expected, "{collected:?}");
        assert!(installed[0].is_dir(), "the MCP server's copy was swept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_idle_session_is_forgotten_and_a_used_one_kept() {
        let sessions = McpSessions::default();
        let key = |server: &str| {
            (
                Identity {
                    marketplace: "sandbox".into(),
                    plugin: "timesheet".into(),
                    id: "timesheet".into(),
                },
                server.to_owned(),
            )
        };
        {
            let mut live = sessions.live.lock().unwrap();
            live.insert(
                key("old"),
                Arc::new(Mutex::new(Live {
                    session: None,
                    used: Some(Instant::now() - Duration::from_secs(600)),
                })),
            );
            live.insert(
                key("new"),
                Arc::new(Mutex::new(Live {
                    session: None,
                    used: Some(Instant::now()),
                })),
            );
        }
        sessions.end_idle(IDLE);
        let live = sessions.live.lock().unwrap();
        assert_eq!(live[&key("old")].lock().unwrap().used, None);
        assert!(live[&key("new")].lock().unwrap().used.is_some());
    }
}
