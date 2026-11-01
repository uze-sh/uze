//! The action vocabulary: every meaning uze's keyboard can reach, named
//! once and independently of any key.
//!
//! This is to input what [`crate::Scope`] is to modality and what a colour
//! token is to appearance: the product decides what an action *means*, and
//! which chord reaches it is a separate, changeable question. Nothing here
//! knows a key.
//!
//! Two properties travel with the name because every surface needs them and
//! none should re-derive them: what the action does in words ([`Action::description`]),
//! which is what the index and the Keys screen read out, and whether
//! performing it destroys something ([`Action::destructive`]), which decides
//! whether it may hold an unmodified letter and whether it may sit first in
//! a menu.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::extension::{self, ExtensionActionId};

macro_rules! actions {
    ($($variant:ident => $name:literal, $destructive:expr, $label:literal, $description:literal;)*) => {
        /// Everything uze's keyboard can reach.
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub enum Action {
            $($variant,)*
            /// Select the nth tab on the strip in front of the person —
            /// in the workspace the agent and the shells opened alongside
            /// it, in management its screens — which is what the numbers
            /// on screen are counted along. One action per position
            /// rather than one action carrying a number, because the
            /// keymap binds positions, not a counter.
            SelectTab(u8),
            /// Show the nth section of a space's overview. Apart from
            /// [`Action::SelectTab`], whose chords stay live over the
            /// overview and choose a tab of the strip behind it.
            SelectSection(u8),
            /// An action a plugin's extension declares, named
            /// `ext.<plugin>@<marketplace>/<id>.<action>` and live only
            /// while that extension's surface is in front. Never in
            /// [`ALL_ACTIONS`]: what plugins declare is not this build's
            /// vocabulary. See [`crate::extension`].
            Extension(ExtensionActionId),
        }

        /// Every action this build knows, in vocabulary order — the order
        /// the index and the Keys screen list them in.
        pub const ALL_ACTIONS: &[Action] = &[
            $(Action::$variant,)*
            Action::SelectTab(1), Action::SelectTab(2), Action::SelectTab(3),
            Action::SelectTab(4), Action::SelectTab(5), Action::SelectTab(6),
            Action::SelectTab(7), Action::SelectTab(8), Action::SelectTab(9),
            Action::SelectSection(1), Action::SelectSection(2),
            Action::SelectSection(3), Action::SelectSection(4),
            Action::SelectSection(5),
        ];

        impl Action {
            /// The name this action carries in a keymap file.
            pub fn name(self) -> String {
                match self {
                    $(Action::$variant => $name.to_owned(),)*
                    Action::SelectTab(index) => format!("select-tab-{index}"),
                    Action::SelectSection(index) => format!("select-section-{index}"),
                    Action::Extension(id) => extension::name(id),
                }
            }

            /// Reads a name back. Unknown names are the caller's to report
            /// as a warning: a keymap written for a newer uze must still
            /// load on this one.
            pub fn parse(name: &str) -> Option<Action> {
                match name {
                    $($name => Some(Action::$variant),)*
                    other if extension::is_name(other) => {
                        Some(Action::Extension(extension::intern(other)))
                    }
                    other => other
                        .strip_prefix("select-tab-")
                        .and_then(|index| index.parse::<u8>().ok())
                        .filter(|index| (1..=9).contains(index))
                        .map(Action::SelectTab)
                        .or_else(|| {
                            other
                                .strip_prefix("select-section-")
                                .and_then(|index| index.parse::<u8>().ok())
                                .filter(|index| (1..=5).contains(index))
                                .map(Action::SelectSection)
                        }),
                }
            }

            /// Whether performing this destroys something. A destructive
            /// action never holds an unmodified letter and is never the
            /// entry a menu opens highlighted.
            pub fn destructive(self) -> bool {
                match self {
                    $(Action::$variant => $destructive,)*
                    Action::SelectTab(_) | Action::SelectSection(_) => false,
                    Action::Extension(id) => extension::destructive(id),
                }
            }

            /// The imperative a menu entry or button carries.
            pub fn label(self) -> String {
                match self {
                    $(Action::$variant => $label.to_owned(),)*
                    Action::SelectTab(index) => format!("Tab {index}"),
                    Action::SelectSection(index) => format!("Section {index}"),
                    Action::Extension(id) => extension::label(id),
                }
            }

            /// What it does, in words — the prose that used to live in a
            /// hand-written help overlay, now attached to the action so no
            /// surface has to restate it.
            pub fn description(self) -> String {
                match self {
                    $(Action::$variant => $description.to_owned(),)*
                    Action::SelectTab(index) => {
                        format!(
                            "Select tab {index} on the strip in front of you: an agent \
                             and its shells, or a management screen"
                        )
                    }
                    Action::SelectSection(index) => {
                        format!("Show section {index} of the space's overview")
                    }
                    Action::Extension(id) => extension::description(id),
                }
            }
        }
    };
}

