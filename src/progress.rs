//! CLI presentation primitives.
//!
//! Text reports remain useful in a pipe, but an interactive terminal gets the
//! same restrained hierarchy as the TUI: warm text, sage for healthy state,
//! amber for attention, and red only for failures.

use std::{io::IsTerminal, sync::Mutex, time::Duration};

use anstyle::{Color, RgbColor, Style};
use indicatif::{ProgressBar, ProgressStyle};
use uze_theme::{Symbol, Token};

// The CLI's half of the design system. Every style below resolves the same
// `uze_theme` token the TUI draws with, so `uze status` and the workspace
// client cannot drift the way two hand-kept palettes always did — and
// `clap_styles()` hands these very values to clap's own Styles builder, so
// even the usage and error text clap generates speaks in one voice.
//
// Functions rather than constants: a theme is resolved at runtime, and the
// active one can change between two invocations of the same process.
fn styled(token: Token) -> Style {
    let rgb = uze_theme::active().color(token);
    Style::new().fg_color(Some(Color::Rgb(RgbColor(rgb.0, rgb.1, rgb.2))))
}

fn bright() -> Style {
    styled(Token::TextBright).bold()
}
// No `.dimmed()` anywhere here: SGR-faint stacked on an already-muted RGB
// color renders inconsistently across terminals (many blend it further
// toward the background), which is what made every gray line look washed
// out. The TUI never combines dim with a color for the same reason — it
// only ever reaches for a darker token.
fn muted() -> Style {
    styled(Token::TextMuted)
}
// Bold muted: section headings need to read as structure, not as another
// muted label, so they get weight instead of a brighter or different hue.
fn heading() -> Style {
    muted().bold()
}
fn accent_style() -> Style {
    styled(Token::Accent)
}
fn warning() -> Style {
    styled(Token::StateWarning)
}
fn danger() -> Style {
    styled(Token::StateDanger)
}

/// `--color`: `auto` asks the terminal, the other two answer for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

static COLOR: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Settles, once per process, what `--color` and `--quiet` asked for.
pub fn configure(color: ColorChoice, quiet: bool) {
    COLOR.store(color as u8, std::sync::atomic::Ordering::Relaxed);
    QUIET.store(quiet, std::sync::atomic::Ordering::Relaxed);
}

pub fn quiet() -> bool {
    QUIET.load(std::sync::atomic::Ordering::Relaxed)
}

/// Leaves a blank line between the command a person typed and what it
/// answers. Only on a terminal: a blank line in a pipe is a line a script
/// has to skip. None closes the output: the prompt that follows brings its
/// own.
pub fn open_frame() {
    if std::io::stdout().is_terminal() && std::io::stderr().is_terminal() {
        println!();
    }
}

// `--color` wins, then `NO_COLOR` and `CLICOLOR_FORCE` — the same order cargo
// and gh read them in — and only then the terminal itself.
fn color_enabled() -> bool {
    match COLOR.load(std::sync::atomic::Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    std::io::stdout().is_terminal() && std::env::var("TERM").is_ok_and(|term| term != "dumb")
}

fn paint(text: impl AsRef<str>, style: Style) -> String {
    let text = text.as_ref();
    if color_enabled() {
        format!("{style}{text}{style:#}")
    } else {
        text.to_owned()
    }
}

/// `text` with the styling this module paints stripped, for a caller that
/// measures or compares what a person sees.
pub fn unpainted(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(char) = chars.next() {
        if char == '\u{1b}' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(char);
        }
    }
    plain
}

/// The columns `text` occupies once drawn.
pub fn width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(unpainted(text).as_str())
}

/// A block of `rgb` itself, for a report whose subject is a colour.
pub fn swatch(rgb: uze_theme::Rgb) -> String {
    paint(
        "██",
        Style::new().fg_color(Some(Color::Rgb(RgbColor(rgb.0, rgb.1, rgb.2)))),
    )
}

pub fn title(text: impl AsRef<str>) -> String {
    paint(text, bright())
}
pub fn label(text: impl AsRef<str>) -> String {
    paint(text, muted())
}
pub fn accent(text: impl AsRef<str>) -> String {
    paint(text, accent_style())
}
pub fn success_text(text: impl AsRef<str>) -> String {
    paint(text, styled(Token::StateSuccess))
}
pub fn warning_text(text: impl AsRef<str>) -> String {
    paint(text, warning())
}
pub fn error_text(text: impl AsRef<str>) -> String {
    paint(text, danger())
}
/// Which part of uze a root command belongs to, which the root help says
/// with the command's hue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandGroup {
    Packages,
    Workspace,
    Machine,
}

