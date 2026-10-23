//! Where a GSD milestone stands, and what to ask the agent next.
//!
//! The dialect table lists GSD's phases as changes and its project
//! documents as specs, which is what every tool shares. What GSD alone
//! keeps is a running account of the milestone: `STATE.md`'s front matter
//! says which milestone is open, how far it got and where work stopped;
//! `ROADMAP.md` orders the phases and gives each a goal; each plan's front
//! matter puts it in a wave, and `autonomous: false` makes it stop for the
//! operator. That account is read here, as code rather than as a table,
//! because it is one tool's file format and not a layout.
//!
//! GSD is driven by asking the agent for its next step, so the surface
//! offers that step, worked out from the files the way GSD's own
//! `gsd-next` would: discuss a phase with no context yet, plan one with
//! context and no plans, execute one with plans left, verify one whose
//! plans all finished, and audit the milestone once every phase is done.

use std::path::Path;

use crate::Host;

/// The steps of GSD's loop the surface can ask the agent for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Discuss,
    Plan,
    Execute,
    Verify,
    /// Every phase is done: check the milestone against what it set out
    /// to do.
    Audit,
    /// The files do not say, so GSD is asked to work it out itself.
    Next,
    Progress,
}

impl Step {
    /// The steps a phase offers, in the order GSD runs them.
    pub const PHASE: [Step; 4] = [Step::Discuss, Step::Plan, Step::Execute, Step::Verify];

    fn skill(self) -> &'static str {
        match self {
            Step::Discuss => "gsd-discuss-phase",
            Step::Plan => "gsd-plan-phase",
            Step::Execute => "gsd-execute-phase",
            Step::Verify => "gsd-verify-work",
            Step::Audit => "gsd-audit-milestone",
            Step::Next => "gsd-next",
            Step::Progress => "gsd-progress",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Step::Discuss => "Discuss",
            Step::Plan => "Plan",
            Step::Execute => "Execute",
            Step::Verify => "Verify",
            Step::Audit => "Audit the milestone",
            Step::Next => "Next step",
            Step::Progress => "Progress",
        }
    }
}

/// One step, and the phase it is about when it is about one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ask {
    pub step: Step,
    pub phase: Option<String>,
}

impl Ask {
    fn of(step: Step, phase: Option<&str>) -> Self {
        Self {
            step,
            phase: phase.map(str::to_owned),
        }
    }

