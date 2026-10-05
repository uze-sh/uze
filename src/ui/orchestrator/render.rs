//! Everything the workspace client draws.
//!
//! Split out of `orchestrator.rs`, which had grown to 3.5k lines covering
//! three unrelated jobs: driving the session, drawing it, and encoding input
//! for the PTY. Nothing here mutates session state — these take a
//! `&WorkspaceModel` and paint it, which is what makes them one module.

use super::*;
use crate::ui::Rows;
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    self, Chip, ChipState, Edge, POPUP_H_PAD, POPUP_V_PAD, Rule, Surface, TRAILING_PAD,
    action_index, chip, mark, row, text,
};

mod pane;
mod popups;
mod sidebar;
mod tab_strip;

pub(super) use pane::*;
pub(super) use popups::*;
pub(super) use sidebar::*;
pub(super) use tab_strip::*;

pub(super) fn blank_pane(pane: PaneId, columns: u16, rows: u16) -> PaneSnapshot {
    PaneSnapshot {
        pane,
        columns,
        rows,
        cursor: Cursor { column: 0, row: 0 },
        alternate_screen: false,
        mouse: uze_terminal::MouseMode::default(),
        bracketed_paste: false,
        cells: vec![blank_cell(); usize::from(columns) * usize::from(rows)],
    }
}

pub(super) fn blank_cell() -> RenderCell {
    RenderCell {
        character: ' ',
        foreground: TerminalColor::DefaultForeground,
        background: TerminalColor::DefaultBackground,
        attributes: CellAttributes::default(),
    }
}

pub(super) struct WorkspaceLayout {
    pub(super) sidebar: Rect,
    pub(super) tab_strip: Rect,
    pub(super) pane: Rect,
}

/// The one source of truth for workspace geometry — both the renderer and
/// the input loop's resize/CreateTab sizing call this, so the PTY dimensions
/// sent to the server always match the rect actually drawn into.
/// `sidebar_width_override` is the user's dragged width, if any (see
/// [`WorkspaceModel::sidebar_width`]); `None` uses the responsive default.
/// Only two areas span the full frame height — menu (sidebar) and main
/// container — there is no separate global header/footer row; the brand
/// and health chrome that used to live in a titlebar now opens the sidebar
/// itself (see [`render_sidebar`]), and this client never shows the help
/// toolbar — that stays exclusive to the management modal.
pub(super) fn compute_layout(
    frame_area: Rect,
    sidebar_width_override: Option<u16>,
) -> WorkspaceLayout {
    let (sidebar, column) = crate::ui::sidebar_and_column(frame_area, sidebar_width_override);
    let content_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(column);
    // No left inset either, matching the sidebar's own flush
    // `Padding::new(1, 0, 0, 0)` on its side of the same divider — the two
    // panes' content used to sit at mismatched distances from it (sidebar
    // text 1 column away, pane cells flush) until the sidebar's own inset
    // dropped to 0; keeping both at 0 here is what makes the divider read
    // as one straight line with even margins on both sides again, not a
    // lopsided one. The right side keeps its 1-column margin — that's
    // independent, matching the tab strip's own right padding against the
    // frame's outer edge, nothing to do with the divider. This is the rect
    // the PTY is actually sized to (see the resize logic that reads
    // `layout.pane.width/height`), so insetting it here — not just where
    // it's drawn — keeps what the shell thinks its size is in sync with
    // what's visible.
    let pane = Rect::new(
        content_rows[1].x,
        content_rows[1].y,
        content_rows[1].width.saturating_sub(1),
        content_rows[1].height,
    );
    WorkspaceLayout {
        sidebar,
        tab_strip: content_rows[0],
        pane,
    }
}

/// What a frame measured that the next event needs back.
///
/// Beside `hits` for the same reason those are: only the render knows how
/// the column came out, and the wheel over the sidebar has to stay inside
/// what it found there.
#[derive(Debug, Default)]
pub(super) struct FrameMetrics {
    /// Rows of the space tree the sidebar could not show — how far the
    /// tree may be scrolled, and zero when it fits.
    pub(super) tree_overflow: u16,
    /// What the code surface's frame left behind: where its navigator
    /// settled, and the scrollbars it drew — see
    /// `extension_view::Rendered`.
    pub(super) code: Option<crate::ui::extension_view::Rendered>,
    /// What the management modal's frame left behind, when it was open.
    pub(super) manage: Option<ManageFrame>,
    /// Whether a caption too long for its room was drawn sliding. The one
    /// thing on an otherwise idle frame that needs the next one: the
    /// clock turns for spinners and for work in flight, and a caption
    /// passing under the pointer is neither.
    pub(super) marquee: bool,
    /// The text the agent drawer drew, when it was open.
    pub(super) drawer: Option<crate::ui::agent_support::DrawerText>,
}

