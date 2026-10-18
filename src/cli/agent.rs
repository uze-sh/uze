//! `uze agent`: the surface an agent UZE launched reads.

use crate::*;

/// The agent's own surface. Every answer here is written for a model: a
/// refusal names what this project would have accepted, because that
/// sentence is the only feedback channel a denied agent has.
pub(crate) fn run_agent(app: &UzeApplication, action: AgentAction) -> Result<()> {
    match action {
        AgentAction::Work { action } => run_agent_work(app, action),
        AgentAction::Memory { action } => run_agent_memory(app, action),
        AgentAction::Artifacts { action } => run_agent_artifacts(action),
        AgentAction::Context { action } => run_context(app, action),
        AgentAction::Market { action } => run_agent_market(app, action),
        AgentAction::Plugin { action } => run_agent_plugin(app, action),
    }
}

/// `uze agent market …` — the marketplace half of the authoring surface.
pub(crate) fn run_agent_market(app: &UzeApplication, action: AgentMarketAction) -> Result<()> {
    match action {
        AgentMarketAction::Create {
            name,
            local,
            at,
            plugins_dir,
            description,
            format,
        } => {
            if local {
                let current_dir = cwd()?;
                let report = with_spinner(
                    &format!("Scaffolding marketplace {name} in this project..."),
                    || {
                        app.project().create_local_marketplace(
                            &name,
                            description.as_deref(),
                            &plugins_dir,
                            &current_dir,
                        )
                    },
                )?;
                emit(format, &report, |report| {
                    format!(
                        "{}{}\n",
                        progress::change(
                            progress::Change::Added,
                            &name,
                            Some(&format!(
                                "{} · this project is the marketplace",
                                report.root.display()
                            )),
                        ),
                        progress::next_step(&format!(
                            "uze agent plugin create <name> --market {name}"
                        ))
                    )
                });
                return Ok(());
            }
            let Some(at) = at else {
                setup_usage_error(
                    "choose a frontier: `--local` (this project is the \
                                   marketplace) or `--at <dir>` (a standalone checkout)",
                );
            };
            // The registry outlives the directory this ran from, so the
            // path is made absolute the way the platform makes one: on
            // Windows that also spells it with one separator throughout.
            let at =
                std::path::absolute(&at).map_err(|source| uze_application::UzeError::Read {
                    path: at.clone(),
                    source,
                })?;
            let report = with_spinner(&format!("Scaffolding marketplace {name}..."), || {
                app.project()
                    .create_marketplace(&name, description.as_deref(), &at)
            })?;
            emit(format, &report, |report| {
                format!(
                    "{}{}\n",
                    progress::change(
                        progress::Change::Added,
                        &name,
                        Some(&format!("{} · registered, linked", report.root.display())),
                    ),
                    progress::next_step(&format!("uze agent plugin create <name> --market {name}"))
                )
            });
        }
        AgentMarketAction::Check { path, format } => {
            let report = app.project().check(&path, true)?;
            emit(format, &report, |report| render_check(report, true));
            if !report.is_clean() {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "check failed ({})",
                    count(report.findings.len(), "problem")
                )));
            }
        }
    }
    Ok(())
}

/// `uze agent plugin …` — the plugin half of the authoring surface.
pub(crate) fn run_agent_plugin(app: &UzeApplication, action: AgentPluginAction) -> Result<()> {
    match action {
        AgentPluginAction::Create {
            name,
            market,
            description,
            category,
            hook,
            mcp,
            agent,
            instructions,
            format,
        } => {
            let report = with_spinner(&format!("Scaffolding plugin {name}..."), || {
                app.project().create_plugin(
                    &market,
                    &name,
                    description.as_deref(),
                    category.as_deref(),
                    uze_application::ScaffoldCapabilities {
                        hook,
                        mcp,
                        agent,
                        instructions,
                    },
                )
            })?;
            emit(format, &report, |report| {
                format!(
                    "{}{}\n",
                    progress::change(
                        progress::Change::Added,
                        &report.name,
                        Some(&format!(
                            "{} · installable from {}",
                            report.root.display(),
                            report.market
                        )),
                    ),
                    progress::next_step(&format!(
                        "uze agent plugin check {}",
                        report.root.display()
                    ))
                )
            });
        }
        AgentPluginAction::Check { path, format } => {
            let report = app.project().check(&path, false)?;
            emit(format, &report, |report| render_check(report, false));
            if !report.is_clean() {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "check failed ({})",
                    count(report.findings.len(), "problem")
                )));
            }
        }
    }
    Ok(())
}

