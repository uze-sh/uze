use super::*;
use std::fs;

const DIALECT_TABLE: &str = include_str!("../../../../../tests/_fixtures/rules/glob-dialect.tsv");

fn rule(text: &str) -> std::result::Result<Rule, NotARule> {
    parse_rule(".agents/rules/r.md", text.as_bytes())
}

fn checkout(label: &str, rules: &[(&str, &str)]) -> PathBuf {
    let root = uze_testkit::temp::scratch(label);
    uze_testkit::git::mark_repository(&root);
    if !rules.is_empty() {
        fs::create_dir_all(root.join(RULES_DIR)).unwrap();
    }
    for (name, text) in rules {
        fs::write(root.join(RULES_DIR).join(name), text).unwrap();
    }
    root
}

#[test]
fn every_row_of_the_shared_dialect_table_holds() {
    let mut rows = 0;
    for (number, line) in DIALECT_TABLE.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        let [pattern, path, expected] = columns[..] else {
            panic!("line {}: three tab-separated columns: {line:?}", number + 1);
        };
        rows += 1;
        match expected {
            "invalid" => assert!(
                Glob::parse(pattern).is_err(),
                "line {}: `{pattern}` is outside the dialect",
                number + 1
            ),
            "match" | "no-match" => {
                let glob = Glob::parse(pattern)
                    .unwrap_or_else(|refused| panic!("line {}: {refused}", number + 1));
                assert_eq!(
                    glob.matches(path),
                    expected == "match",
                    "line {}: `{pattern}` against `{path}`",
                    number + 1
                );
            }
            other => panic!("line {}: unknown expectation `{other}`", number + 1),
        }
    }
    assert!(rows > 40, "the table was read: {rows} rows");
}

#[test]
fn an_empty_pattern_is_outside_the_dialect() {
    assert_eq!(
        Glob::parse_list("**/src/**, ,**/tests/**").unwrap_err(),
        vec![DialectViolation {
            pattern: String::new(),
            construct: Construct::Empty
        }]
    );
}

#[test]
fn each_trigger_is_read_with_the_field_it_needs() {
    assert_eq!(
        rule("---\ntrigger: always_on\n---\nbody\n")
            .unwrap()
            .activation,
        Activation::AlwaysOn
    );
    assert_eq!(
        rule("---\ntrigger: manual\n---\n").unwrap().activation,
        Activation::Manual
    );
    assert_eq!(
        rule("---\ntrigger: model_decision\ndescription: When writing tests\n---\n")
            .unwrap()
            .activation,
        Activation::ModelDecision {
            description: "When writing tests".to_owned()
        }
    );
    let glob = rule("---\ntrigger: glob\nglobs: **/src/**/*.test.ts, **/tests/**\n---\n").unwrap();
    assert!(glob.matches("src/core/math.test.ts"));
    assert!(glob.matches("tests/e2e/login.ts"));
    assert!(!glob.matches("src/core/math.ts"));
}

#[test]
fn unquoted_antigravity_patterns_and_quoted_values_both_read() {
    let unquoted = rule("---\ntrigger: glob\nglobs: **/*.ts\n---\n").unwrap();
    assert!(unquoted.matches("a/b.ts"));
    let quoted = rule("---\ntrigger: \"glob\"\nglobs: '**/src/ui/**' # the UI\n---\n").unwrap();
    assert!(quoted.matches("src/ui/button.ts"));
}

#[test]
fn a_field_outside_the_trigger_s_is_ignored() {
    let read = rule(
        "---\ntrigger: model_decision\ndescription: When migrating\nowner: data-team\ntags:\n  - db\nglobs: not read here\n---\n",
    )
    .unwrap();
    assert_eq!(
        read.activation,
        Activation::ModelDecision {
            description: "When migrating".to_owned()
        }
    );
    assert!(!read.matches("not read here"));
}

