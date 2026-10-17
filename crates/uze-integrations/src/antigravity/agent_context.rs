//! The agent's context on Antigravity: a `PreInvocation` hook in the
//! agent's own `--add-dir` directory, loaded for that process only.
//!
//! Measured on 1.4.0, signed in (`experiments/antigravity/agent-context`):
//! the hook's `injectSteps[].systemMessage` reaches the model apart from
//! the prompt, nothing is drawn in the pane, and the added directory leaves
//! `workspacePaths[0]` on the project. A `systemMessage` stays in the
//! conversation, so the handler gives it once per launch — a marker named
//! by the launch makes every later model call of the process print nothing
//! — and the channel accumulates one copy per launch, like Claude's and
//! Codex's. `ephemeralMessage` was measured too and rejected: it is sent
//! again on every model call, outside the prefix a vendor caches.
//!
//! Hooks run only while the server-side `json-hooks-enabled` flag is on,
//! which the Lab saw on whenever the CLI was signed in. Signed in is
//! therefore the proxy asked before a launch relies on the channel.

use std::{fs, path::Path};

use uze_core::{
    Result, UzeError,
    integration::{AgentContext, ContextContribution},
};

use crate::shared::agent_context::write_private;

/// The file Antigravity's CLI keeps its sign-in in.
const SIGN_IN: &str = "antigravity-oauth-token";

/// What the handler prints, rendered once at launch: the context as the
/// one step Antigravity injects.
const PAYLOAD: &str = "context.antigravity.json";
const MARKER_PREFIX: &str = "delivered-";

/// Why the channel cannot run here: hooks are gated on the account.
pub(super) fn unavailable(cli_root: &Path) -> Option<String> {
    (!cli_root.join(SIGN_IN).is_file()).then(|| {
        "Antigravity runs hooks only while signed in, so this agent's context cannot reach it"
            .to_owned()
    })
}

pub(super) fn deliver(ctx: &AgentContext) -> Result<ContextContribution> {
    sweep_markers(ctx.dir);
    let payload = ctx.dir.join(PAYLOAD);
    let step = serde_json::json!({
        "injectSteps": [{ "systemMessage": { "systemMessage": ctx.text } }]
    });
    write_private(&payload, &step.to_string())?;

    let marker = ctx.dir.join(format!("{MARKER_PREFIX}{}", ctx.launch));
    let script = ctx
        .dir
        .join(uze_platform::shell::spelling("deliver", "deliver.ps1"));
    uze_platform::executable::install(&script, handler(&payload, &marker).as_bytes())
        .map_err(UzeError::write(&script))?;

    let hooks = serde_json::json!({
        "uze-agent-context": {
            "PreInvocation": [{
                "type": "command",
                "command": uze_platform::shell::sealed_script_line(&script.to_string_lossy(), &[]),
                "timeout": 10,
            }]
        }
    });
    write_private(
        &ctx.dir.join(".agents").join("hooks.json"),
        &serde_json::to_string_pretty(&hooks).expect("hooks serialize"),
    )?;
    Ok(ContextContribution {
        args: vec!["--add-dir".into(), ctx.dir.as_os_str().to_owned()],
        env: Vec::new(),
    })
}

/// Prints the payload while this launch's marker is absent, then leaves
/// the marker.
fn handler(payload: &Path, marker: &Path) -> String {
    let payload = uze_platform::shell::quote(&payload.to_string_lossy());
    let marker = uze_platform::shell::quote(&marker.to_string_lossy());
    uze_platform::shell::spelling(
        &format!("#!/bin/sh\n[ -e {marker} ] || {{ cat {payload} && : > {marker}; }}\n"),
        &format!(
            "if (-not (Test-Path -LiteralPath {marker})) {{ \
             [Console]::Out.Write((Get-Content -Raw -LiteralPath {payload})); \
             New-Item -ItemType File -Path {marker} | Out-Null }}\n"
        ),
    )
    .to_owned()
}

/// Earlier launches' markers: a launch is named afresh each time, so none
/// of them can suppress this one, and they are only clutter.
fn sweep_markers(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(MARKER_PREFIX)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_writes_its_hook_its_payload_and_adds_its_directory() {
        let root = uze_testkit::temp::scratch("antigravity-agent-context");
        let dir = root.join("agent");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("delivered-old"), "").unwrap();
        let file = dir.join("context.md");
        let contribution = deliver(&AgentContext {
            dir: &dir,
            file: &file,
            text: "facts \"quoted\"",
            launch: "launch-1",
            session: None,
        })
        .unwrap();

        assert_eq!(
            contribution.args,
            ["--add-dir".into(), dir.as_os_str().to_owned()]
        );
        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(PAYLOAD)).unwrap()).unwrap();
        assert_eq!(
            payload["injectSteps"][0]["systemMessage"]["systemMessage"],
            "facts \"quoted\""
        );
        let hooks: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join(".agents").join("hooks.json")).unwrap(),
        )
        .unwrap();
        assert!(hooks["uze-agent-context"]["PreInvocation"][0]["command"].is_string());
        assert!(!dir.join("delivered-old").exists(), "old markers are swept");
        let _ = fs::remove_dir_all(root);
    }

    // Runs the POSIX handler directly; the PowerShell one is the same two
    // steps, and Windows has no `sh` to run this one with.
    #[cfg(unix)]
    #[test]
    fn the_handler_prints_once_per_launch() {
        let root = uze_testkit::temp::scratch("antigravity-agent-context-once");
        let dir = root.join("agent");
        fs::create_dir_all(&dir).unwrap();
        deliver(&AgentContext {
            dir: &dir,
            file: &dir.join("context.md"),
            text: "facts",
            launch: "launch-1",
            session: None,
        })
        .unwrap();
        let run = || {
            std::process::Command::new(dir.join("deliver"))
                .output()
                .unwrap()
                .stdout
        };
        assert!(String::from_utf8(run()).unwrap().contains("facts"));
        assert!(
            run().is_empty(),
            "a later model call of the same launch prints nothing"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn signed_out_is_unavailable() {
        let root = uze_testkit::temp::scratch("antigravity-agent-context-signed-out");
        fs::create_dir_all(&root).unwrap();
        assert!(unavailable(&root).is_some());
        fs::write(root.join(SIGN_IN), "token").unwrap();
        assert!(unavailable(&root).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
