//! Managed Text Region — ownership of a delimited slice of a shared text
//! file, never the whole file.
//!
//! This module knows nothing about any harness, any package format, or
//! Instructions specifically. It answers exactly one question: "does the
//! region this caller identified by `region_identity` inside `target_file`
//! currently hold `expected_content`, and can it be safely
//! created/inspected/removed without touching anything else in that file?"
//!
//! A region is delimited by two whole-line HTML comment markers derived
//! deterministically from `region_identity`:
//!
//! ```text
//! <!-- uze:begin <region_identity> -->
//! ...content...
//! <!-- uze:end <region_identity> -->
//! ```
//!
//! Ownership is proven structurally (exactly one matching begin marker line,
//! exactly one matching end marker line, end after begin), never by
//! comparing free-form text. Any other marker shape for a given identity —
//! duplicated, out of order, or only half present — is a parsing-safety
//! failure, reported as `Blocked`, never guessed at.

use std::{fs, path::Path};

use crate::{
    error::{Result, UzeError},
    integration::{AttachmentInspection, AttachmentState},
    persistence::{contained_destination, write_atomic_within},
};

/// Why a region whose markers are duplicated, out of order, or only half
/// present is refused.
pub const MALFORMED_MARKERS: &str =
    "managed text region markers are duplicated, out of order, or only half present";

/// Characters a region identity may contain. Deliberately narrow: an
/// identity built from these characters can never itself be mistaken for
/// marker syntax, so a whole-line-equality parser is sufficient without
/// escaping.
fn identity_is_valid(identity: &str) -> bool {
    !identity.is_empty()
        && identity
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.' | '/'))
}

fn markers(region_identity: &str) -> (String, String) {
    (
        format!("<!-- uze:begin {region_identity} -->"),
        format!("<!-- uze:end {region_identity} -->"),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Newline {
    Lf,
    Crlf,
}

impl Newline {
    fn separator(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// One physical line: its text, and the terminator it carried — `None` on a
/// final line the file did not terminate.
///
/// The terminator is kept per line rather than inferred once for the file,
/// because a file of mixed endings is a file UZE must hand back exactly as
/// it found it outside its own region. Inferring one style and re-joining
/// every line with it rewrote whole files that happened to hold a single
/// pasted CRLF line.
#[derive(Clone, Debug)]
struct Line {
    text: String,
    ending: Option<Newline>,
}

impl Line {
    fn terminated(text: impl Into<String>, ending: Newline) -> Self {
        Self {
            text: text.into(),
            ending: Some(ending),
        }
    }
}

/// The lines' texts joined by `\n`, for comparing region content against
/// caller-supplied content, which carries no endings of its own.
fn joined_text(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads `path` into physical lines, each keeping its own terminator, plus
/// the style to give lines UZE inserts: whichever the file uses most, LF on
/// a tie. `Ok(None)` means the file does not exist yet — a legitimate,
/// common state, not an error.
fn read_lines(path: &Path) -> Result<Option<(Vec<Line>, Newline)>> {
    if fs::symlink_metadata(path).is_ok() {
        contained_destination(holding_directory(path), path)?;
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(UzeError::Read {
                path: path.to_path_buf(),
                source: error,
            });
        }
    };
    let content =
        String::from_utf8(bytes).map_err(|_| UzeError::InvalidTextEncoding(path.to_path_buf()))?;
    let lines = lines_of(&content);
    let style = majority_newline(&lines);
    Ok(Some((lines, style)))
}

fn lines_of(content: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(index) => {
                let (line, tail) = rest.split_at(index);
                let (text, ending) = match line.strip_suffix('\r') {
                    Some(text) => (text, Newline::Crlf),
                    None => (line, Newline::Lf),
                };
                lines.push(Line::terminated(text, ending));
                rest = &tail[1..];
            }
            None => {
                lines.push(Line {
                    text: rest.to_owned(),
                    ending: None,
                });
                rest = "";
            }
        }
    }
    lines
}

/// The style to give lines UZE inserts: whichever `lines` use most, LF on a
/// tie.
fn majority_newline(lines: &[Line]) -> Newline {
    let crlf = lines
        .iter()
        .filter(|line| line.ending == Some(Newline::Crlf))
        .count();
    if crlf * 2 > lines.len() {
        Newline::Crlf
    } else {
        Newline::Lf
    }
}

/// Writes the lines back exactly as they stand: every line UZE did not
/// touch keeps the bytes it arrived with, terminator included.
fn write_lines(path: &Path, lines: &[Line]) -> Result<()> {
    let mut out = String::new();
    for line in lines {
        out.push_str(&line.text);
        if let Some(ending) = line.ending {
            out.push_str(ending.separator());
        }
    }
    write_atomic_within(holding_directory(path), path, out.as_bytes())
}

/// The directory a region's file is contained by. A region lives in a file
/// a project keeps at its root (`AGENTS.md`), so the directory holding it
/// is the project: the file may be a link to another file inside it, never
/// to one outside.
fn holding_directory(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Normalizes caller-supplied content into the exact physical lines that
/// belong between the markers: internal CRLF collapsed to LF (the file's own
/// style governs what is written back), and at most one trailing newline
/// stripped so content passed straight from a file's own bytes does not
/// produce a spurious blank line before the end marker.
fn content_lines(expected_content: &str) -> Vec<String> {
    let normalized = expected_content.replace("\r\n", "\n");
    let body = normalized.strip_suffix('\n').unwrap_or(&normalized);
    if body.is_empty() {
        Vec::new()
    } else {
        body.split('\n').map(str::to_owned).collect()
    }
}

enum Scan {
    Missing,
    WellFormed { begin: usize, end: usize },
    Malformed,
}

fn scan(lines: &[Line], begin_marker: &str, end_marker: &str) -> Scan {
    let begins: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.text == begin_marker)
        .map(|(index, _)| index)
        .collect();
    let ends: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.text == end_marker)
        .map(|(index, _)| index)
        .collect();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Scan::Missing,
        (&[begin], &[end]) if end > begin => Scan::WellFormed { begin, end },
        _ => Scan::Malformed,
    }
}

