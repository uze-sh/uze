//! Reading a third party's KDL without letting it take the process down.
//!
//! The `kdl` parser recurses once per nested block with no limit, so a few
//! hundred `{` overflow a thread's stack, which no `catch_unwind` survives;
//! and it takes exponential time on nested slashdashed blocks
//! (kdl-rs#177: 132 malformed bytes, 78 s). So a file reaches the parser
//! only after a size cap and a scan that understands just enough KDL to
//! count real braces, and then on a thread of its own with a stack sized
//! for the depth the scan allowed and a time limit (D22).

use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use kdl::KdlDocument;

use super::Diagnostic;
use super::document::Report;

/// The largest manifest or view read, in bytes.
pub const FILE_MAX: usize = 64 * 1024;
/// The deepest blocks may nest.
pub const DEPTH_MAX: usize = 32;
/// How long a parse may take before the file is refused.
pub const PARSE_LIMIT: Duration = Duration::from_secs(2);
/// Far more than [`DEPTH_MAX`] levels of the parser's recursion take in a
/// debug build, so the stack is never what refuses a file the scan let by.
const PARSE_STACK: usize = 8 * 1024 * 1024;

/// The most `file` may hold: a component's bound for a `.uze`, whose
/// `<view>` is still held to [`FILE_MAX`] on its own; [`FILE_MAX`] for
/// anything else read here.
pub fn max_of(file: &str) -> usize {
    if uze_contract::component::is_component(file) {
        uze_contract::component::COMPONENT_MAX
    } else {
        FILE_MAX
    }
}

/// Reads `path`, refusing it past [`max_of`] without reading the rest.
/// `file` is how a diagnostic names it.
pub fn read(path: &Path, file: &str) -> Result<String, Diagnostic> {
    let unreadable =
        |error: std::io::Error| Diagnostic::whole(file, format!("cannot be read: {error}"), None);
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|opened| opened.take(max_of(file) as u64 + 1).read_to_end(&mut bytes))
        .map_err(unreadable)?;
    decoded(bytes, file)
}

/// `bytes` as the text of `file`, refused past [`max_of`] or when it is
/// not UTF-8, as a file on disk would be.
pub fn decoded(bytes: Vec<u8>, file: &str) -> Result<String, Diagnostic> {
    if bytes.len() > max_of(file) {
        return Err(Diagnostic::whole(
            file,
            format!(
                "is larger than the {} KiB a file may be",
                max_of(file) / 1024
            ),
            Some("split it into smaller views, or move data into a fixture".to_owned()),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        Diagnostic::whole(
            file,
            format!(
                "is not UTF-8 (at byte {})",
                error.utf8_error().valid_up_to()
            ),
            None,
        )
    })
}

/// Text an editor holds for `file`, in place of what is on disk, refused
/// past [`max_of`] like a file would be.
pub fn held(text: &str, file: &str) -> Result<String, Diagnostic> {
    if text.len() > max_of(file) {
        return Err(Diagnostic::whole(
            file,
            format!(
                "is larger than the {} KiB a file may be",
                max_of(file) / 1024
            ),
            Some("split it into smaller views, or move data into a fixture".to_owned()),
        ));
    }
    Ok(text.to_owned())
}

/// Scans, then parses `source` under [`PARSE_LIMIT`]. Every refusal is
/// reported; `None` means there is no document to decode.
pub fn parse(report: &mut Report<'_>) -> Option<KdlDocument> {
    if let Err(refusal) = scan(report.source()) {
        report.at(refusal.offset, refusal.message, refusal.suggestion);
        return None;
    }
    match parse_within(report.source().to_owned(), PARSE_LIMIT) {
        Parsed::Document(document) => Some(document),
        Parsed::Invalid(error) => {
            for diagnostic in error.diagnostics {
                syntax_error(report, &diagnostic);
            }
            None
        }
        Parsed::TimedOut => {
            report.whole(
                format!(
                    "took longer than the {} s a file may take to parse",
                    PARSE_LIMIT.as_secs()
                ),
                Some("look for nested `/-` and unclosed blocks".to_owned()),
            );
            None
        }
        Parsed::NoThread(error) => {
            report.whole(format!("could not be parsed: {error}"), None);
            None
        }
    }
}

enum Parsed {
    Document(KdlDocument),
    Invalid(kdl::KdlError),
    TimedOut,
    NoThread(String),
}

/// Parses on a thread of its own and stops waiting at `limit`. A thread
/// that runs over is left to finish on its own: a thread cannot be killed,
/// and the scan already refused the shapes known to run that long.
fn parse_within(source: String, limit: Duration) -> Parsed {
    let (sender, receiver) = mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new()
        .name("uze-extension-parse".to_owned())
        .stack_size(PARSE_STACK)
        .spawn(move || {
            let _ = sender.send(source.parse::<KdlDocument>());
        });
    if let Err(error) = spawned {
        return Parsed::NoThread(error.to_string());
    }
    match receiver.recv_timeout(limit) {
        Ok(Ok(document)) => Parsed::Document(document),
        Ok(Err(error)) => Parsed::Invalid(error),
        Err(mpsc::RecvTimeoutError::Timeout) => Parsed::TimedOut,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Parsed::NoThread("the parser stopped without an answer".to_owned())
        }
    }
}

