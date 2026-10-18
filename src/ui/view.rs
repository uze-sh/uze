//! TUI view — shared helpers used by every route's render function.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::ui::hit::Hit;
use crate::ui::model::{ResizablePanel, TuiModel};
use crate::ui::side_panel_area;
use crate::ui::theme::{self, Token};
use crate::ui::widget::{Button, Field, drawer};
use uze_application::application::offers::ActionOffer;

pub mod catalog;
pub mod extensions;
pub mod harnesses;
pub mod health;
pub mod keys;
pub mod overview;
pub mod plugins;
pub mod profiles;
pub mod settings;

/// A drawer's width at rest, which a screen sizing a column of its own
/// beside one starts from.
pub(crate) const DRAWER_DEFAULT_WIDTH: u16 = drawer::DEFAULT_WIDTH;

/// How wide `panel`'s drawer is drawn over `content`: where it was dragged
/// to, or its width at rest, within what [`drawer::width`] allows.
pub(crate) fn drawer_width(panel: ResizablePanel, model: &TuiModel, content: Rect) -> u16 {
    drawer::width(panel.width(model), content.width)
}

/// The first line a list of `total` lines shows in `visible`, settled in
/// `scroll`: wherever the wheel left it, until the selection — the lines
/// `selected` spans — moves, which pulls the window just far enough to
/// show all of it. The keys move the selection and the page follows; the
/// wheel moves the page and the selection stays.
pub(crate) fn settle(
    scroll: &std::cell::Cell<crate::ui::extension_view::NavigatorScroll>,
    selected: Option<(usize, usize)>,
    total: usize,
    visible: usize,
) -> usize {
    let previous = scroll.get();
    let mut settled = previous.settled(selected.map(|(_, last)| last), total, visible);
    if let Some((first, _)) = selected
        && settled.revealed != previous.revealed
        && first < settled.first
    {
        settled.first = first;
    }
    scroll.set(settled);
    settled.first
}

/// A list's scrollbar on the rule of the detail drawer over `content`, the
/// line the list already ends at, rather than a second line beside it.
/// Drawn after the drawer, which paints its rule. A list that can be
/// dragged registers only the handle, as `grab`, ahead of the rule's
/// resize handle, so grabbing it scrolls and the rest of the rule still
/// moves the drawer.
pub(crate) fn scrollbar_on_drawer_rule(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    model: &TuiModel,
    rows: Rect,
    (visible, total, first): (usize, usize, usize),
    hits: &mut Vec<(Rect, Hit)>,
    grab: Option<fn(Rect) -> Hit>,
) {
    let panel = ResizablePanel::Drawer;
    let column = content
        .right()
        .saturating_sub(drawer_width(panel, model, content));
    let track = Rect::new(column, rows.y, 1, rows.height);
    let Some(bar) = crate::ui::widget::Scrollbar::measure(track, visible, total) else {
        return;
    };
    bar.render_on_draggable(frame, first, model.dragging_panel == Some(panel));
    if let Some(grab) = grab {
        hits.insert(0, (bar.thumb(first), grab(track)));
    }
}

/// `panel`'s detail drawer off the right of `content`, its rule lit while
/// it is dragged and registered first as `panel`'s resize handle. Returns
/// the padded rectangle its content goes in.
pub(crate) fn drawer(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    panel: ResizablePanel,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) -> Rect {
    let area = side_panel_area(content, drawer_width(panel, model, content));
    let (inner, handle) = drawer::render(frame, area, model.dragging_panel == Some(panel));
    hits.insert(0, (handle, Hit::ResizePanel(panel)));
    inner
}

/// A drawer's content split into its body and the footer
/// [`render_drawer_footer`] draws beneath it.
pub(crate) fn drawer_body_and_footer(inner: Rect, offers: &[ActionOffer]) -> (Rect, Rect) {
    drawer::split(inner, !drawer_buttons(offers).is_empty())
}

/// A list's search row: what has been typed, or `placeholder` when
/// nothing has, with the caret while it is `active`. One row; the screens
/// leave one blank row under it.
pub(crate) fn filter_box(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    text: &str,
    placeholder: &str,
    active: bool,
) {
    Field::new(text, placeholder).focused(active).render_search(
        frame,
        Rect {
            height: 1.min(area.height),
            ..area
        },
    );
}

/// A label over a block — a drawer's or a list's section — in the
/// lowercase the Marketplace tree heads its own with, so every screen
/// reads alike.
pub(crate) fn section_label(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_lowercase(), theme::fg(Token::TextDim)))
}

