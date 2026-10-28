//! `icon "<name>" nerd="…" unicode="…" ascii="…"`: an icon an extension
//! declares beyond the host's, in every glyph set the operator may choose,
//! so its `Icon` nodes draw in theirs.

use kdl::KdlNode;
use uze_contract::IconDecl;
use uze_contract::icons;

use super::{declared_id, required_prop};
use crate::capability::extension::document::{Report, entries, refuse_extra};

pub(super) fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<IconDecl> {
    let parts = entries(node, report);
    refuse_extra(
        node,
        &parts,
        1,
        &["nerd", "unicode", "ascii"],
        false,
        report,
    );
    let name = declared_id(node, parts.args.first().copied(), report)?;
    if icons::is_vocabulary(&name) {
        report.on(
            node,
            format!("`{name}` is one of the host's icons"),
            Some(format!(
                "write `Icon \"{name}\"` without declaring it, or give yours another name"
            )),
        );
        return None;
    }
    let mut glyph = |set: &str, problem: fn(&str) -> Option<&'static str>| {
        let (written, at) = required_prop(node, &parts, set, report)?;
        match problem(&written) {
            None => Some(written),
            Some(wanted) => {
                report.at(at, format!("`{set}` is {wanted}"), None);
                None
            }
        }
    };
    let nerd = glyph("nerd", icons::nerd_glyph_problem);
    let unicode = glyph("unicode", icons::unicode_glyph_problem);
    let ascii = glyph("ascii", icons::ascii_glyph_problem);
    Some(IconDecl {
        name,
        nerd: nerd?,
        unicode: unicode?,
        ascii: ascii?,
    })
}
