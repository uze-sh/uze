//! TUI view — the Marketplace route (`Route::Plugins`).
//!
//! The agentic side of the product: skills, agents, MCP — everything
//! installable from a marketplace. One tree on the left: every
//! marketplace is a group (the embedded `uze-official` snapshot as `uze`,
//! and a `local` group for ad-hoc installs no catalogue knows about, so a
//! direct install never disappears from the TUI), its plugins under it,
//! and under an unfolded plugin its resources by kind. A fixed set of
//! columns runs down the right of every row, and the group the keyboard is
//! in stands on a recessed ground. A detail panel on the right describes
//! the row the keyboard is on and ends in what can be done about it. A
//! live filter narrows the plugins by name, marketplace or keyword.
//!
//! The keyboard walks [`TuiModel::plugin_tree_rows`]; a kind's heading is
//! drawn but never stood on.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use uze_application::CapabilityKind;
use uze_application::application::offers::ActionOffer;
use uze_application::application::{
    DoctorReport, FreshnessState, MarketplacePluginSummary, MarketplaceSummary, PluginCapability,
    Revision,
};

use super::plural;
use crate::ui::agent_support::{capability_label, resource_groups};
use crate::ui::content_area;
use crate::ui::hit::Hit;
use crate::ui::model::{Focus, PluginTreeRow, ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{Button, hint, mark, screen_header, text};

/// Below this many columns the panel goes and the tree takes the width.
const NARROW: u16 = 88;

/// How far the tree's text sits inside its column — the inset every
/// screen's content keeps.
const PAD: u16 = crate::ui::CONTENT_INSET_LEFT;

/// The tree's fixed columns, right of the name: what a row holds, when its
/// source last moved, and where it stands, right-aligned against the edge.
const CONTENTS_WIDTH: usize = 20;
const UPDATED_WIDTH: usize = 14;
const STATUS_WIDTH: usize = 18;
/// The narrowest a name is squeezed to before a column gives way to it.
const NAME_MIN: usize = 12;

/// Where each kind of row starts its text inside its group, past the
/// cursor that points at it: a marketplace, a plugin's chevron, a kind's heading, a
/// resource's branch.
const MARKET_INDENT: usize = 2;
const PLUGIN_INDENT: usize = 4;
const KIND_INDENT: usize = 6;
const RESOURCE_INDENT: usize = 8;

/// The key/value grid's key column in the panel.
const FIELD_KEY_WIDTH: usize = 12;

pub(crate) fn render_plugins(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    // Composed once for the frame: every question below is about the same
    // rows, and composing them compares every install to the catalogue.
    let rows = model.marketplace_rows();
    let visible = model.visible_indices_in(&rows);
    let markets = model.plugin_markets(&rows);

    let content = content_area(area);
    let panel_width = if area.width < NARROW {
        0
    } else {
        super::drawer_width(ResizablePanel::MarketplaceDrawer, model, content)
    };
    let tree = Rect {
        width: if panel_width > 0 {
            content.right().saturating_sub(panel_width + area.x)
        } else {
            area.width
        },
        ..area
    };
    let screen = Screen {
        model,
        rows: &rows,
        visible: &visible,
        cursor: model.plugin_tree_cursor(),
        focused: model.focus == Focus::Content,
    };
    render_tree(frame, tree, &screen, &markets, hits);
    if panel_width > 0 {
        let panel = super::drawer(
            frame,
            content,
            ResizablePanel::MarketplaceDrawer,
            model,
            hits,
        );
        render_panel(frame, panel, &screen, hits);
    }
}

/// What every part of the screen reads, composed once per frame.
struct Screen<'a> {
    model: &'a TuiModel,
    rows: &'a [MarketplacePluginSummary],
    visible: &'a [usize],
    cursor: PluginTreeRow,
    focused: bool,
}

impl Screen<'_> {
    fn plugin(&self, position: usize) -> Option<&MarketplacePluginSummary> {
        self.visible.get(position).map(|&raw| &self.rows[raw])
    }

    fn summary(&self, market: &str) -> Option<&MarketplaceSummary> {
        self.model
            .remembered
            .marketplaces
            .iter()
            .find(|summary| summary.name == market)
    }

    fn offered_by(&self, market: &str) -> Vec<&MarketplacePluginSummary> {
        self.rows
            .iter()
            .filter(|plugin| plugin.marketplace == market)
            .collect()
    }

    /// The market whose group the keyboard is in.
    fn active_market(&self) -> Option<&str> {
        match &self.cursor {
            PluginTreeRow::Market(market) => Some(market.as_str()),
            PluginTreeRow::Plugin(position) | PluginTreeRow::Resource(position, _) => self
                .plugin(*position)
                .map(|plugin| plugin.marketplace.as_str()),
        }
    }
}

/// The group name as rendered: "uze-official" reads oddly right above a
/// child plugin that's *also* named "uze" — the source beside it already
/// says what the suffix did.
fn group_display_name(marketplace: &str) -> &str {
    match marketplace {
        "uze-official" => "uze",
        other => other,
    }
}

/// Where a marketplace comes from, in the few words beside its name. The
/// one that ships with uze wears the official mark the extensions wear,
/// since it is official in the same sense.
fn market_source(summary: Option<&MarketplaceSummary>) -> Span<'static> {
    let dim = |words: String| Span::styled(words, theme::fg(Token::TextDim));
    let Some(summary) = summary else {
        return dim("installed directly".to_owned());
    };
    if summary.source.starts_with("embedded:") {
        return Span::styled(
            format!("{} Official", theme::glyph(Symbol::MarkOfficial)),
            theme::fg(Token::StateInfo),
        );
    }
    if summary.linked_to.is_some() {
        return dim("linked".to_owned());
    }
    let address = summary.homepage.as_deref().unwrap_or(&summary.source);
    dim(address
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_owned())
}

