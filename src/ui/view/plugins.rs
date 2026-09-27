//! TUI view — the Marketplace route (`Route::Plugins`).
//!
//! The agentic side of the product: skills, agents, MCP — everything
//! installable from a marketplace. Three columns: the marketplaces down the
//! left (an "All" first, then every registered one, the embedded
//! `uze-official` snapshot badged as official, and a "local" group for
//! ad-hoc installs no catalogue knows about, so a direct install never
//! disappears from the TUI); the plugins of the one selected beside them,
//! each able to unfold into the resources it offers; and a detail column
//! describing whichever of the two the keyboard is in. A live filter
//! narrows the plugins by name, marketplace or keyword.
//!
//! Selection indexes the *visible* sequence
//! (`TuiModel::marketplace_visible_indices`); the unfolded resource rows
//! are drawn under their plugin and are never selectable, so a position in
//! the list is always a plugin.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use uze_application::CapabilityKind;
use uze_application::application::{
    DoctorReport, FreshnessState, MarketplacePluginSummary, PluginCapability, Revision,
};

use super::super::agent_support::capability_label;
use super::super::hit::Hit;
use super::super::model::{PluginPane, ResizablePanel, Route, TuiModel};
use super::super::{content_area, render_screen_header};
use super::{DrawerStatus, render_drawer_footer};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{Edge, RowState, Rule, mark, row, text};

/// Both status labels are 9 characters (`Installed`/`Available`), but that's
/// incidental — pad explicitly so alignment holds even if a future status
/// label changes length.
const STATUS_WIDTH: usize = 9;

/// Widest text the freshness column ever holds, so a row without that
/// badge still reserves its column and nothing after it drifts.
const UPDATE_WIDTH: usize = "Update available".len();

/// The rail's share of the list side, and the bounds it keeps whatever
/// the terminal's width: narrower and a marketplace's name is all "…",
/// wider and it takes room the plugins' own names need more.
const RAIL_SHARE: u16 = 30;
const RAIL_MIN: u16 = 16;
const RAIL_MAX: u16 = 26;

/// What the rail hangs into the content inset: its selection bar and the
/// gap after it.
const RAIL_GUTTER: u16 = 2;

/// Where the rail's counts go: `installed/offered` for up to 999 of each.
const COUNT_WIDTH: usize = 7;