/// A parser error, said in this check's words where a v1 spelling caused
/// it: KDL v2 is the only dialect, and the parser reports `true` merely as
/// a malformed identifier.
fn syntax_error(report: &mut Report<'_>, diagnostic: &kdl::KdlDiagnostic) {
    let offset = diagnostic.span.offset();
    let end = (offset + diagnostic.span.len()).min(report.source().len());
    let text = report
        .source()
        .get(offset..end)
        .unwrap_or_default()
        .to_owned();
    if let Some((message, suggestion)) = v1_spelling(&text) {
        report.at(offset, message, Some(suggestion));
        return;
    }
    let message = diagnostic
        .message
        .clone()
        .unwrap_or_else(|| "is not valid KDL".to_owned());
    report.at(offset, message, diagnostic.help.clone());
}

fn v1_spelling(text: &str) -> Option<(String, String)> {
    let word = text.trim();
    match word {
        "true" | "false" => Some((
            "KDL v2 writes booleans as `#true` and `#false`".to_owned(),
            format!("write `#{word}`"),
        )),
        "null" => Some((
            "KDL v2 writes null as `#null`".to_owned(),
            "write `#null`".to_owned(),
        )),
        _ if word.starts_with("r\"") || word.starts_with("r#") => Some((
            "KDL v2 writes raw strings without the `r`".to_owned(),
            format!("write `{}`", &word[1..]),
        )),
        _ => None,
    }
}

/// Why the scan refused a file, and where.
#[derive(Debug, Eq, PartialEq)]
pub struct Refusal {
    pub offset: usize,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Walks `source` the way the parser will, skipping strings and comments,
/// and refuses nesting past [`DEPTH_MAX`], a slashdashed block inside
/// another (kdl-rs#177's shape) and braces that do not balance.
pub fn scan(source: &str) -> Result<(), Refusal> {
    Scanner::new(source).run()
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Each open block's offset, and whether a `/-` comments it out.
    open: Vec<(usize, bool)>,
    /// A `/-` was read and the next block, until the node ends, is
    /// commented out with it.
    slashdash: Slashdash,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Slashdash {
    None,
    /// Read, with nothing after it yet: a line break does not end it.
    Fresh,
    /// Read, and what it comments out has begun.
    Applied,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            at: 0,
            open: Vec::new(),
            slashdash: Slashdash::None,
        }
    }

