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
    self, ChipState, Edge, POPUP_H_PAD, POPUP_V_PAD, Rule, Status, StatusKind, Surface,
    TRAILING_PAD, action_index, chip, mark, row, text,
};

mod bar;
mod declared;
mod overrides;
mod pane;
mod popups;
mod recent;
mod sidebar;
mod space_overview;
mod tab_strip;

use bar::*;
#[cfg(test)]
pub(super) use declared::overlay_card;
use declared::*;
use overrides::*;
pub(super) use pane::*;
pub(super) use popups::*;
use recent::*;
pub(super) use sidebar::*;
use space_overview::*;
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
    /// What the pane's terminal is sized to: the agent area, less a
    /// docked drawer's column and the rows bands take along its edges.
    pub(super) pane: Rect,
    /// Where a plugin's drawer stands when one is open and fits: docked,
    /// the column or rows beside the pane, its gap included; floating, the
    /// most of the pane it may cover (one that fits what it holds is cut
    /// to that while it is drawn).
    pub(super) drawer: Option<Rect>,
    /// The rows each band asked for took, by its slot: along the pane's
    /// top or bottom, never over it.
    pub(super) bands: Vec<(SlotId, Rect)>,
    /// The bar under the agent area, its hairline included, while a plugin
    /// puts anything in it: the tab strip's mirror at the column's foot.
    pub(super) bar: Option<Rect>,
}

/// A drawer open beside or over the pane, as the layout is asked for one:
/// where it stands, and the width or rows it was dragged to, `None` for
/// its size at rest.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Dock {
    pub(super) place: uze_contract::DrawerDecl,
    pub(super) width: Option<u16>,
    pub(super) rows: Option<u16>,
}

/// The edge of a drawer's card that faces the pane: where its gap and its
/// drag handle are.
pub(super) fn facing(side: uze_contract::DrawerSide) -> widget::Edge {
    match side {
        uze_contract::DrawerSide::Right => widget::Edge::Left,
        uze_contract::DrawerSide::Left => widget::Edge::Right,
        uze_contract::DrawerSide::Bottom => widget::Edge::Top,
        uze_contract::DrawerSide::Top => widget::Edge::Bottom,
    }
}

/// The pane, and the rect `dock`'s drawer takes of `area`: docked, the
/// pane gives up that column or those rows; floating, the pane keeps
/// `area` whole and the drawer stands over its side. `None` for the
/// drawer when it does not fit, and then the pane keeps `area` too.
pub(super) fn place_drawer(area: Rect, dock: Dock) -> (Rect, Option<Rect>) {
    use uze_contract::DrawerSide;
    use widget::drawer::{DOCKED_GAP, DOCKED_MIN_ROWS, DOCKED_MIN_WIDTH, FIT_WIDTH};
    let side = dock.place.side;
    if dock.place.float {
        let extent = match (side.across(), dock.place.fit) {
            (false, true) => Some(FIT_WIDTH.min(area.width)),
            (false, false) => {
                widget::drawer::floating_extent(dock.width, area.width, DOCKED_MIN_WIDTH)
            }
            // Fitted along the top or bottom, it may take every row there
            // is; what it holds decides how many it does.
            (true, true) => Some(area.height),
            (true, false) => {
                widget::drawer::floating_extent(dock.rows, area.height, DOCKED_MIN_ROWS)
            }
        };
        let Some(extent) = extent.filter(|extent| *extent > 0) else {
            return (area, None);
        };
        let card = match side {
            DrawerSide::Right => Rect {
                x: area.right() - extent,
                width: extent,
                ..area
            },
            DrawerSide::Left => Rect {
                width: extent,
                ..area
            },
            DrawerSide::Bottom => Rect {
                y: area.bottom() - extent,
                height: extent,
                ..area
            },
            DrawerSide::Top => Rect {
                height: extent,
                ..area
            },
        };
        return (area, Some(card));
    }
    let extent = match side.across() {
        false => widget::drawer::docked_width(dock.width, area.width),
        true => widget::drawer::docked_height(dock.rows, area.height),
    };
    let Some(extent) = extent else {
        return (area, None);
    };
    let taken = extent + DOCKED_GAP;
    match side {
        DrawerSide::Right => {
            let pane = Rect {
                width: area.width - taken,
                ..area
            };
            (
                pane,
                Some(Rect::new(pane.right(), area.y, taken, area.height)),
            )
        }
        DrawerSide::Left => {
            let pane = Rect {
                x: area.x + taken,
                width: area.width - taken,
                ..area
            };
            (pane, Some(Rect::new(area.x, area.y, taken, area.height)))
        }
        DrawerSide::Bottom => {
            let pane = Rect {
                height: area.height - taken,
                ..area
            };
            (
                pane,
                Some(Rect::new(area.x, pane.bottom(), area.width, taken)),
            )
        }
        DrawerSide::Top => {
            let pane = Rect {
                y: area.y + taken,
                height: area.height - taken,
                ..area
            };
            (pane, Some(Rect::new(area.x, area.y, area.width, taken)))
        }
    }
}