/// The check's answer: what it would deliver, and every finding, located.
pub(crate) fn render_check(
    report: &uze_application::ValidationReport,
    as_marketplace: bool,
) -> String {
    let mut text = String::new();
    if report.is_clean() {
        let standard = report
            .agent_plugins
            .as_ref()
            .filter(|standard| standard.conformant)
            .map(|standard| format!("valid {}", standard.standard))
            .unwrap_or_else(|| "clean".to_owned());
        let subject = if as_marketplace {
            "marketplace"
        } else {
            "plugin"
        };
        text.push_str(&format!(
            "{} {subject}   {}\n",
            progress::success_icon(),
            progress::label(standard)
        ));
    }
    let rows: Vec<Vec<String>> = report
        .delivers
        .iter()
        .map(|identity| {
            let (kind, name) = delivered_as(identity);
            vec![progress::label(kind), name]
        })
        .collect();
    if !rows.is_empty() {
        text.push_str(&progress::aligned_rows(rows));
        text.push('\n');
    }
    if let Some(standard) = report
        .agent_plugins
        .as_ref()
        .filter(|standard| !standard.conformant)
    {
        text.push_str(&format!(
            "\n{} not yet a valid {} plugin; uze installs it all the same\n",
            progress::warning_icon(),
            standard.standard
        ));
        for divergence in &standard.divergences {
            text.push_str(&format!("  {}\n", progress::label(divergence)));
        }
    }
    if !report.findings.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        for finding in &report.findings {
            text.push_str(&format!("{} {finding}\n", progress::error_icon()));
        }
    }
    for warning in &report.warnings {
        text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    text
}

/// A delivered capability's identity (`package:lint@local:hooks.json:guard`)
/// as the kind and name a person recognises; an identity of any other
/// shape — a marketplace check lists plugins — is its own name.
pub(crate) fn delivered_as(identity: &str) -> (&'static str, String) {
    let Some(rest) = identity.strip_prefix("package:") else {
        return ("plugin", identity.to_owned());
    };
    let path = rest.split_once(':').map_or(rest, |(_, path)| path);
    if let Some(name) = path.strip_prefix("hooks.json:") {
        return ("hook", name.to_owned());
    }
    if let Some(name) = path.strip_prefix("mcp.json:") {
        return ("mcp", name.to_owned());
    }
    if let Some(skill) = path.strip_suffix("/SKILL.md") {
        return (
            "skill",
            skill.rsplit('/').next().unwrap_or(skill).to_owned(),
        );
    }
    if let Some(agent) = path.strip_prefix("agents/") {
        return ("agent", agent.trim_end_matches(".md").to_owned());
    }
    if path.ends_with("AGENTS.md") {
        return ("instructions", path.to_owned());
    }
    ("file", path.to_owned())
}

/// The identity the agent's launch carried, and the key it was issued,
/// inherited by every process the harness starts — this one included.
/// Without them there is no agent to answer: a person's shell, or a
/// harness started by hand, is not an agent UZE launched.
pub(crate) fn launched_agent(
    refusal: fn(String) -> uze_application::UzeError,
) -> Result<(String, String)> {
    let stamped = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    match (
        stamped(uze_terminal::launch::AGENT_IDENTITY_VARIABLE),
        stamped(uze_terminal::launch::AGENT_KEY_VARIABLE),
    ) {
        (Some(id), Some(key)) => Ok((id, key)),
        _ => Err(refusal(
            "this process is not an agent UZE launched".to_owned(),
        )),
    }
}

