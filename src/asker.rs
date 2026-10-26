//! Who is asking for a person's answer: the one reading of the stamps an
//! agent UZE launched carries, shared by the CLI's questions and the
//! workspace's Extensions screen.

/// Who is asking for a person's answer, as this process's environment
/// says: an agent's pane when it carries the stamp of an agent UZE
/// launched. Either stamp is enough to refuse: the key proves which agent
/// is asking, but any process carrying an agent's identity is one a
/// project launched. What a project's commands and an extension's consent
/// are both refused on.
pub fn asker() -> uze_application::Asker {
    let stamped = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if stamped(uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
        || stamped(uze_terminal::launch::AGENT_KEY_VARIABLE)
    {
        uze_application::Asker::AgentPane
    } else {
        uze_application::Asker::Operator
    }
}
