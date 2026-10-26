//! `wants`: the grants an extension asks for.
//!
//! A grant exists in this build only once its enforcement does (D5), so
//! every other name is refused, and the ones a later change brings are told
//! apart from typos by naming that change.

use kdl::KdlNode;
use uze_contract::Grant;

use crate::capability::extension::document::{Report, nearest};

/// The grants later changes bring, by name or by prefix (`exec:` stands
/// for every `exec:<program>`), with the change that brings each.
const LATER: &[(&str, u32)] = &[
    ("exec:", 5),
    ("exec", 5),
    ("store", 5),
    ("agent:", 6),
    ("notify", 6),
    ("resident", 7),
    ("net", 8),
    ("secret", 8),
    ("edit:checkout", 8),
];

/// The grants `node` names, each one this build offers, in order.
pub fn wants(node: &KdlNode, report: &mut Report<'_>) -> Vec<Grant> {
    let mut wanted = Vec::new();
    for (name, offset) in super::string_args(node, report) {
        let Some(grant) = grant(&name, offset, report) else {
            continue;
        };
        if wanted.contains(&grant) {
            report.at(
                offset,
                format!("`{name}` is wanted twice"),
                Some("keep one".to_owned()),
            );
            continue;
        }
        wanted.push(grant);
    }
    wanted
}

fn grant(name: &str, offset: usize, report: &mut Report<'_>) -> Option<Grant> {
    if let Some(grant) = Grant::parse(name) {
        return Some(grant);
    }
    if name == "write:checkout" {
        report.at(
            offset,
            "`write:checkout` does not exist: no extension writes the checkout",
            Some("read it with `read:checkout`".to_owned()),
        );
        return None;
    }
    if let Some(change) = later(name) {
        report.at(
            offset,
            format!("`{name}` arrives in a later change (change {change})"),
            Some(offered()),
        );
        return None;
    }
    let names: Vec<&str> = Grant::ALL.iter().map(|grant| grant.name()).collect();
    report.at(
        offset,
        format!("there is no grant `{name}`"),
        Some(match nearest(name, &names) {
            Some(near) => format!("did you mean `{near}`?"),
            None => offered(),
        }),
    );
    None
}

/// The change that brings `name`, when a later one does.
pub fn later(name: &str) -> Option<u32> {
    LATER
        .iter()
        .find(|(later, _)| match later.strip_suffix(':') {
            Some(prefix) => name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(':')),
            None => name == *later,
        })
        .map(|(_, change)| *change)
}

fn offered() -> String {
    let names: Vec<String> = Grant::ALL
        .iter()
        .map(|grant| format!("`{grant}`"))
        .collect();
    format!("this version offers {}", names.join(", "))
}