/// When anything from a marketplace was last installed or updated on this
/// machine: the most recent of its plugins'. Blank when none of them is
/// installed — when uze last checked for updates is a fact about uze, not
/// about the plugins.
fn market_updated(offered: &[&MarketplacePluginSummary]) -> String {
    offered
        .iter()
        .filter_map(|plugin| plugin.installed_at_unix)
        .max()
        .map(ago)
        .unwrap_or_default()
}

/// A count and its noun, in the plural the count takes.
fn counted(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", plural(count))
}

// --- The tree -----------------------------------------------------------

/// The columns a row is laid out in, shared by the heading and every row
/// so they line up table-style whatever each row's own name is. A column
/// that does not fit gives way rather than squeezing the name to nothing:
/// the update date first, then the contents.
struct Columns {
    contents: bool,
    updated: bool,
}

impl Columns {
    fn fitted(width: usize) -> Self {
        let full = NAME_MIN + CONTENTS_WIDTH + UPDATED_WIDTH + STATUS_WIDTH;
        Self {
            updated: width >= full,
            contents: width >= full - UPDATED_WIDTH,
        }
    }

    /// The cells right of the name, each padded to its column.
    fn cells(
        &self,
        contents: Span<'static>,
        updated: Span<'static>,
        status: Span<'static>,
    ) -> Vec<Span<'static>> {
        let mut cells = Vec::with_capacity(3);
        if self.contents {
            cells.push(padded(contents, CONTENTS_WIDTH));
        }
        if self.updated {
            cells.push(padded(updated, UPDATED_WIDTH));
        }
        let content = text::elide(&status.content, STATUS_WIDTH);
        cells.push(Span::styled(
            format!("{content:>STATUS_WIDTH$}"),
            status.style,
        ));
        cells
    }
}

fn padded(span: Span<'static>, width: usize) -> Span<'static> {
    let content = text::elide(&span.content, width.saturating_sub(1));
    Span::styled(format!("{content:<width$}"), span.style)
}

/// One line of the tree, what a click on it means, and whether the
/// keyboard is on it.
struct TreeLine {
    line: Line<'static>,
    /// Row targets from the line's left edge, first match wins: a chevron
    /// ahead of its row.
    hits: Vec<(u16, u16, Hit)>,
    selected: bool,
    /// A row of air, which a following one never doubles.
    gap: bool,
}

impl TreeLine {
    fn new(line: Line<'static>, hits: Vec<(u16, u16, Hit)>, selected: bool) -> Self {
        Self {
            line,
            hits,
            selected,
            gap: false,
        }
    }

    fn blank(width: u16, ground: Option<Token>) -> Self {
        Self {
            line: grounded(vec![Span::raw(" ".repeat(width.into()))], ground),
            hits: Vec::new(),
            selected: false,
            gap: true,
        }
    }

    /// A row of air inside `market`'s group: part of its card, so a click
    /// on it picks the marketplace the way its heading does — the whole
    /// card answers, not only the rows with words on them.
    fn in_group(width: u16, ground: Option<Token>, market: &str) -> Self {
        Self {
            hits: vec![(0, width, Hit::PluginMarket(Some(market.to_owned())))],
            ..Self::blank(width, ground)
        }
    }
}

fn grounded(mut spans: Vec<Span<'static>>, ground: Option<Token>) -> Line<'static> {
    if let Some(ground) = ground {
        for span in &mut spans {
            span.style = span.style.bg(theme::color(ground));
        }
    }
    Line::from(spans)
}

/// A row of a group: the cursor when the keyboard is on it, `lead` from
/// `indent`, elided to the
/// room the name has, then the cells, then the group's trailing pad —
/// all on `ground`.
fn tree_row(
    width: u16,
    ground: Option<Token>,
    marker: bool,
    indent: usize,
    lead: Vec<Span<'static>>,
    cells: Vec<Span<'static>>,
) -> Line<'static> {
    let width = usize::from(width);
    let trailing: usize = cells.iter().map(Span::width).sum::<usize>() + TRAILING;
    let name_room = width.saturating_sub(indent + trailing + 1);
    let mut lead = Line::from(lead);
    text::clip(&mut lead, name_room);
    let lead_width = lead.width();
    // The cursor stands just ahead of the row's own text, the way the
    // Shortcuts list points at its key, so it moves in with the tree's
    // indent instead of sitting at the group's edge.
    let marker_width = usize::from(theme::width(Symbol::Prompt)) + 1;
    let gap = indent.saturating_sub(marker_width);
    let mut spans = vec![Span::raw(" ".repeat(gap))];
    if marker {
        spans.push(Span::styled(
            format!("{} ", theme::glyph(Symbol::Prompt)),
            theme::fg_bold(Token::Accent),
        ));
    } else {
        spans.push(Span::raw(" ".repeat(indent - gap)));
    }
    spans.extend(lead.spans);
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(indent + lead_width + trailing)),
    ));
    spans.extend(cells);
    spans.push(Span::raw(" ".repeat(TRAILING)));
    grounded(spans, ground)
}

/// The columns a group keeps clear inside its right edge.
const TRAILING: usize = 2;

