//! Hook projections owned by harness integrations (ADR-033, ADR-040):
//! native JSON configuration merging, the generated `hooks/exec` wrapper
//! each command-hook harness runs, and the owned OpenCode bridge.
//!
//! What each harness's hooks *are* — the events and effects it preserves,
//! the tools it names, the dialect its wrapper answers in — is a
//! [`HookTarget`] its own vertical declares (`claude::HOOKS`, ...); nothing
//! here branches on which harness it is projecting into.
//!
//! The wrapper is the only implementation of the hook ABI: it reads that
//! harness's payload, runs the author's handlers and answers in that
//! harness's dialect, with no UZE binary anywhere on the execution path. A
//! platform it has no template for delivers no hook at all, and says so.
//!
//! Everything here is deterministic and vendor-local; the vendor-neutral
//! vocabulary (`PortableHook`, `HookCapabilities`, `assess`, ABI types)
//! lives in `uze-core::hook`. Every vendor contract in this module is a
//! documented mapping, verified by deterministic fixtures — real-binary
//! conformance evidence is recorded per integration in the Conformance Lab.

use std::{fs, path::Path, path::PathBuf};

use uze_core::{
    Result, UzeError,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, ManagedArtifact},
    home::UzeHome,
    hook::{
        CommandHook, HOOKS_FILE_NAME, HarnessToolVocabulary, HookCapabilities, HookEffect,
        HookEvent, HookMatcher, PortableHook, ToolBinding,
    },
    integration::{AttachmentInspection, AttachmentState},
    router::CompatibilityRoute,
};

use crate::shared::json_config;
use crate::shared::plan::{blocked, unsupported};

mod bridge;
mod entries;
mod event_entry;
mod tools;
mod wrapper;

pub(crate) use bridge::*;
pub(crate) use entries::*;
pub(crate) use event_entry::*;
pub(crate) use tools::*;
pub(crate) use wrapper::*;

/// What one harness's hooks are: the semantic axes it preserves, the tools
/// it names, and how a hook reaches it. Each integration declares its own
/// beside the rest of its vertical; this module only reads them.
#[derive(Clone, Copy)]
pub(crate) struct HookTarget {
    /// The harness's name in UZE's own state and in `HOOK_HARNESS`.
    pub key: &'static str,
    pub events: &'static [HookEvent],
    pub effects: &'static [HookEffect],
    /// The harness's binding of the portable tool vocabulary: the single
    /// source the matchers, the generated wrapper and the bridge all read.
    pub tools: &'static [ToolBinding],
    pub runner: HookRunner,
}

/// What runs a delivered hook on the harness.
#[derive(Clone, Copy)]
pub(crate) enum HookRunner {
    /// A command hook in the harness's shared config file, starting the
    /// generated wrapper.
    Wrapper {
        dialect: WrapperDialect,
        entry: EntryShape,
    },
    /// UZE's generated plugin is the runner: the harness has no command
    /// hooks to start a wrapper from.
    Bridge,
}

/// The form a command-hook harness's shared config gives one entry.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum EntryShape {
    /// An array of group entries per event, whose handler is `command` plus
    /// `args`: the wrapper is started directly, with nothing to quote.
    EventExec,
    /// An array of group entries per event, whose handler is one shell line.
    EventLine,
    /// A map of named hooks, each keyed by UZE's entry name.
    Named,
}

impl PartialEq for HookTarget {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

#[cfg(test)]
impl std::fmt::Debug for HookTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.key)
    }
}

#[cfg(test)]
impl std::fmt::Display for HookTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.key)
    }
}

/// One delivered hook entry, as its receipt records it.
pub(crate) struct HookEntry<'a> {
    pub config_file: &'a Path,
    pub entry_name: &'a str,
    pub event: HookEvent,
    pub expected: &'a str,
    pub wrapper: &'a Path,
}

