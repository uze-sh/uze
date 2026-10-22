//! The shell a project's plain `setup` and `gate` lines run in, the same on
//! every platform.
//!
//! A plain line is read by `deno_task_shell`, which carries its own
//! builtins (`cp`, `rm`, `mkdir`, `test`…), so a line means one thing on
//! Linux, macOS and Windows and never meets a host shell's aliases. A line
//! it cannot read, or one it would run differently from POSIX `sh`, is not
//! portable: [`check`] says why, and the caller runs it in `sh` where `sh`
//! exists ([`crate::shell::ShellCommand`]). Each known divergence is a row of
//! the equivalence suite (`tests/cli/portable_shell.rs`); an entry leaves
//! [`divergence`] only in the change that makes its row agree with `sh`.
//!
//! This module is the only one that names the shell's crate.

use std::{collections::HashMap, fmt, path::Path, rc::Rc};

use deno_task_shell::parser::{
    Command, CommandInner, IoFile, PipeSequence, PipeSequenceOperator, PipelineInner, Redirect,
    RedirectFd, RedirectOp, RedirectOpOutput, Sequence, SequentialList, Word, WordPart,
};

/// Why a line cannot run in the portable shell: what stopped it, where,
/// and what to write instead when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotPortable {
    /// The word or operator the reason is about.
    pub construct: String,
    /// Where it starts, counted in characters from 1.
    pub column: usize,
    pub reason: String,
    /// The known divergence this is, when the shell could read the line.
    pub divergence: Option<Divergence>,
}

/// A construct the portable shell reads but runs differently from POSIX
/// `sh`, each proven by a row of the equivalence suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Divergence {
    /// `${NAME}` is left as text.
    BraceParameter,
    /// `[` is not a builtin, so it runs `/usr/bin/[` or nothing.
    BracketTest,
    /// `echo -n` prints its flag.
    EchoFlag,
    /// A background `&` is waited for.
    Background,
    /// A program's stderr piped into a builtin never ends.
    StderrIntoBuiltin,
    /// A function definition or a `{ …; }` group runs `{` as a program.
    BraceGroup,
    /// A builtin given a flag it does not implement, where the system tool
    /// of the same name would read it.
    UnsupportedFlag,
}

impl Divergence {
    pub const ALL: [Self; 7] = [
        Self::BraceParameter,
        Self::BracketTest,
        Self::EchoFlag,
        Self::Background,
        Self::StderrIntoBuiltin,
        Self::BraceGroup,
        Self::UnsupportedFlag,
    ];
}

