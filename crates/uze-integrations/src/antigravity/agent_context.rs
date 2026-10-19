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
//!
//! The instructions do not ride the hook. Measured on 1.4.2
//! (`experiments/antigravity/system-instruction`): a hook's step lands as a
//! user-role message after the operator's first request, below the
//! `systemInstruction` that carries `AGENTS.md`, while an `always_on` rule
//! in the added directory lands in `systemInstruction` beside it, and
//! reaches a resume and an `invoke_subagent` too. So the instructions are
//! that rule, and the hook carries the notes.
//!
//! Two more facts shape where the directory is named
//! (`experiments/antigravity/workspace-order` and `context-link`):
//! - Antigravity sorts its workspace paths, after cleaning them but without
//!   resolving links, and works in the first. The directory is therefore
//!   named through the launch's `link`, a path that sorts after the
//!   checkout, and never by its own path under `$UZE_HOME`, which sorts
//!   first and made the agent work in UZE's runtime directory.
//! - From each added path Antigravity walks up to the nearest `.git`,
//!   loading the `AGENTS.md` and rules it meets. From a link beside a slot
//!   that walk reached the primary checkout, another branch's
//!   instructions. A `.git` file in the directory stops it there.

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

/// The rule the instructions are written as, under the directory's own
/// `.agents/rules/`.
const INSTRUCTIONS_RULE: &str = "uze-instructions.md";

/// Why the channel cannot run here: hooks are gated on the account.
pub(super) fn unavailable(cli_root: &Path) -> Option<String> {
    (!cli_root.join(SIGN_IN).is_file()).then(|| {
        "Antigravity runs hooks only while signed in, so this agent's context cannot reach it"
            .to_owned()
    })
}

pub(super) fn deliver(ctx: &AgentContext) -> Result<ContextContribution> {
    sweep_markers(ctx.dir);
    bound_the_parent_walk(ctx.dir)?;
    write_instructions_rule(ctx.dir, ctx.instructions)?;
    write_notes_hook(ctx)?;
    let named = link(ctx.dir, ctx.link);
    Ok(ContextContribution {
        // The checkout root is named too: it is a prefix of every path
        // inside it, so it sorts first even when the agent was launched in
        // a directory below it that the link would sort ahead of.
        args: vec![
            "--add-dir".into(),
            ctx.checkout.as_os_str().to_owned(),
            "--add-dir".into(),
            named.as_os_str().to_owned(),
        ],
        env: Vec::new(),
    })
}

/// A `.git` file naming no repository, so Antigravity's walk up from the
/// added directory stops at it rather than at the checkout's repository.
fn bound_the_parent_walk(dir: &Path) -> Result<()> {
    let marker = dir.join(".git");
    let pointer = format!("gitdir: {}\n", dir.join(".no-repository").display());
    write_private(&marker, &pointer)
}

fn write_instructions_rule(dir: &Path, instructions: &str) -> Result<()> {
    let rules = dir.join(".agents").join("rules");
    let rule = rules.join(INSTRUCTIONS_RULE);
    if instructions.trim().is_empty() {
        let _ = fs::remove_file(&rule);
        return Ok(());
    }
    fs::create_dir_all(&rules).map_err(UzeError::write(&rules))?;
    write_private(
        &rule,
        &format!("---\ntrigger: always_on\n---\n\n{}\n", instructions.trim()),
    )
}

/// The hook that prints the notes once per launch, or none when there are
/// no notes to give.
fn write_notes_hook(ctx: &AgentContext) -> Result<()> {
    let hooks_file = ctx.dir.join(".agents").join("hooks.json");
    let Some(notes) = ctx.notes else {
        let _ = fs::remove_file(&hooks_file);
        return Ok(());
    };
    let payload = ctx.dir.join(PAYLOAD);
    let step = serde_json::json!({
        "injectSteps": [{ "systemMessage": { "systemMessage": notes } }]
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
        &hooks_file,
        &serde_json::to_string_pretty(&hooks).expect("hooks serialize"),
    )
}

/// Makes `link` reach `dir` and answers with the path to name. A link that
/// cannot be made is a launch that still starts: the directory is named by
/// its own path, which costs the working directory and not the context.
fn link(dir: &Path, link: &Path) -> std::path::PathBuf {
    let made = (|| {
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        if link.symlink_metadata().is_ok() {
            uze_platform::fs::remove_link(link)?;
        }
        uze_platform::fs::symlink(dir, link)
    })();
    match made {
        Ok(()) => link.to_path_buf(),
        Err(error) => {
            tracing::warn!(link = %link.display(), %error, "the agent's context could not be linked beside its checkout");
            dir.to_path_buf()
        }
    }
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

    fn context<'a>(dir: &'a Path, file: &'a Path, link: &'a Path) -> AgentContext<'a> {
        AgentContext {
            dir,
            file,
            checkout: Path::new("/repo"),
            text: "## Instructions\n\nfacts \"quoted\"",
            instructions: "## Instructions",
            notes: Some("facts \"quoted\""),
            link,
            launch: "launch-1",
            session: None,
        }
    }

    #[test]
    fn a_launch_writes_its_rule_its_hook_and_names_its_link() {
        let root = uze_testkit::temp::scratch("antigravity-agent-context");
        let dir = root.join("agent");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("delivered-old"), "").unwrap();
        let file = dir.join("context.md");
        let link = root.join("repo").join(".worktrees").join("ab12cd~uze");
        let contribution = deliver(&context(&dir, &file, &link)).unwrap();

        assert_eq!(
            contribution.args,
            [
                "--add-dir".into(),
                std::ffi::OsString::from("/repo"),
                "--add-dir".into(),
                link.as_os_str().to_owned()
            ],
            "the checkout root is named first, and the directory by a path that sorts after it"
        );
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(&dir).unwrap()
        );
        assert!(
            fs::read_to_string(dir.join(".git"))
                .unwrap()
                .starts_with("gitdir: "),
            "the walk up from the directory stops at it"
        );
        let rule = fs::read_to_string(dir.join(".agents/rules").join(INSTRUCTIONS_RULE)).unwrap();
        assert!(rule.starts_with("---\ntrigger: always_on\n---\n"), "{rule}");
        assert!(rule.contains("## Instructions"), "{rule}");
        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(PAYLOAD)).unwrap()).unwrap();
        assert_eq!(
            payload["injectSteps"][0]["systemMessage"]["systemMessage"], "facts \"quoted\"",
            "the hook carries the notes, never the instructions"
        );
        let hooks: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join(".agents").join("hooks.json")).unwrap(),
        )
        .unwrap();
        assert!(hooks["uze-agent-context"]["PreInvocation"][0]["command"].is_string());
        assert!(!dir.join("delivered-old").exists(), "old markers are swept");

        // A relaunch replaces the link, and a launch with no notes has no hook.
        let mut again = context(&dir, &file, &link);
        again.notes = None;
        deliver(&again).unwrap();
        assert!(!dir.join(".agents").join("hooks.json").exists());
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(&dir).unwrap()
        );
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
        let file = dir.join("context.md");
        let link = root.join("link");
        deliver(&AgentContext {
            notes: Some("facts"),
            ..context(&dir, &file, &link)
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
