use super::*;

/// Lines over every construct the portable shell runs as `sh` would.
const PORTABLE: &[&str] = &[
    "cargo test --workspace",
    "pnpm install && pnpm build",
    "make lint || exit 1",
    "cd web; npm ci",
    "RUST_LOG=warn cargo build",
    "export NODE_ENV=production && node build.js",
    "echo $HOME/x",
    "echo \"$HOME\" 'single $HOME'",
    "test -f .env || cp .env.example .env",
    "test ! -d build && mkdir build",
    "test \"$CI\" = true",
    "test ! 1 -eq 2",
    "mkdir -p build && cp -r assets build",
    "rm -rf dist",
    "git log --oneline | head -n 5 > recent.txt",
    "cat < input.txt",
    "cargo build 2> errors.log",
    "cargo build >> build.log",
    "VERSION=$(git describe --tags) && echo $VERSION",
    "ls src/*.rs",
    "echo ~/cache",
    "cp file.{txt,bak}",
    "echo 2>&1 | cat",
    "cargo test 2>&1 | grep FAIL",
    "(cd web && npm ci)",
    "! grep -q TODO src/main.rs",
    "echo '${literal}'",
    "echo \\${escaped}",
    "echo \"it's\" '${literal}'",
    ": noop",
    "set -e",
    "set -o pipefail",
    "rm -rf --force -- -weird-name",
    "head --lines=3 notes.txt",
    "xargs -d , echo < list.txt",
    "git ls-files -z | xargs -0 rm",
    "git ls-files '*.orig' | xargs rm -f",
];

#[test]
fn every_construct_it_runs_like_sh_is_portable() {
    for line in PORTABLE {
        assert_eq!(check(line), Ok(()), "`{line}` should be portable");
    }
}

/// Each line the portable shell refuses or would run differently, with
/// the construct it names, where, and the divergence it is.
const REFUSED: &[(&str, &str, usize, Option<Divergence>)] = &[
    ("if [ -f a ]; then make; fi", "if", 1, None),
    ("for f in *; do echo $f; done", "for", 1, None),
    ("while true; do sleep 1; done", "while", 1, None),
    ("case $1 in a) echo a;; esac", "case", 1, None),
    ("build() { make; }", "()", 6, Some(Divergence::BraceGroup)),
    ("{ make; make test; }", "{", 1, Some(Divergence::BraceGroup)),
    // The parser points at the character it stopped on; the reason names
    // the construct whole.
    ("echo $$", "$", 6, None),
    ("echo $#", "#", 7, None),
    ("make 2>&1 > out.txt", "2>&1", 6, None),
    ("echo 'unterminated", "'unterminated", 6, None),
    (
        "cp .env.example ${ENV_FILE}",
        "${",
        17,
        Some(Divergence::BraceParameter),
    ),
    (
        "echo \"${HOME}/x\"",
        "${",
        7,
        Some(Divergence::BraceParameter),
    ),
    (
        "echo \"it's\" ${HOME}",
        "${",
        13,
        Some(Divergence::BraceParameter),
    ),
    (
        "[ -f .env ] && echo has",
        "[",
        1,
        Some(Divergence::BracketTest),
    ),
    ("echo -n done", "echo -n", 1, Some(Divergence::EchoFlag)),
    ("echo -e 'a\\nb'", "echo -e", 1, Some(Divergence::EchoFlag)),
    ("watch &", "&", 7, Some(Divergence::Background)),
    (
        "xargs -p rm -f < list.txt",
        "-p",
        7,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "test -x bin/setup && bin/setup",
        "-x",
        6,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "test -f a -a -f b",
        "-a",
        11,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "head -1 VERSION",
        "-1",
        6,
        Some(Divergence::UnsupportedFlag),
    ),
    ("cp -a src dst", "-a", 4, Some(Divergence::UnsupportedFlag)),
    ("mv -r a b", "-r", 4, Some(Divergence::UnsupportedFlag)),
    ("cd - && make", "-", 4, Some(Divergence::UnsupportedFlag)),
    ("cd && make", "cd", 1, Some(Divergence::UnsupportedFlag)),
    (
        "set -euo pipefail",
        "-euo",
        5,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "make && cat -n log.txt",
        "-n",
        13,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "rm --no-preserve-root -rf build",
        "--no-preserve-root",
        4,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "cargo test 2>&1 | cat",
        "2>&1",
        12,
        Some(Divergence::StderrIntoBuiltin),
    ),
    (
        "cargo test |& xargs echo",
        "|&",
        12,
        Some(Divergence::StderrIntoBuiltin),
    ),
    (
        "true && (make 2>&1 | head -n 5)",
        "2>&1",
        15,
        Some(Divergence::StderrIntoBuiltin),
    ),
    // A command substitution, an assignment and a subshell run in the
    // same shell, so what is refused at the top is refused inside them.
    (
        "VERSION=$(head -1 VERSION) && echo $VERSION",
        "-1",
        16,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "A=$(cp -a x y) make",
        "-a",
        8,
        Some(Divergence::UnsupportedFlag),
    ),
    (
        "test \"$(echo -n abc)\" = abc",
        "echo -n",
        9,
        Some(Divergence::EchoFlag),
    ),
    (
        "X=$(make 2>&1 | cat)",
        "2>&1",
        10,
        Some(Divergence::StderrIntoBuiltin),
    ),
    ("(watch &)", "&", 8, Some(Divergence::Background)),
];