pub(crate) fn run_agent_work(app: &UzeApplication, action: AgentWorkAction) -> Result<()> {
    let cwd = cwd()?;
    match action {
        AgentWorkAction::Name { name, format } => {
            let (id, key) = launched_agent(uze_application::UzeError::TaskNaming)?;
            let named = app.workspace().name_task(
                uze_application::Claim {
                    id: &id,
                    key: &key,
                    cwd: &cwd,
                },
                &name,
            )?;
            emit(format, &NamedTaskReport::from(&named), |_| {
                match &named.branch {
                    Some(branch) => format!(
                        "{} named `{}` on branch `{branch}`\n",
                        progress::success_icon(),
                        named.label,
                    ),
                    None => format!(
                        "{} named `{}` in the operator's checkout, branch unchanged\n",
                        progress::success_icon(),
                        named.label,
                    ),
                }
            });
        }
        AgentWorkAction::Split { topic } => {
            let (id, key) = launched_agent(uze_application::UzeError::AgentWork)?;
            let split = app.workspace().split_work(
                uze_application::Claim {
                    id: &id,
                    key: &key,
                    cwd: &cwd,
                },
                &topic,
                &[],
            )?;
            for warning in &split.warnings {
                eprintln!(
                    "{} {}",
                    progress::warning_icon(),
                    uze_application::inert(&warning.to_string())
                );
            }
            // The path alone, so `cd "$(uze agent work split <topic>)"` works.
            println!("{}", split.path.display());
        }
        AgentWorkAction::Join { topic } => {
            let (id, key) = launched_agent(uze_application::UzeError::AgentWork)?;
            match app.workspace().join_work(
                uze_application::Claim {
                    id: &id,
                    key: &key,
                    cwd: &cwd,
                },
                &topic,
            )? {
                uze_application::JoinedWork::Joined { commits } => println!(
                    "{} joined {commits} commit{} from `{topic}`",
                    progress::success_icon(),
                    if commits == 1 { "" } else { "s" }
                ),
                uze_application::JoinedWork::Conflicted { checkout, paths } => {
                    let listed: Vec<String> = paths
                        .iter()
                        .map(|path| format!("  {}", path.display()))
                        .collect();
                    return Err(uze_application::UzeError::AgentWork(format!(
                        "replaying `{topic}` onto this branch stopped on conflicts in {}:\n{}\n\
                         resolve them there, run `git rebase --continue`, and join again",
                        checkout.display(),
                        listed.join("\n")
                    )));
                }
            }
        }
        AgentWorkAction::List { format } => {
            let (id, key) = launched_agent(uze_application::UzeError::AgentWork)?;
            let children = app.workspace().list_work(uze_application::Claim {
                id: &id,
                key: &key,
                cwd: &cwd,
            })?;
            let report: Vec<SubagentReport> = children.iter().map(SubagentReport::from).collect();
            emit(format, &report, |_| {
                children
                    .iter()
                    .map(|child| {
                        format!(
                            "{}\t{}\t{}\t{}\t{}\n",
                            child.topic,
                            child.path.display(),
                            child.branch,
                            if child.dirty { "dirty" } else { "clean" },
                            child.ahead
                        )
                    })
                    .collect()
            });
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub(crate) struct SubagentReport<'a> {
    pub(crate) topic: &'a str,
    pub(crate) path: &'a Path,
    pub(crate) branch: &'a str,
    pub(crate) dirty: bool,
    pub(crate) ahead: usize,
}

impl<'a> From<&'a uze_application::SubagentCheckout> for SubagentReport<'a> {
    fn from(child: &'a uze_application::SubagentCheckout) -> Self {
        Self {
            topic: &child.topic,
            path: &child.path,
            branch: &child.branch,
            dirty: child.dirty,
            ahead: child.ahead,
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct NamedTaskReport<'a> {
    pub(crate) task: &'a str,
    pub(crate) branch: Option<&'a str>,
    pub(crate) label: &'a str,
}

impl<'a> From<&'a uze_application::NamedTask> for NamedTaskReport<'a> {
    fn from(named: &'a uze_application::NamedTask) -> Self {
        Self {
            task: &named.task,
            branch: named.branch.as_deref(),
            label: &named.label,
        }
    }
}

/// Draws every artifact the project declares, exactly as the workspace
/// surface would, and answers with what that found.
///
/// The verdict is the exit code, not something to read: an agent that has
/// just written a diagram needs to be *told* it does not draw, and a
/// report it has to interpret is a report it can decide it passed. The
/// grammar is never restated here — the parser is run and quoted.
pub(crate) fn run_agent_artifacts(action: AgentArtifactsAction) -> Result<()> {
    let AgentArtifactsAction::Check { path, format } = action;
    let project = context_path(path);
    let checkup = uze_extensions::architect::check(
        &uze::ui::extension_host::WorkspaceHost,
        uze::ui::extension_host::artifacts_declared_in(&project),
    );
    let report = ArtifactsCheckReport::from(&checkup);
    emit(format, &report, render_artifacts_check);
    match report.verdict() {
        Some(failure) => Err(uze_application::UzeError::ArtifactsNotDrawable(failure)),
        None => Ok(()),
    }
}

#[derive(serde::Serialize)]
pub(crate) struct ArtifactsCheckReport {
    /// The directory checked, absent where nothing was.
    pub(crate) declared: Option<String>,
    pub(crate) checked: usize,
    /// How many of them do not draw at all.
    pub(crate) undrawable: usize,
    /// How many draw with edges missing — counted apart because the board
    /// still looks finished, which is the failure nobody sees by looking.
    pub(crate) unrouted: usize,
    /// How many have a box whose link opens nothing.
    pub(crate) unlinked: usize,
    pub(crate) artifacts: Vec<CheckedArtifactReport>,
    /// Why there was nothing to check, where there was not.
    pub(crate) nothing: Option<String>,
    /// The same, where it is somebody's mistake rather than an answer.
    pub(crate) unusable: Option<String>,
    pub(crate) hint: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct CheckedArtifactReport {
    pub(crate) origin: String,
    pub(crate) name: String,
    pub(crate) area: &'static str,
    /// `drawn`, `unrouted` or `undrawable` — the three states a diagram
    /// can be in once this build has tried to draw it.
    pub(crate) verdict: &'static str,
    /// How many edges found no path.
    pub(crate) unrouted: usize,
    /// Why it is not drawn at all, quoted from the parser.
    pub(crate) reason: Option<String>,
    /// Every box link that opens nothing, and why.
    pub(crate) broken_links: Vec<String>,
}

impl ArtifactsCheckReport {
    /// What makes this a failed check, said in one sentence, or nothing.
    fn verdict(&self) -> Option<String> {
        if let Some(unusable) = &self.unusable {
            return Some(unusable.clone());
        }
        match (self.undrawable + self.unrouted, self.unlinked) {
            (0, 0) => None,
            (0, unlinked) => Some(format!(
                "{unlinked} of {} artifacts link a box to nothing",
                self.checked
            )),
            (failed, _) => Some(format!(
                "{failed} of {} artifacts do not draw as written",
                self.checked
            )),
        }
    }
}

impl From<&uze_extensions::architect::Checkup> for ArtifactsCheckReport {
    fn from(checkup: &uze_extensions::architect::Checkup) -> Self {
        use uze_extensions::architect::{Checkup, Verdict};
        let empty = Self {
            declared: None,
            checked: 0,
            undrawable: 0,
            unrouted: 0,
            unlinked: 0,
            artifacts: Vec::new(),
            nothing: None,
            unusable: None,
            hint: None,
        };
        match checkup {
            Checkup::Nothing { text, hint } => Self {
                nothing: Some(text.clone()),
                hint: Some(hint.clone()),
                ..empty
            },
            Checkup::Unusable { text, hint } => Self {
                unusable: Some(text.clone()),
                hint: Some(hint.clone()),
                ..empty
            },
            Checkup::Checked {
                declared,
                artifacts,
            } => Self {
                declared: Some(declared.clone()),
                checked: artifacts.len(),
                undrawable: artifacts
                    .iter()
                    .filter(|artifact| matches!(artifact.verdict, Verdict::Undrawable(_)))
                    .count(),
                unrouted: artifacts
                    .iter()
                    .filter(|artifact| matches!(artifact.verdict, Verdict::Unrouted { .. }))
                    .count(),
                unlinked: artifacts
                    .iter()
                    .filter(|artifact| !artifact.broken_links.is_empty())
                    .count(),
                artifacts: artifacts
                    .iter()
                    .map(|artifact| CheckedArtifactReport {
                        origin: artifact.origin.clone(),
                        name: artifact.name.clone(),
                        area: artifact.area,
                        verdict: match artifact.verdict {
                            Verdict::Drawn => "drawn",
                            Verdict::Unrouted { .. } => "unrouted",
                            Verdict::Undrawable(_) => "undrawable",
                        },
                        unrouted: match artifact.verdict {
                            Verdict::Unrouted { edges } => edges,
                            _ => 0,
                        },
                        reason: match &artifact.verdict {
                            Verdict::Undrawable(reason) => Some(reason.clone()),
                            _ => None,
                        },
                        broken_links: artifact.broken_links.clone(),
                    })
                    .collect(),
                ..empty
            },
        }
    }
}

pub(crate) fn render_artifacts_check(report: &ArtifactsCheckReport) -> String {
    if let Some(text) = report.unusable.as_ref().or(report.nothing.as_ref()) {
        let mut out = format!("{}\n", text.trim_end_matches('.'));
        if let Some(hint) = &report.hint {
            // The hint's line breaks balance it on the surface, which
            // centres each line; a terminal folds it to its own width.
            out.push_str(&format!(
                "{}\n",
                progress::aligned_rows_wrapped(
                    vec![vec![hint.replace('\n', " ")]],
                    progress::terminal_width()
                )
            ));
        }
        return out;
    }
    let declared = report.declared.as_deref().unwrap_or_default();
    let mut out = progress::report_title(declared, Some(&format!("{} checked", report.checked)));
    out.push('\n');
    let rows = report
        .artifacts
        .iter()
        .map(|artifact| {
            let (icon, note) = match artifact.verdict {
                "drawn" if !artifact.broken_links.is_empty() => (
                    progress::error_icon(),
                    progress::error_text(artifact.broken_links.join("; ")),
                ),
                "drawn" => (progress::success_icon(), artifact.name.clone()),
                "unrouted" => (
                    progress::warning_icon(),
                    progress::warning_text(format!(
                        "{} {} found no path",
                        artifact.unrouted,
                        plural(artifact.unrouted, "edge", "edges")
                    )),
                ),
                _ => (
                    progress::error_icon(),
                    progress::error_text(artifact.reason.clone().unwrap_or_default()),
                ),
            };
            vec![
                icon,
                artifact.origin.clone(),
                progress::label(artifact.area),
                note,
            ]
        })
        .collect();
    out.push_str(&progress::aligned_rows(rows));
    out.push('\n');
    // Only the passing verdict is said here. The failing one is the error
    // the command exits with, and saying it twice on one screen invites
    // the reader to look for the difference between the two.
    if report.verdict().is_none() {
        out.push_str(&format!(
            "\n{} every artifact draws, every edge routes, every link opens a file\n",
            progress::success_icon()
        ));
    }
    out
}

/// `uze agent memory …` — the facts later agents are given, on any harness.
pub(crate) fn run_agent_memory(app: &UzeApplication, action: AgentMemoryAction) -> Result<()> {
    use uze_application::application::memory::{Kind, MemoryCaller, MemoryScope};
    let cwd = cwd()?;
    let identity = launched_agent(uze_application::UzeError::Memory).ok();
    let caller = match &identity {
        Some((id, key)) => MemoryCaller::Agent(uze_application::Claim { id, key, cwd: &cwd }),
        None => MemoryCaller::Operator { cwd: &cwd },
    };
    let scope = |machine: bool| {
        if machine {
            MemoryScope::Machine
        } else {
            MemoryScope::Project
        }
    };
    match action {
        AgentMemoryAction::Remember {
            name,
            kind,
            description,
            machine,
            format,
        } => {
            let kind = Kind::parse(&kind).expect("clap allows only the four kinds");
            let mut body = String::new();
            if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut body)
                    .map_err(|error| uze_application::UzeError::Memory(error.to_string()))?;
            }
            let kept =
                app.memory()
                    .remember(caller, scope(machine), &name, kind, &description, &body)?;
            let report = MemoryWrite {
                name: &name,
                scope: kept.scope.label(),
                path: &kept.path,
                updated: kept.updated,
                proposed: kept.proposed,
                secret_looking: kept.secret_looking,
                similar: kept.similar.as_ref().map(|fact| fact.name.as_str()),
            };
            emit(format, &report, |report| {
                let mut said = if report.proposed {
                    format!(
                        "{} proposed `{}` for every project; the operator accepts it before any \
                         agent is given it\n",
                        progress::success_icon(),
                        report.name
                    )
                } else {
                    format!(
                        "{} {} `{}` in the {} memory ({})\n",
                        progress::success_icon(),
                        if report.updated {
                            "updated"
                        } else {
                            "remembered"
                        },
                        report.name,
                        report.scope,
                        report.path.display()
                    )
                };
                if let Some(spot) = report.secret_looking {
                    said.push_str(&format!(
                        "{} line {} of the {} assigns what looks like a secret; if it is one, \
                         remember the fact again without it\n",
                        progress::warning_icon(),
                        spot.line,
                        spot.field
                    ));
                }
                if let Some(hint) = near_duplicate_hint(report) {
                    said.push_str(&format!("{} {hint}\n", progress::warning_icon()));
                }
                said
            });
        }
        AgentMemoryAction::Forget { name, machine } => {
            let scope = scope(machine);
            if app.memory().forget(caller, scope, &name)? {
                println!(
                    "{} forgot `{name}` from the {} memory",
                    progress::success_icon(),
                    scope.label()
                );
            } else {
                println!(
                    "no fact named `{name}` is kept in the {} memory",
                    scope.label()
                );
            }
        }
        AgentMemoryAction::List {
            words,
            kind: None,
            format,
        } if words.is_empty() => {
            let report = app.memory().list(caller)?;
            emit(format, &report, render_memory);
        }
        AgentMemoryAction::List {
            words,
            kind,
            format,
        } => {
            let kind = kind.as_deref().and_then(Kind::parse);
            let found = app.memory().find(caller, &words, kind)?;
            emit(format, &found, render_found);
        }
        AgentMemoryAction::Show {
            name,
            machine,
            format,
        } => {
            let shown = app.memory().show(caller, scope(machine), &name)?;
            emit(format, &shown, render_shown);
        }
    }
    Ok(())
}

/// What to do about a near duplicate the write just made: the fact it
/// repeats, and how to take the copy back. A proposal waits for the
/// operator, who discards it; nothing an agent runs removes one.
fn near_duplicate_hint(report: &MemoryWrite<'_>) -> Option<String> {
    let similar = report.similar?;
    let undo = if report.proposed {
        "the operator is asked about this proposal".to_owned()
    } else {
        format!(
            "`uze agent memory forget {}{}` removes this one",
            report.name,
            if report.scope == "machine" { " -m" } else { "" }
        )
    };
    Some(format!(
        "`{similar}` says much the same in the {} memory; remembering under that name \
         updates it instead, and {undo}",
        report.scope
    ))
}

#[derive(serde::Serialize)]
struct MemoryWrite<'a> {
    name: &'a str,
    scope: &'a str,
    path: &'a std::path::Path,
    updated: bool,
    proposed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret_looking: Option<uze_application::application::memory::Spot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    similar: Option<&'a str>,
}