/// One frame of the management modal: where it was drawn, and the hit
/// list its own surface produced — in the modal's vocabulary, not this
/// client's, since its clicks are resolved by its own model.
#[derive(Debug)]
pub(super) struct ManageFrame {
    pub(super) chrome: crate::ui::widget::modal::Chrome,
    pub(super) hits: Vec<(Rect, crate::ui::hit::Hit)>,
}

pub(super) fn render(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    metrics: &mut FrameMetrics,
) {
    widget::root(frame, frame.area());
    let layout = compute_layout(frame.area(), model.sidebar_width);
    render_sidebar(frame, layout.sidebar, model, identities, hits, metrics);
    // The sidebar's own hairline right border doubles as a drag handle —
    // it sits just past `inner` (which `render_sidebar` never draws into),
    // so this can't collide with any row hit pushed there.
    hits.push((
        Rect::new(
            layout.sidebar.right().saturating_sub(1),
            layout.sidebar.y,
            1,
            layout.sidebar.height,
        ),
        WorkspaceHit::ResizeSidebar,
    ));
    let open = open_extension(model, layout.pane);
    let leading = match open {
        Some(_) => LeadingSlot::Surface,
        None => LeadingSlot::AgentTabs,
    };
    let slot = render_tab_strip(frame, layout.tab_strip, model, identities, leading, hits);
    let question = match &open {
        Some(open) => {
            let question = render_extension(frame, layout.pane, model, open, hits, metrics);
            render_extension_navigation(frame, slot, layout.pane, open, hits);
            question
        }
        None => {
            render_pane(frame, layout.pane, model);
            None
        }
    };
    // Drawn last so it sits on top of the pane — same ordering the
    // management modal's dialogs use in its own `render`. Anchored to
    // `picker.anchor` (the "✦" button's own rect) rather than centered on
    // the whole frame — a dropdown hanging off the thing you clicked, not a
    // modal interrupting the screen.
    // The two modal surfaces this client has: centred, and the only thing
    // that answers while they are open. Everything below them is a
    // dropdown hanging off the control that opened it, which stays beside
    // a screen that is still live — so the scrim covers these two and
    // nothing else. Same placement as the management modal's: between
    // what was drawn and what is drawn over it.
    if model.work.is_some()
        || model.action_index.is_some()
        || model.release_notes.is_some()
        || question.is_some()
    {
        crate::ui::widget::scrim::render(frame, frame.area());
    }
    if let Some(question) = &question {
        let mut view_hits = Vec::new();
        crate::ui::extension_view::render_confirm(
            frame,
            &question.confirm,
            frame.area(),
            question.scope,
            &mut view_hits,
        );
        hits.splice(
            0..0,
            view_hits
                .into_iter()
                .map(|(rect, hit)| (rect, WorkspaceHit::Extension((question.tag)(hit)))),
        );
    }
    if let Some(modal) = &model.release_notes {
        let targets = crate::ui::release_notes::render(frame, frame.area(), modal);
        // Prepended: what is underneath must not answer a click meant here.
        hits.insert(0, (targets.popup, WorkspaceHit::ReleaseNotesBody));
    }
    if let Some(overlay) = &model.work {
        render_work(frame, frame.area(), model, overlay, hits);
    }
    if let Some(index) = &model.action_index {
        render_action_index(frame, frame.area(), index, &model.disabled_extensions, hits);
    }
    if let Some(picker) = &model.agent_picker {
        render_agent_picker(frame, frame.area(), picker.anchor, picker, hits);
    }
    if let Some(drawer) = &model.support_dropdown
        && let Some(support) = &drawer.support
    {
        let history = model
            .remembered
            .drawer_prompts
            .as_ref()
            .filter(|history| history.root == drawer.space_root);
        let prompts = crate::ui::agent_support::DrawerPrompts {
            entries: history.map(|history| drawer.prompts(&history.entries)),
            scope: drawer.scope,
            agent_known: drawer.agent.is_some(),
            selected: drawer.selected,
            hovered_scope: match model.hovered {
                Some(WorkspaceHit::DrawerScope(scope)) => Some(scope),
                _ => None,
            },
            clearing: drawer.clearing,
            live_labels: model
                .tabs()
                .filter_map(|tab| {
                    super::launched_agent_id(tab).map(|id| (id.to_owned(), tab.label.clone()))
                })
                .collect(),
            selection: match &model.selection {
                Some(Selection::Drawer(marking)) => Some(marking),
                _ => None,
            },
        };
        let targets = crate::ui::agent_support::render(
            frame,
            layout.pane,
            support,
            &crate::ui::agent_support::DrawerAgent {
                path: drawer.path.clone(),
            },
            &prompts,
        );
        metrics.drawer = Some(targets.text);
        // Innermost first, so a row wins over the drawer's own body.
        hits.splice(
            0..0,
            targets
                .prompts
                .into_iter()
                .map(|(rect, index)| (rect, WorkspaceHit::DrawerPrompt(index)))
                .chain(
                    targets
                        .scopes
                        .into_iter()
                        .map(|(rect, scope)| (rect, WorkspaceHit::DrawerScope(scope))),
                )
                .chain(std::iter::once((targets.body, WorkspaceHit::DrawerBody))),
        );
    }
    if let Some(anchor) = model.status_catalog {
        render_status_catalog(frame, frame.area(), anchor, model.tick);
    }
    if let Some(popup) = &model.commit_detail {
        render_commit_detail(frame, frame.area(), popup);
    }
    if let Some(menu) = &model.context_menu {
        render_context_menu(frame, frame.area(), menu, hits);
    }
    // Last of all, over everything: the management modal seals the whole
    // client, so the whole frame recedes under it — the popups above
    // included, which is why it is not drawn among them.
    if let Some(manage) = &model.manage {
        crate::ui::widget::scrim::render(frame, frame.area());
        let mut manage_hits = Vec::new();
        let chrome = crate::ui::management::render_modal(
            frame,
            frame.area(),
            manage,
            model.manage_close_hovered,
            &mut manage_hits,
        );
        metrics.manage = Some(ManageFrame {
            chrome,
            hits: manage_hits,
        });
        // Prepended: what is underneath must not answer a click meant here.
        hits.splice(
            0..0,
            [
                (chrome.close, WorkspaceHit::CloseManage),
                (chrome.area, WorkspaceHit::ManageSurface),
            ],
        );
    }
    // Over everything, the management modal included. Drawn beneath the
    // popups, a toast was covered by the very dropdowns that hang off the
    // strip above its corner, and one raised while a dialog was open sat
    // dimmed under the scrim with nothing able to reach its `✕`. It is
    // narrow and leaves on its own, so the corner it takes from a dialog
    // costs less than a message nobody can read or put away.
    render_toasts(frame, layout.pane, model, hits);
}