    /// The GSD Skill that takes this step.
    pub fn skill(&self) -> &'static str {
        self.step.skill()
    }

    /// What follows the Skill's name: the phase, when there is one.
    pub fn arguments(&self) -> String {
        self.phase.clone().unwrap_or_default()
    }

    /// The Skill and its arguments, as words. The character a harness
    /// calls a Skill with is not here: the host knows which agent it is
    /// typing into, and this surface does not.
    pub fn words(&self) -> String {
        match &self.phase {
            Some(phase) => format!("{} {phase}", self.skill()),
            None => self.skill().to_owned(),
        }
    }

    /// How a menu offers it: what it does, then what it asks for.
    pub fn entry(&self) -> String {
        format!("{} · {}", self.step.label(), self.words())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mark {
    Done,
    /// In the first wave with a plan left: what executing runs next.
    Current,
    Pending,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    /// The plan's number within its phase, as its file spells it (`02`).
    pub id: String,
    /// The phase and the plan, as its file spells them (`03-02`).
    pub name: String,
    pub wave: u32,
    pub mark: Mark,
    /// `autonomous: false`: execution stops on it for the operator.
    pub checkpoint: bool,
    pub objective: String,
    /// The one-liner its summary opens with, once it ran.
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Phase {
    /// As the roadmap numbers it, decimals included (`2.1`).
    pub number: String,
    pub name: String,
    /// Ticked on the roadmap.
    pub done: bool,
    pub goal: String,
    pub criteria: Vec<String>,
    /// In wave order, then by number.
    pub plans: Vec<Plan>,
    /// The phase's context, research and verification files, by name.
    pub documents: Vec<String>,
    pub next: Ask,
}

impl Phase {
    pub fn finished_plans(&self) -> usize {
        self.plans
            .iter()
            .filter(|plan| plan.mark == Mark::Done)
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// `v1.9`.
    pub version: String,
    pub name: String,
    pub status: String,
    pub percent: u8,
    pub plans_done: u32,
    pub plans_total: u32,
    pub stopped_at: Option<String>,
    pub blockers: Vec<String>,
    pub todos: usize,
    pub phases: Vec<Phase>,
    /// Index into `phases` of the one work is on.
    pub current: Option<usize>,
    pub next: Ask,
}

/// The milestone `root`'s `.planning/` has open, or `None` where there is
/// no GSD project or its last milestone is closed.
pub fn read(host: &dyn Host, root: &Path) -> Option<Milestone> {
    let planning = root.join(".planning");
    let state = host.read_file(&planning.join("STATE.md")).ok()?;
    let (front, body) = crate::shared::front_matter::split(&state);
    let front = fields(front);
    let version = field(&front, "milestone")?.to_owned();
    let status = field(&front, "status").unwrap_or_default().to_owned();
    if closed(&status) {
        return None;
    }
    let roadmap = host
        .read_file(&planning.join("ROADMAP.md"))
        .unwrap_or_default();
    let phase_dirs = host.list_dir(&planning.join("phases")).unwrap_or_default();
    let phases: Vec<Phase> = roadmap_phases(&roadmap)
        .into_iter()
        .map(|(number, name, done)| {
            let directory = phase_dirs
                .iter()
                .find(|entry| entry.directory && same_number(leading_number(&entry.name), &number))
                .map(|entry| planning.join("phases").join(&entry.name));
            read_phase(host, &roadmap, number, name, done, directory.as_deref())
        })
        .collect();
    let current = body
        .lines()
        .find_map(|line| line.strip_prefix("Phase:"))
        .map(|rest| leading_number(rest.trim()).to_owned())
        .filter(|number| !number.is_empty())
        .and_then(|number| {
            phases
                .iter()
                .position(|phase| same_number(&phase.number, &number))
        })
        .or_else(|| phases.iter().position(|phase| !phase.done));
    let next = if !phases.is_empty() && phases.iter().all(|phase| phase.done) {
        Ask::of(Step::Audit, None)
    } else {
        current.map_or(Ask::of(Step::Next, None), |at| phases[at].next.clone())
    };
    let count = |key: &str| field(&front, key).and_then(|value| value.parse::<u32>().ok());
    let number = |key: &str| count(key).unwrap_or(0);
    let (plans_done, plans_total) = (
        number("progress.completed_plans"),
        number("progress.total_plans"),
    );
    // Newer GSD writes the plan counts and no percent; count it from them.
    let percent = count("progress.percent").unwrap_or_else(|| {
        (plans_done.min(plans_total) * 100)
            .checked_div(plans_total)
            .unwrap_or(0)
    });
    Some(Milestone {
        name: field(&front, "milestone_name")
            .unwrap_or_default()
            .to_owned(),
        percent: u8::try_from(percent.min(100)).unwrap_or(100),
        plans_done,
        plans_total,
        stopped_at: field(&front, "stopped_at")
            .filter(|value| !value.is_empty() && *value != "null")
            .map(str::to_owned),
        blockers: bullets(body, "blockers"),
        todos: bullets(body, "pending todos").len(),
        version,
        status,
        phases,
        current,
        next,
    })
}

fn read_phase(
    host: &dyn Host,
    roadmap: &str,
    number: String,
    name: String,
    done: bool,
    directory: Option<&Path>,
) -> Phase {
    let section = roadmap_section(roadmap, &number);
    let goal = section
        .iter()
        .find_map(|line| {
            line.trim()
                .strip_prefix("**Goal**")
                .map(|rest| rest.trim_start_matches(':').trim().to_owned())
        })
        .unwrap_or_default();
    let criteria = success_criteria(&section);
    let files: Vec<String> = directory
        .and_then(|directory| host.list_dir(directory).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !entry.directory)
        .map(|entry| entry.name)
        .collect();
    let plans = directory
        .map(|directory| read_plans(host, directory, &files, &section))
        .unwrap_or_default();
    let documents = files
        .iter()
        .filter(|file| {
            ["CONTEXT", "RESEARCH", "UAT", "VERIFICATION", "VALIDATION"]
                .iter()
                .any(|kind| file.ends_with(&format!("-{kind}.md")))
        })
        .cloned()
        .collect();
    let next = if done {
        Ask::of(Step::Next, None)
    } else {
        recommend(&number, &files, &plans)
    };
    Phase {
        number,
        name,
        done,
        goal,
        criteria,
        plans,
        documents,
        next,
    }
}

fn read_plans(host: &dyn Host, directory: &Path, files: &[String], section: &[&str]) -> Vec<Plan> {
    let mut plans: Vec<Plan> = files
        .iter()
        .filter_map(|file| Some((file, plan_id(file, "PLAN.md")?)))
        .map(|(file, id)| {
            let text = host.read_file(&directory.join(file)).unwrap_or_default();
            let front = fields(crate::shared::front_matter::split(&text).0);
            let summary = files
                .iter()
                .find(|other| plan_id(other, "SUMMARY.md") == Some(id))
                .map(|summary| {
                    one_liner(&host.read_file(&directory.join(summary)).unwrap_or_default())
                });
            Plan {
                id: id.to_owned(),
                name: file.trim_end_matches("-PLAN.md").to_owned(),
                wave: field(&front, "wave")
                    .and_then(|wave| wave.parse().ok())
                    .unwrap_or(1),
                mark: if summary.is_some() {
                    Mark::Done
                } else {
                    Mark::Pending
                },
                checkpoint: field(&front, "autonomous") == Some("false"),
                objective: roadmap_objective(section, id).unwrap_or_else(|| objective(&text)),
                summary: summary.unwrap_or_default(),
            }
        })
        .collect();
    plans.sort_by(|a, b| a.wave.cmp(&b.wave).then_with(|| a.id.cmp(&b.id)));
    let first_open = plans
        .iter()
        .filter(|plan| plan.mark != Mark::Done)
        .map(|plan| plan.wave)
        .min();
    for plan in &mut plans {
        if plan.mark == Mark::Pending && Some(plan.wave) == first_open {
            plan.mark = Mark::Current;
        }
    }
    plans
}

/// What GSD would do next with a phase, read from what its directory has.
fn recommend(number: &str, files: &[String], plans: &[Plan]) -> Ask {
    let phase = Some(number);
    if plans.is_empty() {
        let discussed = files.iter().any(|file| file.ends_with("-CONTEXT.md"));
        return Ask::of(if discussed { Step::Plan } else { Step::Discuss }, phase);
    }
    if plans.iter().any(|plan| plan.mark != Mark::Done) {
        return Ask::of(Step::Execute, phase);
    }
    let verified = files
        .iter()
        .any(|file| file.ends_with("-UAT.md") || file.ends_with("-VERIFICATION.md"));
    if verified {
        Ask::of(Step::Next, None)
    } else {
        Ask::of(Step::Verify, phase)
    }
}

/// Whether a milestone's status says it is over.
fn closed(status: &str) -> bool {
    status
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|word| {
            matches!(
                word,
                "complete" | "completed" | "milestone_complete" | "shipped" | "archived"
            )
        })
}

/// Front matter as flat `key: value` pairs, a nested key named
/// `parent.child`. GSD's own state is written this way and no deeper, so
/// this reads it without a YAML parser the crate does not have.
fn fields(front: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    let mut parent: Option<String> = None;
    for line in front.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let indented = line.starts_with([' ', '\t']);
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let value = value.trim().trim_matches(['"', '\'']).to_owned();
        if !indented {
            parent = value.is_empty().then(|| key.to_owned());
            fields.push((key.to_owned(), value));
        } else if let Some(parent) = &parent {
            fields.push((format!("{parent}.{key}"), value));
        }
    }
    fields
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// The phases a roadmap lists, as `(number, name, done)`: its checklist
/// (`- [x] **Phase 2: Import**`), joined by the `### Phase` headings it has
/// no line for — a phase added under Phase Details alone — in number order.
/// A milestone already shipped is folded in `<details>` and is not read.
fn roadmap_phases(roadmap: &str) -> Vec<(String, String, bool)> {
    let visible = without_details(roadmap);
    let mut listed: Vec<_> = visible
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("- [")?;
            let (tick, rest) = rest.split_once(']')?;
            let rest = rest.trim_start().strip_prefix("**Phase")?;
            let (heading, _) = rest.split_once("**")?;
            let (number, name) = phase_heading(heading)?;
            Some((number, name, matches!(tick, "x" | "X")))
        })
        .collect();
    let headed: Vec<_> = visible
        .lines()
        .filter_map(|line| {
            let heading = line.strip_prefix("### Phase")?;
            let (number, name) = phase_heading(heading)?;
            Some((number, name.replace(['`', '*'], ""), false))
        })
        .filter(|(number, _, _)| {
            !listed
                .iter()
                .any(|(other, _, _)| same_number(other, number))
        })
        .collect();
    if headed.is_empty() {
        return listed;
    }
    listed.extend(headed);
    listed.sort_by_key(|(number, _, _)| number_order(number));
    listed
}

