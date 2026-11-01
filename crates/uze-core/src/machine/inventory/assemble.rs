//! Every harness's answer made into one Overview.

use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    capability::harness::frontmatter_of,
    delivery::{exposure::ManagedArtifact, integration::AttachmentReceipt},
};

use super::{
    health,
    model::{
        Claim, Definition, FoundResource, HarnessInventory, HarnessReport, Location,
        MachineOverview, Origin, OverviewResource, Runs, Source, Status,
    },
    tokens::{estimate_from_size, estimate_tokens},
};

/// Past this size a definition is measured by its size, never read whole.
const READ_LIMIT: u64 = 1024 * 1024;

/// The Overview of `harnesses`, owned as `receipts` say.
pub fn assemble(
    harnesses: Vec<HarnessInventory>,
    receipts: &[AttachmentReceipt],
) -> MachineOverview {
    let mut reports = Vec::new();
    let mut found = Vec::new();
    for harness in harnesses {
        reports.push(HarnessReport {
            id: harness.id.clone(),
            name: harness.name.clone(),
            inventory_known: harness.found.is_some(),
            sessions: harness.sessions,
            uses_known: false,
            uses_since: None,
        });
        for resource in harness.found.into_iter().flatten() {
            let origin = origin_of(&resource, &harness.id, receipts);
            found.push(Measured::of(resource, harness.id.clone(), origin));
        }
    }
    MachineOverview {
        harnesses: reports,
        resources: rows(found),
    }
}

/// One harness's resource with everything read from its files.
struct Measured {
    found: FoundResource,
    harness: String,
    origin: Origin,
    description: Option<String>,
    per_session: Option<u32>,
    when_used: Option<u32>,
    digest: String,
    problem: Option<String>,
    checked: bool,
}

impl Measured {
    fn of(found: FoundResource, harness: String, origin: Origin) -> Self {
        let read = read_definition(&found.definition);
        let problem = found
            .problem
            .clone()
            .or(read.problem)
            .or_else(|| found.runs.as_ref().and_then(health::runs_problem));
        let digest = match (&read.content, &found.runs) {
            (Some(content), _) => crate::digest::sha256(content.as_bytes()),
            (None, Some(Runs::Shell(line) | Runs::Program(line) | Runs::Url(line))) => {
                crate::digest::sha256(line.as_bytes())
            }
            (None, None) => String::new(),
        };
        let checked = !matches!(found.definition, Definition::Entry(_))
            || matches!(found.runs, Some(Runs::Shell(_) | Runs::Program(_)))
            || found.problem.is_some();
        Self {
            checked,
            description: read.description.or_else(|| found.note.clone()),
            per_session: read.per_session,
            when_used: read.when_used,
            found,
            harness,
            origin,
            digest,
            problem,
        }
    }

    /// What merges this resource with the same one on another harness.
    fn merge_key(&self) -> (u8, String, String, String) {
        let kind = self.found.kind as u8;
        let name = self.found.name.clone();
        match &self.origin {
            Origin::Uze { package } => (kind, name, format!("uze:{package}"), String::new()),
            _ => (kind, name, String::from("other"), self.digest.clone()),
        }
    }

    fn location(&self) -> Location {
        Location {
            harness: self.harness.clone(),
            path: self.found.definition.path().clone(),
        }
    }
}

#[derive(Default)]
struct DefinitionRead {
    content: Option<String>,
    description: Option<String>,
    per_session: Option<u32>,
    when_used: Option<u32>,
    problem: Option<String>,
}