impl<'a> HookEntry<'a> {
    /// The entry a `HookConfigEntry` receipt records; `None` for any other
    /// artifact.
    pub(crate) fn recorded(artifact: &'a ManagedArtifact) -> Option<Self> {
        let ManagedArtifact::HookConfigEntry {
            config_file,
            entry_name,
            event,
            expected,
            wrapper,
        } = artifact
        else {
            return None;
        };
        Some(Self {
            config_file,
            entry_name,
            event: *event,
            expected,
            wrapper,
        })
    }
}

impl HookTarget {
    /// The harness's name in UZE's own state and in `HOOK_HARNESS`.
    pub(crate) const fn key(self) -> &'static str {
        self.key
    }

    /// The semantic axes this harness preserves.
    pub(crate) fn capabilities(self) -> HookCapabilities {
        HookCapabilities {
            events: self.events.iter().copied().collect(),
            effects: self.effects.iter().copied().collect(),
            supports_native_matchers: true,
            executes_handlers_in_order: true,
            unfired: wrapper::unfired_here(self)
                .iter()
                .map(|(event, tool, why)| uze_core::hook::UnfiredTool {
                    event: *event,
                    tool: (*tool).to_owned(),
                    why: (*why).to_owned(),
                })
                .collect(),
            ..HookCapabilities::default()
        }
    }

    /// How this harness's payload is read and its decision written; `None`
    /// where UZE's generated plugin is its own runner.
    pub(super) const fn dialect(self) -> Option<WrapperDialect> {
        match self.runner {
            HookRunner::Wrapper { dialect, .. } => Some(dialect),
            HookRunner::Bridge => None,
        }
    }

    fn entry_shape(self) -> Option<EntryShape> {
        match self.runner {
            HookRunner::Wrapper { entry, .. } => Some(entry),
            HookRunner::Bridge => None,
        }
    }

    /// Where this harness keeps its shared wrapper: one file under UZE's own
    /// state, never in the Store and never in the harness's own directories.
    /// Byte-identical for every package, so one file serves them all.
    pub(crate) fn wrapper_path(self, uze_home: &UzeHome) -> PathBuf {
        crate::shared::path::attachment_root(uze_home, self.key()).join(WRAPPER_RELATIVE_PATH)
    }

    /// Whether a wrapper can be written and run for this harness here.
    fn deliverable(self) -> bool {
        wrapper_source(self).is_some()
    }

    /// Whether the shared config is a map of named hooks rather than an
    /// array of group entries per event.
    fn names_entries(self) -> bool {
        self.entry_shape() == Some(EntryShape::Named)
    }

    /// The plan for a command-hook harness: one entry in its shared config
    /// file, running the generated wrapper, and receipt-owned by content.
    pub(crate) fn entry_plan(
        self,
        uze_home: &UzeHome,
        resource: &Resource,
        config_file: PathBuf,
        evidence: &str,
    ) -> ExposurePlan {
        hook_plan(resource, &self.capabilities(), false, evidence, |hook| {
            if !self.deliverable() {
                return None;
            }
            let wrapper = self.wrapper_path(uze_home);
            let entry = if self.names_entries() {
                agy_named_entry(self, hook, &wrapper, &resource.package_root)
            } else {
                self.event_entry(hook, &resource.package_root, &wrapper)
            };
            Some(ManagedArtifact::HookConfigEntry {
                config_file,
                entry_name: hook_entry_name(resource, hook),
                event: hook.event,
                expected: serde_json::to_string(&entry).expect("hook entry serializes"),
                wrapper,
            })
        })
    }

    /// Writes the wrapper, then the entry that names it.
    pub(crate) fn attach_entry(
        self,
        uze_home: &UzeHome,
        integration_id: &str,
        entry: &HookEntry,
    ) -> Result<()> {
        if let Some(source) = wrapper_source(self) {
            materialize_wrapper(entry.wrapper, &source)?;
        }
        let expected: serde_json::Value =
            serde_json::from_str(entry.expected).map_err(|source| UzeError::Json {
                path: entry.config_file.to_path_buf(),
                source,
            })?;
        if self.names_entries() {
            merge_named_entry(entry.config_file, entry.entry_name, &expected)?;
        } else {
            let previous = previous_hook_entry_content(uze_home, integration_id, entry.entry_name)?;
            merge_event_entry(entry.config_file, entry.event, &expected, &previous)?;
        }
        Ok(())
    }

    /// The delivered entry's state: its wrapper first, then the entry.
    pub(crate) fn inspect_entry(self, entry: &HookEntry) -> AttachmentInspection {
        match inspect_wrapper(self, entry.wrapper) {
            WrapperState::Current => self.entry_state(entry),
            WrapperState::Stale => {
                let mut inspection = self.entry_state(entry);
                if inspection.state == AttachmentState::Matched {
                    inspection.reason.push_str(
                        "; the hook wrapper was generated by an earlier build and the next install rewrites it",
                    );
                }
                inspection
            }
            WrapperState::Broken(inspection) => inspection,
        }
    }

    /// Removes a matched entry, then the shared wrapper once nothing runs it.
    pub(crate) fn detach_entry(
        self,
        uze_home: &UzeHome,
        integration_id: &str,
        entry: &HookEntry,
    ) -> Result<AttachmentInspection> {
        let detached = if self.names_entries() {
            remove_named_entry(entry.config_file, entry.entry_name, entry.expected)?
        } else {
            remove_event_entry(entry.config_file, entry.event, entry.expected)?
        };
        prune_shared_wrapper(uze_home, integration_id, self);
        Ok(detached)
    }

    /// The entry alone, without the wrapper it names.
    fn entry_state(self, entry: &HookEntry) -> AttachmentInspection {
        if self.names_entries() {
            inspect_named_entry(entry.config_file, entry.entry_name, entry.expected)
        } else {
            inspect_event_entry(entry.config_file, entry.event, entry.expected)
        }
    }

    /// The group entry for an event-array harness, in the handler form its
    /// entries take.
    fn event_entry(
        self,
        hook: &PortableHook,
        package_root: &Path,
        wrapper: &Path,
    ) -> serde_json::Value {
        let invocation = if self.entry_shape() == Some(EntryShape::EventExec) {
            wrapper_exec(wrapper, hook, package_root)
        } else {
            HookInvocation::Line(wrapper_command_line(wrapper, hook, package_root))
        };
        group_entry(self, hook, &invocation)
    }
}