/// Rows a screen gives its search row: the row and the blank one under it.
pub(crate) const FILTER_ROWS: u16 = 2;

pub(crate) use drawer::Status as DrawerStatus;

/// Rows [`render_drawer_footer`] needs for `offers`.
pub(crate) fn drawer_footer_height(offers: &[ActionOffer]) -> u16 {
    drawer::foot_height(!drawer_buttons(offers).is_empty())
}

/// The offers a drawer draws as buttons: the available ones, what builds
/// before what destroys. `Activate` is left out — it is what opened the
/// drawer, and a button that opens what is already open does nothing.
fn drawer_buttons(offers: &[ActionOffer]) -> Vec<uze_keys::Action> {
    let mut buttons: Vec<uze_keys::Action> = offers
        .iter()
        .filter(|offer| offer.is_available() && offer.action != uze_keys::Action::Activate)
        .map(|offer| offer.action)
        .collect();
    buttons.sort_by_key(|action| action.destructive());
    buttons
}

/// A button's look: soft at rest, the full hue under the pointer — the
/// step between the two is what says it can be clicked. One under way
/// stays `strong`, so the button that started it reads as the one that
/// stops it. `ground` is the surface it sits on, which the soft tint is
/// mixed against.
pub(crate) fn button_style(hue: Token, strong: bool, ground: Token) -> Style {
    let style = if strong {
        theme::on(Token::SurfaceBackground, hue)
    } else {
        Style::default()
            .fg(theme::color(hue))
            .bg(theme::softened(hue, ground))
    };
    style.add_modifier(Modifier::BOLD)
}

/// Every detail drawer ends the same way: where the thing stands, then
/// what can be done about it, as buttons.
///
/// The drawer is the one place a row's actions are performed with the
/// pointer, so its buttons are the selected thing's offers — the available
/// ones only: a button that cannot run is a caption pretending to be a
/// control. The first thing that builds wears the accent, anything else
/// is neutral, and what destroys comes last in the danger colour, so the
/// weight of each is seen before it is clicked. `engaged` is an action
/// already under way (a key being captured), drawn in the warning colour.
pub(crate) fn render_drawer_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    status: DrawerStatus<'_>,
    offers: &[ActionOffer],
    hovered: Option<uze_keys::Action>,
    engaged: Option<uze_keys::Action>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let buttons: Vec<_> = drawer_buttons(offers)
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            let hue = if engaged == Some(action) {
                Token::StateWarning
            } else if action.destructive() {
                Token::StateDanger
            } else if index == 0 {
                Token::Accent
            } else {
                Token::TextSecondary
            };
            (
                Button::new(action.label(), hue)
                    .strong(hovered == Some(action) || engaged == Some(action))
                    .ground(Token::SurfaceBackground),
                Hit::OfferedAction(action),
            )
        })
        .collect();
    hits.extend(drawer::foot(frame, area, status, &buttons));
}

/// The plural suffix a count takes in a screen's English: `""` for one.
fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod drawer_tests {
    use super::*;

    /// A drawer never takes more than half of what there is, whatever it
    /// was dragged to or defaults to.
    ///
    /// Leaving the list its minimum is a floor, not a share: on a narrow
    /// screen the default took two thirds of the content and the list it
    /// is a detail *of* got the remainder. The default itself is the
    /// width a paragraph reads at, not the width there is.
    #[test]
    fn a_drawer_never_takes_more_than_half_the_content() {
        let model = TuiModel::default();
        let panel = ResizablePanel::Drawer;
        for width in [60u16, 80, 100, 140, 200] {
            let content = Rect::new(0, 0, width, 40);
            let drawn = drawer_width(panel, &model, content);
            assert!(
                drawn <= (width / 2).max(drawer::MIN_WIDTH),
                "{width} wide: {drawn}"
            );
            assert!(
                content.width.saturating_sub(drawn) >= drawer::MIN_WIDTH,
                "and the list keeps its own minimum at {width}: {drawn}"
            );
        }
        // Wide enough for both, it is a width at rest and not a share of
        // the screen: a detail panel does not get better by getting wider.
        assert_eq!(
            drawer_width(panel, &model, Rect::new(0, 0, 120, 40)),
            DRAWER_DEFAULT_WIDTH
        );
        assert_eq!(
            drawer_width(panel, &model, Rect::new(0, 0, 200, 40)),
            drawer::WIDE_WIDTH
        );
    }
}
