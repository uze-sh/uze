//! Who is asking for a person's answer: the one reading of the stamps an
//! agent UZE launched carries, and of what the kernel says about this
//! process, shared by the CLI's questions and the workspace's Extensions
//! screen.

/// Who is asking for a person's answer. An agent's pane when this process
/// carries the stamp of an agent UZE launched (either stamp is enough: the
/// key proves which agent is asking, but any process carrying an agent's
/// identity is one a project launched), or when the kernel reports it
/// confined. A sealed agent pane is, and so is every common harness
/// sandbox, and nothing a process does (an environment stripped of the
/// stamps, a terminal emulator around it) undoes that (audit WKS-4). What
/// a project's commands and an extension's consent are both refused on.
pub fn asker() -> uze_application::Asker {
    let stamped = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    asked_by(
        stamped(uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
            || stamped(uze_terminal::launch::AGENT_KEY_VARIABLE),
        uze_platform::seal::this_process_runs_confined(),
    )
}

fn asked_by(stamped: bool, confined: bool) -> uze_application::Asker {
    if stamped || confined {
        uze_application::Asker::AgentPane
    } else {
        uze_application::Asker::Operator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confined_process_is_never_the_operator_whatever_its_environment() {
        assert_eq!(asked_by(false, true), uze_application::Asker::AgentPane);
        assert_eq!(asked_by(true, false), uze_application::Asker::AgentPane);
        assert_eq!(asked_by(false, false), uze_application::Asker::Operator);
    }
}