/// Rows a band asks to have along the pane's top, or its `bottom`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BandRows {
    pub(super) slot: SlotId,
    pub(super) rows: u16,
    pub(super) bottom: bool,
}

/// What the layout takes from the agent area before the pane is sized: a
/// docked drawer's column and the bands' rows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Reserved {
    pub(super) dock: Option<Dock>,
    pub(super) bands: Vec<BandRows>,
    /// Whether the bar under the agent area stands: a plugin offers a view,
    /// a drawer or an item there. Never by what an item answers, so the
    /// pane is sized by what is installed, not by what is drawn.
    pub(super) bar: bool,
}

/// The fewest rows the pane keeps under its bands: a band that would
/// leave it fewer is not laid out, so a short window keeps its agent.
pub(super) const PANE_MIN_ROWS: u16 = 6;

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
///
/// `reserved.dock` is the drawer open beside the pane, if any. The pane
/// gives up its width and the gap before it, so the PTY is sized to what
/// stays visible and no cell an agent draws is covered; the size changes
/// when a drawer opens, closes or is dragged, never because of what one
/// answers. `reserved.bands` are the rows bands declare, taken off the
/// pane's top and bottom the same way and for the same reason: what a band
/// answers never reaches the pane's size, only whether it stands there.
pub(super) fn compute_layout(
    frame_area: Rect,
    sidebar_width_override: Option<u16>,
    reserved: Reserved,
) -> WorkspaceLayout {
    let dock = reserved.dock;
    let (sidebar, column) = crate::ui::sidebar_and_column(frame_area, sidebar_width_override);
    let content_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(if reserved.bar { 2 } else { 0 }),
        ])
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
    let area = Rect::new(
        content_rows[1].x,
        content_rows[1].y,
        content_rows[1].width.saturating_sub(1),
        content_rows[1].height,
    );
    let (pane, drawer) = match dock {
        Some(dock) => place_drawer(area, dock),
        None => (area, None),
    };
    let (pane, bands) = take_bands(pane, &reserved.bands);
    WorkspaceLayout {
        sidebar,
        tab_strip: content_rows[0],
        pane,
        drawer,
        bands,
        bar: reserved.bar.then_some(content_rows[2]),
    }
}