pub(crate) fn render_plugins(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let outer = content_area(area);
    // Composed once for the frame: every question below is about the same
    // rows, and composing them compares every install to the catalogue.
    let marketplace_rows = model.marketplace_rows();
    let visible = model.visible_indices_in(&marketplace_rows);
    let markets = model.plugin_markets(&marketplace_rows);
    let market = model.market_in_rail(&marketplace_rows);
    let selected = visible
        .get(model.remembered.plugin_screen.selected)
        .map(|&raw| &marketplace_rows[raw]);

    // Always there: it describes the plugin the keyboard is on, and the
    // marketplace when the keyboard is on the rail or there is no plugin.
    let drawer_width = super::drawer_width(ResizablePanel::MarketplaceDrawer, model, outer);
    let side = Rect::new(
        outer.x,
        outer.y,
        outer.width.saturating_sub(drawer_width + 1),
        outer.height,
    );
    let trailer = Span::styled(
        format!(
            "{} marketplace{} · {} plugin{}",
            markets.len(),
            plural(markets.len()),
            marketplace_rows.len(),
            plural(marketplace_rows.len()),
        ),
        theme::fg(Token::TextMuted),
    );
    let content = render_screen_header(frame, side, Route::Plugins, Some(trailer));
    let filter_area = Rect::new(content.x, content.y, content.width, 2);
    super::filter_box(
        frame,
        filter_area,
        &model.remembered.plugin_screen.filter,
        "Filter plugins…",
        model.filtering,
    );
    hits.push((filter_area, Hit::FocusFilter));
    let body = Rect::new(
        content.x,
        content.y + 3,
        content.width,
        content.height.saturating_sub(3),
    );

    let rail_width = (body.width * RAIL_SHARE / 100).clamp(RAIL_MIN, RAIL_MAX);
    // Two columns into the inset, so the selection bar and its gap sit in
    // the margin and every name lines up with the title and the filter
    // above it, the way the sidebar's own entries hang their bar.
    let gutter = body.x.min(RAIL_GUTTER);
    let rail = Rect::new(
        body.x - gutter,
        body.y,
        (rail_width + gutter).min(body.width + gutter),
        body.height,
    );
    render_rail(
        frame,
        rail,
        model,
        &marketplace_rows,
        &markets,
        market,
        hits,
    );
    let list_x = rail.right() + 2;
    let list = Rect::new(
        list_x,
        body.y,
        body.right().saturating_sub(list_x),
        body.height,
    );
    render_list(
        frame,
        list,
        model,
        &marketplace_rows,
        &visible,
        market.is_none(),
        hits,
    );

    match selected {
        Some(plugin) if model.plugin_pane == PluginPane::Plugins => {
            render_plugin_drawer(frame, outer, model, plugin, hits)
        }
        _ => render_market_drawer(frame, outer, model, &marketplace_rows, market, hits),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// The group name as rendered: "uze-official" reads oddly right above a
/// child plugin that's *also* named "uze" — the official mark already says
/// what the suffix did — and the synthetic local group gets a capitalized
/// label instead of the bare word.
fn group_display_name(marketplace: &str) -> &str {
    match marketplace {
        "uze-official" => "uze",
        "local" => "Local",
        other => other,
    }
}

/// The ground and the edge mark of a selected row. The keyboard's own
/// row wears the accent bar; a selection the keyboard has left keeps a
/// quieter ground, so the reader still sees what the other column is
/// describing.
fn selection(selected: bool, focused: bool) -> (RowState, String) {
    match (selected, focused) {
        (true, true) => (RowState::Selected, theme::glyph(Symbol::TreeColumnDivider)),
        (true, false) => (RowState::Hovered, " ".to_owned()),
        (false, _) => (RowState::Resting, " ".to_owned()),
    }
}

fn render_rail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    rows: &[MarketplacePluginSummary],
    markets: &[String],
    market: Option<&str>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let inner = Rule::new(Edge::Right).render(frame, area);
    let width = inner.width.saturating_sub(1);
    let focused = model.plugin_pane == PluginPane::Markets
        && model.focus == super::super::model::Focus::Content;
    let mut lines = vec![Line::from(Span::styled(
        "  MARKETPLACES",
        theme::fg(Token::TextMuted),
    ))];
    let mut targets = Vec::new();

    let entries = std::iter::once(None).chain(markets.iter().map(|name| Some(name.as_str())));
    for entry in entries {
        let offered: Vec<&MarketplacePluginSummary> = rows
            .iter()
            .filter(|plugin| entry.is_none_or(|name| plugin.marketplace == name))
            .collect();
        let installed = offered.iter().filter(|plugin| plugin.installed).count();
        let count = match entry {
            None => offered.len().to_string(),
            Some(_) => format!("{installed}/{}", offered.len()),
        };
        let (badge, badge_style) = entry.map_or((String::new(), Style::default()), |name| {
            market_badge(model, name, &offered)
        });
        let (state, bar) = selection(entry == market, focused);
        let name = entry.map_or("All", group_display_name);
        let name_room =
            (width as usize).saturating_sub(2 + COUNT_WIDTH + text::columns(&badge) + 1);
        let name_style = if entry == market {
            theme::fg_bold(Token::TextBright)
        } else {
            theme::fg(Token::TextSecondary)
        };
        let mut spans = vec![
            Span::styled(bar, theme::fg(Token::Accent)),
            Span::raw(" "),
            Span::styled(
                format!("{:<name_room$}", text::elide(name, name_room)),
                name_style,
            ),
            Span::styled(badge, badge_style),
            Span::styled(
                format!("{count:>COUNT_WIDTH$}"),
                theme::fg(Token::TextMuted),
            ),
        ];
        row::fill(&mut spans, width, state);
        targets.push((lines.len(), Hit::PluginMarket(entry.map(str::to_owned))));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    targets.push((
        lines.len(),
        Hit::OfferedAction(uze_keys::Action::AddMarketplace),
    ));
    lines.push(Line::from(Span::styled(
        "  + add",
        theme::fg(Token::TextDim),
    )));

    for (offset, hit) in targets {
        let y = inner.y + offset as u16;
        if y < inner.bottom() {
            hits.push((Rect::new(inner.x, y, width, 1), hit));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The one thing worth knowing about a marketplace before opening it:
/// that something it delivered is behind what it now offers, or that it
/// follows a checkout on this machine. That one is official is said by its
/// drawer, not here.
fn market_badge(
    model: &TuiModel,
    name: &str,
    offered: &[&MarketplacePluginSummary],
) -> (String, Style) {
    if offered.iter().any(|plugin| plugin.freshness.behind()) {
        return (
            format!("{} ", theme::glyph(Symbol::ArrowUp)),
            theme::fg(Token::StateWarning),
        );
    }
    let linked = model
        .remembered
        .marketplaces
        .iter()
        .any(|market| market.name == name && market.linked_to.is_some());
    if linked {
        return ("linked ".to_owned(), theme::fg(Token::TextDim));
    }
    (String::new(), Style::default())
}

/// One line of the plugin list, and what a click on it means.
struct ListLine {
    line: Line<'static>,
    /// Row targets, first match wins: a chevron ahead of its row.
    hits: Vec<(u16, u16, Hit)>,
}

fn render_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    rows: &[MarketplacePluginSummary],
    visible: &[usize],
    every_market: bool,
    hits: &mut Vec<(Rect, Hit)>,
) {
    if rows.is_empty() || visible.is_empty() {
        let filter = model.remembered.plugin_screen.filter.trim();
        let message = if rows.is_empty() {
            "No plugins available.".to_owned()
        } else if !filter.is_empty() {
            format!("No plugins match \"{filter}\".")
        } else {
            "No plugins in this marketplace.".to_owned()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme::fg(Token::TextMuted))),
            Rect {
                y: area.y + 1,
                ..area
            },
        );
        return;
    }

    let name_width = visible
        .iter()
        .map(|&raw| text::columns(&rows[raw].name))
        .max()
        .unwrap_or(0)
        .clamp(6, 28);
    let market_width = every_market.then(|| {
        visible
            .iter()
            .map(|&raw| text::columns(group_display_name(&rows[raw].marketplace)))
            .max()
            .unwrap_or(0)
            .clamp(6, 18)
    });
    let columns = Columns {
        name: name_width,
        market: market_width,
    };

    let focused = model.plugin_pane == PluginPane::Plugins
        && model.focus == super::super::model::Focus::Content;
    let mut lines: Vec<ListLine> = Vec::new();
    let mut selected_line = 0;
    for (position, &raw) in visible.iter().enumerate() {
        let plugin = &rows[raw];
        let id = model.marketplace_plugin_id(plugin);
        let expanded = model.expanded_plugins.contains(&id);
        let is_selected = position == model.remembered.plugin_screen.selected;
        if is_selected {
            selected_line = lines.len();
        }
        lines.push(ListLine {
            line: plugin_line(
                plugin,
                &columns,
                expanded,
                selection(is_selected, focused),
                model.was_just_updated(&id),
                area.width,
            ),
            hits: vec![
                (0, 3, Hit::TogglePluginResources(position)),
                (0, area.width, Hit::MarketplaceRow(position)),
            ],
        });
        if expanded {
            lines.extend(
                resource_tree(model.plugin_resources_of(&id))
                    .into_iter()
                    .map(|line| ListLine {
                        line,
                        hits: vec![(0, area.width, Hit::MarketplaceRow(position))],
                    }),
            );
        }
    }

    frame.render_widget(Paragraph::new(columns.heading()), area);
    let list = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    // Scrolled only as far as keeps the keyboard's row in view: nothing
    // else here moves the list, so there is no offset to remember.
    let offset = (selected_line + 1).saturating_sub(list.height as usize);
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

/// Between two columns of the plugin list: wide enough that a short value
/// does not read as running into the next one.
const GAP: &str = "    ";

/// The chevron and the space after it, ahead of a plugin's name.
const CHEVRON_WIDTH: usize = 2;

/// "Marketplace" in full would take more than most names in it do.
const MARKET_HEADING: &str = "FROM";

/// The list's column widths, shared by its heading and every row so the
/// columns line up table-style whatever each row's own name is.
struct Columns {
    name: usize,
    /// Present only on "All", where a plugin's marketplace is not already
    /// said by the rail.
    market: Option<usize>,
}

impl Columns {
    fn heading(&self) -> Line<'static> {
        // Over the chevron rather than the name: the chevron is where the
        // first column starts, and a heading indented past it reads as
        // belonging to something else.
        let mut heading = format!(
            " {:<width$}{GAP}",
            "PLUGIN",
            width = CHEVRON_WIDTH + self.name
        );
        if let Some(width) = self.market {
            heading.push_str(&format!("{MARKET_HEADING:<width$}{GAP}"));
        }
        heading.push_str(&format!("{:<STATUS_WIDTH$}{GAP}UPDATES", "STATUS"));
        Line::from(Span::styled(heading, theme::fg(Token::TextDim)))
    }
}

fn plugin_line(
    plugin: &MarketplacePluginSummary,
    columns: &Columns,
    expanded: bool,
    (state, bar): (RowState, String),
    just_updated: bool,
    row_width: u16,
) -> Line<'static> {
    let name_style = if expanded || state == RowState::Selected {
        theme::fg_bold(Token::TextBright)
    } else {
        theme::fg(Token::TextSecondary)
    };
    let status_style = if plugin.installed {
        theme::fg(Token::Accent)
    } else {
        theme::fg(Token::TextDim)
    };
    let status = if plugin.installed {
        "Installed"
    } else {
        "Available"
    };
    let (update, update_style) = freshness_label(plugin, just_updated);
    let mut spans = vec![
        Span::styled(bar, theme::fg(Token::Accent)),
        Span::styled(
            format!("{} ", mark::disclosure(expanded)),
            theme::fg(Token::TextDim),
        ),
        Span::styled(
            format!(
                "{:<width$}{GAP}",
                text::elide(&plugin.name, columns.name),
                width = columns.name
            ),
            name_style,
        ),
    ];
    if let Some(width) = columns.market {
        spans.push(Span::styled(
            format!(
                "{:<width$}{GAP}",
                text::elide(group_display_name(&plugin.marketplace), width)
            ),
            theme::fg(Token::TextMuted),
        ));
    }
    spans.push(Span::styled(
        format!("{status:<STATUS_WIDTH$}{GAP}"),
        status_style,
    ));
    spans.push(Span::styled(
        format!("{update:<UPDATE_WIDTH$}"),
        update_style,
    ));
    row::fill(&mut spans, row_width, state);
    Line::from(spans)
}

/// One slot, and "Updated" wins it: the badge is only ever raised by an
/// update that just landed, which is exactly what makes the row current.
///
/// Every other word here is a different fact, and none of them may read
/// like another. "Not checked" in particular must not look like being
/// current — that collapse is what made "Installed" mean both "this is
/// the one that exists" and "nobody has looked".
fn freshness_label(plugin: &MarketplacePluginSummary, just_updated: bool) -> (String, Style) {
    if just_updated {
        return ("Updated".to_owned(), theme::fg(Token::Accent));
    }
    match &plugin.freshness.state {
        FreshnessState::Behind { commits: None } => (
            "Update available".to_owned(),
            theme::fg(Token::StateWarning),
        ),
        FreshnessState::Behind {
            commits: Some(commits),
        } => (format!("{commits} behind"), theme::fg(Token::StateWarning)),
        FreshnessState::Linked { .. } => ("Linked".to_owned(), theme::fg(Token::Accent)),
        FreshnessState::NotChecked if plugin.installed => {
            ("Not checked".to_owned(), theme::fg(Token::TextDim))
        }
        FreshnessState::UpToDate | FreshnessState::Unpinned | FreshnessState::NotChecked => {
            (String::new(), Style::default())
        }
    }
}

/// An unfolded plugin's resources, one branch per kind and a leaf per
/// resource, hung under the plugin's chevron. `None` is a plugin whose
/// resources have not arrived yet: it was unfolded, which asked for them.
fn resource_tree(capabilities: Option<&[PluginCapability]>) -> Vec<Line<'static>> {
    let indent = "  ";
    let faint = theme::fg(Token::TextFaint);
    let leaf = |glyph: Symbol, words: String, style: Style| {
        Line::from(vec![
            Span::styled(format!("{indent}{} ", theme::glyph(glyph)), faint),
            Span::styled(words, style),
        ])
    };
    let Some(capabilities) = capabilities else {
        return vec![leaf(
            Symbol::TreeLast,
            "loading…".to_owned(),
            theme::fg(Token::TextMuted),
        )];
    };
    let groups = resource_groups(capabilities);
    if groups.is_empty() {
        return vec![leaf(
            Symbol::TreeLast,
            theme::glyph(Symbol::MarkUnsupported),
            theme::fg(Token::TextDim),
        )];
    }
    let mut lines = Vec::new();
    let last_group = groups.len() - 1;
    for (group_index, (label, names)) in groups.into_iter().enumerate() {
        let (branch, stem) = if group_index == last_group {
            (Symbol::TreeLast, "  ".to_owned())
        } else {
            (
                Symbol::TreeBranch,
                format!("{} ", theme::glyph(Symbol::TreeVertical)),
            )
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{indent}{} ", theme::glyph(branch)), faint),
            Span::styled(label.to_owned(), theme::fg(Token::TextMuted)),
            Span::styled(format!("  {}", names.len()), theme::fg(Token::TextDim)),
        ]));
        let last_name = names.len() - 1;
        for (name_index, name) in names.into_iter().enumerate() {
            let twig = if name_index == last_name {
                Symbol::TreeLast
            } else {
                Symbol::TreeBranch
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{indent}{stem} {} ", theme::glyph(twig)), faint),
                Span::styled(name.to_owned(), theme::fg(Token::TextPrimary)),
            ]));
        }
    }
    lines
}