#[test]
fn every_refused_line_names_its_construct_and_column() {
    let mismatches = REFUSED
        .iter()
        .filter_map(|(line, construct, column, divergence)| {
            let refused = check(line).err();
            let answer = refused.as_ref().map(|refused| {
                (
                    refused.construct.as_str(),
                    refused.column,
                    refused.divergence,
                )
            });
            let reasoned = refused
                .as_ref()
                .is_some_and(|refused| !refused.reason.is_empty());
            (answer != Some((*construct, *column, *divergence)) || !reasoned).then(|| {
                format!("`{line}`: expected {construct:?}@{column} {divergence:?}, got {refused:?}")
            })
        })
        .collect::<Vec<_>>();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn every_known_divergence_has_a_refused_row() {
    for divergence in Divergence::ALL {
        assert!(
            REFUSED.iter().any(|row| row.3 == Some(divergence)),
            "{divergence:?} has no row in REFUSED: a divergence is refused only with a line that \
             proves it"
        );
    }
}

#[test]
fn a_refusal_reads_as_construct_column_and_reason() {
    let refused = check("for f in *; do echo $f; done").unwrap_err();
    assert_eq!(
        refused.to_string(),
        "`for` at column 1: unsupported reserved word"
    );
}

#[test]
fn a_column_counts_characters_not_bytes() {
    let refused = check("echo ação ${X}").unwrap_err();
    assert_eq!(refused.column, 11);
}

/// One harmless invocation per builtin, each needing nothing on `PATH`.
const BUILTIN_CALLS: &[(&str, &str)] = &[
    (":", ":"),
    ("cat", "cat input.txt"),
    ("cd", "cd ."),
    ("cp", "cp input.txt copy.txt"),
    ("echo", "echo"),
    ("exit", "exit 0"),
    ("export", "export A=1"),
    ("false", "false || true"),
    ("head", "head -n 1 input.txt"),
    ("mkdir", "mkdir -p made"),
    ("mv", "mv copy.txt moved.txt"),
    ("pwd", "pwd"),
    ("rm", "rm -f moved.txt"),
    ("set", "set -e"),
    ("shopt", "shopt -s globstar"),
    ("sleep", "sleep 0"),
    ("test", "test -f input.txt"),
    ("true", "true"),
    ("unset", "unset A"),
    ("xargs", "xargs echo < input.txt"),
];

fn empty_path(directory: &Path) -> HashMap<std::ffi::OsString, std::ffi::OsString> {
    HashMap::from([(
        "PATH".into(),
        directory.join("no-programs").into_os_string(),
    )])
}

#[test]
fn the_builtins_are_the_shells_own() {
    let directory = uze_testkit::temp::TempDir::new("portable-shell");
    std::fs::write(directory.path().join("input.txt"), "line\n").unwrap();
    let names = BUILTIN_CALLS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert_eq!(names, BUILTINS, "every builtin has one call, in order");
    for (name, line) in BUILTIN_CALLS {
        let status = execute(
            line,
            directory.path(),
            empty_path(directory.path()),
            Streams::Silent,
        );
        assert_eq!(status, 0, "`{name}` runs with nothing on PATH");
    }
}

#[test]
fn a_name_that_is_not_a_builtin_needs_a_program() {
    let directory = uze_testkit::temp::TempDir::new("portable-shell");
    for name in ["ls", "grep", "printf", "touch", "env", "which", "["] {
        let status = execute(
            name,
            directory.path(),
            empty_path(directory.path()),
            Streams::Silent,
        );
        assert_eq!(status, 127, "`{name}` is not a builtin of this shell");
    }
}

#[test]
fn a_status_is_the_last_commands() {
    let directory = uze_testkit::temp::TempDir::new("portable-shell");
    let run = |line| {
        execute(
            line,
            directory.path(),
            empty_path(directory.path()),
            Streams::Silent,
        )
    };
    assert_eq!(run("exit 3"), 3);
    assert_eq!(run("false && true"), 1);
    assert_eq!(run("false; true"), 0);
    assert_eq!(run("true | false"), 1);
    assert_eq!(run("missing-program"), 127);
}

#[test]
fn a_flag_the_shell_expands_is_refused_when_it_runs() {
    let directory = uze_testkit::temp::TempDir::new("portable-shell");
    std::fs::write(directory.path().join("a"), "").unwrap();
    let mut environment = empty_path(directory.path());
    environment.insert("FLAG".into(), "-a".into());
    assert_eq!(
        check("cp $FLAG a b"),
        Ok(()),
        "an expanded word is not judged at parse"
    );
    assert_eq!(
        execute(
            "cp $FLAG a b",
            directory.path(),
            environment,
            Streams::Silent
        ),
        1
    );
    assert!(!directory.path().join("b").exists(), "nothing was copied");
}

#[test]
fn a_line_that_does_not_parse_exits_two() {
    let directory = uze_testkit::temp::TempDir::new("portable-shell");
    assert_eq!(
        execute(
            "for f in *; do :; done",
            directory.path(),
            HashMap::new(),
            Streams::Silent
        ),
        2
    );
}

/// The commands of `tests/_fixtures/portable-commands/corpus.tsv`: real
/// one-line commands, each with the repository and commit it came from.
fn corpus() -> Vec<(String, String)> {
    let path = uze_testkit::fixtures::root().join("portable-commands/corpus.tsv");
    let text = std::fs::read_to_string(&path).expect("the corpus is readable");
    text.lines()
        .skip(1)
        .filter_map(|row| {
            let fields = row.split('\t').collect::<Vec<_>>();
            Some((fields.first()?.to_string(), fields.get(4)?.to_string()))
        })
        .collect()
}

/// What E1 recorded (design.md, Evidence): the share of the corpus that runs
/// in the portable shell may grow, and falling below it fails here.
const PORTABLE_AT_LEAST_PER_MILLE: usize = 956;

#[test]
fn the_portable_share_of_real_commands_does_not_fall() {
    let corpus = corpus();
    assert!(
        corpus.len() >= 400,
        "the corpus holds at least 400 commands"
    );
    let mut refused_by = std::collections::BTreeMap::<String, usize>::new();
    let mut portable = 0;
    for (_, line) in &corpus {
        match check(line) {
            Ok(()) => portable += 1,
            Err(refused) => {
                let key = match refused.divergence {
                    Some(divergence) => format!("{divergence:?}"),
                    None => refused.reason,
                };
                *refused_by.entry(key).or_default() += 1;
            }
        }
    }
    let per_mille = portable * 1000 / corpus.len();
    println!(
        "portable: {portable}/{} ({per_mille}‰); refused by: {refused_by:#?}",
        corpus.len()
    );
    assert!(
        per_mille >= PORTABLE_AT_LEAST_PER_MILLE,
        "the portable share fell to {per_mille}‰ from {PORTABLE_AT_LEAST_PER_MILLE}‰"
    );
}

/// Whether a portable line's meaning depends on which `sh` runs it: brace
/// alternatives (bash expands them, dash does not) and an `echo` word with
/// a backslash (dash reads escapes, bash does not). The portable shell
/// answers as bash does.
fn depends_on_which_sh(line: &str) -> bool {
    fn word_has_brace(parts: &[WordPart]) -> bool {
        parts.iter().any(|part| match part {
            WordPart::Brace(_) => true,
            WordPart::Quoted(inner) => word_has_brace(inner),
            _ => false,
        })
    }
    let Ok(list) = parse(line) else { return false };
    nodes(&list).iter().any(|node| {
        let Node::Command(Command {
            inner: CommandInner::Simple(simple),
            ..
        }) = node
        else {
            return false;
        };
        let echo = simple.args.first().and_then(literal).as_deref() == Some("echo");
        simple.args.iter().any(|word| word_has_brace(word.parts()))
            || (echo
                && simple.args[1..]
                    .iter()
                    .filter_map(literal)
                    .any(|word| word.contains('\\')))
    })
}

/// What E2 recorded: the share of the corpus whose meaning differs between
/// the `sh` of one platform and another, which the portable shell settles
/// as bash does. Rising above it fails here.
const DEPENDS_ON_WHICH_SH_AT_MOST_PER_MILLE: usize = 1;

#[test]
fn the_share_that_depends_on_which_sh_does_not_rise() {
    let corpus = corpus();
    let depending = corpus
        .iter()
        .filter(|(_, line)| check(line).is_ok() && depends_on_which_sh(line))
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>();
    let per_mille = depending.len() * 1000 / corpus.len();
    println!(
        "depends on which sh: {}/{} ({per_mille}‰): {depending:#?}",
        depending.len(),
        corpus.len()
    );
    assert!(per_mille <= DEPENDS_ON_WHICH_SH_AT_MOST_PER_MILLE);
}