/// `pane` less the rows of each band in turn, top ones from its top down
/// and bottom ones from its bottom up, while it keeps [`PANE_MIN_ROWS`].
fn take_bands(mut pane: Rect, asked: &[BandRows]) -> (Rect, Vec<(SlotId, Rect)>) {
    let mut taken = Vec::new();
    for band in asked {
        if pane.height < band.rows.saturating_add(PANE_MIN_ROWS) {
            continue;
        }
        let rect = if band.bottom {
            Rect::new(pane.x, pane.bottom() - band.rows, pane.width, band.rows)
        } else {
            Rect::new(pane.x, pane.y, pane.width, band.rows)
        };
        pane.height -= band.rows;
        if !band.bottom {
            pane.y += band.rows;
        }
        taken.push((band.slot, rect));
    }
    (pane, taken)
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
    /// What the open view's frame left behind: where its navigator
    /// settled, and the scrollbars it drew — see
    /// `extension_view::Rendered`.
    pub(super) surface: Option<crate::ui::extension_view::Rendered>,
    /// What the management modal's frame left behind, when it was open.
    pub(super) manage: Option<ManageFrame>,
    /// Whether a caption too long for its room was drawn sliding. The one
    /// thing on an otherwise idle frame that needs the next one: the
    /// clock turns for spinners and for work in flight, and a caption
    /// passing under the pointer is neither.
    pub(super) marquee: bool,
    /// Where recent's list settled and the scrollbar it drew, when it was
    /// open.
    pub(super) recent: Option<RecentFrame>,
    /// Where the space overview's section in front settled, when it was
    /// drawn.
    pub(super) overview: Option<OverviewScroll>,
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
    let layout = compute_layout(frame.area(), model.sidebar_width, model.reserved());
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
    let (surface_area, inspector_area) = declared_split(model, layout.pane);
    // A plugin's view is drawn under its title, which nothing it draws can
    // cover.
    let declared = model
        .slot_in_pane()
        .filter(|slot| slot.owner.declared().is_some());
    let surface_area = match declared {
        Some(slot) => {
            crate::ui::extension_view::tree::render_title(frame, surface_area, &slot.surface.title)
        }
        None => surface_area,
    };
    let open = open_extension(model, surface_area);
    let leading = match open {
        Some(_) => LeadingSlot::Surface,
        None => LeadingSlot::AgentTabs,
    };
    let slot = render_tab_strip(frame, layout.tab_strip, model, identities, leading, hits);
    let question = match &open {
        Some(open) => {
            // The theme the author tries the surface in, on it alone.
            let question = match model.inspected_theme() {
                Some(name) => theme::drawn_in(name, || {
                    render_extension(frame, surface_area, model, open, hits, metrics)
                }),
                None => render_extension(frame, surface_area, model, open, hits, metrics),
            };
            render_extension_navigation(frame, slot, surface_area, open, hits);
            render_declared_overlays(frame, surface_area, inspector_area, model, hits);
            question
        }
        None => {
            match &model.space_overview {
                Some(overview) => {
                    let (scroll, sliding) =
                        render_space_overview(frame, layout.pane, model, overview, hits);
                    metrics.overview = scroll;
                    metrics.marquee |= sliding;
                }
                None => render_pane(frame, layout.pane, model),
            }
            None
        }
    };
    if let Some(area) = layout.bar {
        metrics.marquee |= render_bar(frame, area, model, hits);
    }
    for (slot, area) in &layout.bands {
        metrics.marquee |= render_band(frame, *area, model, *slot, hits);
    }
    // After the bands: a floating drawer stands over the pane's whole
    // side, its bands included.
    model.dock_drawn.set(None);
    if let Some(area) = layout.drawer {
        metrics.marquee |= render_declared_drawer(frame, area, model, hits);
    }
    // A plugin's overlay, over everything the workspace drew and under
    // the client's own questions and popups.
    if model.overlay_slot().is_some() {
        metrics.marquee |= render_overlay(frame, frame.area(), model, hits);
    }
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
    if model.action_index.is_some() || model.release_notes.is_some() || question.is_some() {
        crate::ui::widget::scrim::render(frame, frame.area());
    }
    if let Some(question) = &question {
        let mut view_hits = Vec::new();
        let attribution = model.attribution_of(Some(question.tag));
        crate::ui::extension_view::render_confirm(
            frame,
            &question.confirm,
            attribution.as_deref(),
            frame.area(),
            question.scope,
            &mut view_hits,
        );
        hits.splice(
            0..0,
            view_hits.into_iter().map(|(rect, hit)| {
                (
                    rect,
                    WorkspaceHit::Extension(ExtensionHit {
                        slot: question.tag,
                        hit,
                    }),
                )
            }),
        );
    }
    if let Some(modal) = &model.release_notes {
        let targets =
            crate::ui::release_notes::render(frame, frame.area(), modal, model.hovered_hint());
        // Prepended: what is underneath must not answer a click meant here,
        // and the keys and the mark answer before the body under them.
        hits.insert(0, (targets.popup, WorkspaceHit::ReleaseNotesBody));
        hits.splice(0..0, hint_targets(targets.keys));
    }
    if let Some(index) = &model.action_index {
        render_action_index(
            frame,
            frame.area(),
            index,
            &model.disabled_extensions,
            model.hovered_hint(),
            hits,
        );
    }
    if let Some(step) = model
        .directory_step
        .as_ref()
        .filter(|step| step.restart.is_none())
    {
        render_directory_step(frame, frame.area(), model, step, hits);
    }
    if let Some(picker) = &model.agent_picker {
        render_agent_picker(frame, frame.area(), picker.anchor, picker, hits);
    }
    if let Some(anchor) = model.status_catalog {
        render_status_catalog(frame, frame.area(), anchor, model.tick);
    }
    if let Some(popup) = &model.commit_detail {
        render_commit_detail(frame, frame.area(), popup);
    }
    let menu_row = model
        .context_menu
        .as_ref()
        .and_then(|menu| render_context_menu(frame, frame.area(), menu, hits));
    // Over the menu it stands beside, its targets after the menu's, so a
    // click where they meet is the panel's.
    if let Some(step) = model
        .directory_step
        .as_ref()
        .filter(|step| step.restart.is_some())
    {
        render_restart_panel(frame, frame.area(), model, step, menu_row, hits);
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
    // A question about what runs on this machine, over every surface it
    // could otherwise be mistaken for part of.
    if let Some(open) = &model.approval {
        render_approval(frame, frame.area(), open, hits);
    }
    if let Some(open) = &model.agent_write {
        render_agent_write(frame, frame.area(), open, hits);
    }
    if let Some(open) = &model.extension_offer {
        render_extension_offer(frame, frame.area(), open, model.hovered, hits);
    }
    if let Some(open) = &model.proposal {
        render_proposal(frame, frame.area(), open, hits);
    }
    if let Some(open) = &model.interrupt {
        render_interrupt(frame, frame.area(), open, hits);
    }
    if let Some(question) = model
        .space_overview
        .as_ref()
        .and_then(|overview| overview.question.as_ref())
    {
        render_overview_question(frame, frame.area(), question, hits);
    }
    if let Some(overview) = model
        .space_overview
        .as_ref()
        .filter(|overview| overview.issues_open)
    {
        render_space_issues(frame, frame.area(), overview, model.hovered_hint(), hits);
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
    let host_keys = footer_host_keys(model);
    let mut view_hits = Vec::new();
    metrics.surface = Some(crate::ui::graphics::in_slot(tag, || {
        crate::ui::extension_view::render(
            frame,
            view,
            area,
            crate::ui::extension_view::NavigatorFrame {
                width: model.navigator_width,
                scroll: model.navigator_scroll,
                resizing: model
                    .edge_drag
                    .is_some_and(|drag| drag.intent == Some(EdgeIntent::Resize)),
                sliding: match model.hovered {
                    Some(WorkspaceHit::Extension(ExtensionHit { slot, hit }))
                        if model.open_slot == Some(slot) =>
                    {
                        Some((hit, model.hovered_for()))
                    }
                    _ => None,
                },
            },
            crate::ui::extension_view::FooterKeys {
                scope,
                hovered: model.hovered_hint(),
                host: &host_keys,
            },
            match &model.selection {
                Some(Selection::Text(marking)) => Some(marking),
                _ => None,
            },
            &mut view_hits,
        )
    }));
    metrics.marquee |= metrics.surface.as_ref().is_some_and(|drawn| drawn.marquee);
    crate::ui::extension_view::render_row_menu(frame, view, area, model.menu_at, &mut view_hits);
    hits.extend(view_hits.into_iter().map(|(rect, hit)| {
        (
            rect,
            WorkspaceHit::Extension(ExtensionHit { slot: tag, hit }),
        )
    }));
    // After the row menu's: one opened low enough to cover the footer
    // answers for the cells it covers.
    if let Some(drawn) = &metrics.surface {
        hits.extend(hint_targets(drawn.hints.clone()));
    }
    view.confirm.clone().map(|confirm| Question {
        confirm,
        scope,
        tag,
    })
}

/// What the host adds to a surface's footer: on a declared one, the
/// actions it declares for the surface that hold a key, then its inspector.
/// One with no key is offered in the index, by pointer.
fn footer_host_keys(model: &WorkspaceModel) -> Vec<uze_keys::Action> {
    if !model
        .slot_in_pane()
        .is_some_and(|slot| slot.owner.declared().is_some())
    {
        return Vec::new();
    }
    model
        .declared_actions()
        .into_iter()
        .filter(|(_, chord)| chord.is_some())
        .map(|(action, _)| action)
        .chain([uze_keys::Action::ToggleInspector])
        .collect()
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
        navigation_hits.into_iter().map(|(rect, hit)| {
            (
                rect,
                WorkspaceHit::Extension(ExtensionHit {
                    slot: open.tag,
                    hit,
                }),
            )
        }),
    );
}

/// The surface standing in the pane, as it answered for this frame: read
/// once, so the bar's navigation and the surface below it are drawn from
/// the same answer.
struct OpenExtension {
    view: uze_extensions::view::View,
    scope: uze_keys::Scope,
    /// The slot it stands in, which every hit it drew is tagged with.
    tag: SlotId,
}

fn open_extension(model: &WorkspaceModel, area: Rect) -> Option<OpenExtension> {
    let slot = model.slot_in_pane()?;
    let space = match model.open_layout() {
        Some(uze_extensions::view::Layout::Board) => crate::ui::extension_view::board_space(area),
        _ => crate::ui::extension_view::content_space(area, model.navigator_width),
    };
    let scope = match &slot.owner {
        Owner::Builtin(id) if *id == uze_extensions::code::CATALOG.id => uze_keys::Scope::Code,
        Owner::Builtin(id) if *id == uze_extensions::review::CATALOG.id && model.open_typing() => {
            uze_keys::Scope::ReviewComposing
        }
        Owner::Builtin(id) if *id == uze_extensions::review::CATALOG.id => uze_keys::Scope::Review,
        Owner::Builtin(id) if *id == uze_extensions::architect::CATALOG.id => {
            uze_keys::Scope::Architect
        }
        Owner::Builtin(id) if *id == uze_extensions::spec::CATALOG.id => uze_keys::Scope::Spec,
        _ => uze_keys::Scope::Declared,
    };
    Some(OpenExtension {
        view: model.view_in_pane(space)?,
        scope,
        tag: slot.id,
    })
}

/// The question an open surface waits on. Drawn with the client's other
/// modals, centred on the whole frame over the scrim, rather than inside the
/// pane: until it is answered nothing else responds, and a dialog drawn in
/// the pane says the opposite.
struct Question {
    confirm: uze_extensions::view::Confirm,
    scope: uze_keys::Scope,
    tag: SlotId,
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
    let hue = match chip_state(model, WorkspaceHit::NewAgentMenu) {
        ChipState::Hovered | ChipState::Pressed => Token::StateWarning,
        ChipState::Resting => Token::StateWarningMuted,
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
fn upstream_sync(model: &WorkspaceModel, cwd: &Path) -> Option<uze_application::UpstreamSync> {
    if !is_unisolated(cwd) {
        return None;
    }
    model
        .remembered
        .upstream_syncs
        .get(&evaluation_key(cwd))
        .copied()
        .filter(|sync| sync.pull > 0 || sync.push > 0)
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
