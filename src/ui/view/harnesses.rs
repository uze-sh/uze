//! TUI view — Harnesses route.
//!
//! A responsive integration catalog on the left; a detail drawer slides in
//! from the right once a harness is selected,
//! with a draggable left edge to balance the detail against the cards.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use uze_application::{
    CapabilityKind, HarnessCapabilities,
    application::{ContextMechanism, HarnessContextSupport, HarnessHealth},
};

use super::catalog::{Card, render_card};
use super::{DrawerStatus, render_drawer_footer};
use crate::ui::hit::Hit;
use crate::ui::model::{ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::RowState;
use crate::ui::{content_area, render_screen_header};

/// Two states, because there are two answers a person can act on: UZE has
/// set this harness up, or it has not. A binary that is not on the machine
/// and one the person installed themselves and never handed to UZE are the
/// same answer to the only question the list asks — can it receive
/// plugins? Which of the two it is, and what to do about it, is the
/// drawer's to say (see [`status_note`]).
///
/// `HarnessHealth` tracks far more: whether an explicit `uze setup` run
/// verified the binary rather than an `uze add` preparing it implicitly,
/// and whether UZE's runtime shim is first on `PATH` for the harnesses
/// that project project-context through it. Every one of those is drawer
/// detail. The last of them used to be a state of its own here, and it
/// put `PATH shadowed` across the card of a harness whose only real news
/// was that nobody had set it up — a sentence about this machine's `PATH`
/// where the person was looking for what to do next. A new state here
/// earns its place only when the list needs them to act differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HarnessStatus {
    /// UZE has configured it — ready to receive plugins.
    Configured,
    /// Not on this machine, or on it and never handed to UZE.
    NotConfigured,
}

impl HarnessStatus {
    fn from(harness: &HarnessHealth) -> Self {
        if harness.configured() {
            Self::Configured
        } else {
            Self::NotConfigured
        }
    }

    /// The word the card's foot and the drawer carry. "Enabled" for the
    /// one ready to receive plugins, the word the Extensions cards use for
    /// the same answer, so the two catalogs read alike.
    fn label(self) -> &'static str {
        match self {
            Self::Configured => "Enabled",
            Self::NotConfigured => "Not configured",
        }
    }

    /// Dim rather than a warning for one not configured: that is most of
    /// the catalog, on most machines, with nothing wrong.
    fn color(self) -> Color {
        match self {
            Self::Configured => theme::color(Token::StateSuccess),
            Self::NotConfigured => theme::color(Token::TextDim),
        }
    }
}

/// The drawer footer's note under the label: the state in the words the
/// person acts on. The distinction the card deliberately does not draw
/// lives here, because this is where doing something about it is — a
/// harness that is not on the machine is installed first, one that is
/// already here is set up.
fn status_note(harness: &HarnessHealth) -> &'static str {
    if !harness.detection.present {
        // Setting one up is the same gesture wherever it starts from:
        // UZE provisions through the vendor's own official route, which
        // installs what is missing and updates what is not.
        "Not on this machine — setting it up installs it"
    } else if HarnessStatus::from(harness) == HarnessStatus::Configured {
        "Ready to receive plugins"
    } else {
        "Installed — set it up to receive plugins"
    }
}

/// Width of the drawer's label column, shared by the key/value rows (
/// `Version`, `Provisioning`) and the COMPATIBILITY rows. The longest
/// labels in use are `.agents/skills` and `.agents/agents` (14 chars), so
/// 16 keeps a two-space gap — a pad equal to the longest label glues the
/// value flush against it, which is what it did once those two joined.
const LABEL_COL: usize = 16;

