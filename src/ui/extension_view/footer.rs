//! The footer row and the table of commands an extension's keys stand for.

use super::*;

/// A footer row: the keys on the left, and what the surface has to say
/// right-aligned at the other end. Answers with where each key landed.
///
/// The hints are given what is left of the row rather than the whole
/// width: drawn over each other, a narrow board reads `g ren3 boxes · 2
/// edges`, which is two truths written into one run of cells and no truth
/// at all.
pub(super) fn render_footer_row(
    frame: &mut ratatui::Frame<'_>,
    footer: Rect,
    commands: &[Command],
    keys: FooterKeys,
    trailing: Option<TextSpan<'static>>,
) -> Vec<(Rect, uze_keys::Action)> {
    let Some(trailing) = trailing else {
        return render_footer(frame, footer, commands, keys);
    };
    let width = (trailing.width() as u16).min(footer.width / 2);
    let hints = Rect {
        width: footer.width.saturating_sub(width + FOOTER_GAP),
        ..footer
    };
    let targets = render_footer(frame, hints, commands, keys);
    let mut line = Line::from(trailing);
    text::clip(&mut line, usize::from(width));
    frame.render_widget(
        Paragraph::new(line).alignment(ratatui::layout::Alignment::Right),
        Rect::new(footer.right() - width, footer.y, width, 1),
    );
    targets
}

pub(super) fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    commands: &[Command],
    keys: FooterKeys,
) -> Vec<(Rect, uze_keys::Action)> {
    // The overlay is what is open, so its own scope is what a key would
    // resolve against — the same stack `Attach::scopes` builds.
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Workspace,
        keys.scope,
    ];
    commands
        .iter()
        .copied()
        .filter_map(action_of)
        .fold(Hints::new(&scopes), Hints::key)
        .hovered(keys.hovered)
        .render(frame, area)
}

/// What each extension command means in the product's own vocabulary, read
/// both ways: a key the workspace resolves is handed down as the command
/// beside its action, and a footer names a command by the key its action
/// is bound to. Kept here, beside the render that needs it, rather than in
/// the extension, which knows nothing of either. Where two actions reach
/// one command, the first row is the one a footer names.
pub(super) const COMMAND_ACTIONS: [(Command, uze_keys::Action); 38] = [
    (Command::Close, uze_keys::Action::Dismiss),
    (Command::FocusNext, uze_keys::Action::FocusNext),
    (Command::FocusNext, uze_keys::Action::FocusPrevious),
    (Command::SelectNext, uze_keys::Action::SelectNext),
    (Command::SelectPrevious, uze_keys::Action::SelectPrevious),
    (Command::Collapse, uze_keys::Action::Collapse),
    (Command::Expand, uze_keys::Action::Expand),
    (Command::Activate, uze_keys::Action::Activate),
    (Command::OpenMenu, uze_keys::Action::OpenMenu),
    (Command::ScrollPageUp, uze_keys::Action::ScrollPageUp),
    (Command::ScrollPageDown, uze_keys::Action::ScrollPageDown),
    (Command::Edit, uze_keys::Action::EditFile),
    (Command::TogglePreview, uze_keys::Action::TogglePreview),
    (Command::Save, uze_keys::Action::SaveFile),
    (Command::Delete, uze_keys::Action::DeleteFile),
    (Command::CaretLeft, uze_keys::Action::CaretLeft),
    (Command::CaretRight, uze_keys::Action::CaretRight),
    (Command::CaretLineStart, uze_keys::Action::CaretLineStart),
    (Command::CaretLineEnd, uze_keys::Action::CaretLineEnd),
    (Command::Newline, uze_keys::Action::InsertNewline),
    (Command::Indent, uze_keys::Action::InsertIndent),
    (Command::EraseBack, uze_keys::Action::EraseBack),
    (Command::EraseForward, uze_keys::Action::EraseForward),
    (Command::Pan(PanDirection::Left), uze_keys::Action::PanLeft),
    (
        Command::Pan(PanDirection::Right),
        uze_keys::Action::PanRight,
    ),
    (Command::Pan(PanDirection::Up), uze_keys::Action::PanUp),
    (Command::Pan(PanDirection::Down), uze_keys::Action::PanDown),
    (Command::NextView, uze_keys::Action::NextDiagram),
    (Command::PreviousView, uze_keys::Action::PreviousDiagram),
    (Command::NextMode, uze_keys::Action::NextRendering),
    (Command::ChooseGroup, uze_keys::Action::ChooseArea),
    (Command::ChooseItem, uze_keys::Action::ChooseArtifact),
    (
        Command::SelectToward(PanDirection::Left),
        uze_keys::Action::SelectBoxLeft,
    ),
    (
        Command::SelectToward(PanDirection::Right),
        uze_keys::Action::SelectBoxRight,
    ),
    (
        Command::SelectToward(PanDirection::Up),
        uze_keys::Action::SelectBoxUp,
    ),
    (
        Command::SelectToward(PanDirection::Down),
        uze_keys::Action::SelectBoxDown,
    ),
    (Command::Back, uze_keys::Action::LevelUp),
    (Command::ToggleMap, uze_keys::Action::ToggleMap),
];

/// The action a command is named by. `None` for typing, which has no
/// single key to name.
pub(super) fn action_of(command: Command) -> Option<uze_keys::Action> {
    COMMAND_ACTIONS
        .iter()
        .find(|(candidate, _)| *candidate == command)
        .map(|(_, action)| *action)
}

/// The command a resolved action hands down to an extension's surface,
/// when it means one.
pub(crate) fn command_for(action: uze_keys::Action) -> Option<Command> {
    COMMAND_ACTIONS
        .iter()
        .find(|(_, candidate)| *candidate == action)
        .map(|(command, _)| *command)
}
