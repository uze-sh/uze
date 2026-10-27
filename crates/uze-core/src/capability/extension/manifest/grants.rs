//! `wants`: the grants an extension asks for.
//!
//! A grant exists in this build only once its enforcement does (D5), so
//! every other name is refused, and the ones the plan chose not to build
//! are told apart from typos by saying so.

use kdl::KdlNode;
use uze_contract::Grant;

use crate::capability::extension::document::{Report, nearest};

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
    if let Some(program) = name.strip_prefix(Grant::EXEC) {
        report.at(
            offset,
            format!("`{program}` is not a program's name"),
            Some(
                "write `exec:` and the name the program is run by, such as `exec:nvidia-smi`"
                    .to_owned(),
            ),
        );
        return None;
    }
    if let Some(host) = name.strip_prefix(Grant::NET) {
        report.at(
            offset,
            format!("`{host}` is not a host name"),
            Some(
                "write `net:` and the host alone, in lower case, with no scheme, port, path or \
                 wildcard, such as `net:api.github.com`"
                    .to_owned(),
            ),
        );
        return None;
    }
    if name == "net" {
        report.at(
            offset,
            "`net` names the host it grants",
            Some("write `net:<host>`, such as `net:api.github.com`".to_owned()),
        );
        return None;
    }
    if name == "secret" {
        report.at(
            offset,
            "`secret` is not a grant: a secret is a preference the operator sets",
            Some("declare `secret \"token\" label=\"…\"` inside `preferences`".to_owned()),
        );
        return None;
    }
    if name == "edit:checkout" {
        report.at(
            offset,
            "`edit:checkout` is not offered: no extension writes the checkout yet",
            Some("read it with `read:checkout`".to_owned()),
        );
        return None;
    }
    if name == "exec" {
        report.at(
            offset,
            "`exec` names the program it grants",
            Some("write `exec:<program>`, such as `exec:nvidia-smi`".to_owned()),
        );
        return None;
    }
    if name == "write:checkout" {
        report.at(
            offset,
            "`write:checkout` does not exist: no extension writes the checkout",
            Some("read it with `read:checkout`".to_owned()),
        );
        return None;
    }
    let names: Vec<String> = Grant::FIXED
        .iter()
        .map(|grant| grant.name().into_owned())
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
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

fn offered() -> String {
    let names: Vec<String> = Grant::FIXED
        .iter()
        .map(|grant| format!("`{grant}`"))
        .chain(["`exec:<program>`".to_owned(), "`net:<host>`".to_owned()])
        .collect();
    format!("this version offers {}", names.join(", "))
}