actions! {
    // --- Global ---------------------------------------------------------
    OpenActionIndex => "open-action-index", false,
        "Everything you can do", "List every action available here, with the key that reaches it";
    SwitchMode => "switch-mode", false,
        "Manage", "Open or close the management modal over the workspace";
    Quit => "quit", false,
        "Quit", "Leave uze, detaching from the session rather than ending it";

    // --- Navigation, in either keyboard ---------------------------------
    SelectNext => "select-next", false,
        "Next", "Move the selection down one";
    SelectPrevious => "select-previous", false,
        "Previous", "Move the selection up one";
    FocusNext => "focus-next", false,
        "Next pane", "Move focus to the next part of the screen";
    FocusPrevious => "focus-previous", false,
        "Previous pane", "Move focus to the previous part of the screen";
    Activate => "activate", false,
        "Open", "Open, inspect, or confirm whatever is selected";
    Dismiss => "dismiss", false,
        "Close", "Close what is open, without acting on it";
    Expand => "expand", false,
        "Expand", "Unfold the selected row";
    Collapse => "collapse", false,
        "Collapse", "Fold the selected row";
    ScrollPageDown => "scroll-page-down", false,
        "Page down", "Scroll the focused view down a page";
    ScrollPageUp => "scroll-page-up", false,
        "Page up", "Scroll the focused view up a page";
    EraseBack => "erase-back", false,
        "Erase", "Delete the character before the cursor";

    // --- Management, screen-wide ----------------------------------------
    FocusSidebar => "focus-sidebar", false,
        "Back to the tabs", "Move focus from the content back to the screen tabs";
    FocusContent => "focus-content", false,
        "Into the content", "Move focus from the screen tabs into the screen";
    Refresh => "refresh", false,
        "Refresh", "Re-read the machine: harnesses, plugins, marketplaces";
    StartFilter => "start-filter", false,
        "Search", "Narrow the list by typing";

    // --- Management, things done to a key ------------------------------
    ChangeKey => "change-key", false,
        "Change key", "Bind the next key pressed to the selected action";
    ResetKey => "reset-key", false,
        "Reset key", "Put back the key uze ships with for the selected action";

    // --- Management, things done to a package ---------------------------
    InstallPlugin => "install-plugin", false,
        "Install", "Install the selected plugin onto this machine";
    UpdatePlugin => "update-plugin", false,
        "Update", "Update the selected plugin to what its marketplace offers";
    RemovePlugin => "remove-plugin", true,
        "Remove", "Remove the selected plugin from this machine";
    AddMarketplace => "add-marketplace", false,
        "Add marketplace", "Register a marketplace by path or URL";
    RemoveMarketplace => "remove-marketplace", true,
        "Remove", "Remove the selected marketplace and every plugin it delivered";

    // --- Management, things done to an extension -----------------------
    EnableExtension => "enable-extension", false,
        "Enable", "Offer the selected extension in the workspace again";
    DisableExtension => "disable-extension", false,
        "Disable",
        "Take the selected extension out of the workspace: its keys, its buttons and \
         its entries in the index";
    ResolvePrograms => "resolve-programs", false,
        "Resolve again", "Find the selected extension's programs again on this machine";
    EditPreferences => "edit-preferences", false,
        "Preferences", "Set the selected extension's preferences";

    // --- Management, from one screen to its sibling ---------------------
    ShowExtension => "show-extension", false,
        "Show extension",
        "Show the extension the selected plugin declares on the Extensions screen, where \
         whether it runs is decided";
    ShowPlugin => "show-plugin", false,
        "Show plugin", "Show the plugin the selected extension comes with on the Marketplace";

    // --- Management, things done to a project ---------------------------
    InstallProjectEnvironment => "install-project-environment", false,
        "Install the project's environment", "Install what this project declares but the machine lacks";
    ClearPromptHistory => "clear-prompt-history", true,
        "Clear history", "Forget the prompts recorded for this space, having been asked once";

    // --- Management, things done to a harness ---------------------------
    SetupHarness => "setup-harness", false,
        "Set up", "Prepare the selected harness to receive what uze delivers";
    AnalyzeContext => "analyze-context", false,
        "Analyze context", "Read what this project's context would become";
    ApplyContextPlan => "apply-context-plan", false,
        "Apply the plan", "Write the analyzed context into the project";
    OpenGlossary => "open-glossary", false,
        "What these words mean", "Explain the labels this screen uses";

    // --- Management, choosing values -------------------------------------
    ToggleChoice => "toggle-choice", false,
        "Switch", "Include or leave out the marked row";
    ToggleAllChoices => "toggle-all-choices", false,
        "Switch all", "Include every row, or none once all are";
    NextValue => "next-value", false,
        "Next value", "Change the highlighted preference to the next value";
    PreviousValue => "previous-value", false,
        "Previous value", "Change the highlighted preference to the previous value";

    // --- Workspace, the container ---------------------------------------
    NewShellTab => "new-shell-tab", false,
        "New shell", "Open a shell beside what is running";
    CloseTab => "close-tab", true,
        "Close tab", "Close the selected tab";
    NewAgent => "new-agent", false,
        "New agent", "Start an agent in this space";
    OpenOverrides => "open-overrides", false,
        "Overrides", "Add flags and variables to the new agent's launch";
    IsolateAgent => "isolate-agent", false,
        "To worktree",
        "Move this agent into a worktree of its own, taking the uncommitted \
         changes with it";
    IsolateAgentAtCommit => "isolate-agent-at-commit", false,
        "To worktree, no changes",
        "Move this agent into a worktree cut from the last commit; the \
         uncommitted changes stay here";
    ReviewExtensions => "review-extensions", false,
        "Review extensions",
        "Enable, or put away, the extensions a project's plugins offer where it opens";
    RestartUze => "restart-uze", false,
        "Restart uze",
        "Restart into the release installed while uze was open; every pane keeps running";
    PauseAgentWrites => "pause-agent-writes", false,
        "Pause extensions",
        "Hold every prompt an extension would type into an agent, until pressed again";
    SwitchHarness => "switch-harness", false,
        "Restart in",
        "Restart this agent with other launch overrides, or on another \
         harness, in the same checkout and conversation";
    NewSpace => "new-space", false,
        "New space", "Open a space at a directory";
    RenameSelection => "rename-selection", false,
        "Rename", "Rename the selected tab or space";
    NextSpace => "next-space", false,
        "Next space", "Move to the next space in the sidebar";
    PreviousSpace => "previous-space", false,
        "Previous space", "Move to the previous space in the sidebar";
    NextAgent => "next-agent", false,
        "Next agent", "Move to the next agent in this space";
    PreviousAgent => "previous-agent", false,
        "Previous agent", "Move to the previous agent in this space";
    ToggleChanges => "toggle-changes", false,
        "Review", "Open or close the review of the selected agent's branch against its target";
    ToggleFiles => "toggle-files", false,
        "Files", "Open or close the files of the selected tab's checkout";
    ToggleArchitect => "toggle-architect", false,
        "Architect", "Open or close the project's architecture diagrams";
    ToggleSpec => "toggle-spec", false,
        "Spec", "Open or close what the selected tab's checkout intends: its changes, designs and tasks";

    // --- The architect surface -------------------------------------------
    PanLeft => "pan-left", false,
        "Move left", "Move the board to show what is further left";
    PanRight => "pan-right", false,
        "Move right", "Move the board to show what is further right";
    PanUp => "pan-up", false,
        "Move up", "Move the board to show what is further up";
    PanDown => "pan-down", false,
        "Move down", "Move the board to show what is further down";
    NextDiagram => "next-diagram", false,
        "Next artifact", "Show the next artifact on the board";
    PreviousDiagram => "previous-diagram", false,
        "Previous artifact", "Show the previous artifact on the board";
    NextRendering => "next-rendering", false,
        "Rendering", "Draw the diagram the next way it can be drawn";
    ChooseArea => "choose-area", false,
        "Areas", "Open the list of areas, to show another one's artifacts";
    ChooseArtifact => "choose-artifact", false,
        "Artifacts", "Open the list of artifacts in the area on show";
    SelectBoxLeft => "select-box-left", false,
        "Box to the left", "Select the box to the left of the selected one";
    SelectBoxRight => "select-box-right", false,
        "Box to the right", "Select the box to the right of the selected one";
    SelectBoxUp => "select-box-up", false,
        "Box above", "Select the box above the selected one";
    SelectBoxDown => "select-box-down", false,
        "Box below", "Select the box below the selected one";
    LevelUp => "level-up", false,
        "Level up", "Go back to the diagram this one was entered from";
    ToggleMap => "toggle-map", false,
        "Map", "Show the checkout as a map of where its lines are, or leave it";

    // --- A plugin's surface ----------------------------------------------
    ToggleInspector => "toggle-inspector", false,
        "Inspector", "Show how a plugin's surface was drawn: its nodes, data, state and actions";
    ToggleDrawer => "toggle-drawer", false,
        "Drawer", "Open or close the drawer a plugin docks beside the pane";

    // --- The code surface, and typing into a file ------------------------
    EditFile => "edit-file", false,
        "Edit", "Open the selected file's contents and start typing";
    TogglePreview => "toggle-preview", false,
        "Preview", "Show a markdown file as the document it describes, and back";
    SaveFile => "save-file", false,
        "Save", "Write what was typed back to the file";
    OpenMenu => "open-menu", false,
        "Actions", "Open what can be done to what is selected: an agent, or a change in the code surface";
    DeleteFile => "delete-file", true,
        "Delete", "Delete the selected file, having been asked once";
    FindInFiles => "find-in-files", false,
        "Search", "Search every file of the checkout, from the field over the list";
    CloseFileTab => "close-file-tab", false,
        "Close tab", "Put away the file on show, asking first when it has unsaved changes";
    NextFileTab => "next-file-tab", false,
        "Next tab", "Show the next file open in a tab";
    PreviousFileTab => "previous-file-tab", false,
        "Previous tab", "Show the previous file open in a tab";
    CaretLeft => "caret-left", false,
        "Left", "Move the caret one character left";
    CaretRight => "caret-right", false,
        "Right", "Move the caret one character right";
    CaretLineStart => "caret-line-start", false,
        "Line start", "Move the caret to the start of its line";
    CaretLineEnd => "caret-line-end", false,
        "Line end", "Move the caret to the end of its line";
    InsertNewline => "insert-newline", false,
        "New line", "Split the line at the caret";
    InsertIndent => "insert-indent", false,
        "Indent", "Insert one level of the file's own indentation at the caret";
    EraseForward => "erase-forward", false,
        "Delete", "Delete the character under the caret";

    // --- The review surface ----------------------------------------------
    ReviewComment => "review-comment", false,
        "Comment", "Write a comment on the marked lines, or on the line under the cursor";
    ReviewAdd => "review-add", false,
        "Add", "Keep the comment being written as a draft of the review";
    ReviewSend => "review-send", false,
        "Send", "Tell the agent its review's drafts wait for it";
    ReviewViewed => "review-viewed", false,
        "Viewed", "Mark the file viewed and go on to the next one not viewed, or unmark it";
    ReviewOpenInCode => "review-open-in-code", false,
        "Open in code", "Open the marked line in the code surface";
    ReviewResolve => "review-resolve", false,
        "Resolve", "Take an answered comment out of the review";
    ReviewMarkDown => "review-mark-down", false,
        "Mark down", "Carry the marked lines one line further down";
    ReviewMarkUp => "review-mark-up", false,
        "Mark up", "Carry the marked lines one line further up";

    // --- Workspace, the work --------------------------------------------
    DeliverTask => "deliver-task", false,
        "Deliver", "Deliver the selected task the way the project says";
    DeliverAllTasks => "deliver-all-tasks", false,
        "Deliver all", "Deliver every deliverable task in this space";
    // Named for what it opened when released: an operator's keys.json
    // binds it by this name, and a new one would drop the binding.
    ToggleRecent => "toggle-preserved-work", false,
        "Recent", "Show the agents no pane carries, across every project, and what each waits on";
    // One key for the one thing that takes a row away: the work goes with
    // its checkout, its branch and its conversation, or, when its project
    // is gone from disk, the record is let go of.
    DiscardTask => "discard-task", true,
        "Discard",
        "Discard the selected agent's work: its checkout, branch and conversation";
    // Recent lists named work by default: an agent nobody named is known
    // only by its generated id, which says nothing until asked for.
    ToggleAllWork => "toggle-all-work", false,
        "All", "Show every agent recent holds, not only the named ones";

    // --- The space overview -------------------------------------------
    OpenSpaceOverview => "open-space-overview", false,
        "Overview", "Show this space's overview: its context, plugins, workspace and memory";
    AddItem => "add-item", false,
        "Add", "Add an entry to the list in focus";
    // Written at once, as every Settings edit is, and still not
    // destructive: it takes one entry out of a file Git holds, and the
    // toast that follows names it, which is why it may sit on a bare letter.
    // On a worktree card it only asks, and what it removes when answered
    // is a free slot's directory: the workspace refuses one that holds any
    // work or that anybody stands in, and keeps its branch, so all that
    // goes is output a build makes again.
    RemoveItem => "remove-item", false,
        "Remove", "Remove the entry in focus or unset the key, or remove the free worktree selected";
    MoveItemUp => "move-item-up", false,
        "Move up", "Move the entry in focus up its list";
    MoveItemDown => "move-item-down", false,
        "Move down", "Move the entry in focus down its list";
    DiscardChanges => "discard-changes", false,
        "Discard", "Drop the edits agents.yaml refused";
    ReviewCommands => "review-commands", false,
        "Review", "Read the project's commands waiting for approval, and answer";
}