    fn run(mut self) -> Result<(), Refusal> {
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b'"' => self.skip_quoted(self.at)?,
                b'#' => self.skip_hash()?,
                b'/' if self.peek(1) == Some(b'/') => self.skip_line(),
                b'/' if self.peek(1) == Some(b'*') => self.skip_block_comment()?,
                b'/' if self.peek(1) == Some(b'-') => {
                    self.slashdash = Slashdash::Fresh;
                    self.at += 2;
                }
                b'{' => self.open_block()?,
                b'}' => self.close_block()?,
                b';' => {
                    self.slashdash = Slashdash::None;
                    self.at += 1;
                }
                b'\n' | b'\r' => {
                    if self.slashdash == Slashdash::Applied {
                        self.slashdash = Slashdash::None;
                    }
                    self.at += 1;
                }
                b' ' | b'\t' => self.at += 1,
                _ => {
                    self.mark_token();
                    self.at += 1;
                }
            }
        }
        match self.open.last() {
            Some(&(offset, _)) => Err(Refusal {
                offset,
                message: "this `{` is never closed".to_owned(),
                suggestion: Some("close it with `}`".to_owned()),
            }),
            None => Ok(()),
        }
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.at + ahead).copied()
    }

    fn mark_token(&mut self) {
        if self.slashdash == Slashdash::Fresh {
            self.slashdash = Slashdash::Applied;
        }
    }

    fn open_block(&mut self) -> Result<(), Refusal> {
        let commented = self.slashdash != Slashdash::None;
        if commented && self.open.iter().any(|&(_, commented)| commented) {
            return Err(Refusal {
                offset: self.at,
                message: "a block commented out with `/-` sits inside another".to_owned(),
                suggestion: Some(
                    "remove the inner `/-`, or comment the outer block with `/* */`".to_owned(),
                ),
            });
        }
        if self.open.len() == DEPTH_MAX {
            return Err(Refusal {
                offset: self.at,
                message: format!("blocks nest deeper than the {DEPTH_MAX} levels a file may"),
                suggestion: Some(
                    "flatten the view, or move repeated parts into a `template`".to_owned(),
                ),
            });
        }
        self.open.push((self.at, commented));
        self.slashdash = Slashdash::None;
        self.at += 1;
        Ok(())
    }

    fn close_block(&mut self) -> Result<(), Refusal> {
        if self.open.pop().is_none() {
            return Err(Refusal {
                offset: self.at,
                message: "this `}` closes no block".to_owned(),
                suggestion: Some("remove it, or open the block it closes".to_owned()),
            });
        }
        self.slashdash = Slashdash::None;
        self.at += 1;
        Ok(())
    }

    fn unterminated(&self, start: usize, what: &str) -> Refusal {
        Refusal {
            offset: start,
            message: format!("this {what} is never closed"),
            suggestion: None,
        }
    }

    /// A `"…"` or `"""…"""` string, with its escapes.
    fn skip_quoted(&mut self, start: usize) -> Result<(), Refusal> {
        self.mark_token();
        let multiline = self.bytes[self.at..].starts_with(b"\"\"\"");
        self.at += if multiline { 3 } else { 1 };
        loop {
            match self.bytes.get(self.at) {
                None => return Err(self.unterminated(start, "string")),
                Some(b'\\') => self.at += 2,
                Some(b'"') if !multiline => {
                    self.at += 1;
                    return Ok(());
                }
                Some(b'"') if self.bytes[self.at..].starts_with(b"\"\"\"") => {
                    self.at += 3;
                    return Ok(());
                }
                Some(_) => self.at += 1,
            }
        }
    }

    /// `#true` and friends, or a raw string: `#"…"#`, `##"…"##`, `#"""…"""#`.
    fn skip_hash(&mut self) -> Result<(), Refusal> {
        self.mark_token();
        let start = self.at;
        let hashes = self.bytes[self.at..]
            .iter()
            .take_while(|&&byte| byte == b'#')
            .count();
        self.at += hashes;
        if self.bytes.get(self.at) != Some(&b'"') {
            return Ok(());
        }
        let quotes: &[u8] = if self.bytes[self.at..].starts_with(b"\"\"\"") {
            b"\"\"\""
        } else {
            b"\""
        };
        self.at += quotes.len();
        let mut close = quotes.to_vec();
        close.extend(std::iter::repeat_n(b'#', hashes));
        match find(&self.bytes[self.at..], &close) {
            Some(found) => {
                self.at += found + close.len();
                Ok(())
            }
            None => Err(self.unterminated(start, "raw string")),
        }
    }

    fn skip_line(&mut self) {
        while let Some(&byte) = self.bytes.get(self.at) {
            if byte == b'\n' {
                return;
            }
            self.at += 1;
        }
    }

    /// `/* … */`, which nests in KDL.
    fn skip_block_comment(&mut self) -> Result<(), Refusal> {
        let start = self.at;
        let mut depth = 0usize;
        while self.at < self.bytes.len() {
            if self.bytes[self.at..].starts_with(b"/*") {
                depth += 1;
                self.at += 2;
            } else if self.bytes[self.at..].starts_with(b"*/") {
                depth -= 1;
                self.at += 2;
                if depth == 0 {
                    return Ok(());
                }
            } else {
                self.at += 1;
            }
        }
        Err(self.unterminated(start, "comment"))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn refused(source: &str) -> Refusal {
        scan(source).expect_err(source)
    }

    /// kdl-rs#177's reproducer at the size it reports taking 78 s.
    fn pathological() -> String {
        "a /-{\n".repeat(22)
    }

    #[test]
    fn a_four_hundred_deep_nest_is_refused_by_the_scan() {
        let source = "a {".repeat(400) + &"}".repeat(400);
        assert!(source.len() < 2048);
        let refusal = refused(&source);
        assert_eq!(refusal.offset, DEPTH_MAX * 3 + 2);
        assert!(refusal.message.contains("32"), "{}", refusal.message);
    }

    #[test]
    fn thirty_two_levels_are_allowed() {
        let source = "a {".repeat(DEPTH_MAX) + &"}".repeat(DEPTH_MAX);
        assert_eq!(scan(&source), Ok(()));
    }

    #[test]
    fn braces_inside_strings_and_comments_do_not_count() {
        let source = format!(
            "a \"{open}\" #\"{open}\"# ##\"\"#{open}\"## \"\"\"\n{open}\\\"\n\"\"\" // {open}\n/* /* {open} */ {open} */ b {{ }}",
            open = "{".repeat(40)
        );
        assert_eq!(scan(&source), Ok(()));
    }

    #[test]
    fn unbalanced_braces_are_refused_where_they_are() {
        assert_eq!(refused("a {\nb {\n}").offset, 2);
        assert_eq!(refused("a }").offset, 2);
        assert_eq!(refused("a \"open").offset, 2);
        assert_eq!(refused("/* never").offset, 0);
    }

    #[test]
    fn a_slashdash_inside_a_commented_block_is_fine_unless_it_opens_one() {
        assert_eq!(scan("/-a {\n /-b\n c 1\n}\nd"), Ok(()));
        assert_eq!(scan("a /-{ b }\nc /-{ d }"), Ok(()));
        assert_eq!(scan("/-\na {\n}\n/-b {\n}"), Ok(()));
        assert_eq!(refused("/- a {\n /- b {\n }\n}").offset, 13);
    }

    #[test]
    fn the_exponential_slashdash_input_is_refused_at_once() {
        let source = pathological();
        assert!(source.len() <= 132);
        let started = Instant::now();
        let refusal = refused(&source);
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(refusal.message.contains("/-"), "{}", refusal.message);
    }

    #[test]
    fn a_parse_that_runs_over_its_limit_is_abandoned_on_time() {
        // Past the scan, to prove the timer alone also holds: small enough
        // that the abandoned thread finishes soon after the test.
        let source = "a /-{\n".repeat(16);
        let limit = Duration::from_millis(150);
        let started = Instant::now();
        let parsed = parse_within(source, limit);
        assert!(matches!(parsed, Parsed::TimedOut));
        assert!(started.elapsed() < limit + Duration::from_millis(500));
    }

    #[test]
    fn a_v1_spelling_is_answered_with_its_v2_form() {
        let mut report = Report::new("view.uze", "Toggle \"x\" id=\"x\" on=true");
        assert!(parse(&mut report).is_none());
        let diagnostic = &report.diagnostics()[0];
        assert_eq!((diagnostic.line, diagnostic.column), (1, 22));
        assert!(diagnostic.message.contains("#true"), "{diagnostic}");
        assert_eq!(diagnostic.suggestion.as_deref(), Some("write `#true`"));

        let mut report = Report::new("view.uze", "a null");
        parse(&mut report);
        assert!(report.diagnostics()[0].message.contains("#null"));
    }

    #[test]
    fn a_file_past_the_cap_is_refused_unread() {
        let dir = uze_testkit::temp::TempDir::new("extension-cap");
        let path = dir.join("big.kdl");
        std::fs::write(&path, "a\n".repeat(FILE_MAX)).unwrap();
        let refusal = read(&path, "big.kdl").unwrap_err();
        assert!(refusal.message.contains("64 KiB"), "{refusal}");
    }
}