/// The open extension, drawn where the pane is — a third kind of thing
/// that place shows, beside an agent and a shell. The sidebar and the
/// strip stay live around it, because the surface is about the checkout
/// they are selecting; covering them made reaching another agent a
/// matter of closing the surface first. Whether one was drawn is the
/// answer, since the pane is drawn only when none was.
fn render_extension(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    open: &OpenExtension,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    metrics: &mut FrameMetrics,
) -> Option<Question> {
    // The extension answers with content; the host lays it out and
    // therefore is the only side that can say which rectangle a click
    // landed in. The hits come back in the view's own vocabulary and are
    // tagged with the extension they belong to on the way into the shared
    // `hits` vec — the one place that translation happens.
    let OpenExtension { view, scope, tag } = open;
    let (scope, tag) = (*scope, *tag);
    let mut view_hits = Vec::new();
    metrics.code = Some(crate::ui::extension_view::render(
        frame,
        view,
        area,
        crate::ui::extension_view::NavigatorFrame {
            width: model.code_tree_width,
            scroll: model.code_tree_scroll,
            resizing: model
                .code_edge_drag
                .is_some_and(|drag| drag.intent == Some(EdgeIntent::Resize)),
            sliding: match model.hovered {
                Some(WorkspaceHit::Extension(
                    ExtensionHit::Code(hit)
                    | ExtensionHit::Spec(hit)
                    | ExtensionHit::Architect(hit),
                )) => Some((hit, model.tick)),
                _ => None,
            },
        },
        scope,
        match &model.selection {
            Some(Selection::Text(marking)) => Some(marking),
            _ => None,
        },
        &mut view_hits,
    ));
    metrics.marquee |= metrics.code.as_ref().is_some_and(|drawn| drawn.marquee);
    crate::ui::extension_view::render_row_menu(
        frame,
        view,
        area,
        model.code_menu_at,
        &mut view_hits,
    );
    hits.extend(
        view_hits
            .into_iter()
            .map(|(rect, hit)| (rect, WorkspaceHit::Extension(tag(hit)))),
    );
    view.confirm.clone().map(|confirm| Question {
        confirm,
        scope,
        tag,
    })
}