/// Whether `line` would be read as a region's marker. Content carrying one
/// would open or close a region the moment it is written, and every later
/// scan of the file would disagree with what was attached.
fn is_marker_line(line: &str) -> bool {
    (line.starts_with("<!-- uze:begin ") || line.starts_with("<!-- uze:end "))
        && line.ends_with(" -->")
}

fn blocked(reason: impl Into<String>) -> AttachmentInspection {
    AttachmentInspection {
        state: AttachmentState::Blocked,
        reason: reason.into(),
    }
}

/// Ownership state of one region, scoped to exactly that region — never the
/// rest of the file.
pub fn inspect(
    target_file: &Path,
    region_identity: &str,
    expected_content: &str,
) -> AttachmentInspection {
    match read_region(target_file, region_identity) {
        Ok((lines, scan)) => region_state(&lines, &scan, expected_content),
        Err(inspection) => inspection,
    }
}

/// Reads `target_file` once and locates `region_identity`'s markers in it,
/// or says why there is nothing to locate them in.
fn read_region(
    target_file: &Path,
    region_identity: &str,
) -> std::result::Result<(Vec<Line>, Scan), AttachmentInspection> {
    if !identity_is_valid(region_identity) {
        return Err(invalid_identity(region_identity));
    }
    let (begin_marker, end_marker) = markers(region_identity);
    match read_lines(target_file) {
        Ok(Some((lines, _style))) => {
            let scan = scan(&lines, &begin_marker, &end_marker);
            Ok((lines, scan))
        }
        Ok(None) => Err(target_absent()),
        Err(error @ UzeError::ProjectFileEscapes { .. }) => Err(AttachmentInspection {
            state: AttachmentState::Conflict,
            reason: error.to_string(),
        }),
        Err(error) => Err(blocked(error.to_string())),
    }
}

fn invalid_identity(region_identity: &str) -> AttachmentInspection {
    blocked(format!(
        "invalid managed-region identity `{region_identity}`"
    ))
}

fn target_absent() -> AttachmentInspection {
    AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "managed text region's target file does not exist".to_owned(),
    }
}

fn region_state(lines: &[Line], scan: &Scan, expected_content: &str) -> AttachmentInspection {
    match *scan {
        Scan::Missing => AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "managed text region markers are absent".to_owned(),
        },
        Scan::Malformed => blocked(MALFORMED_MARKERS),
        Scan::WellFormed { begin, end } => {
            let current = joined_text(&lines[begin + 1..end]);
            let expected = content_lines(expected_content).join("\n");
            if current == expected {
                AttachmentInspection {
                    state: AttachmentState::Matched,
                    reason: "managed text region matches expected content".to_owned(),
                }
            } else {
                AttachmentInspection {
                    state: AttachmentState::Drifted,
                    reason: "managed text region content differs from expected".to_owned(),
                }
            }
        }
    }
}

/// Idempotently creates the region if it is currently `Missing`. Never
/// touches the file when the region is already `Matched`. Refuses — never
/// overwrites — when it is `Drifted`, `Blocked`, or `Conflict`: this
/// function's only safe outcome besides "already correct" is "correctly
/// created," never "silently repaired."
pub fn attach(target_file: &Path, region_identity: &str, expected_content: &str) -> Result<()> {
    if !identity_is_valid(region_identity) {
        return Err(UzeError::InvalidRegionIdentity(region_identity.to_owned()));
    }
    let (mut lines, _style) = read_lines(target_file)?.unwrap_or((Vec::new(), Newline::Lf));
    if attach_lines(&mut lines, target_file, region_identity, expected_content)? {
        write_lines(target_file, &lines)?;
    }
    Ok(())
}

/// [`attach`] applied to lines already read: answers whether it added the
/// region, which is the only case the file needs writing.
fn attach_lines(
    lines: &mut Vec<Line>,
    target_file: &Path,
    region_identity: &str,
    expected_content: &str,
) -> Result<bool> {
    if !identity_is_valid(region_identity) {
        return Err(UzeError::InvalidRegionIdentity(region_identity.to_owned()));
    }
    let content = content_lines(expected_content);
    if content.iter().any(|line| is_marker_line(line)) {
        return Err(UzeError::ManagedRegionContentCarriesMarker {
            region: region_identity.to_owned(),
            path: target_file.to_path_buf(),
        });
    }
    let (begin_marker, end_marker) = markers(region_identity);
    match scan(lines, &begin_marker, &end_marker) {
        Scan::WellFormed { begin, end } => {
            if joined_text(&lines[begin + 1..end]) == content.join("\n") {
                Ok(false)
            } else {
                Err(UzeError::ManagedRegionDrift(target_file.to_path_buf()))
            }
        }
        Scan::Malformed => Err(UzeError::ManagedRegionConflict(target_file.to_path_buf())),
        Scan::Missing => {
            let style = majority_newline(lines);
            // A file that did not end in a newline gets one, or the begin
            // marker would land on the end of the user's last line.
            if let Some(last) = lines.last_mut()
                && last.ending.is_none()
            {
                last.ending = Some(style);
            }
            lines.push(Line::terminated(begin_marker, style));
            lines.extend(
                content
                    .into_iter()
                    .map(|text| Line::terminated(text, style)),
            );
            lines.push(Line::terminated(end_marker, style));
            Ok(true)
        }
    }
}