fn read_definition(definition: &Definition) -> DefinitionRead {
    match definition {
        Definition::Entry(_) => DefinitionRead::default(),
        Definition::Listed {
            per_session,
            when_used,
            ..
        } => DefinitionRead {
            content: Some(format!(
                "{per_session}\n{}",
                when_used.as_deref().unwrap_or("")
            )),
            per_session: Some(estimate_tokens(per_session)),
            when_used: when_used.as_deref().map(estimate_tokens),
            ..DefinitionRead::default()
        },
        Definition::Whole(path) => match read_bounded(path) {
            Ok(Read::Text(text)) => DefinitionRead {
                per_session: Some(estimate_tokens(&text)),
                content: Some(text),
                ..DefinitionRead::default()
            },
            Ok(Read::TooLarge(bytes)) => DefinitionRead {
                per_session: Some(estimate_from_size(bytes)),
                content: Some(format!("{bytes}")),
                ..DefinitionRead::default()
            },
            Err(problem) => DefinitionRead {
                problem: Some(problem),
                ..DefinitionRead::default()
            },
        },
        Definition::Markdown(path) => match read_bounded(path) {
            Ok(Read::Text(text)) => markdown(text),
            Ok(Read::TooLarge(bytes)) => DefinitionRead {
                when_used: Some(estimate_from_size(bytes)),
                content: Some(format!("{bytes}")),
                ..DefinitionRead::default()
            },
            Err(problem) => DefinitionRead {
                problem: Some(problem),
                ..DefinitionRead::default()
            },
        },
    }
}