/// A phase number as its parts, for ordering: `2.10` after `2.9`.
fn number_order(number: &str) -> Vec<u32> {
    number_parts(number).unwrap_or_default()
}

/// `02.1` → `[2, 1]`; `None` where a part is not a number.
fn number_parts(number: &str) -> Option<Vec<u32>> {
    number.split('.').map(|part| part.parse().ok()).collect()
}

/// ` 2.1: Import` → `("2.1", "Import")`.
fn phase_heading(heading: &str) -> Option<(String, String)> {
    let heading = heading.trim_start();
    let number = leading_number(heading);
    if number.is_empty() {
        return None;
    }
    let name = heading[number.len()..].trim_start_matches(':').trim();
    Some((number.to_owned(), name.to_owned()))
}

fn without_details(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("<details>") {
        kept.push_str(&rest[..open]);
        rest = match rest[open..].find("</details>") {
            Some(close) => &rest[open + close + "</details>".len()..],
            None => "",
        };
    }
    kept.push_str(rest);
    kept
}

/// The digits and dots a name opens with: `03-sync-engine` → `03`.
fn leading_number(text: &str) -> &str {
    let end = text
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(text.len());
    text[..end].trim_end_matches('.')
}

/// `03` and `3` are one phase; `2.1` and `2.10` are not.
fn same_number(a: &str, b: &str) -> bool {
    match (number_parts(a), number_parts(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// A plan's number from its file name: `03-02-PLAN.md` → `02`.
fn plan_id<'a>(file: &'a str, suffix: &str) -> Option<&'a str> {
    let stem = file.strip_suffix(suffix)?.strip_suffix('-')?;
    let (phase, id) = stem.rsplit_once('-')?;
    let numeric =
        |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit() || c == '.');
    (numeric(phase) && numeric(id) && !id.contains('.')).then_some(id)
}

/// The roadmap's `### Phase N:` block, heading included, up to the next
/// heading of that level or above.
fn roadmap_section<'a>(roadmap: &'a str, number: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = roadmap.lines().collect();
    let Some(start) = lines.iter().position(|line| {
        line.strip_prefix("### Phase")
            .and_then(phase_heading)
            .is_some_and(|(found, _)| same_number(&found, number))
    }) else {
        return Vec::new();
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ")
        })
        .map_or(lines.len(), |at| start + 1 + at);
    lines[start..end].to_vec()
}