/// Removes only the region's own marker lines and content lines. Every byte
/// outside them is preserved untouched, including line-ending style. The
/// region is inspected in the same read it is removed from, and anything but
/// `Matched` is returned unchanged — `Missing` is a safe no-op (already
/// gone), `Drifted`/`Blocked` refuse per ADR-009.
pub fn detach(
    target_file: &Path,
    region_identity: &str,
    expected_content: &str,
) -> Result<AttachmentInspection> {
    let (mut lines, scan) = match read_region(target_file, region_identity) {
        Ok(read) => read,
        Err(inspection) => return Ok(inspection),
    };
    let inspection = region_state(&lines, &scan, expected_content);
    let Scan::WellFormed { begin, end } = scan else {
        return Ok(inspection);
    };
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    lines.drain(begin..=end);
    write_lines(target_file, &lines)?;
    Ok(AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "managed text region detached".to_owned(),
    })
}

/// Ensures a region exists exactly when `should_exist` is true, and is
/// absent otherwise — both directions going through the same safety rules
/// as `attach`/`detach` (idempotent, never overwrites drift, never
/// destructively removes drift). Convenience for a caller whose own state
/// (not this module's business) decides whether a region is currently
/// wanted.
pub fn reconcile(
    target_file: &Path,
    region_identity: &str,
    expected_content: &str,
    should_exist: bool,
) -> AttachmentInspection {
    let refusal = if should_exist {
        attach(target_file, region_identity, expected_content).err()
    } else {
        detach(target_file, region_identity, expected_content).err()
    };
    let inspection = inspect(target_file, region_identity, expected_content);
    // A write that could not happen — a read-only file, a full disk — leaves
    // the region exactly as it was, and the inspection then reads `Missing`:
    // the same answer a healthy file nobody has reconciled yet gives. Only
    // the refusal tells the two apart, so it is what is reported wherever
    // the inspection has not already named a problem of its own.
    match refusal {
        Some(error)
            if !matches!(
                inspection.state,
                AttachmentState::Blocked | AttachmentState::Drifted
            ) =>
        {
            blocked(error.to_string())
        }
        _ => inspection,
    }
}

/// Structural well-formedness of a region's markers, independent of any
/// content comparison. This is the same proof `remove_unconditionally`
/// relies on to act, exposed here so a caller can **preview** a structural-
/// only removal — e.g. for a read-only report — without performing it and
/// without needing `expected_content`, which content-verified `inspect`
/// requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionShape {
    /// The target file does not exist, or has no marker lines for this
    /// identity at all.
    Absent,
    /// Exactly one well-formed begin/end pair — removable by
    /// `remove_unconditionally`, inspectable by `inspect` if a caller has
    /// `expected_content`.
    WellFormed,
    /// Duplicated, out of order, or only half present — refuses any
    /// destructive operation, the same way `inspect`/`detach` do.
    Malformed,
}

/// Previews what `remove_unconditionally` would find, without touching the
/// file. See `RegionShape` for what each outcome means.
pub fn region_shape(target_file: &Path, region_identity: &str) -> RegionShape {
    if !identity_is_valid(region_identity) {
        return RegionShape::Malformed;
    }
    let (begin_marker, end_marker) = markers(region_identity);
    let Ok(Some((lines, _style))) = read_lines(target_file) else {
        return RegionShape::Absent;
    };
    match scan(&lines, &begin_marker, &end_marker) {
        Scan::Missing => RegionShape::Absent,
        Scan::WellFormed { .. } => RegionShape::WellFormed,
        Scan::Malformed => RegionShape::Malformed,
    }
}

/// Removes a region identified by structure alone — exactly one well-formed
/// begin/end pair for `region_identity` — without comparing its content to
/// anything. This is deliberately **weaker** than `detach`: it exists only
/// for a caller that has already determined, by its own means, that no
/// current source owns this identity any more (an "orphaned" region), and
/// therefore has no `expected_content` left to verify drift against.
/// Ownership is proven the same way `attach`/`inspect` always prove it —
/// exact marker match, never a guess — but content drift *inside* an
/// already-orphaned region cannot be detected here, because there is no
/// longer anything to compare it to. Callers should prefer `detach` whenever
/// `expected_content` is available. See `region_shape` to preview this
/// function's outcome read-only.
pub fn remove_unconditionally(
    target_file: &Path,
    region_identity: &str,
) -> Result<AttachmentInspection> {
    if !identity_is_valid(region_identity) {
        return Ok(blocked(MALFORMED_MARKERS));
    }
    let Some((mut lines, _style)) = read_lines(target_file)? else {
        return Ok(markers_absent());
    };
    match remove_lines(&mut lines, region_identity) {
        Ok(removed) => {
            write_lines(target_file, &lines)?;
            Ok(removed)
        }
        Err(untouched) => Ok(untouched),
    }
}

fn markers_absent() -> AttachmentInspection {
    AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "managed text region markers are absent".to_owned(),
    }
}

/// [`remove_unconditionally`] applied to lines already read: `Ok` when the
/// region was taken out of `lines`, `Err` with why they were left alone.
fn remove_lines(
    lines: &mut Vec<Line>,
    region_identity: &str,
) -> std::result::Result<AttachmentInspection, AttachmentInspection> {
    if !identity_is_valid(region_identity) {
        return Err(blocked(MALFORMED_MARKERS));
    }
    let (begin_marker, end_marker) = markers(region_identity);
    match scan(lines, &begin_marker, &end_marker) {
        Scan::Missing => Err(markers_absent()),
        Scan::Malformed => Err(blocked(MALFORMED_MARKERS)),
        Scan::WellFormed { begin, end } => {
            lines.drain(begin..=end);
            Ok(AttachmentInspection {
                state: AttachmentState::Missing,
                reason: "orphaned managed text region removed (no current source owns it)"
                    .to_owned(),
            })
        }
    }
}

/// One desired region after [`converge`]: its inspection, and the write
/// failure behind it when the region could not be created at all — which
/// the inspection alone cannot tell apart from a region nobody has created
/// yet, since both read `Missing`.
#[derive(Clone, Debug)]
pub struct DesiredRegion {
    pub identity: String,
    pub inspection: AttachmentInspection,
    pub write_failure: Option<String>,
}