/// The memory as the agent and the operator read it: where each scope
/// lives, each fact with who wrote it, proposals and what does not read.
pub(crate) fn render_memory(report: &uze_application::application::memory::MemoryReport) -> String {
    use uze_application::application::memory::MemoryScope;
    let mut text = String::new();
    let mut section = |title: &str, dir: Option<&std::path::Path>, scope: MemoryScope| {
        let entries: Vec<_> = report.entries.iter().filter(|e| e.scope == scope).collect();
        text.push_str(&format!(
            "{}{}\n",
            progress::report_section(title),
            dir.map(|dir| format!("  {}", dir.display()))
                .unwrap_or_default()
        ));
        if entries.is_empty() {
            text.push_str("  nothing kept\n");
        }
        for entry in entries {
            text.push_str(&memory_line(entry));
        }
        text.push('\n');
    };
    section(
        "Project memory",
        report.project_dir.as_deref(),
        MemoryScope::Project,
    );
    section(
        "Machine memory",
        Some(&report.machine_dir),
        MemoryScope::Machine,
    );
    if !report.proposals.is_empty() {
        text.push_str(&progress::report_section(
            "Proposed, waiting for the operator",
        ));
        for entry in &report.proposals {
            text.push_str(&memory_line(entry));
            text.push_str(&format!("    {}\n", entry.path.display()));
        }
        text.push('\n');
    }
    for unreadable in &report.unreadable {
        text.push_str(&format!(
            "{} {}: {}\n",
            progress::warning_icon(),
            unreadable.path.display(),
            uze_application::inert(&unreadable.reason)
        ));
    }
    text
}

