//! Each harness's tool vocabulary, and the matcher a portable hook's tools become in it.

use super::*;

/// The harness's binding of the portable tool vocabulary: per alias, the
/// native tool it is matched as and the native input field each portable
/// field is read from. Each table says where its names were read from —
/// the Lab's `--discovery` mode wherever a capture exists, never memory.
pub(crate) fn vocabulary(target: HookTarget) -> HarnessToolVocabulary {
    HarnessToolVocabulary {
        bindings: target.tools,
    }
}

/// An alias no harness tool answers to. Kept in every table so the
/// vocabulary is exhaustive by construction: absence of a native name is
/// stated, never left to a missing row.
pub(crate) const UNBOUND: Option<&'static str> = None;

/// Every native tool name one matcher intercepts on a target. `native:<name>`
/// passes through unchanged; a portable alias yields every tool this harness
/// binds it to, because a vendor that renames its shell tool keeps answering
/// to the old name for a while and a hook must intercept both. An alias the
/// harness binds to no tool falls back to the alias literal, which matches
/// nothing — an honest no-op rather than a fabricated tool name.
pub(crate) fn tool_names(target: HookTarget, matcher: &HookMatcher) -> Vec<String> {
    match matcher {
        HookMatcher::Native(name) | HookMatcher::Source(name) => vec![name.clone()],
        HookMatcher::Portable(alias) => match vocabulary(target).binding(alias) {
            Some(binding) => {
                let names: Vec<String> = binding
                    .native_tool
                    .into_iter()
                    .chain(binding.also_matches.iter().copied())
                    .map(str::to_owned)
                    .collect();
                if names.is_empty() {
                    vec![alias.clone()]
                } else {
                    names
                }
            }
            None => vec![alias.clone()],
        },
    }
}

/// Translates every matcher of a group for one target; `None` for an
/// unmatch-all group (the entry then omits the matcher key).
pub(crate) fn matcher(target: HookTarget, hook: &PortableHook) -> Option<String> {
    // "Every source" is the portable set, spelled out: a harness reporting
    // a source of its own (a compaction) must not run a handler that was
    // promised `HOOK_SOURCE` is one of these.
    if hook.event == HookEvent::SessionStart && hook.matchers.is_empty() {
        return Some(uze_core::hook::SESSION_SOURCES.join("|"));
    }
    (!hook.matchers.is_empty()).then(|| {
        // Two authored matchers can translate to one native tool (a
        // portable alias plus the `native:` name it already resolves to);
        // the entry names it once.
        let mut names: Vec<String> = Vec::new();
        for entry in &hook.matchers {
            for name in tool_names(target, entry) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names.join("|")
    })
}

// ============================================================================
// Native entry rendering
// ============================================================================