impl fmt::Display for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name())
    }
}

impl Serialize for Action {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Action, D::Error> {
        let name = String::deserialize(deserializer)?;
        Action::parse(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown action `{name}`")))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_action_names_itself_uniquely_and_reads_back() {
        let mut seen = BTreeSet::new();
        for action in ALL_ACTIONS {
            let name = action.name();
            assert!(seen.insert(name.clone()), "two actions named `{name}`");
            assert_eq!(Action::parse(&name), Some(*action), "`{name}`");
        }
    }

    #[test]
    fn an_action_this_build_does_not_know_is_not_an_error_here() {
        // The caller reports it as a warning; the vocabulary just says no.
        assert_eq!(Action::parse("teleport"), None);
        assert_eq!(Action::parse("select-tab-0"), None);
        assert_eq!(Action::parse("select-tab-10"), None);
    }

    #[test]
    fn every_action_carries_prose_a_surface_can_print() {
        for action in ALL_ACTIONS {
            assert!(!action.label().is_empty(), "{action} has no label");
            assert!(
                action.description().len() > action.label().len(),
                "{action}'s description says no more than its label"
            );
        }
    }

    #[test]
    fn the_destructive_set_is_stated_not_guessed() {
        let destructive: BTreeSet<String> = ALL_ACTIONS
            .iter()
            .filter(|action| action.destructive())
            .map(|action| action.name())
            .collect();
        assert_eq!(
            destructive,
            BTreeSet::from_iter(
                [
                    "clear-prompt-history",
                    "close-tab",
                    "delete-file",
                    "discard-task",
                    "remove-marketplace",
                    "remove-plugin",
                ]
                .map(str::to_owned)
            ),
            "changing what is destructive changes what may hold a bare letter"
        );
    }
}