fn render_market_drawer(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    model: &TuiModel,
    rows: &[MarketplacePluginSummary],
    market: Option<&str>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let inner = super::drawer(
        frame,
        content,
        ResizablePanel::MarketplaceDrawer,
        model,
        hits,
    );
    let offers = model.selected_offers();
    let (body, status_area) = super::drawer_body_and_footer(inner, &offers);
    let room = body.width.saturating_sub(crate::ui::widget::TRAILING_PAD) as usize;
    let summary = market.and_then(|name| {
        model
            .remembered
            .marketplaces
            .iter()
            .find(|summary| summary.name == name)
    });
    let offered: Vec<&MarketplacePluginSummary> = rows
        .iter()
        .filter(|plugin| market.is_none_or(|name| plugin.marketplace == name))
        .collect();
    let installed = offered.iter().filter(|plugin| plugin.installed).count();
    let behind = offered
        .iter()
        .filter(|plugin| plugin.freshness.behind())
        .count();

    let kind = match market {
        None => "MARKETPLACES",
        Some(_) if summary.is_none() => "INSTALLED DIRECTLY",
        Some(_) => "MARKETPLACE",
    };
    let mut heading = vec![Span::styled(kind, theme::fg(Token::TextMuted))];
    // Said once, where the marketplace is described, rather than on every
    // row of the rail, where it made one entry louder than the rest.
    if market == Some("uze-official") {
        row::push_trailing(
            &mut heading,
            body.width,
            format!("{} Official", theme::glyph(Symbol::MarkOfficial)),
            theme::color(Token::StateInfo),
        );
    }
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(heading),
        Line::from(Span::styled(
            text::elide(market.map_or("All", group_display_name), room),
            theme::fg_bold(Token::TextBright),
        )),
        Line::from(""),
    ];
    let field = |lines: &mut Vec<Line<'static>>, label: &'static str, value: &str| {
        lines.push(Line::from(Span::styled(
            label,
            theme::fg_bold(Token::TextMuted),
        )));
        lines.extend(
            text::fold(value, room)
                .into_iter()
                .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextSecondary)))),
        );
        lines.push(Line::from(""));
    };

    match (market, summary) {
        (None, _) => {
            let names = model
                .remembered
                .marketplaces
                .iter()
                .map(|summary| group_display_name(&summary.name))
                .collect::<Vec<_>>()
                .join(", ");
            field(&mut lines, "REGISTERED", &names);
        }
        (Some(_), Some(summary)) => {
            lines.push(Line::from(Span::styled(
                "ORIGIN",
                theme::fg_bold(Token::TextMuted),
            )));
            match summary.homepage.as_deref() {
                Some(url) => {
                    let y = body.y + lines.len() as u16;
                    lines.push(link_line(model, url, body.width));
                    if y < body.bottom() {
                        hits.push((
                            Rect::new(body.x, y, body.width, 1),
                            Hit::OpenLink(summary.name.clone()),
                        ));
                    }
                }
                None => lines.extend(
                    text::fold(&summary.source, room)
                        .into_iter()
                        .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextSecondary)))),
                ),
            }
            lines.push(Line::from(""));
            if let Some(checkout) = &summary.linked_to {
                field(&mut lines, "LINKED TO", &checkout.display().to_string());
            }
        }
        (Some(_), None) => field(
            &mut lines,
            "SOURCE",
            "Installed from a path or a Git URL that no registered marketplace offers.",
        ),
    }
    let names = offered
        .iter()
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    field(
        &mut lines,
        "PLUGINS",
        if names.is_empty() { "—" } else { &names },
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);

    let tally = format!("{installed} of {} plugins installed", offered.len());
    let (color, headline, subtitle) = if behind > 0 {
        (
            Token::StateWarning,
            format!("{behind} update{} available", plural(behind)),
            tally,
        )
    } else {
        (
            Token::Accent,
            if market.is_none() {
                "Up to date"
            } else {
                "Configured"
            }
            .to_owned(),
            match market {
                Some("uze-official") => format!("Ships with uze · {tally}"),
                Some(_) => tally,
                None => format!("{tally} across every marketplace"),
            },
        )
    };
    render_drawer_footer(
        frame,
        status_area,
        DrawerStatus {
            color: theme::color(color),
            headline: &headline,
            subtitle: &subtitle,
        },
        &offers,
        model.hovered_offer,
        None,
        hits,
    );
}