/// A frontmatter's `name` and `description` are listed every session; the
/// rest loads when the resource is used.
fn markdown(text: String) -> DefinitionRead {
    let Some((frontmatter, body)) = frontmatter_of(&text) else {
        return DefinitionRead {
            problem: Some("invalid frontmatter".to_owned()),
            content: Some(text),
            ..DefinitionRead::default()
        };
    };
    let field = |key: &str| {
        frontmatter
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    let description = field("description");
    let listed = [field("name"), description.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    let rest = body.trim();
    DefinitionRead {
        per_session: Some(estimate_tokens(&listed)),
        when_used: (!rest.is_empty()).then(|| estimate_tokens(rest)),
        description: description.map(|text| first_sentence_line(&text)),
        content: Some(text.clone()),
        problem: None,
    }
}

fn first_sentence_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

enum Read {
    Text(String),
    TooLarge(u64),
}

fn read_bounded(path: &Path) -> Result<Read, String> {
    let metadata = fs::metadata(path).map_err(|_| "cannot be read".to_owned())?;
    if metadata.len() > READ_LIMIT {
        return Ok(Read::TooLarge(metadata.len()));
    }
    let bytes = fs::read(path).map_err(|_| "cannot be read".to_owned())?;
    String::from_utf8(bytes)
        .map(Read::Text)
        .map_err(|_| "cannot be read".to_owned())
}

/// Whether a uze receipt of `harness` owns `found`, and which package.
fn origin_of(found: &FoundResource, harness: &str, receipts: &[AttachmentReceipt]) -> Origin {
    let path = found.definition.path();
    let owner = receipts
        .iter()
        .filter(|receipt| receipt.integration == harness)
        .find(|receipt| receipt_owns(&receipt.artifact, path, found));
    if let Some(receipt) = owner {
        return Origin::Uze {
            package: receipt.package_id.clone(),
        };
    }
    match &found.source {
        Source::Plugin { name } => Origin::Plugin { name: name.clone() },
        Source::BuiltIn => Origin::BuiltIn,
        Source::ByHand => Origin::You,
    }
}

fn receipt_owns(artifact: &ManagedArtifact, definition: &Path, found: &FoundResource) -> bool {
    let claim = found.claim.as_ref();
    match artifact {
        ManagedArtifact::SymlinkReference { path, .. }
        | ManagedArtifact::GeneratedFile { path, .. }
        | ManagedArtifact::GeneratedTree { path, .. }
        | ManagedArtifact::ManagedHookFile { path } => definition.starts_with(path),
        ManagedArtifact::VendorConfigEntry { entry_name, .. } => {
            matches!(claim, Some(Claim::Entry(name)) if name == entry_name)
        }
        // A hook entry is known by the wrapper it runs, which uze generated.
        ManagedArtifact::HookConfigEntry {
            entry_name,
            wrapper,
            ..
        } => {
            matches!(claim, Some(Claim::Entry(name)) if name == entry_name)
                || matches!(&found.runs, Some(Runs::Shell(line) | Runs::Program(line))
                    if line.contains(&*wrapper.to_string_lossy()))
        }
        ManagedArtifact::IntegrationOwned { kind, selector, .. } => matches!(
            claim,
            Some(Claim::Owned { kind: k, selector: s }) if k == kind && s == selector
        ),
        ManagedArtifact::ManagedTextRegion { .. } => false,
    }
}

/// One row per resource: merged across harnesses, then judged against
/// every other resource of its kind and name.
fn rows(found: Vec<Measured>) -> Vec<OverviewResource> {
    let mut merged: BTreeMap<(u8, String, String, String), Vec<Measured>> = BTreeMap::new();
    for measured in found {
        merged
            .entry(measured.merge_key())
            .or_default()
            .push(measured);
    }
    let groups: Vec<Vec<Measured>> = merged.into_values().collect();
    let mut by_name: BTreeMap<(u8, String), Vec<usize>> = BTreeMap::new();
    for (index, group) in groups.iter().enumerate() {
        let first = &group[0];
        by_name
            .entry((first.found.kind as u8, first.found.name.clone()))
            .or_default()
            .push(index);
    }
    let locations: Vec<Vec<Location>> = groups
        .iter()
        .map(|group| group.iter().map(Measured::location).collect())
        .collect();
    let mut rows: Vec<OverviewResource> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| {
            let first = &group[0];
            let siblings = &by_name[&(first.found.kind as u8, first.found.name.clone())];
            // Two entries of one configuration file (two hooks in one
            // `settings.json`) are neighbours, not copies of each other.
            let own_paths: Vec<_> = locations[index].iter().map(|l| &l.path).collect();
            let others: Vec<Location> = siblings
                .iter()
                .filter(|other| **other != index)
                .flat_map(|other| locations[*other].clone())
                .filter(|location| !own_paths.contains(&&location.path))
                .collect();
            let status = match group.iter().find_map(|measured| measured.problem.clone()) {
                Some(reason) => Status::Broken { reason },
                None if !others.is_empty() => Status::Duplicate { others },
                None => Status::Ok,
            };
            OverviewResource {
                id: row_id(first),
                kind: first.found.kind,
                name: first.found.name.clone(),
                origin: first.origin.clone(),
                locations: locations[index].clone(),
                description: group
                    .iter()
                    .find_map(|measured| measured.description.clone()),
                per_session: group
                    .iter()
                    .filter_map(|measured| measured.per_session)
                    .max(),
                when_used: group.iter().filter_map(|measured| measured.when_used).max(),
                status,
                checked: group.iter().all(|measured| measured.checked),
                removal: group
                    .iter()
                    .find_map(|measured| measured.found.removal.clone()),
                uses: Default::default(),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.status.rank().cmp(&b.status.rank()))
            .then(b.per_session.cmp(&a.per_session))
            .then(a.name.cmp(&b.name))
    });
    distinct_ids(&mut rows);
    rows
}

/// Two copies of one name in one harness share an id; the later ones are
/// told apart by their place in the order, which is stable for a machine
/// that did not change.
fn distinct_ids(rows: &mut [OverviewResource]) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        let count = seen.entry(row.id.clone()).or_default();
        *count += 1;
        if *count > 1 {
            row.id = format!("{}#{count}", row.id);
        }
    }
}

/// Stable across reinstalls and upgrades: the package for what uze
/// delivered, the harness for the rest; never a path or a version.
fn row_id(measured: &Measured) -> String {
    let kind = serde_json::to_value(measured.found.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    match &measured.origin {
        Origin::Uze { package } => format!("uze:{package}:{kind}:{}", measured.found.name),
        _ => format!("{}:{kind}:{}", measured.harness, measured.found.name),
    }
}

#[cfg(test)]
mod tests;