/// What [`converge`] did to the regions one owner claims in a file.
#[derive(Clone, Debug, Default)]
pub struct RegionConvergence {
    /// One entry per desired region, in the order given.
    pub desired: Vec<DesiredRegion>,
    /// Owned regions no longer desired, removed.
    pub removed: Vec<String>,
    /// Owned regions no longer desired that could not be removed, and why.
    pub blocked: Vec<(String, String)>,
}

/// The regions `owns` claims in `target_file` that none of
/// `desired_identities` names — what an earlier state of the owner left
/// behind. Claimed by identity shape, never by content. Deduplicated and
/// sorted.
pub fn stale_regions<'a>(
    target_file: &Path,
    owns: impl Fn(&str) -> bool,
    desired_identities: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let Ok(Some((lines, _style))) = read_lines(target_file) else {
        return Vec::new();
    };
    stale_in(&lines, owns, desired_identities)
}

fn stale_in<'a>(
    lines: &[Line],
    owns: impl Fn(&str) -> bool,
    desired_identities: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let desired: std::collections::BTreeSet<&str> = desired_identities.into_iter().collect();
    identities_in(lines)
        .into_iter()
        .filter(|identity| owns(identity) && !desired.contains(identity.as_str()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Brings the regions one owner claims in `target_file` to exactly
/// `desired` — `(identity, content)` pairs: every stale owned region is
/// removed first, so the file never briefly carries an old and a new
/// statement together, then each desired region is created when missing.
/// Drift and malformed markers are reported, never overwritten or guessed
/// at. Removal is structural (see [`remove_unconditionally`]).
///
/// The file is read once and written at most once, however many regions
/// change.
pub fn converge(
    target_file: &Path,
    owns: impl Fn(&str) -> bool,
    desired: &[(String, String)],
) -> RegionConvergence {
    let (read, existed) = match read_lines(target_file) {
        Ok(Some((lines, _style))) => (lines, true),
        Ok(None) => (Vec::new(), false),
        Err(error) => return unreadable_convergence(desired, &error),
    };
    let mut lines = read.clone();
    let mut convergence = RegionConvergence::default();
    let stale = stale_in(
        &lines,
        owns,
        desired.iter().map(|(identity, _)| identity.as_str()),
    );
    for identity in stale {
        match remove_lines(&mut lines, &identity) {
            Ok(_removed) => convergence.removed.push(identity),
            Err(inspection) => convergence.blocked.push((identity, inspection.reason)),
        }
    }
    let mut attached = Vec::new();
    let mut refusals = Vec::new();
    for (identity, content) in desired {
        match attach_lines(&mut lines, target_file, identity, content) {
            Ok(added) => {
                attached.push(added);
                refusals.push(None);
            }
            Err(error) => {
                attached.push(false);
                refusals.push(Some(error));
            }
        }
    }
    let changed = !convergence.removed.is_empty() || attached.contains(&true);
    let written = if changed {
        let written = write_lines(target_file, &lines).map_err(|error| error.to_string());
        if written.is_ok() {
            tracing::info!(
                file = %target_file.display(),
                removed = ?convergence.removed,
                "managed regions were rewritten"
            );
        }
        written
    } else {
        Ok(())
    };
    let on_disk = match (&written, changed || existed) {
        (Ok(()), true) => Some(lines.as_slice()),
        (Err(_), _) if existed => Some(read.as_slice()),
        _ => None,
    };
    if let Err(failure) = &written {
        for identity in std::mem::take(&mut convergence.removed) {
            convergence.blocked.push((identity, failure.clone()));
        }
    }
    for (((identity, content), added), refusal) in desired.iter().zip(attached).zip(refusals) {
        let inspection = inspect_lines(on_disk, identity, content);
        let write_failure = match (refusal, &written) {
            (None, Err(failure)) if added => Some(failure.clone()),
            (None, _) => None,
            (Some(UzeError::ManagedRegionDrift(_) | UzeError::ManagedRegionConflict(_)), _)
                if matches!(
                    inspection.state,
                    AttachmentState::Blocked | AttachmentState::Drifted
                ) =>
            {
                None
            }
            (Some(error), _) => Some(error.to_string()),
        };
        convergence.desired.push(DesiredRegion {
            identity: identity.clone(),
            inspection,
            write_failure,
        });
    }
    convergence
}

/// What [`converge`] reports for a file it could not read: nothing is
/// claimed as stale, and every desired region carries the reason.
fn unreadable_convergence(desired: &[(String, String)], error: &UzeError) -> RegionConvergence {
    let desired = desired
        .iter()
        .map(|(identity, _)| {
            let (inspection, write_failure) = if identity_is_valid(identity) {
                (blocked(error.to_string()), error.to_string())
            } else {
                (
                    invalid_identity(identity),
                    UzeError::InvalidRegionIdentity(identity.clone()).to_string(),
                )
            };
            DesiredRegion {
                identity: identity.clone(),
                inspection,
                write_failure: Some(write_failure),
            }
        })
        .collect();
    RegionConvergence {
        desired,
        ..RegionConvergence::default()
    }
}

/// [`inspect`] of lines already read; `None` is a file that does not exist.
fn inspect_lines(
    lines: Option<&[Line]>,
    region_identity: &str,
    expected_content: &str,
) -> AttachmentInspection {
    if !identity_is_valid(region_identity) {
        return invalid_identity(region_identity);
    }
    let Some(lines) = lines else {
        return target_absent();
    };
    let (begin_marker, end_marker) = markers(region_identity);
    region_state(
        lines,
        &scan(lines, &begin_marker, &end_marker),
        expected_content,
    )
}

/// The `region_identity` named by every begin marker line in
/// `target_file`, in file order — one entry per line, so an identity whose
/// markers are duplicated or half present appears too, as often as its
/// begin marker does. Well-formedness is deliberately not judged here:
/// callers hand each identity to a function that proves it, and one that
/// filtered malformed regions out would hide exactly the regions they must
/// report as blocked. A missing or unreadable file yields an empty list.
pub fn region_identities_present(target_file: &Path) -> Vec<String> {
    let Ok(Some((lines, _style))) = read_lines(target_file) else {
        return Vec::new();
    };
    identities_in(&lines)
}

fn identities_in(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            line.text
                .strip_prefix("<!-- uze:begin ")
                .and_then(|rest| rest.strip_suffix(" -->"))
        })
        .map(str::to_owned)
        .collect()
}