/// The numbered list under the section's "Success Criteria".
fn success_criteria(section: &[&str]) -> Vec<String> {
    let Some(at) = section
        .iter()
        .position(|line| line.to_lowercase().contains("success criteria"))
    else {
        return Vec::new();
    };
    let mut criteria = Vec::new();
    for line in &section[at + 1..] {
        let trimmed = line.trim();
        let item = trimmed
            .split_once(". ")
            .filter(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
            .map(|(_, item)| item.replace('`', ""));
        match item {
            Some(item) => criteria.push(item),
            None if !criteria.is_empty() && !trimmed.is_empty() => break,
            None => {}
        }
    }
    criteria
}

/// What the roadmap says a plan is for: `- [ ] 03-02-PLAN.md — Conflicts`.
fn roadmap_objective(section: &[&str], id: &str) -> Option<String> {
    section.iter().find_map(|line| {
        let at = line.find("-PLAN.md")?;
        plan_id(
            line[..at + "-PLAN.md".len()].rsplit([' ', '`']).next()?,
            "PLAN.md",
        )
        .filter(|found| *found == id)?;
        let rest = line[at + "-PLAN.md".len()..].trim_start_matches('`').trim();
        let rest = rest.trim_start_matches(['—', '–', '-']).trim();
        (!rest.is_empty()).then(|| rest.to_owned())
    })
}

/// The first line of a plan's `<objective>`.
fn objective(plan: &str) -> String {
    plan.split_once("<objective>")
        .and_then(|(_, rest)| rest.split_once("</objective>"))
        .and_then(|(objective, _)| {
            objective
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
        })
        .unwrap_or_default()
        .to_owned()
}

/// What a summary says it did in one line: its `## One-liner`, else its
/// first line in bold.
fn one_liner(summary: &str) -> String {
    let (_, body) = crate::shared::front_matter::split(summary);
    let lines: Vec<&str> = body.lines().collect();
    let after_heading = lines
        .iter()
        .position(|line| {
            let line = line.trim().to_lowercase();
            line.starts_with("##")
                && line.trim_start_matches('#').trim().replace('-', " ") == "one liner"
        })
        .and_then(|at| {
            lines[at + 1..]
                .iter()
                .map(|line| line.trim())
                .find(|line| !line.is_empty())
        });
    let bold = lines.iter().find_map(|line| {
        line.trim()
            .strip_prefix("**")
            .and_then(|rest| rest.strip_suffix("**"))
    });
    after_heading.or(bold).unwrap_or_default().to_owned()
}

/// The bullets under the first heading naming `heading`, up to the next
/// heading, leaving out the "None" a template writes in an empty list.
fn bullets(text: &str, heading: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(at) = lines.iter().position(|line| {
        let level = line.chars().take_while(|c| *c == '#').count();
        (2..=4).contains(&level) && line.to_lowercase().contains(heading)
    }) else {
        return Vec::new();
    };
    lines[at + 1..]
        .iter()
        .take_while(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let item = line.trim_start();
            item.strip_prefix("- ").or_else(|| item.strip_prefix("* "))
        })
        .map(|item| item.replace(['`'], "").replace("**", "").trim().to_owned())
        .filter(|item| {
            let lower = item.to_lowercase();
            !lower.starts_with("none") && !lower.is_empty()
        })
        .collect()
}

/// The milestone at a glance, as a document the surface renders.
/// The milestone as a document the surface renders, laid out for `width`
/// columns.
pub fn overview(milestone: &Milestone, width: usize) -> String {
    let mut text = format!("# {}", milestone.version);
    if !milestone.name.is_empty() {
        text.push_str(&format!(" · {}", milestone.name));
    }
    let mut line = vec![format!("**{}**", milestone.status)];
    line.push(format!("{}%", milestone.percent));
    if milestone.plans_total > 0 {
        line.push(format!(
            "{} of {} plans",
            milestone.plans_done, milestone.plans_total
        ));
    }
    text.push_str(&format!("\n\n{}\n", line.join(" · ")));
    if let Some(stopped_at) = &milestone.stopped_at {
        text.push_str(&format!("\nStopped at: {stopped_at}\n"));
    }
    text.push_str("\n---\n");
    if !milestone.phases.is_empty() {
        text.push_str("\n## Phases\n\n| Phase | Name | Plans | State |\n|--:|---|:-:|---|\n");
        for (at, phase) in milestone.phases.iter().enumerate() {
            let current = milestone.current == Some(at);
            // The current phase is the row the eye looks for first.
            let strong = |text: &str| {
                if current {
                    format!("**{text}**")
                } else {
                    text.to_owned()
                }
            };
            text.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                strong(&phase.number),
                strong(&cell(&phase.name)),
                plans_done_of_all(phase),
                strong(standing(phase, current)),
            ));
        }
    }
    if let Some(phase) = milestone.current.map(|at| &milestone.phases[at])
        && !phase.plans.is_empty()
    {
        text.push_str(&format!("\n## Phase {} · {}\n", phase.number, phase.name));
        push_plans(&mut text, phase, width);
    }
    if !milestone.blockers.is_empty() {
        text.push_str("\n## Blockers\n\n");
        for blocker in &milestone.blockers {
            text.push_str(&format!("- {blocker}\n"));
        }
    }
    if milestone.todos > 0 {
        let noun = if milestone.todos == 1 {
            "todo"
        } else {
            "todos"
        };
        text.push_str(&format!("\n{} pending {noun}\n", milestone.todos));
    }
    text.push_str(&format!("\n## Next\n\n`{}`\n", milestone.next.words()));
    text
}