fn render_tree(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    screen: &Screen<'_>,
    markets: &[String],
    hits: &mut Vec<(Rect, Hit)>,
) {
    let model = screen.model;
    let text_area = Rect {
        x: area.x + PAD,
        width: area.width.saturating_sub(2 * PAD),
        ..area
    };
    let mut rows = crate::ui::Rows::over(text_area);
    if let Some(rect) = rows.next(1) {
        render_tree_header(frame, rect, screen, markets.len(), hits);
    }
    rows.gap();
    if let Some(rect) = rows.next(1) {
        super::filter_box(
            frame,
            rect,
            &model.remembered.plugin_screen.filter,
            "filter plugins and capabilities…",
            model.filtering,
        );
        hits.push((rect, Hit::FocusFilter));
    }
    rows.gap();
    // A selected group's ground spans the screen's text column and no
    // further, so its edges fall under the title like every other screen's
    // selected band; the rows indent inside it.
    let group_area = text_area;
    let columns =
        Columns::fitted(usize::from(group_area.width).saturating_sub(PLUGIN_INDENT + TRAILING + 4));
    if let Some(rect) = rows.next(1) {
        let heading = tree_row(
            group_area.width,
            None,
            false,
            MARKET_INDENT,
            vec![Span::styled("name", theme::fg(Token::TextDim))],
            columns.cells(
                Span::styled("contents", theme::fg(Token::TextDim)),
                Span::styled("updated", theme::fg(Token::TextDim)),
                Span::styled("status", theme::fg(Token::TextDim)),
            ),
        );
        frame.render_widget(
            Paragraph::new(heading),
            Rect {
                x: group_area.x,
                width: group_area.width,
                ..rect
            },
        );
    }
    rows.gap();
    let Some(list_top) = rows.next(1).map(|rect| rect.y) else {
        return;
    };
    let list = Rect::new(
        group_area.x,
        list_top,
        group_area.width,
        area.bottom().saturating_sub(list_top),
    );

    if screen.visible.is_empty() {
        let filter = model.remembered.plugin_screen.filter.trim();
        let message = if !filter.is_empty() {
            format!("nothing matches “{filter}”")
        } else {
            "No plugins available.".to_owned()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme::fg(Token::TextDim))),
            Rect {
                x: text_area.x,
                width: text_area.width,
                height: 1.min(list.height),
                ..list
            },
        );
        return;
    }

    let lines = tree_lines(screen, markets, &columns, list.width);
    let selected_line = lines.iter().position(|line| line.selected).unwrap_or(0);
    // Scrolled only as far as keeps the keyboard's row in view, with the
    // row after it when there is one: nothing else here moves the list, so
    // there is no offset to remember.
    let offset = (selected_line + 2).saturating_sub(list.height as usize);
    for (index, entry) in lines.into_iter().skip(offset).enumerate() {
        let y = list.y + index as u16;
        if y >= list.bottom() {
            break;
        }
        for (x, width, hit) in entry.hits {
            hits.push((Rect::new(list.x + x, y, width.min(list.width), 1), hit));
        }
        frame.render_widget(
            Paragraph::new(entry.line),
            Rect::new(list.x, y, list.width, 1),
        );
    }
}

/// The tree's own header: the screen's name and what it holds, and the
/// key that registers another marketplace pinned to the right — the one
/// offer here that is about no row.
fn render_tree_header(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    screen: &Screen<'_>,
    markets: usize,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let model = screen.model;
    let action = uze_keys::Action::AddMarketplace;
    let mut offer = hint::line(&model.scopes(), &[action]);
    if model.hovered_offer == Some(action) {
        for span in &mut offer.spans {
            span.style = span.style.fg(theme::color(Token::Accent));
        }
    }
    let offer_width = (offer.width() as u16).min(rect.width);
    hits.push((
        Rect::new(
            rect.right().saturating_sub(offer_width),
            rect.y,
            offer_width,
            1,
        ),
        Hit::OfferedAction(action),
    ));
    let note = Span::styled(
        format!(
            "{} · {}",
            counted(markets, "marketplace"),
            counted(screen.rows.len(), "plugin")
        ),
        theme::fg(Token::TextMuted),
    );
    screen_header::inline(frame, rect, Route::Plugins.label(), note, Some(offer));
}

/// Every line the tree draws, top to bottom: each group's heading, a row
/// of air, its plugins — an unfolded one followed by its resources by
/// kind and a row of air — and a row of air closing the group, with
/// another between groups.
fn tree_lines(
    screen: &Screen<'_>,
    markets: &[String],
    columns: &Columns,
    width: u16,
) -> Vec<TreeLine> {
    let model = screen.model;
    let filtering = !model.remembered.plugin_screen.filter.trim().is_empty();
    let active = screen.active_market();
    let mut lines = Vec::new();
    for market in markets {
        let members: Vec<usize> = screen
            .visible
            .iter()
            .enumerate()
            .filter(|(_, raw)| screen.rows[**raw].marketplace == *market)
            .map(|(position, _)| position)
            .collect();
        if members.is_empty() && filtering {
            continue;
        }
        // The container the keyboard is in stands out from the rest: the
        // whole group on the selection's ground while its heading is the
        // selected row, on the recessed one while a plugin inside it is.
        let ground = if screen.cursor == PluginTreeRow::Market(market.clone()) {
            Some(Token::SurfaceSelected)
        } else {
            (active == Some(market.as_str())).then_some(Token::SurfaceRecessed)
        };
        if !lines.is_empty() {
            lines.push(TreeLine::blank(width, None));
        }
        lines.push(market_line(screen, market, ground, columns, width));
        lines.push(TreeLine::in_group(width, ground, market));
        let last = members.len().saturating_sub(1);
        for (index, position) in members.into_iter().enumerate() {
            let Some(plugin) = screen.plugin(position) else {
                continue;
            };
            let id = model.marketplace_plugin_id(plugin);
            let expanded = model.expanded_plugins.contains(&id);
            if expanded && index > 0 && !lines.last().is_some_and(|line| line.gap) {
                lines.push(TreeLine::in_group(width, ground, market));
            }
            // A selected plugin is a container of its own: its row and every
            // resource under it share the selection's ground, and the cursor
            // alone says which of them the keyboard is on.
            let block = match &screen.cursor {
                PluginTreeRow::Plugin(at) | PluginTreeRow::Resource(at, _) if *at == position => {
                    Some(Token::SurfaceSelected)
                }
                _ => ground,
            };
            lines.push(plugin_line(
                screen, position, plugin, expanded, block, columns, width,
            ));
            if expanded {
                lines.extend(resource_lines(
                    screen, position, plugin, block, columns, width,
                ));
                if index < last {
                    lines.push(TreeLine::in_group(width, ground, market));
                }
            }
        }
        lines.push(TreeLine::in_group(width, ground, market));
    }
    lines
}