// ============================================================================
// Matcher translation
// ============================================================================

/// All hook groups a package's stored `hooks.json` declares, in manifest
/// order — the materialization input for the OpenCode bridge.
pub(crate) fn package_hook_groups(package_root: &Path) -> Result<Vec<PortableHook>> {
    let manifest_path = package_root.join(HOOKS_FILE_NAME);
    let bytes = fs::read(&manifest_path).map_err(UzeError::read(&manifest_path))?;
    uze_core::hook::parse_manifest(&manifest_path, &bytes)
}

/// Canonical hook groups filtered to a set of group ids, preserving
/// manifest order; `None` when the package declares no canonical hooks.
pub(crate) fn groups_with_ids(
    package_root: &Path,
    keep: &dyn Fn(&str) -> bool,
) -> Result<Vec<PortableHook>> {
    let mut groups = package_hook_groups(package_root)?;
    groups.retain(|group| keep(&group.id));
    Ok(groups)
}

/// Parses a hook resource's payload into its portable group and computes the
/// per-resource plan: semantic compatibility from the vendor profile —
/// `bridged` when UZE's own generated runner carries the hook — and the
/// artifact `deliver` renders for it. A `degraded` or `unsupported` route
/// never attaches, and neither does a group `deliver` has no artifact for
/// on this platform: the mechanism carries the diagnostic instead.
pub(crate) fn hook_plan(
    resource: &Resource,
    capabilities: &HookCapabilities,
    bridged: bool,
    evidence: &str,
    deliver: impl FnOnce(&PortableHook) -> Option<ManagedArtifact>,
) -> ExposurePlan {
    let Ok(hook) = serde_json::from_slice::<PortableHook>(&resource.capability.payload) else {
        return unsupported("hook resource payload is not a valid portable hook group");
    };
    let compatibility = uze_core::hook::assess(&hook, capabilities, bridged);
    // What the route cannot carry leads, so a report showing one sentence
    // shows the reason; the mechanism follows.
    let with_compatibility =
        |reason: &str| format!("{}. {evidence}", reason.trim_end_matches('.').trim_end());
    if matches!(
        compatibility.route,
        CompatibilityRoute::Unsupported | CompatibilityRoute::Degraded
    ) {
        return ExposurePlan {
            route: compatibility.route,
            mechanism: ExposureMechanism::Unsupported {
                rationale: compatibility
                    .reason
                    .clone()
                    .unwrap_or_else(|| "no compatible hook route".to_owned()),
            },
            evidence: compatibility
                .reason
                .as_deref()
                .map_or_else(|| evidence.to_owned(), with_compatibility),
        };
    }
    // A handler is never run in a shell it was not written for, so a group
    // with one that has no spelling here delivers nothing here.
    if let Some(unspelled) = hook
        .handlers
        .iter()
        .find(|handler| handler.command.here().is_none())
    {
        return unsupported(format!(
            "hook `{}` has no {} spelling for `{}`, so it is not delivered on this platform",
            hook.id,
            uze_core::shell::ShellCommand::platform(),
            unspelled.command
        ));
    }
    match deliver(&hook) {
        Some(artifact) => ExposurePlan {
            route: compatibility.route,
            mechanism: ExposureMechanism::Managed(artifact),
            evidence: compatibility
                .reason
                .as_deref()
                .map_or_else(|| evidence.to_owned(), with_compatibility),
        },
        // A semantic route the delivery cannot take is not a route.
        None => ExposurePlan {
            route: CompatibilityRoute::Unsupported,
            mechanism: ExposureMechanism::Unsupported {
                rationale: NO_WRAPPER_TEMPLATE.to_owned(),
            },
            evidence: compatibility.reason.as_deref().map_or_else(
                || format!("{NO_WRAPPER_TEMPLATE}. {evidence}"),
                with_compatibility,
            ),
        },
    }
}