pub(crate) fn render_harnesses(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let area = content_area(area);
    // What is on the machine, not how many harnesses UZE knows about: the
    // catalog draws a card for every one it supports, so counting cards
    // said "4 installed" on a machine carrying three.
    let count = model.remembered.doctor.as_ref().map_or(0, |doctor| {
        doctor
            .harnesses
            .iter()
            .filter(|harness| harness.detection.present)
            .count()
    });
    // The drawer overlays from the right rather than sharing a permanent
    // split, but the header/list still need to lay out *around* it when
    // it's open — otherwise their own right-aligned content runs straight
    // under the drawer and gets clipped mid-word by its Clear. Its initial
    // width is an even split; dragging the divider lets either panel take
    // priority for the task at hand.
    // Shown whenever there is a harness to describe: the drawer is the
    // screen's detail column, not something opened and closed.
    let drawer_shown = model.selected_harness().is_some();
    let drawer_width = if drawer_shown {
        super::drawer_width(ResizablePanel::Drawer, model, area)
    } else {
        0
    };
    let list_area = Rect::new(
        area.x,
        area.y,
        area.width
            .saturating_sub(drawer_width)
            .saturating_sub(if drawer_shown { 1 } else { 0 }),
        area.height,
    );
    let content = render_screen_header(
        frame,
        list_area,
        Route::Harnesses,
        Some(Span::styled(
            format!("{count} installed"),
            theme::fg(Token::TextMuted),
        )),
    );

    let mut y = content.y;
    let bottom = content.y + content.height;
    // The cards' window, kept for the scrollbar drawn on the drawer's rule.
    let mut window = None;

    if y + 2 <= bottom {
        let filter_area = Rect::new(content.x, y, content.width, 1);
        hits.push((filter_area, Hit::FocusFilter));
        super::filter_box(
            frame,
            filter_area,
            &model.remembered.harness_screen.filter,
            "filter integrations…",
            model.filtering,
        );
        y += super::FILTER_ROWS;
    }

    match &model.remembered.doctor {
        None => {
            if y < bottom {
                frame.render_widget(
                    Paragraph::new(Span::styled("Loading…", theme::fg(Token::TextMuted))),
                    Rect::new(content.x, y, content.width, bottom.saturating_sub(y)),
                );
            }
        }
        Some(doctor) => {
            let visible = model.harness_visible_indices();
            if visible.is_empty() {
                if y < bottom {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!(
                                "No integrations match \"{}\".",
                                model.remembered.harness_screen.filter.trim()
                            ),
                            theme::fg(Token::TextMuted),
                        )),
                        Rect::new(content.x, y, content.width, 1),
                    );
                }
            } else {
                let area = Rect::new(content.x, y, content.width, bottom.saturating_sub(y));
                let screen = &model.remembered.harness_screen;
                let grid =
                    super::catalog::cards(area, visible.len(), screen.selected, &screen.scroll);
                for &(position, rect) in &grid.cards {
                    let state = RowState::of(
                        position == screen.selected,
                        model.hovered == Some(Hit::HarnessRow(position)),
                    );
                    render_harness_card(frame, rect, &doctor.harnesses[visible[position]], state);
                    hits.push((rect, Hit::HarnessRow(position)));
                }
                y = grid.bottom;
                window = Some((area, grid));
            }
            if let Some(status) = &model.remembered.context_status
                && !status.warnings.is_empty()
                && y < bottom
            {
                y += 1;
                for warning in &status.warnings {
                    if y >= bottom {
                        break;
                    }
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!("! {warning}"),
                            theme::fg(Token::StateWarning),
                        )),
                        Rect::new(content.x, y, content.width, 1),
                    );
                    y += 1;
                }
            }
        }
    }

    if let Some(harness) = model.selected_harness() {
        render_harness_drawer(frame, area, model, harness, hits);
        if let Some((rows, grid)) = window {
            super::scrollbar_on_drawer_rule(
                frame,
                area,
                model,
                rows,
                (grid.visible, grid.rows, grid.first),
                hits,
                None,
            );
        }
    }
}

fn render_harness_card(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    harness: &HarnessHealth,
    state: RowState,
) {
    // The state sits at the foot beside the id, where the Extensions
    // cards say theirs, rather than as a badge beside the name.
    let status = HarnessStatus::from(harness);
    render_card(
        frame,
        rect,
        Card {
            name: &harness.display_name,
            badge: None,
            description: &harness.description,
            caption: Span::styled(harness.integration.as_str(), theme::fg(Token::TextMuted)),
            state: Some(Span::styled(
                status.label(),
                Style::default().fg(status.color()),
            )),
        },
        state,
    );
}