fn name_style(selected: bool, resting: Token) -> Style {
    if selected {
        theme::fg_bold(Token::TextBright)
    } else {
        theme::fg(resting)
    }
}

fn market_line(
    screen: &Screen<'_>,
    market: &str,
    group: Option<Token>,
    columns: &Columns,
    width: u16,
) -> TreeLine {
    let summary = screen.summary(market);
    let offered = screen.offered_by(market);
    let installed = offered.iter().filter(|plugin| plugin.installed).count();
    let behind = offered
        .iter()
        .filter(|plugin| plugin.freshness.behind())
        .count();
    let selected = screen.cursor == PluginTreeRow::Market(market.to_owned());
    let resting = if group.is_some() {
        Token::TextPrimary
    } else {
        Token::TextSecondary
    };
    let status = if behind > 0 {
        Span::styled(
            format!(
                "{} {}",
                theme::glyph(Symbol::ArrowUp),
                counted(behind, "update")
            ),
            theme::fg(Token::StateWarning),
        )
    } else {
        Span::styled(
            format!("{installed}/{} installed", offered.len()),
            theme::fg(Token::TextMuted),
        )
    };
    TreeLine::new(
        tree_row(
            width,
            group,
            selected && screen.focused,
            MARKET_INDENT,
            vec![
                Span::styled(
                    group_display_name(market).to_owned(),
                    name_style(selected, resting).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                market_source(summary),
            ],
            columns.cells(
                Span::styled(
                    counted(offered.len(), "plugin"),
                    theme::fg(Token::TextMuted),
                ),
                Span::styled(market_updated(&offered), theme::fg(Token::TextMuted)),
                status,
            ),
        ),
        vec![(0, width, Hit::PluginMarket(Some(market.to_owned())))],
        selected,
    )
}

fn plugin_line(
    screen: &Screen<'_>,
    position: usize,
    plugin: &MarketplacePluginSummary,
    expanded: bool,
    group: Option<Token>,
    columns: &Columns,
    width: u16,
) -> TreeLine {
    let model = screen.model;
    let id = model.marketplace_plugin_id(plugin);
    let selected = screen.cursor == PluginTreeRow::Plugin(position);
    let contents = model
        .plugin_resources_of(&id)
        .map(|resources| counted_capabilities(resources.len()))
        .unwrap_or_default();
    let (status, status_style) = plugin_status(model, plugin);
    // When it was installed or last updated here, read from the listing
    // itself, so it does not change as the selection passes over it. A
    // plugin that is not installed has no such date.
    let updated = plugin.installed_at_unix.map(ago).unwrap_or_default();
    TreeLine::new(
        tree_row(
            width,
            group,
            selected && screen.focused,
            PLUGIN_INDENT,
            vec![
                Span::styled(
                    format!("{} ", mark::disclosure(expanded)),
                    theme::fg(Token::TextDim),
                ),
                Span::styled(
                    plugin.name.clone(),
                    name_style(selected, Token::TextPrimary),
                ),
            ],
            columns.cells(
                Span::styled(contents, theme::fg(Token::TextMuted)),
                Span::styled(updated, theme::fg(Token::TextMuted)),
                Span::styled(status, status_style),
            ),
        ),
        vec![
            (
                PLUGIN_INDENT as u16 - 1,
                3,
                Hit::TogglePluginResources(position),
            ),
            (0, width, Hit::MarketplaceRow(position)),
        ],
        selected,
    )
}

fn counted_capabilities(count: usize) -> String {
    match count {
        1 => "1 capability".to_owned(),
        count => format!("{count} capabilities"),
    }
}

/// An unfolded plugin's resources: a heading per kind and a leaf per
/// resource, hung under the plugin's chevron. A leaf is a row the keyboard
/// and the pointer stand on; a kind's heading is not. A plugin whose
/// resources have not arrived yet says so: it was unfolded, which asked
/// for them.
fn resource_lines(
    screen: &Screen<'_>,
    position: usize,
    plugin: &MarketplacePluginSummary,
    group: Option<Token>,
    columns: &Columns,
    width: u16,
) -> Vec<TreeLine> {
    let model = screen.model;
    let id = model.marketplace_plugin_id(plugin);
    let note = |words: String| {
        TreeLine::new(
            tree_row(
                width,
                group,
                false,
                KIND_INDENT,
                vec![Span::styled(words, theme::fg(Token::TextDim))],
                Vec::new(),
            ),
            vec![(0, width, Hit::MarketplaceRow(position))],
            false,
        )
    };
    let Some(capabilities) = model.plugin_resources_of(&id) else {
        return vec![note("loading…".to_owned())];
    };
    let groups = resource_groups(capabilities);
    if groups.is_empty() {
        return vec![note(theme::glyph(Symbol::MarkUnsupported))];
    }
    let blank = || Span::raw("");
    let mut lines = Vec::new();
    for (kind, resources) in groups {
        lines.push(TreeLine::new(
            tree_row(
                width,
                group,
                false,
                KIND_INDENT,
                vec![Span::styled(
                    capability_label(kind).to_lowercase(),
                    theme::fg(Token::TextDim),
                )],
                columns.cells(
                    Span::styled(resources.len().to_string(), theme::fg(Token::TextDim)),
                    blank(),
                    blank(),
                ),
            ),
            vec![(0, width, Hit::MarketplaceRow(position))],
            false,
        ));
        let last = resources.len() - 1;
        for (index, resource) in resources.into_iter().enumerate() {
            let selected =
                screen.cursor == PluginTreeRow::Resource(position, resource.identity.clone());
            let branch = if index == last {
                Symbol::TreeLast
            } else {
                Symbol::TreeBranch
            };
            let resting = if plugin.installed {
                Token::TextSecondary
            } else {
                Token::TextMuted
            };
            let status = if plugin.installed {
                Span::styled("active", theme::fg(Token::AccentMuted))
            } else {
                blank()
            };
            lines.push(TreeLine::new(
                tree_row(
                    width,
                    group,
                    selected && screen.focused,
                    RESOURCE_INDENT,
                    vec![
                        Span::styled(
                            format!("{} ", theme::glyph(branch)),
                            theme::fg(Token::TextFaint),
                        ),
                        Span::styled(resource.name.clone(), name_style(selected, resting)),
                    ],
                    columns.cells(blank(), blank(), status),
                ),
                vec![(
                    0,
                    width,
                    Hit::PluginResource(position, resource.identity.clone()),
                )],
                selected,
            ));
        }
    }
    lines
}

/// Where a plugin stands, in the word its row and its panel say it.
///
/// One slot, and "updated" wins it: the badge is only ever raised by an
/// update that just landed, which is exactly what makes the row current.
/// Every other word here is a different fact, and none of them may read
/// like another.
fn plugin_status(model: &TuiModel, plugin: &MarketplacePluginSummary) -> (String, Style) {
    if !plugin.installed {
        return ("available".to_owned(), theme::fg(Token::TextMuted));
    }
    if model.was_just_updated(&model.marketplace_plugin_id(plugin)) {
        return ("updated".to_owned(), theme::fg(Token::Accent));
    }
    match &plugin.freshness.state {
        FreshnessState::Behind {
            commits: Some(commits),
        } => (
            format!("{} {commits} behind", theme::glyph(Symbol::ArrowUp)),
            theme::fg(Token::StateWarning),
        ),
        FreshnessState::Behind { commits: None } => (
            format!("{} update available", theme::glyph(Symbol::ArrowUp)),
            theme::fg(Token::StateWarning),
        ),
        FreshnessState::Linked { .. } => ("linked".to_owned(), theme::fg(Token::Accent)),
        _ => ("installed".to_owned(), theme::fg(Token::Accent)),
    }
}

/// The revision whichever detail matches this plugin carries: what you
/// have when it is installed, what you would be getting when it is not.
fn plugin_revision<'a>(
    model: &'a TuiModel,
    plugin: &MarketplacePluginSummary,
) -> Option<&'a Revision> {
    if plugin.installed {
        let id = model.marketplace_plugin_id(plugin);
        model
            .plugin_detail
            .as_ref()
            .filter(|detail| detail.plugin.id == id)
            .and_then(|detail| detail.revision.as_ref())
    } else {
        model
            .marketplace_detail
            .as_ref()
            .filter(|detail| {
                detail.summary.name == plugin.name
                    && detail.summary.marketplace == plugin.marketplace
            })
            .and_then(|detail| detail.revision.as_ref())
    }
}

