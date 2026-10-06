//! `uze config` and `uze context`: themes, glyphs, extensions, notifications.

use crate::*;

/// `uze agent context …` — the project-scoped half of the grammar, on the
/// agent's surface: what this project declares, what reconciling it would
/// write, and writing it. See ADR-019 for why none of these ever touch
/// machine state.
///
/// It sits under `agent` because its reader is one: a person asks `uze
/// status` whether the project is ready and `uze install` to make it so,
/// and neither ever needs the region-by-region detail these three answer
/// with.
pub(crate) fn run_context(app: &UzeApplication, action: ContextAction) -> Result<()> {
    match action {
        ContextAction::Inspect { path, format } => {
            let status = app.context().inspect(&context_path(path))?;
            emit(format, &status, render_context_status);
        }
        ContextAction::Plan { path, format } => {
            let plan = app.context().plan(&context_path(path))?;
            emit(format, &plan, |plan| render_context_plan(plan, app));
        }
        ContextAction::Reconcile { path, format } => {
            let report = app.context().reconcile(&context_path(path))?;
            emit(format, &report, |report| {
                render_context_reconciliation(report, app)
            });
        }
    }
    Ok(())
}

/// `uze market …` — the marketplaces a machine knows about.
/// `uze config …` — this machine's authored configuration: the palette,
/// the glyph set the installed font can draw, and which finished turns
/// ring, and which extensions the workspace offers. All machine-scoped; the file they write is the operator's own
/// `config.toml`, never a project's.
pub(crate) fn run_config(
    app: &UzeApplication,
    home: &UzeHome,
    action: ConfigAction,
    verbose: bool,
) -> Result<()> {
    match action {
        ConfigAction::Theme { action } => match action.unwrap_or(ConfigThemeAction::List {
            format: OutputFormat::Text,
        }) {
            ConfigThemeAction::List { format } => {
                let themes = app.themes().list(uze_theme::builtin_names())?;
                let adaptive = adaptive_caption(app)?;
                emit(format, &themes, |themes| {
                    render_theme_list(themes, &adaptive)
                });
            }
            ConfigThemeAction::Set { id, light, dark } => {
                choose_adaptive_halves(app, home, &id, light, dark)?;
                // Load it before recording the choice: a theme that will not
                // resolve should be refused here, where the operator is
                // looking, rather than accepted and complained about on
                // every later run.
                let resolved = uze::theme::resolve(app, home, &id)?;
                for warning in &resolved.warnings {
                    progress::warn(&warning.to_string());
                }
                app.themes().select(&id)?;
                uze_theme::set_active(resolved.theme);
                progress::success(&format!("theme {}", progress::title(&id)));
            }
            ConfigThemeAction::Show { id, format } => {
                let id = match id {
                    Some(id) => id,
                    None => app
                        .themes()
                        .active()?
                        .unwrap_or_else(|| uze_theme::builtin_names()[0].to_owned()),
                };
                let (resolved, layers) = uze::theme::resolve_with_layers(app, home, &id)?;
                emit(
                    format,
                    &theme_report(&id, layers.clone(), &resolved),
                    |_| render_theme(&id, layers, &resolved, verbose),
                );
            }
        },
        ConfigAction::Icons { set, format } => match set {
            None => {
                let sets = app.themes().glyph_sets(uze_theme::glyph_sets())?;
                emit(format, &sets, |sets| render_glyph_sets(sets));
            }
            Some(set) => {
                if !uze_theme::glyph_sets().contains(&set.as_str()) {
                    return Err(uze_application::UzeError::UnusableTheme(format!(
                        "no glyph set `{set}` — UZE carries {}",
                        uze_theme::glyph_sets().join(", ")
                    )));
                }
                app.themes().select_glyphs(&set)?;
                // Re-resolve so this very invocation, and every later one,
                // draws with it: the set is a layer under whichever theme
                // is active, including none.
                let id = app
                    .themes()
                    .active()?
                    .unwrap_or_else(|| uze_theme::builtin_names()[0].to_owned());
                if let Ok(resolved) = uze::theme::resolve(app, home, &id) {
                    uze_theme::set_active(resolved.theme);
                }
                progress::success(&format!("glyphs {}", progress::title(&set)));
            }
        },
        ConfigAction::Extension { id, state, format } => {
            run_config_extension(app, id.as_deref(), state.as_deref(), format)?
        }
        ConfigAction::Notification { state, format } => match state.as_deref() {
            None => {
                let written = app.notifications().agent_finished_as_written()?;
                let chime = chime_label(written.in_force);
                if matches!(format, OutputFormat::Json) {
                    // Chime carries no read model of its own; the choice's
                    // label is the answer, and the word left unread beside it.
                    print_json(&serde_json::json!({
                        "finished_turns_ring": chime,
                        "unrecognised": written.unrecognised,
                    }));
                } else {
                    println!(
                        "{}",
                        progress::aligned_rows(vec![vec![
                            progress::label("notification"),
                            progress::title(chime),
                        ]])
                    );
                }
                // Reported, never written over: the file is the operator's
                // text, and the word may be one a newer build knows.
                if let Some(word) = &written.unrecognised {
                    progress::warn(&format!(
                        "config.toml sets notifications.agent_finished to `{word}`, which this \
                         build does not recognise — {chime} is in force"
                    ));
                }
            }
            Some("test") => {
                // Ring once, whatever the choice in force: the answer to
                // "does this machine actually speak?" asked where the
                // choice was just made. The choice itself is untouched.
                let chime = app.notifications().agent_finished()?;
                // The terminal's own bell — the same control character the
                // workspace client rings, here written to stdout because
                // the CLI has no client and owns no pane.
                print!("\x07");
                progress::success(&format!(
                    "rang once; notification is {}",
                    chime_label(chime)
                ));
            }
            Some(state) => {
                let Some(chime) = parse_chime(state) else {
                    setup_usage_error("notification choice is `on`, `off`, `silent` or `test`");
                };
                app.notifications().set_agent_finished(chime)?;
                if matches!(format, OutputFormat::Text) {
                    progress::success(&format!(
                        "notification {}",
                        progress::title(chime_label(app.notifications().agent_finished()?))
                    ));
                }
            }
        },
    }
    Ok(())
}

