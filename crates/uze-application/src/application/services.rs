//! Capability-scoped views onto [`UzeApplication`].
//!
//! One handle that can do everything is the shape that makes "I want to add
//! one feature and I have to touch the middle" true: every operation lands
//! on the same `impl`, and the type says nothing about what a caller is
//! allowed to reach. It also makes scoping inexpressible — "this caller may
//! read packages but must never write the Store" has no way to be said when
//! the only handle there is can do both.
//!
//! Each service here is a borrowed view: no state of its own, no cost, and
//! the state stays owned in one place. What changes is that a caller now
//! names the capability it wants, and gets only that.
//!
//! Service boundaries follow the module the operations already lived in —
//! those files were drawn deliberately, and redrawing them in the same
//! change would have made the diff argue two things at once.

use super::UzeApplication;

/// The current directory's workspace, as presentation needs to see it:
/// what kind of workspace it is and how each harness is actually receiving
/// its context.
///
/// Read-only by construction. Nothing reachable from here writes.
pub struct Workspace<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Read models about the workspace a directory sits in.
    pub fn workspace(&self) -> Workspace<'_> {
        Workspace(self)
    }
}

/// Which theme is active, and which themes this machine has.
pub struct Themes<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Selecting a theme. Only the selection: what a theme *is* belongs to
    /// the design system, which the domain does not name.
    pub fn themes(&self) -> Themes<'_> {
        Themes(self)
    }
}

/// When this machine rings for a finished agent.
pub struct Notifications<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// The operator's choice of which finished turns make a sound. Only the
    /// choice: ringing is the client's, which owns the terminal.
    pub fn notifications(&self) -> Notifications<'_> {
        Notifications(self)
    }
}

/// The names the operator gives projects.
pub struct ProjectLabels<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// The operator's own name for each project, in place of its
    /// directory's. Kept on this machine, never in the project.
    pub fn project_labels(&self) -> ProjectLabels<'_> {
        ProjectLabels(self)
    }
}

/// Which of the workspace's extensions are switched on, and the declared
/// ones installed plugins carry.
pub struct Extensions<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// The operator's choice of which extensions the workspace offers, and
    /// the declared extensions installed plugins carry, compiled. What an
    /// extension draws belongs to the client.
    pub fn extensions(&self) -> Extensions<'_> {
        Extensions(self)
    }
}

/// Marketplaces this machine knows, and the plugins they publish.
pub struct Marketplace<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Marketplace registration, catalogue reads, and installs sourced
    /// from one.
    pub fn marketplace(&self) -> Marketplace<'_> {
        Marketplace(self)
    }
}

/// Whether this machine's UZE installation is sound, and the work to make
/// it so.
pub struct Health<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Diagnostics, harness presence, and bounded environment maintenance.
    pub fn health(&self) -> Health<'_> {
        Health(self)
    }
}

/// A project's portable instruction context — `AGENTS.md` and the bridges
/// projected from it.
///
/// Separate from [`Project`] because the CLI grammar already draws that
/// line (ADR-019): `uze agent context …` is its own command group, scoped to a
/// directory, and it never touches the machine-scoped environment.
pub struct Context<'a>(pub(super) &'a UzeApplication);

/// A project's declared agent environment — `agents.lock` and what it
/// takes to satisfy it.
pub struct Project<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Instruction context: inspect, plan, reconcile.
    pub fn context(&self) -> Context<'_> {
        Context(self)
    }

    /// The project environment a directory declares.
    pub fn project(&self) -> Project<'_> {
        Project(self)
    }
}

/// The plugins this machine has installed, and their lifecycle.
///
/// The only service here that writes the Store.
pub struct Plugins<'a>(pub(super) &'a UzeApplication);

impl UzeApplication {
    /// Installing, removing, updating and reading installed plugins.
    pub fn plugins(&self) -> Plugins<'_> {
        Plugins(self)
    }
}

/// One agent harness as presentation needs to recognize it: the binary a
/// running process would be called, and the label to show instead of that
/// binary's raw name.
///
/// Built from the integrations this application already holds, so no
/// caller has to reach the registry — naming a concrete harness is what
/// `cli_and_tui_never_name_a_vendor_harness` forbids, and a descriptor is
/// how presentation stays on the right side of it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentIdentity {
    pub binary: &'static str,
    pub integration: &'static str,
    pub display_name: &'static str,
    /// The program an agent of this harness is launched by: UZE's own
    /// launcher when the operator has one for this harness, the plain
    /// binary otherwise. Naming it here rather than leaving it to `PATH`
    /// is what keeps a conversation carrying over without asking the
    /// operator to reorder their own environment.
    pub launch: std::path::PathBuf,
    /// Whether `launch` is UZE's launcher, which carries an agent's
    /// overrides itself; a plain binary is handed them by the client.
    pub through_launcher: bool,
    /// Why an agent of this harness starts a conversation it will not be
    /// able to continue, when that is the case. `None` is the ordinary
    /// answer; a value is meant to be said once, on the tab.
    pub continuity_gap: Option<String>,
    /// Whether this harness is set up for the workspace: its executable
    /// verified and its launcher in place. Every identity still recognizes
    /// a running agent; only a set-up one is offered to launch, because an
    /// agent started without its launcher neither resumes its conversation
    /// nor receives what the launcher carries.
    pub configured: bool,
    /// How this harness's prompt invokes a skill by name (`/`, `$`, `@`),
    /// or `None` for one that has no way to.
    pub skill_prefix: Option<&'static str>,
    /// What an agent of this harness may be launched with beyond what UZE
    /// composes, for an overrides step to offer and check.
    pub overrides: OverrideCatalog,
}

impl AgentIdentity {
    /// What typing `skill` with `args` into this harness's prompt is: its
    /// own spelling of a skill's invocation, or `None` where it has none.
    pub fn skill_invocation(&self, skill: &str, args: &[String]) -> Option<String> {
        Some(skill_invocation(self.skill_prefix?, skill, args))
    }
}

/// `skill` with `args`, as a harness whose prompt invokes a skill with
/// `prefix` is typed it.
pub fn skill_invocation(prefix: &str, skill: &str, args: &[String]) -> String {
    let mut typed = format!("{prefix}{skill}");
    for arg in args {
        typed.push(' ');
        typed.push_str(arg);
    }
    typed
}

/// What the workspace needs before it opens, decided from the harnesses
/// this machine has rather than from whether UZE ever ran on it: a machine
/// is only ever in one of these, and each asks for something different.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceEntry {
    /// Every harness on this machine is set up for the workspace.
    Ready,
    /// These harnesses are installed and not set up: they are set up from
    /// what is already here, without asking, because using them is why
    /// they were installed.
    SetUp(Vec<String>),
    /// No harness is installed: the operator is asked which to provision,
    /// since the workspace has nothing to launch an agent with.
    Choose,
}

/// The workspace service's own file — named for what it holds rather
/// than for the handle, since `workspace` is already `uze-core`'s module
/// for resolving a project root.
mod artifacts;
mod checkouts;
mod declaration;
pub(crate) mod disk;
mod launchers;
mod leftover_regions;
mod overrides;
mod policy_commands;
mod space_overview;
mod tasks;
mod work;

pub use artifacts::*;
pub use checkouts::*;
pub use declaration::*;
pub(crate) use disk::measure;
pub use disk::*;
pub use overrides::*;
pub use policy_commands::*;
pub use space_overview::*;
pub use tasks::*;
pub use work::*;
