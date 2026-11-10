//! Which way an agent is told to remember, by the harness it runs on: a
//! harness whose own system prompt carries a memory instruction keeps its
//! agents remembering its way, and UZE reads what they keep.

use uze_application::application::memory::{MEMORY_INSTRUCTION, context_sections};
use uze_core::{UzeHome, integration::IntegrationPort};
use uze_integrations::{claude::ClaudeIntegration, codex::CodexIntegration};
use uze_workspace::agent_context::{MEMORY_SHARE, SectionAsk};

fn told_to_write_through_uze(
    launching: &dyn IntegrationPort,
    home: &UzeHome,
    project: &std::path::Path,
) -> bool {
    let ask = SectionAsk {
        project_root: project,
        checkout: project,
        room: MEMORY_SHARE,
    };
    context_sections(home, &ask, launching, &[launching])
        .iter()
        .any(|section| section.text.contains(MEMORY_INSTRUCTION))
}

/// One model told two ways to remember splits the memory, so a Claude
/// agent, whose system prompt tells it to keep Claude's own, is never also
/// told to write through UZE; a Codex agent is.
#[test]
fn only_an_agent_whose_harness_keeps_no_memory_of_its_own_is_told_to_write_through_uze() {
    let root = uze_testkit::temp::scratch("harness-memory-instruction");
    let project = root.join("repo");
    std::fs::create_dir_all(&project).unwrap();
    let home = UzeHome::at(root.join("uze"));
    let claude = ClaudeIntegration::new(root.join("home/.claude"), home.clone());
    let codex = CodexIntegration::new(root.join("home/.codex"), home.clone());

    assert!(!told_to_write_through_uze(&claude, &home, &project));
    assert!(told_to_write_through_uze(&codex, &home, &project));
    let _ = std::fs::remove_dir_all(root);
}