/// `uze config extension` — the same switch the management screen flips,
/// written to the same `[extensions]` section, so a workspace launched
/// afterwards offers exactly what either surface last chose.
pub(crate) fn run_config_extension(
    app: &UzeApplication,
    id: Option<&str>,
    state: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let registry = uze_extensions::registry::ExtensionRegistry::builtin();
    let Some(id) = id else {
        let disabled = app.extensions().disabled()?;
        let extensions: Vec<_> = registry
            .all()
            .iter()
            .map(|extension| (extension, !disabled.contains(extension.id)))
            .collect();
        emit(
            format,
            &extensions
                .iter()
                .map(|(extension, enabled)| extension_json(extension, *enabled))
                .collect::<Vec<_>>(),
            |_| render_extensions(&extensions),
        );
        return Ok(());
    };
    let Some(extension) = registry.get(id) else {
        setup_usage_error(&format!(
            "no extension `{id}`; UZE carries {}",
            registry.ids().join(", ")
        ));
    };
    match state {
        None => {
            let enabled = !app.extensions().disabled()?.contains(extension.id);
            emit(format, &extension_json(extension, enabled), |_| {
                render_extensions(&[(extension, enabled)])
            });
        }
        Some(state) => {
            let enabled = match state {
                "on" => true,
                "off" => false,
                _ => setup_usage_error("extension state is `on` or `off`"),
            };
            app.extensions().set_enabled(extension.id, enabled)?;
            if matches!(format, OutputFormat::Text) {
                progress::success(&format!(
                    "{} {}",
                    progress::title(extension.id),
                    extension_label(enabled)
                ));
            } else {
                print_json(&extension_json(extension, enabled));
            }
        }
    }
    Ok(())
}

pub(crate) fn extension_label(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

pub(crate) fn extension_json(
    extension: &uze_extensions::registry::BuiltinExtension,
    enabled: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": extension.id,
        "name": extension.name,
        "enabled": enabled,
    })
}