/// A marketplace's address as a link: underlined, which is what a link
/// looks like everywhere else a person reads one, muted until the pointer
/// is on it so it does not compete with the row the reader selected, and
/// written out in full so it can be checked before it is trusted — and
/// copied, when the terminal has no browser to hand it to.
fn link_line(model: &TuiModel, url: &str, width: u16) -> Line<'static> {
    let tone = if model.source_link_hovered {
        Token::Accent
    } else {
        Token::TextMuted
    };
    let link = Style::default()
        .fg(theme::color(tone))
        .add_modifier(Modifier::UNDERLINED);
    Line::from(vec![
        Span::styled(text::elide(url, width.saturating_sub(2) as usize), link),
        Span::raw(" "),
        Span::styled(theme::glyph(Symbol::ArrowExternal), link),
    ])
}

fn render_plugin_drawer(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    model: &TuiModel,
    plugin: &MarketplacePluginSummary,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let inner = super::drawer(
        frame,
        content,
        ResizablePanel::MarketplaceDrawer,
        model,
        hits,
    );
    let offers = plugin.offers();
    let (body, status_area) = super::drawer_body_and_footer(inner, &offers);

    // One cell short of the edge. Folding at the full width put every row
    // flush against the border, which is what made a drawer with rows to
    // spare read as crowded.
    let room = body.width.saturating_sub(crate::ui::widget::TRAILING_PAD) as usize;
    // No `PLUGIN` label: this drawer is about a plugin, so the name is the
    // heading rather than a value under one. Every other label here says
    // something its value would be ambiguous without.
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.extend(text::fold(&plugin.name, room).into_iter().map(|row| {
        Line::from(Span::styled(
            row,
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        ))
    }));
    lines.push(Line::from(""));
    lines.extend(
        text::fold(plugin.description.as_deref().unwrap_or_default(), room)
            .into_iter()
            .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextSecondary)))),
    );
    if !plugin.keywords.is_empty() {
        if plugin.description.is_some() {
            lines.push(Line::from(""));
        }
        lines.extend(
            text::fold(&plugin.keywords.join(", "), room)
                .into_iter()
                .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextDim)))),
        );
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "SOURCE",
        theme::fg_bold(Token::TextMuted),
    )));
    let source_row_y = body.y + lines.len() as u16;
    let name = group_display_name(&plugin.marketplace);
    let homepage = model
        .remembered
        .marketplaces
        .iter()
        .find(|entry| entry.name == plugin.marketplace)
        .and_then(|entry| entry.homepage.clone());
    // Just the name: one glyph on the card, and it belongs to the address
    // below, which is the row that leaves the application. Marking this
    // row too made the pair read as two links to the same place.
    lines.push(Line::from(Span::styled(
        name.to_owned(),
        theme::fg(Token::TextPrimary),
    )));
    if source_row_y < body.y + body.height {
        hits.push((
            Rect::new(body.x, source_row_y, body.width, 1),
            Hit::JumpToMarketplace(plugin.marketplace.clone()),
        ));
    }
    // The address itself, on a row of its own and clickable along its
    // whole length. It was a one-column "↗" beside the name to begin
    // with, which is a target you miss by moving the mouse one cell —
    // and missing it landed on the jump underneath, which re-selects the
    // group already selected and so reads as nothing happening at all.
    // Writing the address out also gives the reader something to check
    // before trusting it, and something to copy when the terminal this
    // drawer draws into has no browser to hand it to.
    if let Some(url) = homepage.as_deref() {
        let url_row_y = body.y + lines.len() as u16;
        lines.push(link_line(model, url, body.width));
        if url_row_y < body.y + body.height {
            hits.push((
                Rect::new(body.x, url_row_y, body.width, 1),
                Hit::OpenLink(plugin.marketplace.clone()),
            ));
        }
    }
    lines.push(Line::from(""));

    // Installed rows read their resources/deliveries from the installed
    // package inspection (`InspectPlugin`), available rows from the
    // catalog detail (`InspectMarketplacePlugin`) — each fetch lands in a
    // different cache, so the drawer consults whichever matches this row.
    let installed_inspection = plugin.installed.then(|| {
        let id = model.marketplace_plugin_id(plugin);
        model
            .plugin_detail
            .as_ref()
            .filter(|detail| detail.plugin.id == id)
    });
    let catalog_detail = (!plugin.installed).then(|| {
        model.marketplace_detail.as_ref().filter(|detail| {
            detail.summary.name == plugin.name && detail.summary.marketplace == plugin.marketplace
        })
    });

    // What you actually have, before what it offers. The row's status
    // column says whether something newer exists; this says how old the
    // thing in front of you is, which is the question the state alone
    // cannot answer.
    // Asked of whichever detail matches this row: what you have when it is
    // installed, what you would be getting when it is not. "Is this
    // abandoned" is the same question one step earlier.
    let revision = installed_inspection
        .flatten()
        .and_then(|detail| detail.revision.as_ref())
        .or_else(|| {
            catalog_detail
                .flatten()
                .and_then(|detail| detail.revision.as_ref())
        });
    if let Some(revision) = revision {
        lines.push(Line::from(Span::styled(
            "REVISION",
            theme::fg_bold(Token::TextMuted),
        )));
        match revision {
            Revision::Commit {
                short,
                age,
                subject,
            } => {
                lines.push(Line::from(vec![
                    Span::styled(short.clone(), theme::fg(Token::TextSecondary)),
                    Span::raw("  "),
                    Span::styled(age.clone(), theme::fg(Token::TextMuted)),
                ]));
                // Folded, not elided: a commit subject is a sentence, and
                // a truncated one loses the half that says what the
                // change was. The drawer has rows to spare and the
                // resource list below already folds the same way.
                lines.extend(
                    text::fold(subject, room)
                        .into_iter()
                        .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextDim)))),
                );
            }
            // Shipped inside the binary: there is no repository to ask,
            // and the release it came with is the only date that is true.
            Revision::Bundled { version } => {
                lines.push(Line::from(Span::styled(
                    format!("ships with uze {version}"),
                    theme::fg(Token::TextSecondary),
                )));
            }
            // No revision to name: what is installed is whatever its
            // author last saved, so the checkout is the only honest
            // answer.
            Revision::Checkout { path } => {
                lines.push(Line::from(Span::styled(
                    "follows your working tree",
                    theme::fg(Token::TextSecondary),
                )));
                lines.extend(
                    text::fold(&path.display().to_string(), room)
                        .into_iter()
                        .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextMuted)))),
                );
            }
        }
        lines.push(Line::from(""));
    }

    // What it offers is the unfolded row's tree in the list beside this,
    // which shows it against its siblings; saying it here as well only
    // restated it.
    // Where the plugin reaches and how its receipts stand are left to the
    // status line below: a uze plugin is meant for every harness, so a
    // per-harness list restated the product's premise, and the receipt
    // counts are what that line's one-word health is derived from.
    // Untrimmed: every line is folded already.
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);

    // The buttons below say what can be done, so the note no longer names
    // the key that does it.
    let status = if plugin.installed {
        let qualified_id = model.marketplace_plugin_id(plugin);
        if model.was_just_updated(&qualified_id) {
            DrawerStatus {
                color: theme::color(Token::Accent),
                headline: "Updated",
                subtitle: "Brought up to date automatically when uze started",
            }
        // Only an actionable freshness state takes this slot. It carries
        // attachment health — whether the plugin is actually delivered —
        // and "nobody has compared this against its marketplace" says
        // nothing about that. The row's own status column is where every
        // freshness state is reported.
        } else if plugin.freshness.behind() {
            DrawerStatus {
                color: theme::color(Token::StateWarning),
                headline: "Update available",
                subtitle: "Needs your confirmation to apply",
            }
        } else if matches!(plugin.freshness.state, FreshnessState::Linked { .. }) {
            DrawerStatus {
                color: theme::color(Token::Accent),
                headline: "Linked",
                subtitle: "Follows a checkout on this machine",
            }
        } else {
            DrawerStatus {
                color: theme::color(Token::Accent),
                headline: "Installed",
                subtitle: match plugin_health(model.remembered.doctor.as_ref(), &qualified_id) {
                    "ready" => "Ready to use in your projects",
                    "missing" => "Installation is missing artifacts",
                    "needs attention" => "Managed state needs attention",
                    _ => "Health unknown",
                },
            }
        }
    } else {
        DrawerStatus {
            color: theme::color(Token::TextMuted),
            headline: "Not installed",
            subtitle: "Available from this marketplace",
        }
    };
    render_drawer_footer(
        frame,
        status_area,
        status,
        &offers,
        model.hovered_offer,
        None,
        hits,
    );
}

/// The order a reader meets a plugin's resources in: what they invoke
/// first, what runs on its own after.
const RESOURCE_ORDER: [CapabilityKind; 5] = [
    CapabilityKind::AgentSkill,
    CapabilityKind::Agent,
    CapabilityKind::Hook,
    CapabilityKind::Mcp,
    CapabilityKind::Instruction,
];

/// A plugin's resources by kind, in [`RESOURCE_ORDER`], leaving out the
/// kinds it declares none of.
fn resource_groups(capabilities: &[PluginCapability]) -> Vec<(&'static str, Vec<&str>)> {
    RESOURCE_ORDER
        .iter()
        .map(|kind| {
            let names = capabilities
                .iter()
                .filter(|capability| capability.kind == *kind)
                .map(|capability| capability.name.as_str())
                .collect::<Vec<_>>();
            (capability_label(*kind), names)
        })
        .filter(|(_, names)| !names.is_empty())
        .collect()
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