// --- The panel ----------------------------------------------------------

/// What the panel says about the row the keyboard is on.
struct Detail {
    kind: String,
    status: (String, Style),
    title: String,
    /// The path a resource sits at in its package, under its name.
    subtitle: Option<String>,
    description: Option<String>,
    tags: Option<String>,
    fields: Vec<(&'static str, String, Style, Option<Hit>)>,
    /// A newer revision to take, said above the action.
    update: Option<String>,
    offers: Vec<ActionOffer>,
    /// The plugin a resource's action is about, named on its button.
    acting_on: Option<String>,
}

fn render_panel(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    screen: &Screen<'_>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let model = screen.model;
    let inner = area;
    let (detail, resource) = match screen.cursor.clone() {
        PluginTreeRow::Market(market) => (market_detail(screen, &market), None),
        PluginTreeRow::Plugin(position) => match screen.plugin(position) {
            Some(plugin) => (plugin_detail(model, plugin), None),
            None => return,
        },
        PluginTreeRow::Resource(position, _) => {
            match (screen.plugin(position), model.selected_resource()) {
                (Some(plugin), Some(resource)) => {
                    (resource_detail(plugin, &resource), Some(resource))
                }
                (Some(plugin), None) => (plugin_detail(model, plugin), None),
                _ => return,
            }
        }
    };
    let room = usize::from(inner.width);
    let footer_height = panel_footer_height(&detail);
    let body = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };

    let mut heading = vec![Span::styled(
        text::elide(
            &detail.kind,
            room.saturating_sub(detail.status.0.chars().count() + 2),
        ),
        theme::fg(Token::TextDim),
    )];
    crate::ui::widget::row::push_trailing(
        &mut heading,
        inner.width + crate::ui::widget::TRAILING_PAD,
        detail.status.0.clone(),
        detail
            .status
            .1
            .fg
            .unwrap_or_else(|| theme::color(Token::TextMuted)),
    );
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(heading),
        Line::from(Span::styled(
            text::elide(&detail.title, room),
            theme::fg_bold(Token::TextBright),
        )),
    ];
    if let Some(subtitle) = &detail.subtitle {
        lines.push(Line::from(Span::styled(
            text::elide_head(subtitle, room),
            theme::fg(Token::TextMuted),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    if let Some(resource) = &resource {
        let header_rows = lines.len() as u16;
        frame.render_widget(Paragraph::new(lines), body);
        let preview = Rect {
            y: body.y + header_rows,
            height: body.height.saturating_sub(header_rows),
            ..body
        };
        frame.render_widget(
            Paragraph::new(preview_rows(
                resource,
                preview.width,
                preview.height,
                model.resource_scroll,
            )),
            preview,
        );
        hits.push((preview, Hit::ResourcePreview));
    } else {
        if let Some(description) = &detail.description {
            lines.extend(
                text::fold(description, room)
                    .into_iter()
                    .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextTertiary)))),
            );
        }
        if let Some(tags) = &detail.tags {
            lines.push(Line::from(""));
            lines.extend(
                text::fold(tags, room)
                    .into_iter()
                    .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextMuted)))),
            );
        }
        if detail.description.is_some() || detail.tags.is_some() {
            lines.push(Line::from(""));
            lines.push(Line::from(""));
        }
        // Folded rather than cut: an address is read in full before it is
        // trusted, and a list of names cut at the edge hides which ones
        // are missing.
        let value_room = room.saturating_sub(FIELD_KEY_WIDTH).max(1);
        for (key, value, style, hit) in &detail.fields {
            let y = body.y + lines.len() as u16;
            let rows = text::fold(value, value_room);
            if let Some(hit) = hit
                && y < body.bottom()
            {
                let height = (rows.len() as u16).min(body.bottom() - y);
                hits.push((Rect::new(body.x, y, body.width, height), hit.clone()));
            }
            for (index, row) in rows.into_iter().enumerate() {
                let key = if index == 0 { *key } else { "" };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{key:<FIELD_KEY_WIDTH$}"),
                        theme::fg(Token::TextDim),
                    ),
                    Span::styled(row, *style),
                ]));
            }
        }
        frame.render_widget(Paragraph::new(lines), body);
    }

    render_panel_footer(
        frame,
        Rect::new(inner.x, body.bottom(), inner.width, footer_height),
        model,
        &detail,
        hits,
    );
}

/// The action the panel's foot offers first, and the one Enter performs:
/// what builds before what destroys, the update aside — it has a row of
/// its own above.
pub(crate) fn primary_offer(offers: &[ActionOffer]) -> Option<uze_keys::Action> {
    let mut available: Vec<uze_keys::Action> = offers
        .iter()
        .filter(|offer| {
            offer.is_available()
                && !matches!(
                    offer.action,
                    uze_keys::Action::Activate | uze_keys::Action::UpdatePlugin
                )
        })
        .map(|offer| offer.action)
        .collect();
    available.sort_by_key(|action| action.destructive());
    available.first().copied()
}

fn update_offered(offers: &[ActionOffer]) -> bool {
    offers
        .iter()
        .any(|offer| offer.action == uze_keys::Action::UpdatePlugin && offer.is_available())
}

/// Rows the panel's foot takes: a row of air, the update and a row of air
/// when there is one to take, the action, and a row of air under it.
fn panel_footer_height(detail: &Detail) -> u16 {
    let update = if detail.update.is_some() { 2 } else { 0 };
    let action = if primary_offer(&detail.offers).is_some() {
        2
    } else {
        0
    };
    let rows = update + action;
    // A row of air above the foot, so what scrolls in the body never runs
    // into the buttons.
    if rows > 0 { rows + 1 } else { 0 }
}

/// The panel's foot, anchored to its bottom: a newer revision and the
/// button that takes it, then the action. The key that reaches it is the
/// footer's to say.
fn render_panel_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    detail: &Detail,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let mut y = area.y + 1;
    if let Some(note) = &detail.update {
        if update_offered(&detail.offers) {
            let action = uze_keys::Action::UpdatePlugin;
            let button = Button::new(action.label(), Token::StateWarning)
                .strong(model.hovered_offer == Some(action));
            let rect = Rect::new(
                area.right().saturating_sub(button.width()),
                y,
                button.width().min(area.width),
                1,
            );
            button.render(frame, rect);
            hits.push((rect, Hit::OfferedAction(action)));
        }
        let room = area
            .width
            .saturating_sub(Button::new("Update", Token::StateWarning).width() + 2);
        frame.render_widget(
            Paragraph::new(Span::styled(
                text::elide(note, room.into()),
                theme::fg(Token::StateWarning),
            )),
            Rect::new(area.x, y, room, 1),
        );
        y += 2;
    }
    let Some(action) = primary_offer(&detail.offers) else {
        return;
    };
    if y >= area.bottom() {
        return;
    }
    let hue = if action.destructive() {
        Token::StateDanger
    } else if action == uze_keys::Action::InstallPlugin {
        Token::Accent
    } else {
        Token::TextSecondary
    };
    let label = match &detail.acting_on {
        Some(plugin) => format!("{} {plugin}", action.label()),
        None => action.label(),
    };
    let button = Button::new(label, hue).strong(model.hovered_offer == Some(action));
    let rect = Rect::new(area.x, y, button.width().min(area.width), 1);
    button.render(frame, rect);
    hits.push((rect, Hit::OfferedAction(action)));
}