fn render_harness_drawer(
    frame: &mut ratatui::Frame<'_>,
    content: Rect,
    model: &TuiModel,
    harness: &HarnessHealth,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let status = HarnessStatus::from(harness);
    let offers = harness.offers();
    let (inner, footer) = super::drawer_body_and_footer(
        super::drawer(frame, content, ResizablePanel::Drawer, model, hits),
        &offers,
    );
    render_drawer_footer(
        frame,
        footer,
        DrawerStatus {
            color: status.color(),
            headline: status.label(),
            subtitle: status_note(harness),
        },
        &offers,
        model.hovered_offer,
        None,
        hits,
    );

    let mut lines = vec![
        super::section_label("harness"),
        Line::from(Span::styled(
            harness.display_name.clone(),
            Style::default()
                .fg(theme::color(Token::TextBright))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            label_span("Version", theme::fg(Token::TextMuted)),
            Span::styled(
                harness
                    .detection
                    .version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
                theme::fg(Token::TextTertiary),
            ),
        ]),
    ];
    if let Some(provisioning) = &harness.provisioning {
        lines.push(Line::from(vec![
            label_span("Provisioning", theme::fg(Token::TextMuted)),
            Span::styled(
                format!("{:?} ({:?})", provisioning.status, provisioning.action),
                theme::fg(Token::TextTertiary),
            ),
        ]));
    }
    lines.push(Line::from(""));
    let (project, supports) = compatibility_rows(harness);
    for (index, (group, rows)) in [("project", project), ("supports", supports)]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(super::section_label(group));
        for (label, status, style) in rows {
            lines.push(Line::from(vec![
                label_span(label, theme::fg(Token::TextSecondary)),
                Span::styled(status, style),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

/// A drawer key/value row's label, padded to the shared `LABEL_COL` column
/// so every row's value starts at the same x position.
fn label_span(label: &str, style: Style) -> Span<'static> {
    Span::styled(format!("{label:<width$}", width = LABEL_COL), style)
}

/// One row per capability UZE knows about, in two groups: the portable
/// *project* resources (`AGENTS.md` and `.agents/`), then the
/// `capabilities()`-derived rows of what the harness supports. The project
/// ones are listed apart from the others, and read from a different
/// model: they are not `CapabilityKind` resources routed through
/// `HarnessCapabilities` — mixing them into the same lookup would silently
/// mislabel them "not supported" on every harness, since none of them ever
/// populate `CapabilityKind::Instruction`.
///
/// Every row here is machine-scoped: what this harness supports, on this
/// machine, regardless of where `uze` was launched from. Whether one
/// particular project is actually being delivered is the workspace's
/// per-agent support popup's question, answered against that pane's own
/// cwd — asking it here, against the TUI's launch directory, is how the
/// screen used to report "none in project" about `$HOME`.
fn compatibility_rows(harness: &HarnessHealth) -> (Vec<Row>, Vec<Row>) {
    let routed = [
        ("Skills", CapabilityKind::AgentSkill),
        ("MCP", CapabilityKind::Mcp),
        ("Agents", CapabilityKind::Agent),
        ("Hooks", CapabilityKind::Hook),
    ]
    .into_iter()
    .map(|(label, kind)| {
        let (status, style) = capability_status(&harness.capabilities, kind);
        (label, status, style)
    });
    (context_rows(&harness.context_support), routed.collect())
}

/// A compatibility row: its label, and its state in words and style.
type Row = (&'static str, String, Style);

/// The drawer's project-context rows. Each names the mechanism through
/// which this harness receives that resource, so a harness receiving
/// `AGENTS.md` through the runtime shim reads as supported instead of the
/// old "— Not needed" (which meant only that no installed package had
/// contributed a managed region — a fact about plugins, never about
/// whether the harness could see a project's instructions).
fn context_rows(support: &HarnessContextSupport) -> Vec<(&'static str, String, Style)> {
    let (instructions, instructions_style) = context_row(support.instructions);
    let (skills, skills_style) = context_row(support.project_skills);
    let (agents, agents_style) = context_row(support.project_agents);
    let (rules, rules_style) = context_row(support.project_rules);
    vec![
        ("AGENTS.md", instructions, instructions_style),
        (".agents/skills", skills, skills_style),
        (".agents/agents", agents, agents_style),
        (".agents/rules", rules, rules_style),
    ]
}

fn context_row(mechanism: ContextMechanism) -> (String, Style) {
    match mechanism {
        ContextMechanism::Native => (
            format!("{} Native", theme::glyph(Symbol::MarkNative)),
            theme::fg(Token::StateSuccess),
        ),
        ContextMechanism::RuntimeShim => (
            format!("{} Inside the workspace", theme::glyph(Symbol::MarkNative)),
            theme::fg(Token::StateSuccess),
        ),
        ContextMechanism::RulesEngine => (
            format!("{} Rules engine", theme::glyph(Symbol::MarkNative)),
            theme::fg(Token::StateSuccess),
        ),
        ContextMechanism::ShimMissing | ContextMechanism::RulesEngineMissing => (
            format!("{} Run uze setup", theme::glyph(Symbol::MarkAttention)),
            theme::fg(Token::StateWarning),
        ),
        ContextMechanism::Unsupported => (
            format!("{} Not supported", theme::glyph(Symbol::MarkUnsupported)),
            theme::fg(Token::StateDanger),
        ),
    }
}

fn capability_status(capabilities: &HarnessCapabilities, kind: CapabilityKind) -> (String, Style) {
    if capabilities.native.contains(&kind) {
        (
            format!("{} Native", theme::glyph(Symbol::MarkNative)),
            theme::fg(Token::StateSuccess),
        )
    } else if capabilities.adaptable.contains(&kind) {
        (
            format!("{} Adapted", theme::glyph(Symbol::MarkAdapted)),
            theme::fg(Token::StateWarning),
        )
    } else if capabilities.degraded.contains(&kind) {
        (
            format!("{} Degraded", theme::glyph(Symbol::MarkAdapted)),
            theme::fg(Token::StateWarning),
        )
    } else {
        (
            format!("{} Not supported", theme::glyph(Symbol::MarkUnsupported)),
            theme::fg(Token::StateDanger),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_harness(runtime_shim_active: bool) -> HarnessHealth {
        HarnessHealth {
            below_minimum: None,
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            description: "test harness".to_owned(),
            detection: uze_core::integration::HarnessDetection {
                present: true,
                version: Some("1.0.0".to_owned()),
            },
            setup: "configured".to_owned(),
            strategy: None,
            provisioning: None,
            publication: uze_core::integration::PublicationStatus::NotApplicable,
            capabilities: HarnessCapabilities::default(),
            runtime_shim_active,
            agent_context: None,
            context_support: HarnessContextSupport {
                instructions: ContextMechanism::RuntimeShim,
                project_skills: ContextMechanism::RuntimeShim,
                project_agents: ContextMechanism::RuntimeShim,
                project_rules: ContextMechanism::RulesEngine,
            },
        }
    }

    /// A shim that is not first on `PATH` is a fact about this machine's
    /// environment, and it used to be the whole of what a card said —
    /// `PATH shadowed`, in warning colour, over a harness that was set up
    /// and working. The card answers what the person can act on; where the
    /// shim actually matters is per resource, and the drawer's own
    /// compatibility rows still say it (see
    /// `a_shadowed_shim_reads_as_an_environment_warning`).
    #[test]
    fn a_shadowed_shim_is_drawer_detail_rather_than_the_cards_state() {
        let harness = configured_harness(false);
        let status = HarnessStatus::from(&harness);

        assert_eq!(status, HarnessStatus::Configured);
        assert_eq!(status.label(), "Enabled");
        assert_eq!(status_note(&harness), "Ready to receive plugins");
    }

    /// One state, two shapes. A harness nobody installed and one the
    /// person installed themselves and never handed to UZE are the same
    /// answer to what the list asks, and neither wears a mark: a card
    /// carrying nothing is already "not configured", and marking it too
    /// would put a warning on most of the catalog with nothing wrong.
    #[test]
    fn nothing_uze_configured_carries_a_badge_and_both_shapes_are_said_in_the_drawer() {
        let mut absent = configured_harness(true);
        absent.detection.present = false;
        let mut theirs = configured_harness(true);
        theirs.setup = "not configured".to_owned();

        for harness in [&absent, &theirs] {
            let status = HarnessStatus::from(harness);
            assert_eq!(status, HarnessStatus::NotConfigured);
            assert_eq!(status.label(), "Not configured");
            assert_eq!(status.color(), theme::color(Token::TextDim));
        }

        assert_eq!(
            status_note(&absent),
            "Not on this machine — setting it up installs it"
        );
        assert_eq!(
            status_note(&theirs),
            "Installed — set it up to receive plugins",
            "the drawer is where doing something about it is"
        );
    }

    fn support(
        instructions: ContextMechanism,
        agents_directory: ContextMechanism,
    ) -> HarnessContextSupport {
        HarnessContextSupport {
            instructions,
            project_skills: agents_directory,
            project_agents: agents_directory,
            project_rules: ContextMechanism::RulesEngine,
        }
    }

    // The reported regression: Claude Code's AGENTS.md row read
    // "— Not needed" in every project, because the old model asked whether
    // an installed *package* had contributed a managed region rather than
    // whether the harness could receive project instructions at all.
    #[test]
    fn a_runtime_projection_reads_as_supported_not_as_not_needed() {
        let rows = context_rows(&support(
            ContextMechanism::RuntimeShim,
            ContextMechanism::RuntimeShim,
        ));
        assert_eq!(rows[0].0, "AGENTS.md");
        assert_eq!(
            rows[0].1,
            format!("{} Inside the workspace", theme::glyph(Symbol::MarkNative))
        );
        assert_eq!(rows[0].2.fg, Some(theme::color(Token::StateSuccess)));
        assert_eq!(rows[1].0, ".agents/skills");
        assert_eq!(
            rows[1].1,
            format!("{} Inside the workspace", theme::glyph(Symbol::MarkNative))
        );
    }

    #[test]
    fn each_resource_is_answered_independently() {
        // A harness may read `AGENTS.md` itself while `.agents/` reaches
        // it only through the workspace.
        let rows = context_rows(&support(
            ContextMechanism::Native,
            ContextMechanism::ShimMissing,
        ));
        assert_eq!(
            rows[0].1,
            format!("{} Native", theme::glyph(Symbol::MarkNative))
        );
        assert_eq!(
            rows[1].1,
            format!("{} Run uze setup", theme::glyph(Symbol::MarkAttention))
        );
    }

    #[test]
    fn each_kind_of_the_agents_directory_has_a_row_of_its_own() {
        // A harness reading `.agents/skills` itself may read no
        // `.agents/agents` and be handed none: one `.agents` row said
        // Native over agents that never arrived.
        let rows = context_rows(&HarnessContextSupport {
            instructions: ContextMechanism::Native,
            project_skills: ContextMechanism::Native,
            project_agents: ContextMechanism::Unsupported,
            project_rules: ContextMechanism::Unsupported,
        });
        assert_eq!(rows[1].0, ".agents/skills");
        assert_eq!(
            rows[1].1,
            format!("{} Native", theme::glyph(Symbol::MarkNative))
        );
        assert_eq!(rows[2].0, ".agents/agents");
        assert_eq!(rows[2].2.fg, Some(theme::color(Token::StateDanger)));
    }

    #[test]
    fn a_shadowed_shim_reads_as_an_environment_warning() {
        let rows = context_rows(&support(
            ContextMechanism::ShimMissing,
            ContextMechanism::ShimMissing,
        ));
        assert_eq!(
            rows[0].1,
            format!("{} Run uze setup", theme::glyph(Symbol::MarkAttention))
        );
        assert_eq!(rows[0].2.fg, Some(theme::color(Token::StateWarning)));
    }

    #[test]
    fn a_harness_with_no_mechanism_reads_as_unsupported() {
        let rows = context_rows(&support(
            ContextMechanism::Unsupported,
            ContextMechanism::Unsupported,
        ));
        assert_eq!(rows[0].1, "— Not supported");
        assert_eq!(rows[0].2.fg, Some(theme::color(Token::StateDanger)));
    }

    // The drawer is machine-scoped: the same harness reads identically no
    // matter which directory `uze` was launched from, because nothing in
    // the rows is resolved against a project.
    #[test]
    fn compatibility_rows_lead_with_the_portable_resources() {
        let harness = configured_harness(true);
        let (project, supports) = compatibility_rows(&harness);
        let labels = |rows: Vec<Row>| {
            rows.into_iter()
                .map(|(label, _, _)| label)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(project),
            [
                "AGENTS.md",
                ".agents/skills",
                ".agents/agents",
                ".agents/rules"
            ]
        );
        assert_eq!(labels(supports), ["Skills", "MCP", "Agents", "Hooks"]);
    }

    #[test]
    fn the_rules_row_says_how_the_project_rules_reach_the_harness() {
        let mut support = configured_harness(true).context_support;
        for (mechanism, words) in [
            (ContextMechanism::Native, "Native"),
            (ContextMechanism::RulesEngine, "Rules engine"),
            (ContextMechanism::RulesEngineMissing, "Run uze setup"),
            (ContextMechanism::Unsupported, "Not supported"),
        ] {
            support.project_rules = mechanism;
            let rows = context_rows(&support);
            let (label, value, _) = &rows[3];
            assert_eq!(*label, ".agents/rules");
            assert!(value.ends_with(words), "{mechanism:?}: {value}");
        }
    }

    /// Every label the drawer prints leaves two columns before its value,
    /// whatever row it heads.
    #[test]
    fn every_label_leaves_room_before_its_value() {
        let (project, supports) = compatibility_rows(&configured_harness(true));
        let longest = project
            .into_iter()
            .chain(supports)
            .map(|(label, _, _)| label.chars().count())
            .chain(["Version", "Provisioning"].map(str::len))
            .max()
            .unwrap();
        assert!(
            longest + 2 <= LABEL_COL,
            "a {longest}-column label leaves no gap in a {LABEL_COL}-column field"
        );
    }
}