pub(crate) fn render_extensions(
    extensions: &[(&uze_extensions::registry::BuiltinExtension, bool)],
) -> String {
    let rows = extensions
        .iter()
        .map(|(extension, enabled)| {
            let state = if *enabled {
                progress::success_text(extension_label(true))
            } else {
                progress::label(extension_label(false))
            };
            vec![progress::title(extension.id), state]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

pub(crate) fn chime_label(chime: Chime) -> &'static str {
    match chime {
        Chime::Silent => "silent",
        Chime::OutOfSight => "out of sight",
        Chime::Always => "always",
    }
}

pub(crate) fn parse_chime(state: &str) -> Option<Chime> {
    match state {
        "on" => Some(Chime::Always),
        "off" => Some(Chime::OutOfSight),
        "silent" => Some(Chime::Silent),
        _ => None,
    }
}

pub(crate) fn render_glyph_sets(sets: &[uze_application::application::GlyphSetSummary]) -> String {
    let none_chosen = !sets.iter().any(|set| set.active);
    let rows = sets
        .iter()
        .enumerate()
        .map(|(index, set)| {
            // Drawn in the set's own glyphs, so the row is the answer to
            // "can this terminal render it" rather than a claim about it.
            vec![
                chosen_mark(set.active || (none_chosen && index == 0), &set.id),
                progress::label(preview_of(&set.id)),
            ]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// A list row's name, with the mark that says it is the one in force.
pub(crate) fn chosen_mark(chosen: bool, name: &str) -> String {
    let symbol = uze_theme::Symbol::StatusSelected;
    if chosen {
        format!(
            "{} {}",
            progress::accent(progress::glyph(symbol)),
            progress::title(name)
        )
    } else {
        format!("{} {name}", " ".repeat(progress::glyph_width(symbol)))
    }
}

/// A handful of a set's own marks, resolved from the set rather than from
/// the active theme — the only place in UZE that deliberately draws a glyph
/// it is not currently drawing with.
pub(crate) fn preview_of(id: &str) -> String {
    const SHOWN: &[uze_theme::Symbol] = &[
        uze_theme::Symbol::MarkOk,
        uze_theme::Symbol::MarkOfficial,
        uze_theme::Symbol::MarkAdapted,
        uze_theme::Symbol::MarkAttention,
        uze_theme::Symbol::StatusSelected,
        uze_theme::Symbol::StatusIdle,
        uze_theme::Symbol::ChevronCollapsed,
        uze_theme::Symbol::ArrowTo,
        uze_theme::Symbol::Prompt,
    ];
    let mut layers = vec![uze_theme::default_file()];
    layers.extend(uze_theme::glyph_set_file(id));
    let Ok(resolved) = uze_theme::resolve_stack(
        &uze_theme::Identity::from_file(id, uze_theme::default_file()),
        &layers,
    ) else {
        return String::new();
    };
    SHOWN
        .iter()
        .map(|symbol| resolved.theme.glyph(*symbol))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Records the themes `adaptive` draws in, every one of them loaded first so
/// a typo is refused before anything is written.
fn choose_adaptive_halves(
    app: &UzeApplication,
    home: &UzeHome,
    id: &str,
    light: Option<String>,
    dark: Option<String>,
) -> Result<()> {
    use uze_application::{ADAPTIVE, Background};
    let halves: Vec<(Background, String)> = [(Background::Light, light), (Background::Dark, dark)]
        .into_iter()
        .filter_map(|(background, half)| half.map(|half| (background, half)))
        .collect();
    if halves.is_empty() {
        return Ok(());
    }
    if id != ADAPTIVE {
        return Err(uze_application::UzeError::UnusableTheme(format!(
            "--light and --dark choose what `{ADAPTIVE}` draws in, and `{id}` is one theme \
             whatever the terminal is; run `uze config theme set {ADAPTIVE}` with them"
        )));
    }
    for (_, half) in &halves {
        if half == ADAPTIVE {
            return Err(uze_application::UzeError::UnusableTheme(format!(
                "`{ADAPTIVE}` cannot draw in itself; name a theme for each background"
            )));
        }
        uze::theme::resolve(app, home, half)?;
    }
    for (background, half) in &halves {
        app.themes().set_adaptive(*background, half)?;
    }
    Ok(())
}

/// What `adaptive` stands for on this machine, as the theme list shows it.
fn adaptive_caption(app: &UzeApplication) -> Result<String> {
    use uze_application::Background;
    Ok(format!(
        "{} when light, {} when dark",
        uze::theme::adaptive_half(app, Background::Light)?,
        uze::theme::adaptive_half(app, Background::Dark)?
    ))
}

pub(crate) fn render_theme_list(
    themes: &[uze_application::application::ThemeSummary],
    adaptive: &str,
) -> String {
    // No theme chosen is the default theme in force, and the list says so.
    let none_chosen = !themes.iter().any(|theme| theme.active);
    let rows = themes
        .iter()
        .map(|theme| {
            let active = theme.active || (none_chosen && theme.id == uze_theme::builtin_names()[0]);
            let source = match &theme.path {
                Some(path) => progress::label(progress::path(path)),
                None if theme.id == uze_application::ADAPTIVE => progress::label(adaptive),
                None => progress::label("built in"),
            };
            vec![chosen_mark(active, &theme.id), source]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// What `theme show` reports, in the shape `--format json` prints.
#[derive(serde::Serialize)]
pub(crate) struct ThemeReport {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    /// The layers this resolved from, bottom-up. With `extends` and the
    /// operator's own overrides in play, "where did this colour come from"
    /// is the first thing an author asks.
    pub(crate) layers: Vec<String>,
    pub(crate) syntax_theme: String,
    pub(crate) colors: std::collections::BTreeMap<String, String>,
    pub(crate) symbols: std::collections::BTreeMap<String, Vec<String>>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn theme_report(
    id: &str,
    layers: Vec<String>,
    loaded: &uze_theme::Loaded,
) -> ThemeReport {
    let theme = &loaded.theme;
    ThemeReport {
        id: id.to_owned(),
        name: theme.name().to_owned(),
        description: theme.description().to_owned(),
        layers,
        syntax_theme: theme.syntax_theme().to_owned(),
        colors: uze_theme::Token::ALL
            .iter()
            .map(|token| (token.to_string(), theme.color(*token).to_string()))
            .collect(),
        symbols: uze_theme::Symbol::ALL
            .iter()
            .map(|symbol| (symbol.to_string(), theme.symbol(*symbol).frames().to_vec()))
            .collect(),
        warnings: loaded
            .warnings
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    }
}

pub(crate) fn render_theme(
    id: &str,
    layers: Vec<String>,
    loaded: &uze_theme::Loaded,
    verbose: bool,
) -> String {
    let report = theme_report(id, layers, loaded);
    let mut out = progress::report_title(id, Some(&report.layers.join(" > ")));
    if !report.description.is_empty() {
        out.push_str(&format!("{}\n", progress::label(&report.description)));
    }
    out.push('\n');
    if verbose {
        out.push_str(&progress::aligned_rows(
            uze_theme::Token::ALL
                .iter()
                .map(|token| {
                    let rgb = loaded.theme.color(*token);
                    vec![
                        progress::label(token.to_string()),
                        format!("{} {}", progress::swatch(rgb), rgb),
                    ]
                })
                .collect(),
        ));
        out.push_str("\n\n");
        out.push_str(&progress::aligned_rows(
            report
                .symbols
                .iter()
                .map(|(symbol, frames)| vec![progress::label(symbol), frames.join(" ")])
                .collect(),
        ));
        out.push('\n');
    } else {
        // A family of colours on one line, in the order the tokens are
        // declared: the palette is read at a glance, and `--verbose` has
        // every value.
        let mut families: Vec<(String, String)> = Vec::new();
        for token in uze_theme::Token::ALL {
            let name = token.to_string();
            let family = match name.split_once('.') {
                Some((family, _)) => family.to_owned(),
                None => name.split('-').next().unwrap_or(&name).to_owned(),
            };
            let swatch = progress::swatch(loaded.theme.color(*token));
            match families.iter_mut().find(|(known, _)| *known == family) {
                Some((_, swatches)) => swatches.push_str(&swatch),
                None => families.push((family, swatch)),
            }
        }
        let mut rows: Vec<Vec<String>> = families
            .into_iter()
            .map(|(family, swatches)| vec![progress::label(family), swatches])
            .collect();
        rows.push(vec![
            progress::label("marks"),
            [
                uze_theme::Symbol::MarkOk,
                uze_theme::Symbol::MarkAttention,
                uze_theme::Symbol::MarkCross,
                uze_theme::Symbol::MarkUnsupported,
                uze_theme::Symbol::MarkAdapted,
                uze_theme::Symbol::StatusSelected,
                uze_theme::Symbol::StatusIdle,
                uze_theme::Symbol::ArrowTo,
            ]
            .iter()
            .map(|symbol| loaded.theme.glyph(*symbol))
            .collect::<Vec<_>>()
            .join(" "),
        ]);
        out.push_str(&progress::aligned_rows(rows));
        out.push('\n');
    }
    if !report.warnings.is_empty() {
        out.push('\n');
        for warning in &report.warnings {
            out.push_str(&format!("{} {warning}\n", progress::warning_icon()));
        }
    }
    out
}