fn market_detail(screen: &Screen<'_>, market: &str) -> Detail {
    let model = screen.model;
    let summary = screen.summary(market);
    let offered = screen.offered_by(market);
    let installed = offered.iter().filter(|plugin| plugin.installed).count();
    let behind = offered
        .iter()
        .filter(|plugin| plugin.freshness.behind())
        .count();
    let kind = match summary {
        None => "installed directly",
        Some(summary) if summary.source.starts_with("embedded:") => "marketplace · official",
        Some(_) => "marketplace",
    };
    let status = if behind > 0 {
        (
            format!("{} update available", theme::glyph(Symbol::ArrowUp)),
            theme::fg(Token::StateWarning),
        )
    } else {
        ("up to date".to_owned(), theme::fg(Token::Accent))
    };
    let mut fields = Vec::new();
    match summary {
        Some(summary) => {
            match summary.homepage.as_deref() {
                Some(url) => fields.push((
                    "source",
                    format!("{url} {}", theme::glyph(Symbol::ArrowExternal)),
                    link_style(model),
                    Some(Hit::OpenLink(summary.name.clone())),
                )),
                None => fields.push((
                    "source",
                    summary.source.clone(),
                    theme::fg(Token::TextTertiary),
                    None,
                )),
            }
            if let Some(checkout) = &summary.linked_to {
                fields.push((
                    "linked to",
                    checkout.display().to_string(),
                    theme::fg(Token::TextTertiary),
                    None,
                ));
            }
        }
        None => fields.push((
            "source",
            "a path or a Git URL no marketplace offers".to_owned(),
            theme::fg(Token::TextTertiary),
            None,
        )),
    }
    let names = offered
        .iter()
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    fields.push((
        "plugins",
        if names.is_empty() {
            "—".to_owned()
        } else {
            names
        },
        theme::fg(Token::TextSecondary),
        None,
    ));
    Detail {
        kind: kind.to_owned(),
        status,
        title: group_display_name(market).to_owned(),
        subtitle: None,
        description: Some(format!(
            "{} · {installed}/{} installed",
            counted(offered.len(), "plugin"),
            offered.len()
        )),
        tags: None,
        fields,
        update: (behind > 0).then(|| format!("{} with a new revision", counted(behind, "plugin"))),
        offers: summary.map(MarketplaceSummary::offers).unwrap_or_default(),
        acting_on: None,
    }
}

fn plugin_detail(model: &TuiModel, plugin: &MarketplacePluginSummary) -> Detail {
    let id = model.marketplace_plugin_id(plugin);
    let mut fields = Vec::new();
    if let Some(revision) = plugin_revision(model, plugin) {
        let value = match revision {
            Revision::Commit { short, age, .. } => format!("{short} · {age}"),
            Revision::Bundled { version } => format!("ships with uze {version}"),
            Revision::Checkout { path } => format!("follows {}", path.display()),
        };
        fields.push(("revision", value, theme::fg(Token::TextTertiary), None));
    }
    for (kind, resources) in model
        .plugin_resources_of(&id)
        .map(resource_groups)
        .unwrap_or_default()
    {
        let names = resources
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        fields.push((kind_key(kind), names, theme::fg(Token::TextSecondary), None));
    }
    // Whether the plugin actually reaches the harnesses: the one thing
    // about an installed plugin its status word cannot say.
    if plugin.installed {
        let health = plugin_health(model.remembered.doctor.as_ref(), &id);
        let tone = if health == "ready" {
            Token::TextSecondary
        } else {
            Token::StateWarning
        };
        if health != "unknown" {
            fields.push(("health", health.to_owned(), theme::fg(tone), None));
        }
    }
    Detail {
        kind: format!("plugin · {}", group_display_name(&plugin.marketplace)),
        status: plugin_status(model, plugin),
        title: plugin.name.clone(),
        subtitle: None,
        description: plugin.description.clone(),
        tags: (!plugin.keywords.is_empty()).then(|| plugin.keywords.join(", ")),
        fields,
        update: (plugin.installed && plugin.freshness.behind()).then(|| {
            format!(
                "new revision from {}",
                group_display_name(&plugin.marketplace)
            )
        }),
        offers: plugin.offers(),
        acting_on: None,
    }
}

fn resource_detail(plugin: &MarketplacePluginSummary, resource: &PluginCapability) -> Detail {
    let status = if plugin.installed {
        ("active".to_owned(), theme::fg(Token::Accent))
    } else {
        (
            format!("install {} to use", plugin.name),
            theme::fg(Token::TextDim),
        )
    };
    Detail {
        kind: format!("{} · {}", resource_kind_label(resource.kind), plugin.name),
        status,
        title: resource.name.clone(),
        subtitle: (!resource.preview.path.is_empty()).then(|| resource.preview.path.clone()),
        description: None,
        tags: None,
        fields: Vec::new(),
        update: (plugin.installed && plugin.freshness.behind()).then(|| {
            format!(
                "new revision from {}",
                group_display_name(&plugin.marketplace)
            )
        }),
        offers: plugin.offers(),
        acting_on: Some(plugin.name.clone()),
    }
}

/// A resource kind as the panel's grid names it.
fn kind_key(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::AgentSkill => "skills",
        CapabilityKind::Agent => "agents",
        CapabilityKind::Hook => "hooks",
        CapabilityKind::Mcp => "mcp",
        CapabilityKind::Instruction => "instructions",
    }
}