/// The open surface's navigation, in the bar's leading slot. Drawn after
/// the surface, since a selector's list opens over it, and its hits put
/// ahead of everything for the same reason.
fn render_extension_navigation(
    frame: &mut ratatui::Frame<'_>,
    slot: Rect,
    surface: Rect,
    open: &OpenExtension,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let mut navigation_hits = Vec::new();
    crate::ui::extension_view::render_navigation(
        frame,
        &open.view,
        slot,
        surface,
        &mut navigation_hits,
    );
    hits.splice(
        0..0,
        navigation_hits
            .into_iter()
            .map(|(rect, hit)| (rect, WorkspaceHit::Extension((open.tag)(hit)))),
    );
}

/// The surface standing in the pane, as it answered for this frame: read
/// once, so the bar's navigation and the surface below it are drawn from
/// the same answer.
struct OpenExtension {
    view: uze_extensions::view::View,
    scope: uze_keys::Scope,
    tag: fn(ViewHit) -> ExtensionHit,
}

fn open_extension(model: &WorkspaceModel, area: Rect) -> Option<OpenExtension> {
    let (view, scope, tag): (_, _, fn(ViewHit) -> ExtensionHit) =
        if let Some(architect) = &model.architect {
            (
                uze_extensions::architect::view(
                    architect,
                    crate::ui::extension_view::board_space(area),
                ),
                uze_keys::Scope::Architect,
                ExtensionHit::Architect,
            )
        } else if let Some(spec) = &model.spec {
            (
                uze_extensions::spec::view(
                    spec,
                    crate::ui::extension_view::code_space(area, model.code_tree_width, None),
                ),
                uze_keys::Scope::Spec,
                ExtensionHit::Spec,
            )
        } else if let Some(code) = &model.code {
            (
                uze_extensions::code::view(
                    code,
                    crate::ui::extension_view::code_space(area, model.code_tree_width, Some(code)),
                ),
                uze_keys::Scope::Code,
                ExtensionHit::Code,
            )
        } else {
            return None;
        };
    Some(OpenExtension { view, scope, tag })
}

/// The question an open surface waits on. Drawn with the client's other
/// modals, centred on the whole frame over the scrim, rather than inside the
/// pane: until it is answered nothing else responds, and a dialog drawn in
/// the pane says the opposite.
struct Question {
    confirm: uze_extensions::view::Confirm,
    scope: uze_keys::Scope,
    tag: fn(ViewHit) -> ExtensionHit,
}

