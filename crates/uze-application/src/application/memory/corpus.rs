//! Recall over a memory of realistic size: the questions are frozen, and
//! the count they answer may only rise. A change to matching is measured
//! against them, never the questions tuned to the change.

use super::*;
use uze_core::memory::{Fact, render};

/// How many questions today's matching answers. Raise it when a change
/// answers more; never lower it.
const RECALL_FLOOR: usize = 30;

const FACTS: &[(&str, FactKind, &str, &str)] = &[
    (
        "reply-in-ptbr",
        FactKind::Feedback,
        "Answer the operator in Brazilian Portuguese; code and commits stay English",
        "User-facing replies are in Portuguese. Identifiers, commits and docs stay in English.",
    ),
    (
        "no-commit-trailers",
        FactKind::Feedback,
        "Never add co-author or session trailers to commits or request bodies",
        "The operator signs their own work; a trailer naming a tool is noise in the history.",
    ),
    (
        "commit-after-validation",
        FactKind::Feedback,
        "Commit only after the operator validated the change by hand",
        "They test the terminal UI themselves before anything lands. Run the gates, report, stop.",
    ),
    (
        "squash-at-merge",
        FactKind::Feedback,
        "Squash with the merge button; never rewrite a review branch's history",
        "Rebase and force-with-lease keep the branch current; squashing happens at merge.",
    ),
    (
        "no-retry-in-tests",
        FactKind::Feedback,
        "A flaky test is fixed by removing the race, never with a retry loop",
        "Shared descriptors across a fork were the last cause. Sleeping hides it.",
    ),
    (
        "exit-codes-in-pipes",
        FactKind::Feedback,
        "Piping a gate to tail reports the pipe's status; capture the exit code directly",
        "`cargo test | tail` exits zero when the tests fail.",
    ),
    (
        "docs-without-em-dashes",
        FactKind::Feedback,
        "User-facing docs use commas, colons and periods instead of em dashes",
        "README, site copy and guides. Code comments are not affected.",
    ),
    (
        "finish-without-asking",
        FactKind::Feedback,
        "Pick the next item and finish instead of ending a turn with a question",
        "A turn that ends offering two options leaves the work half done.",
    ),
    (
        "uze-gender-ptbr",
        FactKind::Feedback,
        "Em português, uze é masculino: o uze, nunca a uze",
        "Evite textos em inglês que deixem o gênero para a tradução automática.",
    ),
    (
        "decisao-de-arquitetura",
        FactKind::Project,
        "Decisão: o índice da memória é calculado a cada leitura, nunca mantido em disco",
        "Um índice mantido à mão diverge dos fatos que lista; um calculado não diverge.",
    ),
    (
        "wsl-session-kill",
        FactKind::Project,
        "The WSL session died because kill parsed a negative pid as minus one",
        "procps read `-<pid>` by its first digit, so a pid starting with 1 became kill(-1). \
         A process group is now ended with libc::kill directly.",
    ),
    (
        "otel-exit-hang",
        FactKind::Project,
        "Every command waited five seconds at exit because the telemetry endpoint hung",
        "A shell profile exported an OTLP endpoint on a localhost that never answered. The \
         flush is now bounded.",
    ),
    (
        "lab-image-provenance",
        FactKind::Project,
        "The Lab rebuilds a stale image; read the provenance line before a failure",
        "The image is labelled with the checkout it was built from.",
    ),
    (
        "terminal-server-tmp",
        FactKind::Project,
        "systemd-tmpfiles wiped /tmp and orphaned the terminal server",
        "The endpoint fell back to /tmp; the next attach started a second server.",
    ),
    (
        "plugin-freshness",
        FactKind::Project,
        "O commit é a versão: install reproduz, update move",
        "Um marketplace linkado é escopo de máquina.",
    ),
    (
        "marketplace-access",
        FactKind::Project,
        "A marketplace's identity is the shape of its URL, reached over one transport ladder",
        "Host aliases map a short name to a host. A fallback between transports was rejected.",
    ),
    (
        "app-server-daemon-slots",
        FactKind::Project,
        "A harness's app-server daemon keeps checkout slots pinned",
        "It runs detached with its working directory inside the slot.",
    ),
    (
        "windows-sandbox-testing",
        FactKind::Project,
        "Cross-build from WSL and test in Windows Sandbox",
        "Smart App Control blocks unsigned binaries on the host.",
    ),
    (
        "release-checklist",
        FactKind::Project,
        "Bump the workspace version before distributing any binary",
        "Development rebuilds do not need a bump.",
    ),
    (
        "no-desktop-app",
        FactKind::Project,
        "uze is a CLI only; never a desktop, mobile or web app",
        "Review comments go on the operator's list instead.",
    ),
    (
        "agents-md-native",
        FactKind::Project,
        "One harness reads AGENTS.md natively, so the bridge file was deleted",
        "The behaviour has been on by default since an early release.",
    ),
    (
        "secret-scanning",
        FactKind::Project,
        "CI scans the history for secrets; fixtures must not hold real-looking tokens",
        "Build test tokens by concatenation.",
    ),
    (
        "perf-budget",
        FactKind::Project,
        "Every CLI leaf command is classified as budgeted or justified slow",
        "An unclassified command fails the test suite by name.",
    ),
    (
        "public-handles",
        FactKind::Reference,
        "The project's GitHub organisation, X and Reddit handles",
        "One handle across every network.",
    ),
    (
        "jaeger-local",
        FactKind::Reference,
        "make observe starts a local Jaeger on port 16686",
        "Point the OTLP exporter at localhost:4318.",
    ),
    (
        "dependabot-flow",
        FactKind::Reference,
        "Pass the subject when merging dependency bumps; regenerate attributions",
        "Bots revert titles edited by hand.",
    ),
    (
        "design-tokens",
        FactKind::Reference,
        "Colours are theme tokens and glyphs are theme symbols",
        "A colour value written anywhere else fails the architecture suite.",
    ),
    (
        "operator-role",
        FactKind::User,
        "The operator maintains the project alone and reviews every change by hand",
        "",
    ),
    (
        "operator-editor",
        FactKind::User,
        "O operador usa Neovim no WSL com zsh",
        "",
    ),
    (
        "operator-timezone",
        FactKind::User,
        "Works from Brazil, three hours behind UTC",
        "",
    ),
];