/// The stable UZE identity for one hook group entry, mirroring the
/// qualified-capability naming policy (ADR-026): `<package>:<hook-id>`.
pub(crate) fn hook_entry_name(resource: &Resource, hook: &PortableHook) -> String {
    format!("{}:{}", resource.package_id.as_str(), hook.id)
}

#[cfg(test)]
mod tests;

/// The wrapper this platform's harnesses run, on its own platform.
#[cfg(test)]
mod host_wrapper_tests;

/// The fixtures every wrapper answers, and the answers recorded for them.
#[cfg(test)]
mod fixture_set;

/// The recorded answers, held to by the wrapper of the platform it runs on.
#[cfg(test)]
mod wrapper_parity_tests;

/// The generated wrapper against real `sh`: the same cases the reference
/// runtime answers, run through the file a harness would actually execute.
// The generated POSIX wrapper, run by a real `sh`.
#[cfg(all(test, unix))]
mod wrapper_tests;

/// The generated OpenCode plugin against the real Bun runtime, driven with a
/// V2-shaped plugin context. Skipped where Bun is absent: the plugin is a
/// delivered artifact for a harness that embeds Bun, and the goldens above
/// keep its bytes honest without it.
// Runs the generated plugin under Bun with Unix file modes.
#[cfg(all(test, unix))]
mod opencode_runtime_tests;