pub fn command_name(text: impl AsRef<str>, group: CommandGroup) -> String {
    let token = match group {
        CommandGroup::Packages => Token::CommandPackages,
        CommandGroup::Workspace => Token::CommandWorkspace,
        CommandGroup::Machine => Token::CommandMachine,
    };
    paint(text, styled(token).bold())
}

/// The same palette, handed to `clap`'s own Styles builder so a missing
/// argument or an unrecognized subcommand renders in the same voice as
/// every hand-written report in this module, instead of clap's defaults.
pub fn clap_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .header(heading())
        .usage(heading())
        .literal(accent_style())
        .placeholder(muted())
        .error(danger().bold())
        .valid(accent_style())
        .invalid(danger())
}

/// Renders `rows` as left-aligned, two-space-indented columns, gapped by
/// three spaces, the last column left free. Widths are measured on what a
/// person sees — the text without this module's colour — so a painted cell
/// and a plain one line up. One construction path means every list in the
/// CLI lines up the same way.
pub fn aligned_rows(rows: Vec<Vec<String>>) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| width(cell))
                .max()
                .unwrap_or_default()
        })
        .collect();
    rows.iter()
        .map(|row| {
            let last = row.iter().rposition(|cell| !cell.is_empty()).unwrap_or(0);
            let mut line = String::from("  ");
            for (column, cell) in row.iter().enumerate().take(last + 1) {
                line.push_str(cell);
                if column < last {
                    line.push_str(&" ".repeat(widths[column] - width(cell) + 3));
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The columns there are to draw in: the terminal's, or 80 off one.
pub fn terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .filter(|_| std::io::stdout().is_terminal())
        .map_or(80, |(columns, _)| usize::from(columns))
}

/// [`aligned_rows`], with the last column wrapped to `width` and each
/// continuation hung under where that column starts.
pub fn aligned_rows_wrapped(rows: Vec<Vec<String>>, width: usize) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
    if columns == 0 {
        return String::new();
    }
    let lead: usize = 2
        + (0..columns - 1)
            .map(|column| {
                rows.iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| self::width(cell))
                    .max()
                    .unwrap_or_default()
                    + 3
            })
            .sum::<usize>();
    let room = width.saturating_sub(lead).max(24);
    aligned_rows(rows)
        .lines()
        .map(|line| {
            let plain = unpainted(line);
            if self::width(&plain) <= width || self::width(&plain) <= lead {
                return line.to_owned();
            }
            // Only plain text is wrapped: a description carries no colour.
            let tail = &plain[plain
                .char_indices()
                .nth(lead)
                .map_or(plain.len(), |(index, _)| index)..];
            let head = &line[..line.len() - tail.len()];
            let mut wrapped = head.to_owned();
            let mut current = 0;
            for word in tail.split(' ') {
                let size = self::width(word);
                if current > 0 && current + 1 + size > room {
                    wrapped.push('\n');
                    wrapped.push_str(&" ".repeat(lead));
                    current = 0;
                } else if current > 0 {
                    wrapped.push(' ');
                    current += 1;
                }
                wrapped.push_str(word);
                current += size;
            }
            wrapped
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders several row-groups shown under separate headings (e.g. the
/// top-level help's `Project:` and `Machine:` command lists) against one
/// shared column layout. Calling `aligned_rows` once per group has each
/// group compute its own column width in isolation, so two lists of the
/// same kind of content land at different gutters purely because of how
/// they happened to be split into calls — this builds one table across all
/// of them, then hands back each group's already-aligned lines separately
/// so the caller can still print its own heading before each.
pub fn aligned_groups(groups: Vec<Vec<Vec<String>>>) -> Vec<String> {
    let sizes: Vec<usize> = groups.iter().map(Vec::len).collect();
    let rendered = aligned_rows(groups.into_iter().flatten().collect());
    let mut lines = rendered.lines();
    sizes
        .into_iter()
        .map(|size| lines.by_ref().take(size).collect::<Vec<_>>().join("\n"))
        .collect()
}

/// A block's heading: its name in bold and nothing under it. The blank
/// line before the next block is what separates them, the way cargo and
/// uv lay a report out.
pub fn report_section(name: &str) -> String {
    format!("{}\n", title(name))
}

/// What a read-only report is about, on one line: the subject, and beside
/// it, muted, where it is.
pub fn report_title(name: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("{}  {}\n", title(name), label(detail)),
        None => format!("{}\n", title(name)),
    }
}

/// `path` as a person reads it: under `$HOME` it starts with `~`.
pub fn path(path: &std::path::Path) -> String {
    uze_platform::home::shorten(path)
}

/// `1 plugin`, `3 plugins`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// When the command began, for the time its closing line reports.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// What happened to one thing a command changed. Every command that
/// changes the machine or the project reports in these, one line each,
/// the way a package manager does: the mark says what happened and the
/// rest of the line what it happened to.
#[derive(Clone, Copy)]
pub enum Change {
    Added,
    Removed,
    Updated,
    /// Done, but short of whole: something the reader should know.
    Attention,
    Failed,
}

/// One line of a change report: `+ flow@market 3f2a91c`.
pub fn change(kind: Change, subject: &str, detail: Option<&str>) -> String {
    let mark = match kind {
        Change::Added => success_text("+"),
        Change::Removed => label("-"),
        Change::Updated => accent(glyph(Symbol::ArrowUp)),
        Change::Attention => warning_icon(),
        Change::Failed => error_icon(),
    };
    match detail {
        Some(detail) => format!("{mark} {subject}   {}\n", label(detail)),
        None => format!("{mark} {subject}\n"),
    }
}

/// A line under the change it qualifies, indented beneath it.
pub fn change_detail(kind: Change, subject: &str, detail: Option<&str>) -> String {
    format!("  {}", change(kind, subject, detail))
}

/// A whole change report: which command is speaking, a line per thing
/// changed, and one closing line with the outcome and how long it took —
/// `uze install v1.0.0` over `1 plugin installed [652ms]`, as bun does.
pub fn change_report(verb: &str, lines: &str, outcome: &str) -> String {
    let took = STARTED
        .get()
        .map(|began| format!(" {}", label(format!("[{}]", took_to_say(began.elapsed())))))
        .unwrap_or_default();
    let header = format!(
        "{} {}\n",
        title(format!("uze {verb}")),
        label(format!("v{}", env!("CARGO_PKG_VERSION")))
    );
    if lines.is_empty() {
        format!("{header}\n{outcome}{took}\n")
    } else {
        format!("{header}\n{lines}\n{outcome}{took}\n")
    }
}

/// One `key   value` line, the key padded by what it shows rather than by
/// the bytes its colour adds.
pub fn key_value(key: &str, value: impl AsRef<str>) -> String {
    let pad = 14usize.saturating_sub(width(key)).max(1);
    format!("  {}{}{}", label(key), " ".repeat(pad + 2), value.as_ref())
}

/// The spinner currently drawing, if one is. A question asked while a
/// spinner ticks has to get the terminal to itself — see [`uninterrupted`].
static DRAWING: Mutex<Option<ProgressBar>> = Mutex::new(None);

/// Creates a spinner for long-running operations.
pub fn spinner(message: &str) -> ProgressBar {
    let pb = if quiet() {
        ProgressBar::hidden()
    } else {
        ProgressBar::new_spinner()
    };
    let message = message.trim_end_matches("...").trim_end_matches('…');
    pb.set_message(format!("{message}{}", glyph(Symbol::Ellipsis)));
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.green} {msg}")
            .expect("valid progress template"),
    );
    if let Ok(mut drawing) = DRAWING.lock() {
        *drawing = Some(pb.clone());
    }
    pb
}

/// Whether finished steps, and an attempt that failed before another
/// answered, stay on screen. Without it a step lives on the spinner's line
/// only, and the one that failed is the only one left behind.
static VERBOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The step the spinner is showing, and when it began.
static CURRENT: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

/// Shows what an operation is doing on the spinner drawing now: every step
/// the domain reports takes the spinner's line. With `verbose`, the one it
/// replaces stays above it, checked, with how long it took, and so does an
/// attempt that failed and was followed by another.
pub fn follow_steps(verbose: bool) {
    STARTED.get_or_init(std::time::Instant::now);
    VERBOSE.store(verbose, std::sync::atomic::Ordering::Relaxed);
    uze::steps::listen(on_step);
}

fn on_step(step: &uze::steps::Step) {
    let Some(bar) = DRAWING
        .lock()
        .ok()
        .and_then(|drawing| drawing.clone())
        .filter(|bar| !bar.is_finished())
    else {
        return;
    };
    let verbose = VERBOSE.load(std::sync::atomic::Ordering::Relaxed);
    match describe(step) {
        Some(Described::Now(line)) => {
            // Two receipts at one harness read as one step, not two lines.
            if CURRENT.lock().ok().is_some_and(|current| {
                current
                    .as_ref()
                    .is_some_and(|(showing, _)| *showing == line)
            }) {
                return;
            }
            settle(&bar, true);
            bar.set_message(format!("{line}…"));
            if let Ok(mut current) = CURRENT.lock() {
                *current = Some((line, std::time::Instant::now()));
            }
        }
        Some(Described::Failed(line)) => {
            let began = CURRENT.lock().ok().and_then(|mut current| current.take());
            if verbose {
                let took = began.map(|(_, at)| at.elapsed()).unwrap_or_default();
                say(
                    &bar,
                    &format!("{} {line} {}", error_icon(), label(took_to_say(took))),
                );
            }
        }
        None => {}
    }
}

/// Prints the step still showing as where the operation failed, or, with
/// `verbose`, as finished.
pub fn settle_steps(bar: &ProgressBar, succeeded: bool) {
    settle(bar, succeeded);
}

fn settle(bar: &ProgressBar, succeeded: bool) {
    let Some((line, at)) = CURRENT.lock().ok().and_then(|mut current| current.take()) else {
        return;
    };
    if succeeded && !VERBOSE.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let icon = if succeeded {
        success_icon()
    } else {
        error_icon()
    };
    say(
        bar,
        &format!("{icon} {line} {}", label(took_to_say(at.elapsed()))),
    );
}

/// A line above the spinner, or on stderr when there is no terminal to
/// draw one on — a CI log still gets the account.
fn say(bar: &ProgressBar, line: &str) {
    if quiet() {
        return;
    }
    if bar.is_hidden() {
        eprintln!("{line}");
    } else {
        bar.println(line);
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;
    use uze::steps::Step;

    #[test]
    fn each_step_reads_as_what_it_does_to_what() {
        let reach = Step::of(&[
            ("step", "reach"),
            ("repository", "github.com/hiukky/ai"),
            ("via", "ssh"),
        ]);
        assert_eq!(
            describe(&reach),
            Some(Described::Now(
                "Reaching github.com/hiukky/ai over SSH".to_owned()
            ))
        );
        let failed = Step::of(&[
            ("step", "reach_failed"),
            ("repository", "github.com/hiukky/ai"),
            ("via", "https (anonymous)"),
            ("reason", "terminal prompts disabled"),
        ]);
        assert_eq!(
            describe(&failed),
            Some(Described::Failed(
                "github.com/hiukky/ai over HTTPS: terminal prompts disabled".to_owned()
            ))
        );
        let download = Step::of(&[("step", "download"), ("files", "5")]);
        assert_eq!(
            describe(&download),
            Some(Described::Now("Downloading 5 files".to_owned()))
        );
        assert_eq!(describe(&Step::of(&[("step", "unheard-of")])), None);
    }

    #[test]
    fn a_fast_step_reads_in_milliseconds_and_a_slow_one_in_seconds() {
        assert_eq!(took_to_say(Duration::from_millis(310)), "310ms");
        assert_eq!(took_to_say(Duration::from_millis(2140)), "2.1s");
    }
}

/// `42s`, `12m`, `3h`: how old a fetched copy is, to the unit that matters.
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        _ => format!("{}h", seconds / 3600),
    }
}

/// `310ms` under a second, `2.1s` from there: a fast step reads as fast
/// rather than as `0.0s`, and a slow one is not a wall of digits.
fn took_to_say(took: Duration) -> String {
    if took < Duration::from_secs(1) {
        format!("{}ms", took.as_millis())
    } else {
        format!("{:.1}s", took.as_secs_f32())
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Described {
    /// What is happening now.
    Now(String),
    /// An attempt that did not work, and why; the operation goes on.
    Failed(String),
}

/// The words for a step. Kept here, beside the spinner that shows them,
/// because the domain reports what it does and never how to say it.
fn describe(step: &uze::steps::Step) -> Option<Described> {
    let field = |name: &str| step.get(name).unwrap_or_default().to_owned();
    let over = |via: &str| match via {
        "https (anonymous)" => "over HTTPS".to_owned(),
        "https (credentials)" => "over HTTPS with your credentials".to_owned(),
        "ssh" => "over SSH".to_owned(),
        "local" => "on this machine".to_owned(),
        other => format!("over {other}"),
    };
    Some(match step.get("step")? {
        "reach" => Described::Now(format!(
            "Reaching {} {}",
            field("repository"),
            over(&field("via"))
        )),
        "reach_failed" => Described::Failed(format!(
            "{} {}: {}",
            field("repository"),
            over(&field("via")),
            field("reason")
        )),
        "catalogue" => Described::Now("Reading the marketplace catalogue".to_owned()),
        "fresh" => Described::Now(format!(
            "Using {} as fetched {} ago (uze update fetches the latest)",
            field("repository"),
            ago(field("age_secs").parse().unwrap_or_default())
        )),
        "download" => Described::Now(format!("Downloading {} files", field("files"))),
        "deliver" => Described::Now(format!("Delivering to {}", agents(&field("harness")))),
        "detach" => Described::Now(format!("Removing from {}", agents(&field("harness")))),
        "lock" => Described::Now("Recording it in agents.yaml and agents.lock".to_owned()),
        _ => return None,
    })
}

/// The agents a step names: by name while two fit on the line, and counted
/// past that, when their order would only be noise.
fn agents(names: &str) -> String {
    let count = names.split(", ").filter(|name| !name.is_empty()).count();
    if count > 2 {
        format!("{count} agents")
    } else {
        names.to_owned()
    }
}

/// Runs `question` with the terminal to itself.
///
/// A spinner redraws its line on stderr every 120 ms, and so does the
/// prompt widget — so a question asked mid-operation was overwritten as
/// fast as it was drawn, which for the trust prompt meant a person
/// answering something they could not read. Held only for as long as the
/// question takes; nothing is suspended when no spinner is drawing.
pub fn uninterrupted<T>(question: impl FnOnce() -> T) -> T {
    // Cloned out rather than held: the question may start a spinner of its
    // own, and a lock held across it would deadlock. A finished bar is
    // skipped rather than suspended — suspending one redraws the line it
    // already closed.
    let drawing = DRAWING
        .lock()
        .ok()
        .and_then(|drawing| drawing.clone())
        .filter(|spinner| !spinner.is_finished());
    match drawing {
        Some(spinner) => spinner.suspend(question),
        None => question(),
    }
}

/// A one-line outcome: `✓ theme dracula`.
pub fn success(msg: &str) {
    if !quiet() {
        println!("{} {}", success_icon(), msg);
    }
}
pub fn warn(msg: &str) {
    eprintln!("{} {}", warning_icon(), msg);
}
/// The one way a failure is said: `error: …` on stderr, and under it, when
/// there is one, the command that answers it.
pub fn error(msg: &str, hint: Option<&str>) {
    eprintln!("{} {msg}", paint("error:", danger().bold()));
    if let Some(hint) = hint {
        eprintln!("{}", next_step(hint));
    }
}

/// The last line of a report that leaves something to do: `→ uze install`.
pub fn next_step(command: &str) -> String {
    format!("{} {command}", accent(glyph(Symbol::ArrowTo)))
}

pub fn success_icon() -> String {
    success_text(glyph(Symbol::MarkOk))
}
pub fn warning_icon() -> String {
    warning_text(glyph(Symbol::MarkAttention))
}
pub fn error_icon() -> String {
    error_text(glyph(Symbol::MarkCross))
}
pub fn log_prefix() -> String {
    label(glyph(Symbol::TreeColumnDivider))
}

/// The CLI's half of the symbol library. Same set the TUI draws from, for
/// the same reason: a terminal without a Unicode font is not a terminal UZE
/// should be unusable in, and the marks the CLI prints are as much chrome as
/// the ones the TUI does.
pub fn glyph(symbol: Symbol) -> String {
    uze_theme::active().glyph(symbol).to_owned()
}

/// The columns a glyph occupies. A caller laying a column out from a mark
/// — an interactive prompt's cursor gutter, say — must ask rather than
/// assume one: a theme may have replaced it with something wider.
pub fn glyph_width(symbol: Symbol) -> usize {
    uze_theme::active().symbol(symbol).width().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_and_the_tui_resolve_a_shared_token_to_the_same_colour() {
        // The point of the whole exercise. These two surfaces used to hold
        // separate copies of the palette, in different style types, and a
        // colour changed in one was silently wrong in the other.
        for token in [
            Token::TextBright,
            Token::TextMuted,
            Token::Accent,
            Token::StateWarning,
            Token::StateDanger,
        ] {
            let cli = styled(token).get_fg_color();
            let tui = uze_theme::active().color(token);
            assert_eq!(
                cli,
                Some(Color::Rgb(RgbColor(tui.0, tui.1, tui.2))),
                "`{token}` differs between the CLI and the TUI"
            );
        }
    }

    #[test]
    fn colour_is_dropped_rather_than_approximated_when_the_terminal_will_not_take_it() {
        // `paint` is the only place that decides, and it decides by asking
        // the terminal — a themed CLI must still pipe cleanly.
        let plain = paint("text", accent_style());
        if color_enabled() {
            assert!(plain.contains("text"));
        } else {
            assert_eq!(plain, "text");
        }
    }
}
