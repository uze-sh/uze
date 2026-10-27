//! `sprite "<id>" sheet=… frame="WxH" columns=… fallback=… { state … }`:
//! a sheet of equal frames inside the extension and the states that play
//! them (`extension-media-and-resident`). Whether the frames fit the sheet
//! is the check's, once it can read the sheet's size.

use kdl::{KdlNode, KdlValue};
use uze_contract::{SpriteDecl, SpriteState};

use super::{declared_id, prop, relative_file, required_prop, unknown_child};
use crate::capability::extension::document::{Report, entries, refuse_extra, spelled};

/// The most frames a second a state plays.
pub const FPS_MAX: u32 = 30;
/// The most frames one state lists.
const FRAMES_MAX: usize = 256;

pub(super) fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<SpriteDecl> {
    let parts = entries(node, report);
    refuse_extra(
        node,
        &parts,
        1,
        &["sheet", "frame", "columns", "fallback"],
        true,
        report,
    );
    let id = declared_id(node, parts.args.first().copied(), report)?;
    let sheet = required_prop(node, &parts, "sheet", report)
        .filter(|(file, at)| relative_file(file, "png", *at, report))
        .map(|(file, _)| file);
    let frame = required_prop(node, &parts, "frame", report).and_then(|(written, at)| {
        let size = written
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
            .filter(|(w, h)| (1..=1024).contains(w) && (1..=1024).contains(h));
        if size.is_none() {
            report.at(
                at,
                format!("`{written}` is not a frame size"),
                Some("write it as `32x32`, each side 1 to 1024 pixels".to_owned()),
            );
        }
        size
    });
    let columns = match prop(&parts, "columns") {
        None => {
            report.on(node, "a sprite says how many `columns` its sheet has", None);
            None
        }
        Some(entry) => match entry.value() {
            KdlValue::Integer(n) if (1..=256).contains(n) => Some(*n as u32),
            other => {
                report.on(
                    entry,
                    format!(
                        "`columns` is a whole number from 1 to 256, not {}",
                        spelled(other)
                    ),
                    None,
                );
                None
            }
        },
    };
    let fallback = required_prop(node, &parts, "fallback", report).map(|(name, _)| name);
    let mut states: Vec<SpriteState> = Vec::new();
    for child in node
        .children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
    {
        if child.name().value() != "state" {
            unknown_child(child, "a `sprite`", &["state"], report);
            continue;
        }
        if let Some(state) = state(child, report) {
            if states.iter().any(|seen| seen.name == state.name) {
                report.on(
                    child,
                    format!("the state `{}` is declared twice", state.name),
                    None,
                );
            } else {
                states.push(state);
            }
        }
    }
    if states.is_empty() {
        report.on(node, "a sprite plays at least one `state`", None);
        return None;
    }
    let fallback = fallback?;
    if !states.iter().any(|state| state.name == fallback) {
        report.on(
            node,
            format!("the fallback `{fallback}` is none of its states"),
            None,
        );
        return None;
    }
    Some(SpriteDecl {
        id,
        sheet: sheet?,
        frame: frame?,
        columns: columns?,
        states,
        fallback,
    })
}

/// `state "<name>" frames="0-3,6" fps=8 loop=#true`.
fn state(node: &KdlNode, report: &mut Report<'_>) -> Option<SpriteState> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &["frames", "fps", "loop"], false, report);
    let name = declared_id(node, parts.args.first().copied(), report)?;
    let frames = required_prop(node, &parts, "frames", report).and_then(|(written, at)| {
        let frames = frame_list(&written);
        if frames.is_none() {
            report.at(
                at,
                format!("`{written}` is not a list of frames"),
                Some("write indices and ranges, as `0-3,6`".to_owned()),
            );
        }
        frames
    });
    let fps = match prop(&parts, "fps") {
        None => 8,
        Some(entry) => match entry.value() {
            KdlValue::Integer(n) if (1..=i128::from(FPS_MAX)).contains(n) => *n as u32,
            other => {
                report.on(
                    entry,
                    format!(
                        "`fps` is a whole number from 1 to {FPS_MAX}, not {}",
                        spelled(other)
                    ),
                    None,
                );
                return None;
            }
        },
    };
    let looped = match prop(&parts, "loop") {
        None => true,
        Some(entry) => match entry.value() {
            KdlValue::Bool(flag) => *flag,
            other => {
                report.on(
                    entry,
                    format!("`loop` is #true or #false, not {}", spelled(other)),
                    None,
                );
                return None;
            }
        },
    };
    Some(SpriteState {
        name,
        frames: frames?,
        fps,
        looped,
    })
}

/// `0-3,6` as `[0, 1, 2, 3, 6]`, or `None` for anything else.
fn frame_list(written: &str) -> Option<Vec<u32>> {
    let mut frames = Vec::new();
    for part in written.split(',') {
        let part = part.trim();
        match part.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (
                    from.trim().parse::<u32>().ok()?,
                    to.trim().parse::<u32>().ok()?,
                );
                if from > to {
                    return None;
                }
                frames.extend(from..=to);
            }
            None => frames.push(part.parse().ok()?),
        }
        if frames.len() > FRAMES_MAX {
            return None;
        }
    }
    (!frames.is_empty()).then_some(frames)
}

/// The width and height a PNG's header says, or `None` for bytes that are
/// not one.
pub fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if head.len() < 24 || !head.starts_with(SIGNATURE) || &head[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(head[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(head[20..24].try_into().ok()?);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_indices_and_ranges() {
        assert_eq!(frame_list("0-3,6"), Some(vec![0, 1, 2, 3, 6]));
        assert_eq!(frame_list("3-1"), None);
        assert_eq!(frame_list("a"), None);
        assert_eq!(frame_list("0-1000"), None);
    }

    #[test]
    fn a_pngs_size_is_read_from_its_header() {
        let mut head = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        head.extend(128u32.to_be_bytes());
        head.extend(64u32.to_be_bytes());
        assert_eq!(png_size(&head), Some((128, 64)));
        assert_eq!(png_size(b"GIF89a"), None);
    }
}