/// What a search found, best first: each fact, where the words were in
/// it, and the command that reads it.
fn render_found(found: &uze_application::application::memory::Found) -> String {
    let mut text = if found.matches.is_empty() {
        "no fact matches\n".to_owned()
    } else if found.truncated {
        format!(
            "{} facts match; the {} best:\n",
            found.total,
            found.matches.len()
        )
    } else {
        String::new()
    };
    for matched in &found.matches {
        text.push_str(&memory_line(&matched.entry));
        for hit in &matched.hits {
            let at = match hit.line {
                Some(line) => format!("{} {line}", hit.field),
                None => hit.field.to_owned(),
            };
            text.push_str(&format!(
                "    {}  {}\n",
                progress::label(at),
                uze_application::inert(&hit.text)
            ));
        }
        text.push_str(&format!("    {}\n", matched.show));
    }
    for unreadable in &found.unreadable {
        text.push_str(&format!(
            "{} {}: {}\n",
            progress::warning_icon(),
            unreadable.path.display(),
            uze_application::inert(&unreadable.reason)
        ));
    }
    text
}

/// One fact in full, then the facts around it.
fn render_shown(shown: &uze_application::application::memory::Shown) -> String {
    let entry = &shown.entry;
    let mut text = format!(
        "{} ({}, {} memory{}): {}\n{}\n\n{}\n",
        entry.name,
        entry.kind.label(),
        entry.scope.label(),
        if entry.writable {
            String::new()
        } else {
            format!(", kept by {}", entry.origin)
        },
        uze_application::inert(&entry.description),
        entry.path.display(),
        uze_application::inert(shown.body.trim())
    );
    if !entry.missing.is_empty() {
        text.push_str(&format!(
            "\n{}",
            progress::report_section("Cites what this checkout does not have")
        ));
        for cited in &entry.missing {
            text.push_str(&format!("  {}\n", uze_application::inert(cited)));
        }
    }
    if !shown.links.is_empty() {
        text.push_str(&format!("\n{}", progress::report_section("Links to")));
        for link in &shown.links {
            text.push_str(&match &link.fact {
                Some(fact) => memory_line(fact),
                None => format!(
                    "  {}  {}\n",
                    uze_application::inert(&link.written),
                    progress::label("no fact by that name")
                ),
            });
        }
    }
    if !shown.linked_from.is_empty() {
        text.push_str(&format!("\n{}", progress::report_section("Linked from")));
        for fact in &shown.linked_from {
            text.push_str(&memory_line(fact));
        }
    }
    text
}