impl fmt::Display for NotPortable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` at column {}: {}",
            self.construct, self.column, self.reason
        )
    }
}

/// The builtins the shell runs itself, ahead of any program of the same
/// name. Kept beside the pinned version: a bump that changes the set is
/// caught by `the_builtins_are_the_shells_own`.
pub const BUILTINS: &[&str] = &[
    ":", "cat", "cd", "cp", "echo", "exit", "export", "false", "head", "mkdir", "mv", "pwd", "rm",
    "set", "shopt", "sleep", "test", "true", "unset", "xargs",
];

/// Whether `line` runs in the portable shell, and if not, why.
pub fn check(line: &str) -> Result<(), NotPortable> {
    let list = parse(line)?;
    match divergence(line, &list) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// Runs `line` in `cwd` with this process's environment and streams, and
/// returns its exit status. What `uze shell -c` does; a line [`check`]
/// refuses is never handed here, and one that does not parse exits 2 as
/// `sh` would.
pub fn run(line: &str, cwd: &Path) -> i32 {
    execute(line, cwd, std::env::vars_os().collect(), Streams::Inherited)
}

/// Where a line's standard streams go.
enum Streams {
    Inherited,
    /// Nothing in, everything out discarded: for the builtin census, whose
    /// lines print.
    #[cfg_attr(not(test), allow(dead_code))]
    Silent,
}

fn execute(
    line: &str,
    cwd: &Path,
    environment: HashMap<std::ffi::OsString, std::ffi::OsString>,
    streams: Streams,
) -> i32 {
    let list = match deno_task_shell::parser::parse(line) {
        Ok(list) => list,
        Err(error) => {
            eprintln!("uze shell: {error}");
            return 2;
        }
    };
    let cwd = match std::path::absolute(cwd) {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("uze shell: {}: {error}", cwd.display());
            return 1;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("uze shell: {error}");
            return 1;
        }
    };
    let state = deno_task_shell::ShellState::new(
        environment,
        cwd,
        HashMap::<String, Rc<dyn deno_task_shell::ShellCommand>>::new(),
        Default::default(),
    );
    let (stdin, stdout, stderr) = match streams {
        Streams::Inherited => (
            deno_task_shell::ShellPipeReader::stdin(),
            deno_task_shell::ShellPipeWriter::stdout(),
            deno_task_shell::ShellPipeWriter::stderr(),
        ),
        Streams::Silent => (
            deno_task_shell::pipe().0,
            deno_task_shell::ShellPipeWriter::null(),
            deno_task_shell::ShellPipeWriter::null(),
        ),
    };
    // A background `&` is spawned onto the local set, so the line runs in
    // one: the shell's futures are `!Send` and stay on this thread.
    tokio::task::LocalSet::new().block_on(
        &runtime,
        deno_task_shell::execute_with_pipes(list, state, stdin, stdout, stderr),
    )
}

fn parse(line: &str) -> Result<SequentialList, NotPortable> {
    deno_task_shell::parser::parse(line).map_err(|error| {
        let snippet = error.code_snippet.unwrap_or_default();
        let construct = snippet
            .split_whitespace()
            .next()
            .unwrap_or(snippet.as_str())
            .to_owned();
        NotPortable {
            column: column_of(line, &snippet),
            construct: if construct.is_empty() {
                line.trim().to_owned()
            } else {
                construct
            },
            reason: error.message.trim_end_matches('.').to_lowercase(),
            divergence: None,
        }
    })
}

/// The first construct the portable shell would run differently from `sh`.
fn divergence(line: &str, list: &SequentialList) -> Option<NotPortable> {
    if let Some(column) = unquoted_brace_parameter(line) {
        return Some(NotPortable {
            construct: "${".into(),
            column,
            reason: "the portable shell does not expand `${NAME}`; write `$NAME`".into(),
            divergence: Some(Divergence::BraceParameter),
        });
    }
    let mut found = None;
    visit_list(list, &mut |command| {
        if found.is_none() {
            found = command_divergence(line, command);
        }
    });
    if found.is_some() {
        return found;
    }
    if list.items.iter().any(|item| item.is_async) {
        return Some(NotPortable {
            construct: "&".into(),
            column: column_of(line, "&"),
            reason: "the portable shell waits for a background command before the line ends, \
                     and `sh` does not"
                .into(),
            divergence: Some(Divergence::Background),
        });
    }
    visit_pipes(list, &mut |pipe| {
        if found.is_none() {
            found = stderr_into_builtin(line, pipe);
        }
    });
    found
}

fn command_divergence(line: &str, command: &Command) -> Option<NotPortable> {
    let brace_group = |construct: &str| NotPortable {
        column: column_of(line, construct),
        construct: construct.to_owned(),
        reason: "the portable shell has no functions or `{ …; }` groups; use `&&` or the \
                 `posix`/`windows` pair"
            .into(),
        divergence: Some(Divergence::BraceGroup),
    };
    let simple = match &command.inner {
        CommandInner::Simple(simple) => simple,
        // `name()` reads as a command and an empty subshell.
        CommandInner::Subshell(list) if list.items.is_empty() => return Some(brace_group("()")),
        CommandInner::Subshell(_) => return None,
    };
    let name = simple.args.first().and_then(literal)?;
    if name == "{" {
        return Some(brace_group("{"));
    }
    if let Some(flag) = unsupported_flag(&name, &simple.args[1..]) {
        return Some(NotPortable {
            column: column_of(line, &format!("{name} ")).max(column_of(line, &flag)),
            reason: format!(
                "the portable `{name}` does not implement `{flag}`; the system's `{name}` does"
            ),
            construct: flag,
            divergence: Some(Divergence::UnsupportedFlag),
        });
    }
    if name == "[" {
        return Some(NotPortable {
            column: column_of(line, "["),
            construct: name,
            reason: "`[` is a program on Linux and macOS and nothing on Windows; write `test`"
                .into(),
            divergence: Some(Divergence::BracketTest),
        });
    }
    if name == "echo"
        && let Some(flag) = simple.args.get(1).and_then(literal)
        && flag.len() > 1
        && flag.starts_with('-')
        && flag[1..].chars().all(|c| matches!(c, 'n' | 'e' | 'E'))
    {
        return Some(NotPortable {
            column: column_of(line, &format!("echo {flag}")),
            construct: format!("echo {flag}"),
            reason: format!("the portable `echo` prints `{flag}` instead of reading it as a flag"),
            divergence: Some(Divergence::EchoFlag),
        });
    }
    None
}

/// A program's stderr sent down a pipe into a builtin: the builtin never
/// sees the end of its input and the line never finishes.
fn stderr_into_builtin(line: &str, pipe: &PipeSequence) -> Option<NotPortable> {
    let into_pipe = pipe.op == PipeSequenceOperator::StdoutStderr
        || pipe
            .current
            .redirect
            .as_ref()
            .is_some_and(is_stderr_to_stdout);
    let from_program = command_name(&pipe.current).is_none_or(|name| !is_builtin(&name));
    let into_builtin = first_command(&pipe.next)
        .and_then(command_name)
        .is_some_and(|name| is_builtin(&name));
    (into_pipe && from_program && into_builtin).then(|| {
        let operator = if pipe.op == PipeSequenceOperator::StdoutStderr {
            "|&"
        } else {
            "2>&1"
        };
        NotPortable {
            construct: operator.into(),
            column: column_of(line, operator),
            reason: "a program's stderr piped into a builtin never reaches its end in the \
                     portable shell"
                .into(),
            divergence: Some(Divergence::StderrIntoBuiltin),
        }
    })
}

/// The flags each builtin implements, read from the pinned version's
/// sources: short letters, and long names (a trailing `=` takes a value).
/// `echo` is [`Divergence::EchoFlag`]'s; `test` reads operators, not flags;
/// `:`, `true` and `false` ignore their words as `sh` does.
const FLAGS: &[(&str, &str, &[&str])] = &[
    ("cat", "", &[]),
    ("cd", "", &[]),
    ("cp", "rR", &["recursive"]),
    ("exit", "", &[]),
    ("export", "", &[]),
    ("head", "n", &["lines="]),
    ("mkdir", "p", &["parents"]),
    ("mv", "rR", &["recursive"]),
    ("pwd", "LP", &[]),
    ("rm", "rRfd", &["recursive", "force", "dir"]),
    ("shopt", "su", &[]),
    ("sleep", "", &[]),
    ("unset", "v", &[]),
    ("xargs", "d", &["null", "delimiter="]),
];

/// The first word of `arguments` that `name`, a builtin, would refuse as a
/// flag. A word the shell expands is not judged here: the builtin refuses
/// it at run time, naming it.
fn unsupported_flag(name: &str, arguments: &[Word]) -> Option<String> {
    if name == "set" {
        return unsupported_set_option(arguments);
    }
    let (_, shorts, longs) = FLAGS.iter().find(|(builtin, ..)| *builtin == name)?;
    for word in arguments.iter().map(literal) {
        let Some(word) = word else { continue };
        if word == "--" {
            return None;
        }
        let accepted = if let Some(long) = word.strip_prefix("--") {
            longs
                .iter()
                .any(|accepted| match accepted.strip_suffix('=') {
                    Some(prefix) => long
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('=')),
                    None => long == *accepted,
                })
        } else if let Some(letters) = word.strip_prefix('-').filter(|letters| !letters.is_empty()) {
            letters.chars().all(|letter| shorts.contains(letter))
        } else {
            true
        };
        if !accepted {
            return Some(word);
        }
    }
    None
}

/// `set` implements `-e`/`+e` and `-o`/`+o` with `errexit` or `pipefail`.
fn unsupported_set_option(arguments: &[Word]) -> Option<String> {
    let mut words = arguments.iter().map(literal);
    while let Some(word) = words.next() {
        let Some(word) = word else { continue };
        match word.as_str() {
            "-e" | "+e" => {}
            "-o" | "+o" => match words.next().flatten() {
                Some(option) if option == "errexit" || option == "pipefail" => {}
                Some(option) => return Some(format!("{word} {option}")),
                None => return Some(word),
            },
            _ => return Some(word),
        }
    }
    None
}

fn is_stderr_to_stdout(redirect: &Redirect) -> bool {
    matches!(redirect.maybe_fd, Some(RedirectFd::Fd(2)))
        && matches!(redirect.op, RedirectOp::Output(RedirectOpOutput::Overwrite))
        && matches!(redirect.io_file, IoFile::Fd(1))
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn command_name(command: &Command) -> Option<String> {
    match &command.inner {
        CommandInner::Simple(simple) => simple.args.first().and_then(literal),
        CommandInner::Subshell(_) => None,
    }
}

fn first_command(inner: &PipelineInner) -> Option<&Command> {
    match inner {
        PipelineInner::Command(command) => Some(command),
        PipelineInner::PipeSequence(pipe) => Some(&pipe.current),
    }
}

/// A word's text when it is plain text, quoted or not: what it names
/// before the shell expands anything.
fn literal(word: &Word) -> Option<String> {
    fn text(parts: &[WordPart], into: &mut String) -> Option<()> {
        for part in parts {
            match part {
                WordPart::Text(text) => into.push_str(text),
                WordPart::Quoted(inner) => text(inner, into)?,
                _ => return None,
            }
        }
        Some(())
    }
    let mut spelled = String::new();
    text(word.parts(), &mut spelled).map(|()| spelled)
}

fn visit_list(list: &SequentialList, visit: &mut impl FnMut(&Command)) {
    for item in &list.items {
        visit_sequence(&item.sequence, visit);
    }
}

fn visit_sequence(sequence: &Sequence, visit: &mut impl FnMut(&Command)) {
    match sequence {
        Sequence::ShellVar(_) => {}
        Sequence::Pipeline(pipeline) => visit_pipeline(&pipeline.inner, visit),
        Sequence::BooleanList(list) => {
            visit_sequence(&list.current, visit);
            visit_sequence(&list.next, visit);
        }
    }
}

fn visit_pipeline(inner: &PipelineInner, visit: &mut impl FnMut(&Command)) {
    match inner {
        PipelineInner::Command(command) => visit_command(command, visit),
        PipelineInner::PipeSequence(pipe) => {
            visit_command(&pipe.current, visit);
            visit_pipeline(&pipe.next, visit);
        }
    }
}

fn visit_command(command: &Command, visit: &mut impl FnMut(&Command)) {
    visit(command);
    if let CommandInner::Subshell(list) = &command.inner {
        visit_list(list, visit);
    }
}

fn visit_pipes(list: &SequentialList, visit: &mut impl FnMut(&PipeSequence)) {
    for item in &list.items {
        pipes_in_sequence(&item.sequence, visit);
    }
}

fn pipes_in_sequence(sequence: &Sequence, visit: &mut impl FnMut(&PipeSequence)) {
    match sequence {
        Sequence::ShellVar(_) => {}
        Sequence::Pipeline(pipeline) => pipes_in_pipeline(&pipeline.inner, visit),
        Sequence::BooleanList(list) => {
            pipes_in_sequence(&list.current, visit);
            pipes_in_sequence(&list.next, visit);
        }
    }
}

fn pipes_in_pipeline(inner: &PipelineInner, visit: &mut impl FnMut(&PipeSequence)) {
    match inner {
        PipelineInner::Command(command) => pipes_in_subshell(command, visit),
        PipelineInner::PipeSequence(pipe) => {
            visit(pipe);
            pipes_in_subshell(&pipe.current, visit);
            pipes_in_pipeline(&pipe.next, visit);
        }
    }
}

fn pipes_in_subshell(command: &Command, visit: &mut impl FnMut(&PipeSequence)) {
    if let CommandInner::Subshell(list) = &command.inner {
        visit_pipes(list, visit);
    }
}

/// Where `${` first appears outside single quotes, by character from 1:
/// inside single quotes it is literal for both shells.
fn unquoted_brace_parameter(line: &str) -> Option<usize> {
    let mut single = false;
    let mut characters = line.chars().enumerate().peekable();
    while let Some((index, character)) = characters.next() {
        match character {
            '\'' => single = !single,
            '\\' if !single => {
                characters.next();
            }
            '$' if !single && characters.peek().is_some_and(|(_, next)| *next == '{') => {
                return Some(index + 1);
            }
            _ => {}
        }
    }
    None
}

/// The column, by character from 1, where `fragment` first appears in
/// `line`; the start of the line when it does not.
fn column_of(line: &str, fragment: &str) -> usize {
    line.find(fragment)
        .filter(|_| !fragment.is_empty())
        .map_or(1, |byte| line[..byte].chars().count() + 1)
}

#[cfg(test)]
mod tests;