#[test]
fn every_reason_a_file_is_not_a_rule() {
    let cases: &[(&[u8], NotARule)] = &[
        (
            b"\xff\xfe not text",
            NotARule::Unreadable("it is not UTF-8 text".to_owned()),
        ),
        (b"# Notes\n\nno frontmatter\n", NotARule::NoFrontmatter),
        (b"---\ntrigger: glob\n", NotARule::NoFrontmatter),
        (
            b"---\ntrigger: always_on\njust words\n---\n",
            NotARule::MalformedFrontmatter { line: 2 },
        ),
        (
            b"---\ntrigger: glob\nglobs: **/a/**\nglobs: **/b/**\n---\n",
            NotARule::DuplicateField(Field::Globs),
        ),
        (
            b"---\ntrigger: glob\nglobs:\n  - **/src/**\n---\n",
            NotARule::BlockValue(Field::Globs),
        ),
        (
            b"---\ntrigger: model_decision\ndescription: >\n  folded\n---\n",
            NotARule::BlockValue(Field::Description),
        ),
        (
            b"---\ntrigger: model_decision\ndescription: When writing\n  tests\n---\n",
            NotARule::BlockValue(Field::Description),
        ),
        (b"---\ndescription: hello\n---\n", NotARule::MissingTrigger),
        (b"---\ntrigger:\n---\n", NotARule::MissingTrigger),
        (
            b"---\ntrigger: sometimes\n---\n",
            NotARule::UnknownTrigger("sometimes".to_owned()),
        ),
        (
            b"---\ntrigger: Always_On\n---\n",
            NotARule::UnknownTrigger("Always_On".to_owned()),
        ),
        (
            b"---\ntrigger: glob\n---\n",
            NotARule::MissingField {
                trigger: Trigger::Glob,
                field: Field::Globs,
            },
        ),
        (
            b"---\ntrigger: model_decision\ndescription: \"\"\n---\n",
            NotARule::MissingField {
                trigger: Trigger::ModelDecision,
                field: Field::Description,
            },
        ),
        (
            b"---\ntrigger: glob\nglobs: **/src/**/*.{ts,tsx}, **/docs/[!ab].md, src/ui/**, **/ok/**\n---\n",
            NotARule::OutsideDialect(vec![
                DialectViolation {
                    pattern: "**/src/**/*.{ts,tsx}".to_owned(),
                    construct: Construct::Alternation,
                },
                DialectViolation {
                    pattern: "**/docs/[!ab].md".to_owned(),
                    construct: Construct::Negation,
                },
                DialectViolation {
                    pattern: "src/ui/**".to_owned(),
                    construct: Construct::Unanchored,
                },
            ]),
        ),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            parse_rule(".agents/rules/r.md", bytes).as_ref(),
            Err(expected),
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn a_reason_names_what_an_author_must_change() {
    let refused = rule("---\ntrigger: glob\nglobs: src/**/*.{ts,tsx}\n---\n").unwrap_err();
    let said = refused.to_string();
    assert!(said.contains("src/**/*.{ts,tsx}"), "{said}");
    assert!(said.contains("outside the supported dialect"), "{said}");
    assert!(
        NotARule::NoFrontmatter.to_string().contains("trigger"),
        "a file with no frontmatter is reported for its missing trigger"
    );
}

#[test]
fn crlf_and_a_byte_order_mark_read_like_plain_lf() {
    let read = parse_rule(
        ".agents/rules/r.md",
        "\u{feff}---\r\ntrigger: glob\r\nglobs: **/src/**\r\n---\r\nbody\r\n".as_bytes(),
    )
    .unwrap();
    assert!(read.matches("src/a.rs"));
}

#[test]
fn rules_are_the_markdown_files_directly_under_the_rules_directory() {
    let root = checkout(
        "rules-discovery",
        &[
            (
                "ui.md",
                "---\ntrigger: glob\nglobs: **/src/ui/**\n---\nUI\n",
            ),
            (
                "testing.md",
                "---\ntrigger: model_decision\ndescription: When writing tests\n---\n",
            ),
            ("notes.md", "# notes\n"),
            ("readme.txt", "not markdown"),
        ],
    );
    fs::create_dir_all(root.join(RULES_DIR).join("nested")).unwrap();
    fs::write(
        root.join(RULES_DIR).join("nested/deep.md"),
        "---\ntrigger: always_on\n---\n",
    )
    .unwrap();

    let found = discover(&root).unwrap();
    assert!(found.present);
    let names: Vec<&str> = found.rules.iter().map(|rule| rule.path.as_str()).collect();
    assert_eq!(names, [".agents/rules/testing.md", ".agents/rules/ui.md"]);
    assert_eq!(
        found.not_rules,
        [NotARuleFile {
            path: ".agents/rules/notes.md".to_owned(),
            reason: NotARule::NoFrontmatter,
        }]
    );
    assert_eq!(found.count(Trigger::Glob), 1);
    assert_eq!(found.count(Trigger::ModelDecision), 1);
    assert_eq!(found.count(Trigger::AlwaysOn), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_checkout_without_a_rules_directory_has_no_rules_and_nothing_wrong() {
    let root = checkout("rules-absent", &[]);
    let found = discover(&root).unwrap();
    assert!(!found.present);
    assert!(found.rules.is_empty() && found.not_rules.is_empty());
    assert!(
        !root.join(".agents").exists(),
        "reading rules creates nothing"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_worktree_is_answered_by_its_own_root() {
    let main = checkout(
        "rules-worktree",
        &[("ui.md", "---\ntrigger: always_on\n---\n")],
    );
    let worktree = main.join(".worktrees/abc");
    fs::create_dir_all(worktree.join("src/ui")).unwrap();
    fs::write(worktree.join(".git"), "gitdir: ../../.git/worktrees/abc\n").unwrap();

    let root = checkout_root(&worktree.join("src/ui")).unwrap();
    assert_eq!(root, worktree.canonical().unwrap());
    assert!(
        !discover(&root).unwrap().present,
        "the worktree has no rules of its own, and never inherits the main checkout's"
    );
    assert_eq!(
        checkout_root(&main.join(RULES_DIR).join("ui.md")).unwrap(),
        main.canonical().unwrap()
    );
    fs::remove_dir_all(main).unwrap();
}

#[test]
fn a_control_character_is_outside_the_dialect() {
    assert_eq!(
        Glob::parse("**/src/\u{1b}x/**").unwrap_err().construct,
        Construct::Control
    );
}

/// A case of the shared manifest table staged as a checkout: the root as
/// discovery canonicalizes it, so an absolute entry names the same place.
fn staged(case: &uze_testkit::fixtures::RulesManifestCase) -> PathBuf {
    let scratch = uze_testkit::temp::scratch(&format!("rules-manifest-{}", case.name));
    let root = scratch.canonical().unwrap();
    uze_testkit::git::mark_repository(&root);
    case.stage(&root);
    root
}

#[test]
fn every_case_of_the_shared_manifest_table_holds() {
    let cases = uze_testkit::fixtures::rules_manifest_cases();
    assert!(cases.len() >= 10, "the table is read: {}", cases.len());
    for case in cases {
        let root = staged(&case);
        let found = discover(&root).unwrap();
        assert!(
            found.not_rules.is_empty(),
            "{}: {:?}",
            case.name,
            found.not_rules
        );
        let paths: Vec<&str> = found.rules.iter().map(|rule| rule.path.as_str()).collect();
        assert_eq!(paths, case.expected, "{}", case.name);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn a_registered_folder_is_read_as_the_projects_own_rules() {
    let case = uze_testkit::fixtures::rules_manifest_cases()
        .into_iter()
        .find(|case| case.name == "folder")
        .unwrap();
    let root = staged(&case);
    let found = discover(&root).unwrap();
    assert!(found.present);
    assert_eq!(found.manifest, ManifestFindings::default());
    assert!(
        found
            .rules
            .iter()
            .any(|rule| rule.path == ".agents/rules/team/ui.md")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_manifest_that_does_not_parse_adds_nothing_and_is_reported() {
    for name in ["does-not-parse", "not-an-object", "entries-not-a-list"] {
        let case = uze_testkit::fixtures::rules_manifest_cases()
            .into_iter()
            .find(|case| case.name == name)
            .unwrap();
        let root = staged(&case);
        let found = discover(&root).unwrap();
        let paths: Vec<&str> = found.rules.iter().map(|rule| rule.path.as_str()).collect();
        assert_eq!(paths, [".agents/rules/flat.md"], "{name}");
        assert!(found.manifest.unreadable.is_some(), "{name}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn inherits_is_not_followed_and_is_reported() {
    let case = uze_testkit::fixtures::rules_manifest_cases()
        .into_iter()
        .find(|case| case.name == "inherits")
        .unwrap();
    let root = staged(&case);
    let found = discover(&root).unwrap();
    assert!(found.manifest.inherits);
    assert_eq!(found.manifest.unreadable, None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_manifest_alone_is_a_place_rules_are_kept() {
    let root = checkout("rules-manifest-alone", &[]);
    fs::create_dir_all(root.join("team-rules")).unwrap();
    fs::write(
        root.join("team-rules/a.md"),
        "---\ntrigger: always_on\n---\nA\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".agents")).unwrap();
    fs::write(
        root.join(manifest::MANIFEST_FILE),
        r#"{"entries": [{"path": "team-rules"}]}"#,
    )
    .unwrap();
    assert!(has_rules(&root));
    let found = discover(&root).unwrap();
    assert!(found.present);
    assert_eq!(found.rules[0].path, "team-rules/a.md");
    fs::remove_dir_all(root).unwrap();
}