/// The words an agent would type, and the fact that answers them.
const QUESTIONS: &[(&[&str], &str)] = &[
    (&["portuguese"], "reply-in-ptbr"),
    (&["pt-br"], "reply-in-ptbr"),
    (&["co-author", "trailer"], "no-commit-trailers"),
    (&["validated", "commit"], "commit-after-validation"),
    (&["squash"], "squash-at-merge"),
    (&["flaky", "retry"], "no-retry-in-tests"),
    (&["exit", "pipe"], "exit-codes-in-pipes"),
    (&["dash"], "docs-without-em-dashes"),
    (&["indice", "memoria"], "decisao-de-arquitetura"),
    (&["kill", "pid"], "wsl-session-kill"),
    (&["telemetry", "exit"], "otel-exit-hang"),
    (&["provenance"], "lab-image-provenance"),
    (&["tmp", "orphaned"], "terminal-server-tmp"),
    (&["versao", "commit"], "plugin-freshness"),
    (&["marketplace", "transport"], "marketplace-access"),
    (&["daemon", "slots"], "app-server-daemon-slots"),
    (&["windows", "sandbox"], "windows-sandbox-testing"),
    (&["version", "release"], "release-checklist"),
    (&["github", "handle"], "public-handles"),
    (&["jaeger"], "jaeger-local"),
    (&["dependency", "bump"], "dependabot-flow"),
    (&["colour", "token"], "design-tokens"),
    (&["editor"], "operator-editor"),
    (&["timezone", "utc"], "operator-timezone"),
    (&["desktop"], "no-desktop-app"),
    (&["agents.md", "bridge"], "agents-md-native"),
    (&["secrets", "fixtures"], "secret-scanning"),
    (&["budgeted", "slow"], "perf-budget"),
    (&["masculino"], "uze-gender-ptbr"),
    (&["genero", "traducao"], "uze-gender-ptbr"),
];

/// The words, and a fact that must not be among the first three.
const NOT: &[(&[&str], &str)] = &[
    (&["desktop"], "agents-md-native"),
    (&["dash"], "dependabot-flow"),
    (&["kill", "pid"], "finish-without-asking"),
];

fn corpus() -> MemoryReport {
    let root = uze_testkit::temp::scratch("app-memory-corpus");
    let (home, project) = (UzeHome::at(root.join("uze")), root.join("repo"));
    std::fs::create_dir_all(&project).unwrap();
    let dir = MemoryStore::new(&home, Some(&project))
        .dir(Scope::Project)
        .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    for (index, (name, kind, description, body)) in FACTS.iter().enumerate() {
        let day = format!("2026-09-{:02}", index % 28 + 1);
        let fact = Fact {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            kind: *kind,
            created: day.clone(),
            updated: day,
            source: Source::Operator,
            body: (*body).to_owned(),
        };
        std::fs::write(dir.join(format!("{name}.md")), render(&fact)).unwrap();
    }
    let report = gather(&home, Some(&project), &[], None);
    assert_eq!(report.entries.len(), FACTS.len(), "{:?}", report.unreadable);
    report
}

fn first_three(report: &MemoryReport, words: &[&str]) -> Vec<String> {
    let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    find::find(report, &words, None)
        .matches
        .into_iter()
        .take(3)
        .map(|found| found.entry.name)
        .collect()
}

#[test]
fn recall_over_the_corpus_never_falls() {
    let report = corpus();
    let missed: Vec<String> = QUESTIONS
        .iter()
        .filter(|(words, answer)| !first_three(&report, words).iter().any(|n| n == answer))
        .map(|(words, answer)| format!("{words:?} -> {answer}"))
        .collect();
    let answered = QUESTIONS.len() - missed.len();
    assert!(
        answered >= RECALL_FLOOR,
        "{answered} of {} answered, the floor is {RECALL_FLOOR}; missed: {missed:#?}",
        QUESTIONS.len()
    );
    assert!(
        answered <= RECALL_FLOOR,
        "{answered} answered: raise RECALL_FLOOR to it; still missed: {missed:#?}"
    );
}

#[test]
fn the_corpus_keeps_unrelated_facts_out_of_the_first_three() {
    let report = corpus();
    for (words, wrong) in NOT {
        let first = first_three(&report, words);
        assert!(!first.iter().any(|n| n == wrong), "{words:?}: {first:?}");
    }
}