/// Whether two versions of a file say the same thing outside the regions
/// UZE manages in them: the same non-blank lines, in the same order, once
/// every well-formed region is set aside. Blank lines are not compared,
/// because attaching a region is what adds the one that separates it.
pub fn same_outside_managed_regions(left: &str, right: &str) -> bool {
    authored_lines(&lines_of(left)).eq(authored_lines(&lines_of(right)))
}

fn authored_lines(lines: &[Line]) -> impl Iterator<Item = &str> {
    let mut owned = std::collections::BTreeSet::new();
    for identity in identities_in(lines) {
        let (begin_marker, end_marker) = markers(&identity);
        if let Scan::WellFormed { begin, end } = scan(lines, &begin_marker, &end_marker) {
            owned.extend(begin..=end);
        }
    }
    lines
        .iter()
        .enumerate()
        .filter(move |(index, line)| !owned.contains(index) && !line.text.trim().is_empty())
        .map(|(_, line)| line.text.as_str())
}

/// Whether `target_file` holds any content that is not part of a
/// well-formed UZE-managed region — i.e. whether a user (or anything other
/// than UZE) wrote something into it independently. A missing file, or one
/// containing only managed regions and blank lines, reports `false`. A
/// malformed region's lines count as content *outside* any region, since
/// ownership of them cannot be proven — the same fail-closed default every
/// other function in this module applies.
pub fn has_content_outside_managed_regions(target_file: &Path) -> bool {
    let Ok(Some((lines, _style))) = read_lines(target_file) else {
        return false;
    };
    authored_lines(&lines).next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `target_file` currently holds *any* UZE-managed region at
    /// all, regardless of identity. Exercised only by the tests below; a
    /// production caller would live this check next to its own manager.
    fn any_region_present(target_file: &Path) -> bool {
        let Ok(Some((lines, _style))) = read_lines(target_file) else {
            return false;
        };
        lines
            .iter()
            .any(|line| line.text.starts_with("<!-- uze:begin ") && line.text.ends_with(" -->"))
    }

    // --- containment ---------------------------------------------------

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_region_file_linked_out_of_its_project_is_a_conflict_and_never_written() {
        let base = uze_testkit::temp::scratch("region-escape");
        let project = base.join("project");
        let outside = base.join("outside");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let file = project.join("AGENTS.md");
        std::os::unix::fs::symlink("../outside/AGENTS.md", &file).unwrap();

        let refused = attach(&file, "pkg-a/instructions", "planted");
        assert!(
            matches!(refused, Err(UzeError::ProjectFileEscapes { .. })),
            "{refused:?}"
        );
        assert!(
            !outside.join("AGENTS.md").exists(),
            "the dangling link was followed"
        );

        fs::write(outside.join("AGENTS.md"), "someone else's file\n").unwrap();
        assert_eq!(
            inspect(&file, "pkg-a/instructions", "planted").state,
            AttachmentState::Conflict
        );
        let convergence = converge(
            &file,
            |_| true,
            &[("pkg-a/instructions".to_owned(), "planted".to_owned())],
        );
        assert!(convergence.desired[0].write_failure.is_some());
        assert_eq!(
            fs::read_to_string(outside.join("AGENTS.md")).unwrap(),
            "someone else's file\n"
        );
    }

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_region_file_linked_inside_its_project_is_written_through_the_link() {
        let project = uze_testkit::temp::scratch("region-inner-link");
        fs::create_dir_all(project.join("docs")).unwrap();
        let file = project.join("AGENTS.md");
        std::os::unix::fs::symlink("docs/AGENTS.md", &file).unwrap();
        attach(&file, "pkg-a/instructions", "kept").unwrap();
        assert!(fs::symlink_metadata(&file).unwrap().is_symlink());
        assert!(
            fs::read_to_string(project.join("docs/AGENTS.md"))
                .unwrap()
                .contains("kept")
        );
    }

    // --- attach --------------------------------------------------------

    #[test]
    fn attach_creates_the_file_when_it_does_not_exist() {
        let root = uze_testkit::temp::scratch("create");
        let file = root.join("NOTES.md");
        attach(&file, "pkg-a/instructions", "hello\nworld").unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(
            content,
            "<!-- uze:begin pkg-a/instructions -->\nhello\nworld\n<!-- uze:end pkg-a/instructions -->\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attach_preserves_existing_user_content_and_appends() {
        let root = uze_testkit::temp::scratch("preserve");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, "user text A\nuser text B\n").unwrap();
        attach(&file, "pkg-a/instructions", "managed content").unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(
            content,
            "user text A\nuser text B\n<!-- uze:begin pkg-a/instructions -->\nmanaged content\n<!-- uze:end pkg-a/instructions -->\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attach_is_idempotent_when_already_matched() {
        let root = uze_testkit::temp::scratch("idempotent");
        let file = root.join("project-context.md");
        attach(&file, "id", "content").unwrap();
        let before = fs::read_to_string(&file).unwrap();
        attach(&file, "id", "content").unwrap();
        let after = fs::read_to_string(&file).unwrap();
        assert_eq!(before, after, "a matched region is never rewritten");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attach_refuses_to_overwrite_drifted_content() {
        let root = uze_testkit::temp::scratch("drift-refuse");
        let file = root.join("NOTES.md");
        attach(&file, "id", "original").unwrap();
        let path = file.clone();
        let mut content = fs::read_to_string(&path).unwrap();
        content = content.replace("original", "user-edited");
        fs::write(&path, content).unwrap();
        let error = attach(&file, "id", "original").unwrap_err();
        assert!(matches!(error, UzeError::ManagedRegionDrift(_)));
        assert!(fs::read_to_string(&file).unwrap().contains("user-edited"));
        fs::remove_dir_all(root).unwrap();
    }

    // --- inspect ---------------------------------------------------------

    #[test]
    fn inspect_scopes_to_the_declared_region_only() {
        let root = uze_testkit::temp::scratch("scoped-inspect");
        let file = root.join("NOTES.md");
        attach(&file, "id", "managed").unwrap();

        // Editing text OUTSIDE the region leaves it MATCHED.
        let mut content = fs::read_to_string(&file).unwrap();
        content = format!("user note\n{content}user note after\n");
        fs::write(&file, &content).unwrap();
        assert_eq!(
            inspect(&file, "id", "managed").state,
            AttachmentState::Matched
        );

        // Editing text INSIDE the region becomes DRIFTED.
        let drifted = content.replace("managed", "tampered");
        fs::write(&file, drifted).unwrap();
        assert_eq!(
            inspect(&file, "id", "managed").state,
            AttachmentState::Drifted
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspect_reports_missing_for_absent_file_and_absent_region() {
        let root = uze_testkit::temp::scratch("missing");
        let file = root.join("NOTES.md");
        assert_eq!(
            inspect(&file, "id", "x").state,
            AttachmentState::Missing,
            "no file at all"
        );
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, "just user content\n").unwrap();
        assert_eq!(
            inspect(&file, "id", "x").state,
            AttachmentState::Missing,
            "file exists but has no markers"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_or_malformed_markers_are_blocked_never_guessed() {
        let root = uze_testkit::temp::scratch("malformed");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");

        fs::write(
            &file,
            "<!-- uze:begin id -->\na\n<!-- uze:end id -->\n<!-- uze:begin id -->\nb\n<!-- uze:end id -->\n",
        )
        .unwrap();
        assert_eq!(inspect(&file, "id", "a").state, AttachmentState::Blocked);

        fs::write(&file, "<!-- uze:begin id -->\nno end marker\n").unwrap();
        assert_eq!(inspect(&file, "id", "x").state, AttachmentState::Blocked);

        fs::write(&file, "<!-- uze:end id -->\nno begin marker\n").unwrap();
        assert_eq!(inspect(&file, "id", "x").state, AttachmentState::Blocked);

        // End marker appears before begin marker: still malformed, not a
        // reversed-but-valid region.
        fs::write(&file, "<!-- uze:end id -->\nstuff\n<!-- uze:begin id -->\n").unwrap();
        assert_eq!(inspect(&file, "id", "x").state, AttachmentState::Blocked);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_literal_marker_line_inside_user_content_for_the_same_identity_is_blocked_not_guessed() {
        let root = uze_testkit::temp::scratch("literal-marker");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        // Two begin markers for "id": one real, one that just happens to be
        // literal package content. The parser cannot know which is
        // authoritative, so it must refuse rather than pick one.
        fs::write(
            &file,
            "<!-- uze:begin id -->\n<!-- uze:begin id -->\n<!-- uze:end id -->\n",
        )
        .unwrap();
        assert_eq!(inspect(&file, "id", "x").state, AttachmentState::Blocked);
        fs::remove_dir_all(root).unwrap();
    }

    // --- detach ------------------------------------------------------------

    #[test]
    fn detach_removes_only_the_region_between_two_pieces_of_user_content() {
        let root = uze_testkit::temp::scratch("detach-between");
        let file = root.join("NOTES.md");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, "user text A\n").unwrap();
        attach(&file, "pkg/resource", "managed content").unwrap();
        let mut content = fs::read_to_string(&file).unwrap();
        content.push_str("user text B\n");
        fs::write(&file, &content).unwrap();

        let result = detach(&file, "pkg/resource", "managed content").unwrap();
        assert_eq!(result.state, AttachmentState::Missing);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "user text A\nuser text B\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detach_on_missing_region_is_a_safe_no_op() {
        let root = uze_testkit::temp::scratch("detach-missing");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, "just user content\n").unwrap();
        let result = detach(&file, "id", "x").unwrap();
        assert_eq!(result.state, AttachmentState::Missing);
        assert_eq!(fs::read_to_string(&file).unwrap(), "just user content\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drifted_region_blocks_destructive_detach_per_adr_009() {
        let root = uze_testkit::temp::scratch("detach-drifted");
        let file = root.join("NOTES.md");
        attach(&file, "id", "original").unwrap();
        let tampered = fs::read_to_string(&file)
            .unwrap()
            .replace("original", "user-edited");
        fs::write(&file, &tampered).unwrap();

        let result = detach(&file, "id", "original").unwrap();
        assert_eq!(result.state, AttachmentState::Drifted);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            tampered,
            "a drifted region must never be destructively removed"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detach_leaves_an_empty_file_rather_than_deleting_a_preexisting_file() {
        let root = uze_testkit::temp::scratch("detach-empty");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, "").unwrap(); // pre-existing, empty, user-owned file
        attach(&file, "id", "content").unwrap();
        detach(&file, "id", "content").unwrap();
        assert!(
            file.exists(),
            "detach never deletes a file it found pre-existing"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiple_regions_from_different_identities_coexist_and_detach_independently() {
        let root = uze_testkit::temp::scratch("multi-region");
        let file = root.join("NOTES.md");
        attach(&file, "pkg-a/instructions", "content A").unwrap();
        attach(&file, "pkg-b/instructions", "content B").unwrap();
        assert_eq!(
            inspect(&file, "pkg-a/instructions", "content A").state,
            AttachmentState::Matched
        );
        assert_eq!(
            inspect(&file, "pkg-b/instructions", "content B").state,
            AttachmentState::Matched
        );

        detach(&file, "pkg-a/instructions", "content A").unwrap();
        assert_eq!(
            inspect(&file, "pkg-a/instructions", "content A").state,
            AttachmentState::Missing
        );
        assert_eq!(
            inspect(&file, "pkg-b/instructions", "content B").state,
            AttachmentState::Matched,
            "detaching one package's region must not disturb another's"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crlf_files_are_preserved_through_attach_and_detach() {
        let root = uze_testkit::temp::scratch("crlf");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, "user text A\r\nuser text B\r\n").unwrap();
        attach(&file, "id", "managed").unwrap();
        let raw = fs::read(&file).unwrap();
        let content = String::from_utf8(raw).unwrap();
        assert!(content.contains("\r\n"), "CRLF style must be preserved");
        assert!(!content.replace("\r\n", "").contains('\r'), "no stray CR");

        detach(&file, "id", "managed").unwrap();
        let restored = fs::read(&file).unwrap();
        assert_eq!(restored, b"user text A\r\nuser text B\r\n");
        fs::remove_dir_all(root).unwrap();
    }

    /// One pasted CRLF line is common in a hand-kept `AGENTS.md`, and used
    /// to make the whole file CRLF the first time UZE attached a region:
    /// a whole-file diff in the user's repository, from an operation that
    /// only ever owns its own delimited slice.
    #[test]
    fn a_file_of_mixed_line_endings_comes_back_byte_for_byte() {
        let root = uze_testkit::temp::scratch("mixed-endings");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, b"a\nb\r\nc\n").unwrap();

        attach(&file, "id", "managed").unwrap();
        let attached = fs::read(&file).unwrap();
        assert!(
            attached.starts_with(b"a\nb\r\nc\n"),
            "every untouched line keeps its own terminator: {}",
            String::from_utf8_lossy(&attached)
        );
        assert!(
            attached.ends_with(b"<!-- uze:begin id -->\nmanaged\n<!-- uze:end id -->\n"),
            "inserted lines take the file's majority style: {}",
            String::from_utf8_lossy(&attached)
        );

        detach(&file, "id", "managed").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"a\nb\r\nc\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_file_missing_a_trailing_newline_is_normalized_on_attach() {
        let root = uze_testkit::temp::scratch("no-trailing-newline");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        fs::write(&file, "user text with no trailing newline").unwrap();
        attach(&file, "id", "managed").unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("user text with no trailing newline\n<!-- uze:begin id -->"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unicode_content_round_trips_untouched() {
        let root = uze_testkit::temp::scratch("unicode");
        let file = root.join("NOTES.md");
        attach(&file, "id", "café — 日本語 — emoji 🎉").unwrap();
        assert_eq!(
            inspect(&file, "id", "café — 日本語 — emoji 🎉").state,
            AttachmentState::Matched
        );
        detach(&file, "id", "café — 日本語 — emoji 🎉").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn region_shape_previews_remove_unconditionally_without_writing() {
        let root = uze_testkit::temp::scratch("shape-preview");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("NOTES.md");
        assert_eq!(region_shape(&file, "id"), RegionShape::Absent);

        attach(&file, "id", "content").unwrap();
        let before = fs::read_to_string(&file).unwrap();
        assert_eq!(region_shape(&file, "id"), RegionShape::WellFormed);
        let after = fs::read_to_string(&file).unwrap();
        assert_eq!(before, after, "region_shape must never write");

        fs::write(
            &file,
            "<!-- uze:begin dup -->\na\n<!-- uze:end dup -->\n<!-- uze:begin dup -->\nb\n<!-- uze:end dup -->\n",
        )
        .unwrap();
        assert_eq!(region_shape(&file, "dup"), RegionShape::Malformed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn has_content_outside_managed_regions_distinguishes_user_content_from_pure_regions() {
        let root = uze_testkit::temp::scratch("outside-content");
        let file = root.join("NOTES.md");
        assert!(
            !has_content_outside_managed_regions(&file),
            "missing file has no content"
        );

        attach(&file, "id", "managed only").unwrap();
        assert!(
            !has_content_outside_managed_regions(&file),
            "a file holding only a well-formed region has no *outside* content"
        );

        let mut content = fs::read_to_string(&file).unwrap();
        content = format!("user note\n{content}");
        fs::write(&file, &content).unwrap();
        assert!(has_content_outside_managed_regions(&file));

        // A malformed region's own lines count as unattributable content.
        fs::write(
            &file,
            "<!-- uze:begin dup -->\na\n<!-- uze:end dup -->\n<!-- uze:begin dup -->\nb\n<!-- uze:end dup -->\n",
        )
        .unwrap();
        assert!(has_content_outside_managed_regions(&file));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn any_region_present_detects_regardless_of_identity() {
        let root = uze_testkit::temp::scratch("any-present");
        let file = root.join("NOTES.md");
        assert!(!any_region_present(&file), "missing file has no regions");
        attach(&file, "id", "content").unwrap();
        assert!(any_region_present(&file));
        detach(&file, "id", "content").unwrap();
        assert!(!any_region_present(&file));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconcile_creates_when_wanted_and_removes_when_not() {
        let root = uze_testkit::temp::scratch("reconcile");
        let file = root.join("NOTES.md");
        assert_eq!(
            reconcile(&file, "id", "content", true).state,
            AttachmentState::Matched
        );
        assert_eq!(
            reconcile(&file, "id", "content", true).state,
            AttachmentState::Matched,
            "reconciling an already-wanted region is a no-op, not a rewrite"
        );
        assert_eq!(
            reconcile(&file, "id", "content", false).state,
            AttachmentState::Missing
        );
        assert_eq!(
            reconcile(&file, "id", "content", false).state,
            AttachmentState::Missing,
            "reconciling an already-absent, unwanted region is a safe no-op"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconcile_refuses_to_remove_a_drifted_but_unwanted_region() {
        let root = uze_testkit::temp::scratch("reconcile-drift");
        let file = root.join("NOTES.md");
        attach(&file, "id", "original").unwrap();
        let tampered = fs::read_to_string(&file)
            .unwrap()
            .replace("original", "user-edited");
        fs::write(&file, &tampered).unwrap();
        let result = reconcile(&file, "id", "original", false);
        assert_eq!(result.state, AttachmentState::Drifted);
        assert_eq!(fs::read_to_string(&file).unwrap(), tampered);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn region_identities_present_lists_every_well_formed_region() {
        let root = uze_testkit::temp::scratch("identities");
        let file = root.join("NOTES.md");
        assert!(region_identities_present(&file).is_empty());
        attach(&file, "package:a:instructions", "A").unwrap();
        attach(&file, "package:b:instructions", "B").unwrap();
        assert_eq!(
            region_identities_present(&file),
            vec![
                "package:a:instructions".to_owned(),
                "package:b:instructions".to_owned()
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_unconditionally_removes_a_well_formed_orphaned_region_but_refuses_malformed_ones() {
        let root = uze_testkit::temp::scratch("orphan");
        let file = root.join("NOTES.md");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, "user text A\n").unwrap();
        attach(&file, "package:gone:instructions", "old content").unwrap();
        let mut content = fs::read_to_string(&file).unwrap();
        content.push_str("user text B\n");
        fs::write(&file, &content).unwrap();

        let result = remove_unconditionally(&file, "package:gone:instructions").unwrap();
        assert_eq!(result.state, AttachmentState::Missing);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "user text A\nuser text B\n"
        );

        // A malformed/duplicated shape still refuses, exactly like detach.
        fs::write(
            &file,
            "<!-- uze:begin dup -->\na\n<!-- uze:end dup -->\n<!-- uze:begin dup -->\nb\n<!-- uze:end dup -->\n",
        )
        .unwrap();
        assert_eq!(
            remove_unconditionally(&file, "dup").unwrap().state,
            AttachmentState::Blocked
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_carrying_a_marker_line_is_refused_and_the_file_is_untouched() {
        let root = uze_testkit::temp::scratch("marker-content");
        let file = root.join("NOTES.md");
        fs::create_dir_all(&root).unwrap();
        attach(&file, "package:b:instructions", "B").unwrap();
        let before = fs::read_to_string(&file).unwrap();

        let smuggled = "intro\n<!-- uze:end package:b:instructions -->\ntail";
        let refusal = attach(&file, "package:a:instructions", smuggled).unwrap_err();

        assert!(matches!(
            refusal,
            UzeError::ManagedRegionContentCarriesMarker { .. }
        ));
        assert!(refusal.to_string().starts_with(
            "the instructions for `package:a:instructions` contain a line UZE uses as a region \
             marker (`<!-- uze:begin …` / `<!-- uze:end …`); remove it from the plugin's content"
        ));

        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converge_removes_the_stale_and_creates_the_desired_around_user_text() {
        let root = uze_testkit::temp::scratch("converge");
        let file = root.join("NOTES.md");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, "user text\n").unwrap();
        attach(&file, "package:gone:instructions", "old").unwrap();
        attach(&file, "package:kept:instructions", "kept").unwrap();
        let owns = |identity: &str| identity.starts_with("package:");

        let convergence = converge(
            &file,
            owns,
            &[
                ("package:kept:instructions".to_owned(), "kept".to_owned()),
                ("package:new:instructions".to_owned(), "new".to_owned()),
                (
                    "package:bad:instructions".to_owned(),
                    "<!-- uze:begin package:x:instructions -->".to_owned(),
                ),
            ],
        );

        assert_eq!(convergence.removed, vec!["package:gone:instructions"]);
        assert!(convergence.blocked.is_empty());
        let states: Vec<_> = convergence
            .desired
            .iter()
            .map(|region| (region.inspection.state, region.write_failure.is_some()))
            .collect();
        assert_eq!(
            states,
            vec![
                (AttachmentState::Matched, false),
                (AttachmentState::Matched, false),
                (AttachmentState::Missing, true),
            ]
        );
        assert!(
            convergence.desired[2]
                .write_failure
                .as_deref()
                .is_some_and(|failure| failure.contains("region marker"))
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "user text\n\
             <!-- uze:begin package:kept:instructions -->\nkept\n<!-- uze:end package:kept:instructions -->\n\
             <!-- uze:begin package:new:instructions -->\nnew\n<!-- uze:end package:new:instructions -->\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converge_on_a_missing_file_with_nothing_to_create_leaves_it_missing() {
        let root = uze_testkit::temp::scratch("converge-absent");
        let file = root.join("NOTES.md");
        let convergence = converge(
            &file,
            |_| true,
            &[("has spaces".to_owned(), "x".to_owned())],
        );
        assert!(!file.exists());
        assert_eq!(
            convergence.desired[0].inspection.state,
            AttachmentState::Blocked
        );
        assert!(convergence.desired[0].write_failure.is_some());
    }

    #[test]
    fn invalid_region_identity_is_rejected_not_sanitized() {
        let root = uze_testkit::temp::scratch("invalid-id");
        let file = root.join("NOTES.md");
        assert!(attach(&file, "has spaces", "x").is_err());
        assert!(attach(&file, "has\nnewline", "x").is_err());
        assert_eq!(
            inspect(&file, "has spaces", "x").state,
            AttachmentState::Blocked
        );
    }
}