/// One phase in full, as a document the surface renders for `width`
/// columns: where it stands and what runs next first, then its goal,
/// criteria and plans, each set apart.
pub fn detail(phase: &Phase, width: usize) -> String {
    let mut text = format!("# Phase {} · {}\n", phase.number, phase.name);
    let mut line = vec![format!("**{}**", standing(phase, false))];
    if !phase.plans.is_empty() {
        line.push(format!(
            "{} of {} plans",
            phase.finished_plans(),
            phase.plans.len()
        ));
    }
    if !phase.done {
        line.push(format!("next `{}`", phase.next.words()));
    }
    text.push_str(&format!("\n{}\n\n---\n", line.join(" · ")));
    if !phase.goal.is_empty() {
        text.push_str(&format!("\n## Goal\n\n{}\n", phase.goal));
    }
    if !phase.criteria.is_empty() {
        text.push_str("\n## Success criteria\n");
        // A paragraph each rather than a list: a criterion runs to several
        // lines, and the blank line between them is what tells one from the
        // next. A loose list would put the number on a line of its own.
        for (at, criterion) in phase.criteria.iter().enumerate() {
            text.push_str(&format!("\n**{}.** {criterion}\n", at + 1));
        }
    }
    if !phase.plans.is_empty() {
        text.push_str("\n---\n\n## Plans\n");
        push_plans(&mut text, phase, width);
    }
    if !phase.documents.is_empty() {
        text.push_str("\n## Documents\n\n");
        for document in &phase.documents {
            text.push_str(&format!("- {document}\n"));
        }
    }
    text
}

/// A phase's plans as one table, a row each: the wave named on its first
/// plan only, a plan by the first sentence of what it delivered (or, not
/// run yet, of what it sets out to do), and its mark. The sentence is cut
/// to the room `width` leaves once the other columns are drawn, so the
/// table spans the surface and no row folds.
fn push_plans(text: &mut String, phase: &Phase, width: usize) {
    let mut wave = None;
    let rows: Vec<(String, &Plan, &str, String)> = phase
        .plans
        .iter()
        .map(|plan| {
            let wave_cell = if wave == Some(plan.wave) {
                String::new()
            } else {
                wave = Some(plan.wave);
                plan.wave.to_string()
            };
            let said = if plan.mark == Mark::Done && !plan.summary.is_empty() {
                plan.summary.as_str()
            } else {
                plan.objective.as_str()
            };
            let mut status = match plan.mark {
                Mark::Done => "done",
                Mark::Current => "current",
                Mark::Pending => "pending",
            }
            .to_owned();
            if plan.checkpoint && plan.mark != Mark::Done {
                status.push_str(" · stops for you");
            }
            (wave_cell, plan, said, status)
        })
        .collect();
    let column = |header: &str, cells: &mut dyn Iterator<Item = usize>| {
        cells.chain([header.chars().count()]).max().unwrap_or(0)
    };
    let wave_width = column("Wave", &mut rows.iter().map(|row| row.0.chars().count()));
    let plan_width = column("Plan", &mut rows.iter().map(|row| row.1.id.chars().count()));
    let status_width = column("Status", &mut rows.iter().map(|row| row.3.chars().count()));
    // The markdown table's own borders: `│ ` before each cell, ` │` after
    // the last and ` ` between, for four columns.
    const BORDERS: usize = 3 * 4 + 1;
    let room = width
        .saturating_sub(BORDERS + wave_width + plan_width + status_width)
        .max(MIN_OBJECTIVE);
    text.push_str("\n| Wave | Plan | Objective | Status |\n|:-:|:-:|---|---|\n");
    for (wave_cell, plan, said, status) in rows {
        let status = match plan.mark {
            Mark::Current => status.replacen("current", "**current**", 1),
            _ => status,
        }
        .replacen("stops for you", "*stops for you*", 1);
        text.push_str(&format!(
            "| {wave_cell} | {} | {} | {status} |\n",
            plan.id,
            cell(&headline(said, room))
        ));
    }
}

/// The narrowest an objective is cut to, however little room is left: a
/// few words still say which plan it is.
const MIN_OBJECTIVE: usize = 16;

/// Where a phase stands, in a word or two.
fn standing(phase: &Phase, current: bool) -> &'static str {
    if phase.done {
        "done"
    } else if current {
        "current"
    } else if phase.plans.is_empty() {
        "not started"
    } else {
        "in progress"
    }
}

fn plans_done_of_all(phase: &Phase) -> String {
    if phase.plans.is_empty() {
        "—".to_owned()
    } else {
        format!("{}/{}", phase.finished_plans(), phase.plans.len())
    }
}