/// What a resource is, as the panel heads it.
fn resource_kind_label(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::AgentSkill => "skill",
        CapabilityKind::Agent => "agent",
        CapabilityKind::Hook => "hook",
        CapabilityKind::Mcp => "mcp server",
        CapabilityKind::Instruction => "instructions",
    }
}

/// How long ago `at_unix` was, in the largest whole unit: `just now`,
/// `5m ago`, `3h ago`, `2d ago`.
fn ago(at_unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(at_unix, |elapsed| elapsed.as_secs());
    match now.saturating_sub(at_unix) {
        seconds if seconds < 60 => "just now".to_owned(),
        seconds if seconds < 3_600 => format!("{}m ago", seconds / 60),
        seconds if seconds < 86_400 => format!("{}h ago", seconds / 3_600),
        seconds => format!("{}d ago", seconds / 86_400),
    }
}

/// A Markdown file's leading `---` frontmatter and the body after it.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let end = rest.find("\n---")?;
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    Some((&rest[..end], body))
}

/// A resource's text as the drawer renders it: Markdown as Markdown, with
/// its frontmatter shown as the YAML it is rather than read as a rule and
/// a heading; a JSON definition as highlighted JSON.
fn preview_markdown(resource: &PluginCapability) -> String {
    let text = &resource.preview.text;
    match resource.kind {
        CapabilityKind::Mcp | CapabilityKind::Hook => format!("```json\n{text}\n```\n"),
        CapabilityKind::AgentSkill | CapabilityKind::Agent | CapabilityKind::Instruction => {
            match split_frontmatter(text) {
                Some((frontmatter, body)) => format!("```yaml\n{frontmatter}\n```\n\n{body}"),
                None => text.clone(),
            }
        }
    }
}

/// The last preview drawn, and what it was drawn from.
struct RenderedPreview {
    identity: String,
    theme: String,
    text: String,
    width: u16,
    /// Folded to `width` already, one entry per row on screen.
    rows: Vec<Line<'static>>,
    /// How many of them the drawer had room for.
    height: u16,
}

thread_local! {
    static RENDERED_PREVIEW: std::cell::RefCell<Option<RenderedPreview>> =
        const { std::cell::RefCell::new(None) };
}

/// The rows of `resource`'s preview on show in a column `width` wide and
/// `height` tall, `scroll` rows down.
///
/// Rendered and folded once per resource, text, theme and width rather
/// than once per frame: a Markdown parse and a syntect pass over every
/// fenced block cost milliseconds for a long skill, and a wrapped
/// paragraph refolds the whole document even to show one screen of it —
/// while the drawer is redrawn on every keystroke, pointer move and tick.
/// Kept here rather than on the model because it is a cost of drawing,
/// not state anything else reads; one entry because the drawer shows one
/// resource.
fn preview_rows(
    resource: &PluginCapability,
    width: u16,
    height: u16,
    scroll: u16,
) -> Vec<Line<'static>> {
    let theme = uze_theme::active().syntax_theme().to_owned();
    RENDERED_PREVIEW.with_borrow_mut(|cached| {
        let current = cached.as_ref().is_some_and(|cached| {
            cached.identity == resource.identity
                && cached.width == width
                && cached.theme == theme
                && cached.text == resource.preview.text
        });
        if !current {
            let rows = uze_extensions::code::markdown(&preview_markdown(resource), &theme)
                .iter()
                .flat_map(|line| crate::ui::extension_view::prose_rows(line, width.into()))
                .collect();
            *cached = Some(RenderedPreview {
                identity: resource.identity.clone(),
                theme,
                text: resource.preview.text.clone(),
                width,
                rows,
                height,
            });
        }
        let Some(cached) = cached.as_mut() else {
            return Vec::new();
        };
        cached.height = height;
        cached
            .rows
            .iter()
            .skip(scroll.into())
            .take(height.into())
            .cloned()
            .collect()
    })
}

/// How far the preview drawn last can scroll before its last row reaches
/// the drawer's bottom — what keeps the wheel from scrolling on into
/// nothing, and the way back up from costing the presses spent there.
pub(crate) fn preview_scroll_limit() -> u16 {
    RENDERED_PREVIEW.with_borrow(|cached| {
        cached.as_ref().map_or(0, |cached| {
            u16::try_from(cached.rows.len())
                .unwrap_or(u16::MAX)
                .saturating_sub(cached.height)
        })
    })
}

/// A marketplace's address as a link: the accent, which is what the
/// design gives an address, underlined under the pointer, and written out
/// in full so it can be checked before it is trusted — and copied, when
/// the terminal has no browser to hand it to.
fn link_style(model: &TuiModel) -> Style {
    let style = theme::fg(Token::Accent);
    if model.source_link_hovered {
        style.add_modifier(Modifier::UNDERLINED)
    } else {
        style
    }
}

/// Attachment health for one plugin, derived from the doctor report every
/// refresh carries — never fetched per row, so the status line always has
/// a real answer instead of a masked placeholder.
fn plugin_health(doctor: Option<&DoctorReport>, plugin: &str) -> &'static str {
    let Some(state) = doctor
        .and_then(|doctor| doctor.attachments.iter().find(|item| item.plugin == plugin))
        .map(|item| &item.state)
    else {
        return "unknown";
    };
    if state.drifted + state.conflicts + state.blocked > 0 {
        "needs attention"
    } else if state.missing > 0 {
        "missing"
    } else {
        "ready"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_is_said_in_its_largest_whole_unit() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(ago(now), "just now");
        assert_eq!(ago(now - 5 * 60), "5m ago");
        assert_eq!(ago(now - 3 * 3_600), "3h ago");
        assert_eq!(ago(now - 2 * 86_400), "2d ago");
    }
}
