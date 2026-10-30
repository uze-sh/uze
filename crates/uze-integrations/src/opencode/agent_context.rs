//! The agent's context on OpenCode: a section of UZE's one plugin, whose
//! `session.hook("context")` pushes onto the request's system parts the
//! document UZE wrote for that conversation.
//!
//! One shared background server serves every OpenCode launch and keeps the
//! environment and configuration of whichever launch started it, so nothing
//! can be handed to it per launch. What it does see is the conversation:
//! UZE assigns the session id before the launch, writes the document under
//! that id, and the plugin — found in the auto-discovered global plugins
//! directory, loaded by a running server without a restart — reads it on
//! every request. A conversation UZE did not launch has no document and is
//! given nothing, unless it is a child of one that has: a subagent runs in
//! a child session with an id of its own, and the plugin follows its
//! `parentID` (asked of `ctx.session.get`) to the nearest session holding a
//! document.
//!
//! Measured on 2.0.31 (`experiments/opencode/agent-context`): the document
//! lands in a `system` part, a resumed process reads the current one, and
//! no copy is kept in the conversation (`opencode session export` holds
//! none). Measured on 2.0.32 (`experiments/opencode/session-parent`): the
//! `context` event of a subagent's request names only the child session,
//! and `ctx.session.get({ sessionID })` answers it with the parent's id.

use std::{fs, path::Path};

use uze_core::{
    Result,
    integration::{AgentContext, ContextContribution},
};

use crate::shared::agent_context::write_private;

use super::runtime_plugin::Section;

/// The plugin's section reading documents from `dir`. The directory is
/// written into the source rather than read from the environment, because
/// the shared server's environment is not the launch's.
pub(super) fn section(dir: &Path) -> Section {
    let dir = serde_json::to_string(&dir.to_string_lossy()).expect("a path serializes");
    Section {
        name: "agent-context",
        source: format!(
            r#"// The context UZE composed for the conversation it launched, pushed onto the
// system parts of every request. A subagent runs in a child session of its
// own and is given the document of the nearest session above it that has
// one.
async function agentContext(ctx) {{
  const DIR = {dir};
  // A chain of parents is short; the bound only keeps a record that names
  // itself from looping.
  const ANCESTORS = 8;

  async function documentFor(sessionID) {{
    let id = sessionID;
    for (let depth = 0; id && depth <= ANCESTORS; depth++) {{
      const file = join(DIR, `${{id}}.md`);
      if (existsSync(file)) return file;
      let info;
      try {{
        info = await ctx.session.get({{ sessionID: id }});
      }} catch {{
        return null;
      }}
      id = (info?.data ?? info)?.parentID;
    }}
    return null;
  }}

  await ctx.session.hook("context", async (event) => {{
    if (!event.sessionID || !Array.isArray(event.system)) return;
    const file = await documentFor(event.sessionID);
    if (!file) return;
    const text = readFileSync(file, "utf8");
    const first = event.system[0];
    event.system.push(typeof first === "object" && first !== null ? {{ ...first, text }} : text);
  }});
}}"#
        ),
    }
}

/// Writes the document under the conversation UZE assigned this launch.
pub(super) fn deliver(dir: &Path, ctx: &AgentContext) -> Result<ContextContribution> {
    let Some(session) = ctx.session else {
        return Ok(ContextContribution::default());
    };
    write_private(&dir.join(format!("{}.md", session.as_str())), ctx.text)?;
    Ok(ContextContribution::default())
}

/// Removes the conversation's document, so the plugin gives it nothing.
pub(super) fn withdraw(dir: &Path, session: Option<&uze_core::session::SessionId>) {
    if let Some(session) = session {
        let _ = fs::remove_file(dir.join(format!("{}.md", session.as_str())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uze_core::session::SessionId;

    #[test]
    fn the_section_reads_the_conversations_document_and_falls_back_to_its_parent() {
        let dir = PathBuf::from("/runtime/sessions/opencode");
        let source = section(&dir).source;
        assert!(source.contains("ctx.session.hook(\"context\""));
        assert!(
            source.contains("ctx.session.get({ sessionID: id })") && source.contains(".parentID"),
            "a child session falls back to its parent's document"
        );
        let dir_literal = serde_json::to_string(&dir.to_string_lossy()).unwrap();
        assert!(source.contains(&format!("const DIR = {dir_literal};")));
    }

    #[test]
    fn the_document_is_written_under_the_assigned_conversation() {
        let root = uze_testkit::temp::scratch("opencode-agent-context-deliver");
        let session = SessionId::parse("ses_abc").unwrap();
        let contribution = deliver(
            &root,
            &AgentContext {
                dir: &root.join("agent"),
                file: &root.join("agent").join("context.md"),
                text: "facts",
                instructions: "",
                notes: Some("facts"),
                checkout: &root,
                link: &root.join("link"),
                launch: "l",
                session: Some(&session),
            },
        )
        .unwrap();
        assert_eq!(
            contribution,
            ContextContribution::default(),
            "nothing in argv or env"
        );
        assert_eq!(
            fs::read_to_string(root.join("ses_abc.md")).unwrap(),
            "facts"
        );
        withdraw(&root, Some(&session));
        assert!(
            !root.join("ses_abc.md").exists(),
            "a launch with nothing to give leaves nothing behind"
        );
        let _ = fs::remove_dir_all(root);
    }
}