fn memory_line(entry: &uze_application::application::memory::Entry) -> String {
    let by = match (&entry.source, entry.writable) {
        (_, false) => format!("kept by {}", entry.origin),
        (Some(source), true) => format!("by {}", source.describe()),
        (None, true) => String::new(),
    };
    let stale = if entry.missing.is_empty() {
        String::new()
    } else {
        format!(
            "  {}",
            progress::label("cites a path this checkout does not have")
        )
    };
    format!(
        "  {} ({})  {}  {}{stale}\n",
        entry.name,
        entry.kind.label(),
        uze_application::inert(&entry.description),
        progress::label(by)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written<'a>(scope: &'a str, proposed: bool) -> MemoryWrite<'a> {
        MemoryWrite {
            name: "portuguese-replies",
            scope,
            path: std::path::Path::new("/m/portuguese-replies.md"),
            updated: false,
            proposed,
            secret_looking: None,
            similar: Some("reply-in-ptbr"),
        }
    }

    #[test]
    fn a_near_duplicate_hint_undoes_the_write_in_its_own_scope() {
        let project = near_duplicate_hint(&written("project", false)).unwrap();
        assert!(
            project.ends_with("`uze agent memory forget portuguese-replies` removes this one"),
            "{project}"
        );
        let machine = near_duplicate_hint(&written("machine", false)).unwrap();
        assert!(
            machine.ends_with("`uze agent memory forget portuguese-replies -m` removes this one"),
            "{machine}"
        );
        let proposal = near_duplicate_hint(&written("machine", true)).unwrap();
        assert!(!proposal.contains("forget"), "{proposal}");
    }
}