/// A plan's text cut to its first sentence, and that to `width`
/// columns: GSD's objectives and summaries run to a paragraph, and a table
/// of them folds every row into several. A colon introduces what follows
/// (`WR-06: the ledger…`) rather than ending the sentence.
fn headline(text: &str, width: usize) -> String {
    let sentence = text
        .char_indices()
        .find(|&(at, c)| {
            matches!(c, '.' | ';')
                && text[at + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
        })
        .map_or(text, |(at, c)| &text[..at + c.len_utf8()])
        .trim();
    if sentence.chars().count() <= width {
        return sentence.to_owned();
    }
    // One column is the ellipsis'. A cut that lands on a word's end keeps
    // the word; one inside a word goes back to the space before it.
    let keep = width.saturating_sub(1);
    let cut: String = sentence.chars().take(keep).collect();
    let on_a_space = sentence.chars().nth(keep).is_some_and(char::is_whitespace);
    let cut = if on_a_space {
        cut.as_str()
    } else {
        cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head)
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

/// Text made safe for one Markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::DiskHost;
    use uze_testkit::temp::TempDir;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    const STATE: &str = "---\nmilestone: v1.9\nmilestone_name: Sync\nstatus: executing\n\
                         stopped_at: Completed 03-01-PLAN.md\nprogress:\n  total_plans: 4\n  \
                         completed_plans: 2\n  percent: 50\n---\n\n# Project State\n\n\
                         Phase: 3 of 4 (Sync Engine)\n\n### Blockers/Concerns\n\n\
                         - Waiting on the API key\n\n### Pending Todos\n\n- One\n- Two\n";

    const ROADMAP: &str = "# Roadmap\n\n<details>\n<summary>v1.0</summary>\n\n\
                           - [x] **Phase 1: Foundation**\n</details>\n\n\
                           - [x] **Phase 2: Import**\n- [ ] **Phase 3: Sync Engine**\n\
                           - [ ] **Phase 4: Sharing**\n\n## Phase Details\n\n\
                           ### Phase 3: Sync Engine\n**Goal**: Offline saves reach the server\n\
                           **Success Criteria** (what must be TRUE):\n  1. A save made offline syncs\n  \
                           2. Conflicts keep both\n**Plans**: 3 plans\n\nPlans:\n\
                           - [x] 03-01-PLAN.md — Change log\n- [ ] 03-02-PLAN.md — Conflicts\n\n\
                           ### Phase 4: Sharing\n**Goal**: Links\n";

    fn project(label: &str) -> TempDir {
        let dir = TempDir::new(label);
        let root = dir.path();
        write(root, ".planning/STATE.md", STATE);
        write(root, ".planning/ROADMAP.md", ROADMAP);
        let plan = |wave: u32, autonomous: bool| {
            format!(
                "---\nwave: {wave}\nautonomous: {autonomous}\n---\n\n<objective>\nFrom the plan.\n</objective>\n"
            )
        };
        let phase = ".planning/phases/03-sync-engine";
        write(root, &format!("{phase}/03-CONTEXT.md"), "# Context\n");
        write(root, &format!("{phase}/03-01-PLAN.md"), &plan(1, true));
        write(
            root,
            &format!("{phase}/03-01-SUMMARY.md"),
            "---\nphase: 03\n---\n\n# Summary\n\n**Every write is logged**\n",
        );
        write(root, &format!("{phase}/03-02-PLAN.md"), &plan(2, true));
        write(root, &format!("{phase}/03-03-PLAN.md"), &plan(2, false));
        write(
            root,
            ".planning/phases/02-import/02-01-PLAN.md",
            &plan(1, true),
        );
        write(
            root,
            ".planning/phases/02-import/02-01-SUMMARY.md",
            "**Done**\n",
        );
        write(
            root,
            ".planning/phases/04-sharing/04-CONTEXT.md",
            "# Context\n",
        );
        dir
    }

    #[test]
    fn the_open_milestone_is_read_from_its_state_and_roadmap() {
        let dir = project("gsd-milestone");
        let milestone = read(&DiskHost, dir.path()).expect("an open milestone");
        assert_eq!(milestone.version, "v1.9");
        assert_eq!(milestone.name, "Sync");
        assert_eq!(
            (
                milestone.percent,
                milestone.plans_done,
                milestone.plans_total
            ),
            (50, 2, 4)
        );
        assert_eq!(
            milestone.stopped_at.as_deref(),
            Some("Completed 03-01-PLAN.md")
        );
        assert_eq!(milestone.blockers, ["Waiting on the API key"]);
        assert_eq!(milestone.todos, 2);
        let numbers: Vec<_> = milestone
            .phases
            .iter()
            .map(|phase| phase.number.as_str())
            .collect();
        assert_eq!(
            numbers,
            ["2", "3", "4"],
            "a shipped milestone's phases stay folded away"
        );
        assert_eq!(milestone.current, Some(1));
        assert_eq!(milestone.next.words(), "gsd-execute-phase 3");
    }

    #[test]
    fn a_phase_carries_its_goal_criteria_and_plans_by_wave() {
        let dir = project("gsd-phase");
        let milestone = read(&DiskHost, dir.path()).unwrap();
        let phase = &milestone.phases[1];
        assert_eq!(phase.goal, "Offline saves reach the server");
        assert_eq!(
            phase.criteria,
            ["A save made offline syncs", "Conflicts keep both"]
        );
        let plans: Vec<_> = phase
            .plans
            .iter()
            .map(|plan| (plan.id.as_str(), plan.wave, plan.mark, plan.checkpoint))
            .collect();
        assert_eq!(
            plans,
            [
                ("01", 1, Mark::Done, false),
                ("02", 2, Mark::Current, false),
                ("03", 2, Mark::Current, true),
            ]
        );
        assert_eq!(
            phase.plans[0].objective, "Change log",
            "the roadmap names it first"
        );
        assert_eq!(phase.plans[2].objective, "From the plan.");
        assert_eq!(phase.plans[0].summary, "Every write is logged");
        assert_eq!(phase.documents, ["03-CONTEXT.md"]);
    }

    #[test]
    fn each_phase_recommends_the_step_its_files_are_ready_for() {
        let dir = project("gsd-next");
        let milestone = read(&DiskHost, dir.path()).unwrap();
        let next: Vec<_> = milestone
            .phases
            .iter()
            .map(|phase| phase.next.words())
            .collect();
        assert_eq!(
            next,
            ["gsd-next", "gsd-execute-phase 3", "gsd-plan-phase 4"]
        );
        assert_eq!(
            recommend("5", &[], &[]).words(),
            "gsd-discuss-phase 5",
            "nothing discussed yet"
        );
    }

    #[test]
    fn a_closed_or_missing_milestone_is_not_read() {
        let dir = TempDir::new("gsd-closed");
        assert!(read(&DiskHost, dir.path()).is_none());
        write(
            dir.path(),
            ".planning/STATE.md",
            "---\nmilestone: v1.0\nstatus: milestone_complete\n---\n",
        );
        assert!(read(&DiskHost, dir.path()).is_none());
        write(
            dir.path(),
            ".planning/STATE.md",
            "---\nmilestone: v1.3\nstatus: completed\n---\n",
        );
        assert!(
            read(&DiskHost, dir.path()).is_none(),
            "GSD writes `completed` once the milestone is done"
        );
        write(
            dir.path(),
            ".planning/STATE.md",
            "---\nstatus: executing\n---\n",
        );
        assert!(read(&DiskHost, dir.path()).is_none(), "no milestone named");
    }

    #[test]
    fn a_phase_with_only_a_heading_is_read_with_the_listed_ones() {
        let dir = project("gsd-heading-only");
        let roadmap = ROADMAP.replace(
            "### Phase 4: Sharing\n**Goal**: Links\n",
            "### Phase 4: Sharing\n**Goal**: Links\n\n\
             ### Phase 3.1: Hotfix\n**Goal**: Patch\n\n\
             ### Phase 5: Public Links\n**Goal**: By PIN\n",
        );
        write(dir.path(), ".planning/ROADMAP.md", &roadmap);
        write(
            dir.path(),
            ".planning/STATE.md",
            &STATE.replace("Phase: 3 of 4 (Sync Engine)", "Phase: 5 (Public Links)"),
        );
        let milestone = read(&DiskHost, dir.path()).unwrap();
        let phases: Vec<_> = milestone
            .phases
            .iter()
            .map(|phase| (phase.number.as_str(), phase.name.as_str(), phase.done))
            .collect();
        assert_eq!(
            phases,
            [
                ("2", "Import", true),
                ("3", "Sync Engine", false),
                ("3.1", "Hotfix", false),
                ("4", "Sharing", false),
                ("5", "Public Links", false),
            ],
            "a phase added under Phase Details without a checklist line"
        );
        assert_eq!(milestone.current, Some(4));
        assert_eq!(milestone.phases[4].goal, "By PIN");
        assert_eq!(milestone.next.words(), "gsd-discuss-phase 5");
    }

    #[test]
    fn a_missing_percent_is_counted_from_the_plans() {
        let dir = project("gsd-percent");
        write(
            dir.path(),
            ".planning/STATE.md",
            &STATE.replace("  percent: 50\n", "").replace(
                "total_plans: 4\n  completed_plans: 2",
                "total_plans: 65\n  completed_plans: 64",
            ),
        );
        let milestone = read(&DiskHost, dir.path()).unwrap();
        assert_eq!(
            (
                milestone.percent,
                milestone.plans_done,
                milestone.plans_total
            ),
            (98, 64, 65)
        );
        write(
            dir.path(),
            ".planning/STATE.md",
            &STATE.replace("  percent: 50\n", "").replace(
                "total_plans: 4\n  completed_plans: 2",
                "total_plans: 0\n  completed_plans: 0",
            ),
        );
        assert_eq!(read(&DiskHost, dir.path()).unwrap().percent, 0);
    }

    #[test]
    fn phase_numbers_match_part_by_part() {
        assert!(same_number("03", "3"));
        assert!(same_number("02.1", "2.1"));
        assert!(
            !same_number("2.1", "2.10"),
            "an eleventh insert is not the first"
        );
        assert!(!same_number("2", "2.0"));
        assert!(!same_number("", ""), "no number is no phase");
    }

    #[test]
    fn every_phase_done_asks_for_the_audit() {
        let dir = project("gsd-audit");
        write(
            dir.path(),
            ".planning/ROADMAP.md",
            "- [x] **Phase 1: One**\n- [x] **Phase 2: Two**\n",
        );
        let milestone = read(&DiskHost, dir.path()).unwrap();
        assert_eq!(milestone.next.words(), "gsd-audit-milestone");
    }

    #[test]
    fn the_overview_tabulates_the_phases_and_the_current_plans() {
        let dir = project("gsd-documents");
        let milestone = read(&DiskHost, dir.path()).unwrap();
        let overview = overview(&milestone, WIDE);
        assert!(overview.starts_with(
            "# v1.9 · Sync\n\n**executing** · 50% · 2 of 4 plans\n\n\
             Stopped at: Completed 03-01-PLAN.md\n\n---\n"
        ));
        assert!(overview.contains(
            "| Phase | Name | Plans | State |\n|--:|---|:-:|---|\n\
             | 2 | Import | 1/1 | done |\n\
             | **3** | **Sync Engine** | 1/3 | **current** |\n\
             | 4 | Sharing | — | not started |\n"
        ));
        assert!(overview.contains(
            "## Phase 3 · Sync Engine\n\n\
             | Wave | Plan | Objective | Status |\n|:-:|:-:|---|---|\n\
             | 1 | 01 | Every write is logged | done |\n\
             | 2 | 02 | Conflicts | **current** |\n\
             |  | 03 | From the plan. | **current** · *stops for you* |\n"
        ));
        assert!(overview.contains("- Waiting on the API key"));
        assert!(overview.ends_with("## Next\n\n`gsd-execute-phase 3`\n"));
    }

    /// Wide enough that no plan in the fixture is cut.
    const WIDE: usize = 200;

    #[test]
    fn a_phase_says_where_it_stands_above_its_plans() {
        let dir = project("gsd-detail");
        let milestone = read(&DiskHost, dir.path()).unwrap();
        let detail = detail(&milestone.phases[1], WIDE);
        assert!(detail.starts_with(
            "# Phase 3 · Sync Engine\n\n\
             **in progress** · 1 of 3 plans · next `gsd-execute-phase 3`\n\n---\n"
        ));
        assert!(
            detail.contains("\n## Goal\n\nOffline saves reach the server\n"),
            "the goal under a heading of its own"
        );
        assert!(
            detail.contains("**1.** A save made offline syncs\n\n**2.** Conflicts keep both\n"),
            "a blank line between criteria"
        );
        assert!(detail.contains("\n---\n\n## Plans\n"));
        assert!(detail.contains("| 1 | 01 | Every write is logged | done |"));
        assert!(!detail.contains("## Next"), "the next step heads the phase");
        assert!(
            super::detail(&milestone.phases[2], WIDE)
                .starts_with("# Phase 4 · Sharing\n\n**not started** · next `gsd-plan-phase 4`\n")
        );
        assert!(
            super::detail(&milestone.phases[0], WIDE)
                .starts_with("# Phase 2 · Import\n\n**done** · 1 of 1 plans\n")
        );
    }

    #[test]
    fn a_plan_takes_the_room_the_table_leaves_it() {
        let dir = project("gsd-room");
        let mut milestone = read(&DiskHost, dir.path()).unwrap();
        let phase = &mut milestone.phases[1];
        phase.plans[1].objective =
            "Conflicting saves keep both versions and ask the owner which one stays".to_owned();
        // 80 columns, less the table's 13 of borders and the Wave (4), Plan
        // (4) and Status (23, `current · stops for you`) columns: 36.
        let detail = detail(phase, 80);
        assert!(
            detail.contains("| 2 | 02 | Conflicting saves keep both… | **current** |"),
            "{detail}"
        );
        assert!(
            super::detail(phase, usize::MAX).contains(
                "| 2 | 02 | Conflicting saves keep both versions and ask the owner which one stays |"
            ),
            "no width to fit means nothing is cut"
        );
    }

    #[test]
    fn a_plan_is_named_by_its_first_sentence() {
        assert_eq!(headline("Change log", 60), "Change log");
        assert_eq!(
            headline(
                "Pedido vira CONVERTED na hora. A exportação LGPD lista as sacolas.",
                60
            ),
            "Pedido vira CONVERTED na hora."
        );
        assert_eq!(
            headline("R$ 123.456,78 em 32px", 60),
            "R$ 123.456,78 em 32px"
        );
        assert_eq!(
            headline("WR-06/SC4: o ledger grava o custo. Depois o resto.", 60),
            "WR-06/SC4: o ledger grava o custo.",
            "a colon introduces the sentence rather than ending it"
        );
        assert_eq!(
            headline(
                "GET /public/:storeSlug/cart-restore/:token devolve a sacola revalidada pelo cardápio de hoje",
                60
            ),
            "GET /public/:storeSlug/cart-restore/:token devolve a sacola…"
        );
        assert_eq!(cell("a | b"), "a \\| b", "a pipe would split the cell");
    }
}