/// Pins, on the space in front, the control that places a new agent in
/// it to the space header's right edge — the column every row in the
/// sidebar keeps free, the one the agent rows pin their task mark to.
///
/// The control is here rather than in the tab strip because an agent is
/// placed in a space, and this is the row that names it. Only the
/// selected space carries it: the new agent lands in the space in front,
/// and one on every header would be a column of the same word to read
/// past, each promising a space it would not land in.
fn push_trailing_controls(
    spans: &mut Vec<Span<'_>>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    rect: Rect,
    model: &WorkspaceModel,
    selected: bool,
) {
    if !selected {
        return;
    }
    let label = "new".to_owned();
    let width = Span::raw(label.as_str()).width() as u16;
    let Some(gap) = rect.width.checked_sub(
        spans.iter().map(|span| span.width() as u16).sum::<u16>() + width + TRAILING_PAD,
    ) else {
        return;
    };
    // The hue the caption of the agent receiving keystrokes wears
    // (`caption_color`), because this is where the next one lands —
    // held back until the pointer asks for it, so the row's one
    // coloured word does not outshout the name beside it.
    let hue = match chip_state(model, Some(WorkspaceHit::NewAgentMenu)) {
        ChipState::Hovered | ChipState::Pressed => Token::StateWarning,
        ChipState::Resting | ChipState::Static => Token::StateWarningMuted,
    };
    // Ahead of the row's own `SelectSpace`, so it wins the click.
    hits.push((
        Rect::new(rect.right() - TRAILING_PAD - width, rect.y, width, 1),
        WorkspaceHit::NewAgentMenu,
    ));
    spans.push(Span::raw(" ".repeat(gap as usize)));
    spans.push(Span::styled(label, theme::fg_bold(hue)));
    spans.push(Span::raw(" ".repeat(TRAILING_PAD as usize)));
}

/// What a pull and a push would move for the checkout in front, when it
/// is outside any slot, its branch is the delivery target and something
/// is due either way: `↓1` in the danger hue for what is to pull, `↑2` in
/// the success hue for what is to push, each only while its count is
/// non-zero, and nothing between them — one reading, the shape a shell
/// prompt gives the same fact. No word: the two colours say which is
/// which. Empty inside a slot, on any other branch, without an upstream,
/// or in sync — a caption that says "nothing to do" says it best by
/// saying nothing. A slot's branch owes upstream what its delivery chip
/// already counts.
///
/// Ordinary digits, not the small forms. Those exist so a count can ride
/// *inside* a line of text without breaking it, and this one does not
/// ride inside anything — it stands alone in its zone. At that size two
/// small digits beside an arrow read as a smudge on the arrow rather than
/// as a number, which is the one thing a count has to be.
fn sync_counts(model: &WorkspaceModel, cwd: &Path) -> Vec<Span<'static>> {
    if !is_unisolated(cwd) {
        return Vec::new();
    }
    let Some(sync) = model.remembered.upstream_syncs.get(&evaluation_key(cwd)) else {
        return Vec::new();
    };
    [
        (Symbol::SyncBehind, sync.pull, Token::StateDanger),
        (Symbol::SyncAhead, sync.push, Token::StateSuccess),
    ]
    .into_iter()
    .filter(|(_, count, _)| *count > 0)
    .map(|(arrow, count, hue)| {
        Span::styled(format!("{}{count}", theme::glyph(arrow)), theme::fg(hue))
    })
    .collect()
}

/// The stack of outcomes, against the top-right of the pane.
///
/// Flush with the pane's own right edge, which is already inset a column
/// from the frame (see `compute_layout`) and is the same column the tab
/// strip's controls end at — so a toast lines up under the chip above it
/// rather than a column short of it. Insetting again here is what put a
/// second margin on that side.
///
/// Flush with the pane's own top row, which already sits a row below the
/// tab strip's text. A row of air was added on top of that one, and two
/// rows is far enough that the message stops reading as an answer to what
/// the strip above it says and starts reading as something floating in the
/// pane — which is the opposite of what a toast anchored to the top-right
/// is for.
fn render_toasts(
    frame: &mut ratatui::Frame<'_>,
    pane: Rect,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let stack = model.toast_stack();
    if stack.is_empty() || pane.width < 16 || pane.height < 5 {
        return;
    }
    let area = pane;
    let mut targets = Vec::new();
    for (index, placed) in widget::toast::stack(frame, area, &stack)
        .into_iter()
        .enumerate()
    {
        // The offer first: it sits inside the box, and the box answers
        // everything else by putting the message away.
        if let Some(action) = placed.action {
            targets.push((action, WorkspaceHit::ToastAction(index)));
        }
        // The mark first, then the row behind it: both put the message
        // away, and asking the row first would make the mark unreachable
        // rather than merely redundant.
        targets.push((placed.close, WorkspaceHit::DismissToast(index)));
        targets.push((placed.box_rect, WorkspaceHit::DismissToast(index)));
    }
    // Prepended: everything underneath answers a click somewhere, so a
    // toast asked after it would never be the answer.
    hits.splice(0..0, targets);
}
