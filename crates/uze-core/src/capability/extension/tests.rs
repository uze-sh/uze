use std::path::Path;

use serde_json::json;
use uze_contract::binding::{Path as DataPath, Scope};
use uze_contract::{Binding, CompiledView, Condition, Identity, Item, NodeName, Slot};
use uze_testkit::temp::TempDir;

use super::*;

const MANIFEST: &str = r#"// extension.kdl
protocol 1
id "gsd-band"
name "GSD band"
description "O milestone GSD do checkout"
surface "board" slot="view" view="board.uze" source="board"
surface "summary" slot="section" view="board.uze" source="board" title="GSD"
source "board" { fixture "fixtures/board.json"; }
"#;

const BOARD: &str = r#"template "phase-row" {
    Row gap=1 {
        Text "{number}" min-width=3
        Text "{name}" grow=#true truncate=#true
        Marks {
            Mark role="{d}"
            Mark role="{p}"
            Mark role="{e}"
        }
        Pips done="{summaries}" total="{plans}" when="active"
        Text "{summaries}/{plans}" role="muted" when="finished"
        Text "{status}" role="{tone:e}"
    }
}

lookup "tone" default="muted" {
    done "success"
    partial "warning"
}

lookup "t" locale="pt" { next "▶ executar fase"; }
lookup "t" locale="en" { next "▶ run phase"; }

Stack {
    Row align="center" {
        Activity agent="self"
        Text "GSD · {version} {name}" role="muted" grow=#true
        Tabs id="tab" {
            Tab "Fases" value="phases"
            Tab "Log" value="log"
        }
        Button "limpar" action="clear"
        Button "{t::next} {next.phase}" action="next" item="{next}" when="next"
    }
    Row gap=1 {
        Progress grow=#true {
            Segment each="phases" weight="{weight}" ratio="{ratio}" role="{e}"
        }
        Text "{percent}%"
        Text "· {done} de {total} fases" role="muted"
    }
    List each="phases" show="ui.tab=phases" {
        use "phase-row"
    }
    Log lines="{log}" show-any="ui.tab=log,ui.tab=phases"
}
"#;

fn identity(id: &str) -> Identity {
    Identity {
        marketplace: "luizfbalves".into(),
        plugin: "gsd".into(),
        id: id.into(),
    }
}

/// A plugin directory holding `extensions/<id>/` with `files`.
fn plugin<T: AsRef<str>>(label: &str, extensions: &[(&str, &[(&str, T)])]) -> TempDir {
    let dir = TempDir::new(label);
    for (id, files) in extensions {
        for (file, text) in *files {
            let path = dir.join(EXTENSIONS_DIR).join(id).join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_ref()).unwrap();
        }
    }
    dir
}

/// `view` as a component holding it alone.
fn as_view(view: &str) -> String {
    format!("<view>\n{view}\n</view>\n")
}

fn gsd_files(manifest: &str, board: &str) -> [(&'static str, String); 3] {
    [
        (MANIFEST_FILE, manifest.to_owned()),
        ("board.uze", as_view(board)),
        ("fixtures/board.json", "{}".to_owned()),
    ]
}

fn compile_in(dir: &Path, id: &str) -> Result<CompiledExtension, Vec<Diagnostic>> {
    compile(identity(id), &dir.join(EXTENSIONS_DIR).join(id))
}

/// Compiles `source` as a view, returning it or what is wrong with it.
fn view(source: &str) -> Result<CompiledView, Vec<Diagnostic>> {
    let mut report = Report::new("board.uze", source);
    let compiled = bounded::parse(&mut report)
        .map(|document| view::compile(&document, "board.uze", &mut report));
    match compiled {
        Some(view) if report.diagnostics().is_empty() => Ok(view),
        _ => Err(report.into_diagnostics()),
    }
}

fn refused(source: &str) -> Vec<Diagnostic> {
    match view(source) {
        Ok(_) => panic!("accepted:\n{source}"),
        Err(diagnostics) => diagnostics,
    }
}

fn the_one(source: &str) -> Diagnostic {
    let mut diagnostics = refused(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    diagnostics.remove(0)
}

fn first_node(view: &CompiledView) -> &uze_contract::Node {
    match &view.items[0] {
        Item::Node(node) => node,
        other => panic!("{other:?}"),
    }
}

fn path(scope: Scope, segments: &[&str]) -> DataPath {
    DataPath {
        scope,
        segments: segments.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn a_drawer_surface_compiles() {
    let manifest = MANIFEST.replace(
        r#"surface "summary" slot="section""#,
        r#"surface "summary" slot="drawer""#,
    );
    let dir = plugin(
        "extension-gsd",
        &[("gsd-band", &gsd_files(&manifest, BOARD))],
    );
    let compiled = compile_in(dir.path(), "gsd-band").expect("a drawer compiles");
    assert_eq!(
        compiled.manifest.surface("summary").unwrap().slot,
        Slot::Drawer
    );
}

#[test]
fn the_gsd_band_compiles_into_both_surfaces() {
    let dir = plugin(
        "extension-gsd",
        &[("gsd-band", &gsd_files(MANIFEST, BOARD))],
    );
    let compiled = compile_in(dir.path(), "gsd-band").expect("the gsd band compiles");
    assert_eq!(compiled.identity, identity("gsd-band"));
    assert_eq!(compiled.manifest.surfaces.len(), 2);
    assert_eq!(
        compiled.views.len(),
        1,
        "one view file serves both surfaces"
    );
    let board = compiled.view_of("board").unwrap();
    assert_eq!(compiled.view_of("summary"), Some(board));
    assert_eq!(
        compiled.manifest.surface("summary").unwrap().slot,
        Slot::Section
    );

    assert_eq!(board.file, "board.uze");
    assert_eq!(board.templates.len(), 1);
    assert_eq!(board.lookups.len(), 3);
    let stack = first_node(board);
    assert_eq!(stack.name, NodeName::Stack);
    assert_eq!(stack.children.len(), 4);
    let Item::Node(list) = &stack.children[2] else {
        panic!()
    };
    assert_eq!(list.each, Some(path(Scope::Item, &["phases"])));
    assert_eq!(
        list.condition,
        Some(Condition::Show {
            id: "tab".into(),
            value: "phases".into()
        })
    );
    assert_eq!(
        list.children,
        vec![Item::Use {
            template: "phase-row".into(),
            span: match &list.children[0] {
                Item::Use { span, .. } => *span,
                Item::Node(_) => unreachable!(),
            }
        }]
    );
    let Item::Node(progress_row) = &stack.children[1] else {
        panic!()
    };
    let Item::Node(progress) = &progress_row.children[0] else {
        panic!()
    };
    let Item::Node(segment) = &progress.children[0] else {
        panic!()
    };
    assert_eq!(
        segment.props["weight"],
        Binding::Ref(path(Scope::Item, &["weight"])),
        "a lone reference keeps its type"
    );
    assert_eq!(progress_row.props["gap"], Binding::Literal(json!(1)));
    let Item::Node(log) = &stack.children[3] else {
        panic!()
    };
    assert_eq!(
        log.condition,
        Some(Condition::ShowAny(vec![
            ("tab".into(), "log".into()),
            ("tab".into(), "phases".into())
        ]))
    );
}

#[test]
fn spans_point_back_at_the_source() {
    let compiled = view(BOARD).unwrap();
    let stack = first_node(&compiled);
    assert_eq!(&BOARD[stack.span.offset..stack.span.offset + 5], "Stack");
}

#[test]
fn a_lone_reference_keeps_its_type_and_mixed_text_is_text() {
    let compiled = view(
        r#"Progress { Segment weight="{plans}" role="{tone:e}"; }
lookup "tone" { partial "warning"; }
Text "{plans} planos""#,
    )
    .unwrap();
    let Item::Node(segment) = &first_node(&compiled).children[0] else {
        panic!()
    };
    assert_eq!(
        segment.props["weight"],
        Binding::Ref(path(Scope::Item, &["plans"]))
    );
    assert!(matches!(segment.props["role"], Binding::Lookup { ref table, .. } if table == "tone"));
    let Item::Node(text) = &compiled.items[1] else {
        panic!()
    };
    assert!(matches!(&text.args[0], Binding::Text(parts) if parts.len() == 2));
}

#[test]
fn a_catalog_node_in_the_wrong_case_is_named() {
    let diagnostic = the_one("Text \"a\"\nstack {\n}");
    assert_eq!((diagnostic.line, diagnostic.column), (2, 1));
    assert!(diagnostic.message.contains("PascalCase"), "{diagnostic}");
    assert_eq!(diagnostic.suggestion.as_deref(), Some("write `Stack`"));
    assert_eq!(
        the_one("Template \"x\" { Text \"a\"; }")
            .suggestion
            .as_deref(),
        Some("write `template`")
    );
    let unknown = the_one("Heatmap { }");
    assert!(unknown.suggestion.unwrap().contains("Stack, Row"));
}

#[test]
fn a_brace_pasted_from_jsx_is_caught() {
    let diagnostic = the_one("Stack {\n    Text {id}\n}");
    assert_eq!((diagnostic.line, diagnostic.column), (2, 10));
    assert!(
        diagnostic.message.contains("`Text` takes a string"),
        "{diagnostic}"
    );
    assert_eq!(
        diagnostic.suggestion.as_deref(),
        Some("write `Text \"{id}\"`")
    );
}

#[test]
fn arguments_are_counted() {
    let missing = the_one("Badge role=\"info\"");
    assert!(
        missing
            .message
            .contains("takes one argument, and none is given"),
        "{missing}"
    );
    let extra = the_one("Text \"a\" \"b\"");
    assert_eq!((extra.line, extra.column), (1, 10));
    assert!(extra.message.contains("takes one argument"), "{extra}");
    let leaf = the_one("Text \"a\" {\n    Text \"b\"\n}");
    assert!(leaf.message.contains("holds no other nodes"), "{leaf}");
}

#[test]
fn a_repeated_property_is_refused_rather_than_resolved() {
    let diagnostic = the_one(r#"Button "Go" action="a" action="b""#);
    assert_eq!((diagnostic.line, diagnostic.column), (1, 24));
    assert!(
        diagnostic
            .message
            .contains("`action` is given more than once"),
        "{diagnostic}"
    );
}

#[test]
fn an_unknown_property_lists_what_the_node_takes() {
    let diagnostic = the_one(r#"Text "a" colour="red""#);
    assert!(
        diagnostic
            .message
            .contains("`Text` has no property `colour`")
    );
    assert!(diagnostic.suggestion.as_deref().unwrap().contains("`role`"));
    let case = the_one(r#"Text "a" Role="muted""#);
    assert_eq!(
        case.suggestion.as_deref(),
        Some("properties are kebab-case: write `role=`")
    );
}

#[test]
fn type_annotations_are_refused() {
    let diagnostics = refused(r#"(widget)Text "a" role=(name)"muted""#);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].column, 1);
    assert!(diagnostics[0].message.contains("`(widget)`"));
    assert_eq!(diagnostics[1].column, 23);
}

#[test]
fn a_v1_boolean_gets_its_v2_form() {
    let diagnostic = the_one(r#"Toggle "Compact" id="x" on=true"#);
    assert!(diagnostic.message.contains("#true"), "{diagnostic}");
    assert_eq!((diagnostic.line, diagnostic.column), (1, 28));
}

#[test]
fn a_colour_value_is_not_a_role() {
    let diagnostic = the_one(r##"Text "a" role="#ff0000""##);
    assert!(
        diagnostic.suggestion.unwrap().contains("muted"),
        "the roles are listed"
    );
    let mixed = the_one(r#"Text "a" role="tone-{e}""#);
    assert!(mixed.message.contains("one role"), "{mixed}");
}

#[test]
fn a_box_takes_roles_and_sizes_and_refuses_a_colour_or_a_share_past_whole() {
    let ground = the_one(r##"Box ground="#5b5bf7" { Text "a"; }"##);
    assert!(
        ground.suggestion.unwrap().contains("info"),
        "a ground is a role, and the roles are listed"
    );
    let border = the_one(r##"Box border="line" border-role="#5b5bf7" { Text "a"; }"##);
    assert!(border.message.contains("role"), "{border}");
    let share = the_one(r#"Box width="120%" { Text "a"; }"#);
    assert!(share.message.contains("percentage"), "{share}");
    for accepted in [
        r#"Box direction="row" wrap=#true width="20%" padding-x=1 ground="info" tone="soft" { Text "a"; }"#,
        r#"Box width=12 height=3 border="round" border-role="faint" { Progress ratio=1 style="line"; }"#,
    ] {
        assert!(
            view(accepted).is_ok(),
            "{accepted}: {:#?}",
            view(accepted).err()
        );
    }
}

#[test]
fn identity_is_never_interpolated() {
    let diagnostic = the_one(r#"Button "Go" action="{kind}""#);
    assert!(
        diagnostic.message.contains("`action` must be a literal"),
        "{diagnostic}"
    );
    assert!(
        the_one(r#"List each="{phases}" { Text "a"; }"#)
            .message
            .contains("must be a literal")
    );
    assert!(
        the_one(r#"Tabs id="my tab" { Tab "a" value="a"; }"#)
            .message
            .contains("`id`")
    );
}

#[test]
fn words_counts_and_flags_are_checked() {
    assert!(
        the_one(r#"Row align="middle" { }"#)
            .suggestion
            .unwrap()
            .contains("start, center, end")
    );
    assert!(
        the_one(r#"Row gap="2" { }"#)
            .suggestion
            .unwrap()
            .contains("write `2`")
    );
    assert!(
        the_one(r#"Row gap=-1 { }"#)
            .message
            .contains("a whole number")
    );
    assert!(
        the_one(r#"Text "a" strong="yes""#)
            .message
            .contains("`#true` or `#false`")
    );
    assert!(
        the_one(r#"Text "a" strong=1"#)
            .message
            .contains("`#true` or `#false`")
    );
    assert!(view(r#"Text "a" strong="{bold}" min-width="{w}""#).is_ok());
}

#[test]
fn an_expression_is_not_a_reference() {
    let diagnostic = the_one(r#"Text "{done > 0}""#);
    assert!(
        diagnostic.message.contains("computes nothing"),
        "{diagnostic}"
    );
    assert!(
        the_one(r#"Text "a" when="done > 0""#)
            .message
            .contains("is not a field")
    );
}

#[test]
fn conditions_are_one_per_node_and_name_declared_state() {
    assert!(
        the_one(r#"Text "a" when="x" unless="y""#)
            .message
            .contains("takes one of")
    );
    let parsed = view(
        r#"Tabs id="tab" { Tab "a" value="phases"; }
Text "a" show="ui.tab=phases""#,
    )
    .unwrap();
    let Item::Node(text) = &parsed.items[1] else {
        panic!()
    };
    assert_eq!(
        text.condition,
        Some(Condition::Show {
            id: "tab".into(),
            value: "phases".into()
        })
    );
    assert!(
        the_one(r#"Text "a" show="tab=phases""#)
            .message
            .contains("does not compare")
    );
    assert!(
        the_one(r#"Text "a" show="ui.tab=phases""#)
            .message
            .contains("never set")
    );
    let missing = the_one(
        r#"Tabs id="tab" { Tab "a" value="phases"; }
Text "a" show="ui.tab=log""#,
    );
    assert!(missing.message.contains("no choice `log`"), "{missing}");
    assert!(
        view(
            r#"Toggle "Compact" id="compact"
Text "a" show="ui.compact=true""#
        )
        .is_ok()
    );
}

#[test]
fn children_are_the_ones_a_node_holds() {
    let diagnostic = the_one(r#"Marks { Text "a"; }"#);
    assert!(diagnostic.message.contains("`Marks` does not hold `Text`"));
    assert_eq!(diagnostic.suggestion.as_deref(), Some("`Marks` holds Mark"));
    assert!(
        the_one(r#"Stack { lookup "t" { a "b"; }; }"#)
            .message
            .contains("only at the top")
    );
    let part = the_one(r#"Segment weight=1"#);
    assert_eq!(part.message, "`Segment` is drawn only inside Progress");
    assert!(
        view(
            r#"template "bar" { Segment weight=1; }
Progress { use "bar"; }"#
        )
        .is_ok()
    );
}

#[test]
fn each_nests_at_most_twice() {
    assert!(view(r#"List each="a" { List each="b" { Text "{c}"; }; }"#).is_ok());
    let deep = the_one(r#"List each="a" { List each="b" { List each="c" { Text "x"; }; }; }"#);
    assert!(deep.message.contains("deeper than the 2"), "{deep}");
    let through = the_one(
        r#"template "inner" { List each="b" { Text "x"; }; }
template "outer" { List each="a" { use "inner"; }; }
List each="top" { use "outer"; }"#,
    );
    assert!(
        through.message.contains("through the template `outer`"),
        "{through}"
    );
}

#[test]
fn templates_are_named_used_and_acyclic() {
    let cycle = the_one(
        r#"template "a" { Stack { use "b"; }; }
template "b" { Stack { use "a"; }; }
Stack { use "a"; }"#,
    );
    assert!(cycle.message.contains("`a` → `b` → `a`"), "{cycle}");
    let missing = the_one(
        r#"template "row" { Text "a"; }
Stack { use "rows"; }"#,
    );
    assert!(missing.message.contains("no template `rows`"));
    assert_eq!(missing.suggestion.as_deref(), Some("did you mean `row`?"));
    assert!(
        the_one(
            r#"template "a" { }
Text "x""#
        )
        .message
        .contains("holds nothing")
    );
    assert!(
        the_one(
            r#"Stack { use "x" when="y"; }
template "x" { Text "a"; }"#
        )
        .message
        .contains("no property `when`")
    );
}

#[test]
fn lookups_carry_defaults_and_locales() {
    let compiled = view(
        r#"lookup "tone" default="muted" { done "success"; }
lookup "t" locale="pt" { next "executar"; }
lookup "t" locale="en" { next "run"; }
Text "{t::next}" role="{tone:e}""#,
    )
    .unwrap();
    let tone = &compiled.lookups[0];
    assert_eq!(tone.default.as_deref(), Some("muted"));
    assert_eq!(tone.entries["done"], "success");
    assert_eq!(
        uze_contract::compiled::look_up(&compiled.lookups, "t", "next", "pt"),
        Some("executar")
    );
    assert!(
        the_one(r#"Text "{nope:x}""#)
            .message
            .contains("no lookup table `nope`")
    );
    assert!(
        the_one(
            r#"lookup "t" { a "x"; a "y"; }
Text "a""#
        )
        .message
        .contains("given twice")
    );
    assert!(
        the_one(
            r#"lookup "t" { a "x"; }
lookup "t" { b "y"; }
Text "a""#
        )
        .message
        .contains("declared twice")
    );
}

#[test]
fn every_problem_in_a_view_is_reported_not_only_the_first() {
    let diagnostics = refused(
        "stack {\n    Text \"a\" colour=\"red\"\n    Button \"Go\" action=\"{kind}\"\n}\nHeatmap",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
    let inner = refused(
        "Stack {\n    Text \"a\" colour=\"red\"\n    Button \"Go\" action=\"{kind}\"\n    Heatmap\n}",
    );
    assert_eq!(inner.len(), 3, "{inner:#?}");
    assert_eq!(
        inner.iter().map(|d| d.line).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
}

#[test]
fn every_file_of_an_extension_is_reported() {
    let manifest = MANIFEST.replace("fixtures/board.json", "fixtures/missing.json");
    let dir = plugin(
        "extension-all-files",
        &[("gsd-band", &gsd_files(&manifest, "stack {\n}\nText {id}"))],
    );
    let diagnostics = compile_in(dir.path(), "gsd-band").unwrap_err();
    let files: Vec<&str> = diagnostics.iter().map(|d| d.file.as_str()).collect();
    assert_eq!(
        files,
        vec![MANIFEST_FILE, "board.uze", "board.uze"],
        "{diagnostics:#?}"
    );
    assert!(diagnostics[0].message.contains("fixtures/missing.json"));
    assert_eq!(diagnostics[0].line, 8);
}

#[test]
fn a_surface_naming_a_missing_view_or_one_outside_is_refused() {
    let manifest = MANIFEST.replace(
        "view=\"board.uze\" source=\"board\" title",
        "view=\"other.uze\" source=\"board\" title",
    );
    let dir = plugin(
        "extension-missing-view",
        &[("gsd-band", &gsd_files(&manifest, BOARD))],
    );
    let diagnostics = compile_in(dir.path(), "gsd-band").unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert!(
        diagnostics[0]
            .message
            .contains("`other.uze` does not exist")
    );
    assert_eq!(diagnostics[0].line, 7);
}

#[test]
fn a_script_must_be_a_file_inside_the_extension() {
    let manifest = format!(
        "{}action \"archive\" label=\"Archive\" on=\"board\" {{ script \"archive.uze\"; }}\n",
        MANIFEST.replace("fixture \"fixtures/board.json\"", "script \"board.uze\"")
    );
    let missing = plugin(
        "extension-missing-script",
        &[("gsd-band", &gsd_files(&manifest, BOARD))],
    );
    let diagnostics = compile_in(missing.path(), "gsd-band").unwrap_err();
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        // `board.uze` is there, holding the view; whether it holds a
        // script is the check's to say, which compiles it.
        vec!["the script `archive.uze` does not exist in the extension's directory"]
    );
    let present = plugin(
        "extension-present-script",
        &[(
            "gsd-band",
            &[
                (MANIFEST_FILE, manifest.clone()),
                (
                    "board.uze",
                    format!(
                        "{}\n<script>\nfn answer(input, context) {{ #{{ data: #{{}} }} }}\n</script>\n",
                        as_view(BOARD)
                    ),
                ),
                (
                    "archive.uze",
                    "<script>\nfn answer(input, context) { #{ data: #{} } }\n</script>\n"
                        .to_owned(),
                ),
            ],
        )],
    );
    let compiled = compile_in(present.path(), "gsd-band").unwrap();
    assert_eq!(
        compiled.manifest.source("board").unwrap().script(),
        Some("board.uze")
    );
}

// A symbolic link needs a privilege on Windows that a test run may lack;
// Unix makes one freely.
#[cfg(unix)]
#[test]
fn a_view_linked_from_outside_the_extension_is_refused() {
    let dir = plugin(
        "extension-linked-view",
        &[("gsd-band", &gsd_files(MANIFEST, BOARD))],
    );
    let outside = dir.join("outside.uze");
    std::fs::write(&outside, as_view(BOARD)).unwrap();
    let view = dir.join(EXTENSIONS_DIR).join("gsd-band").join("board.uze");
    std::fs::remove_file(&view).unwrap();
    std::os::unix::fs::symlink(&outside, &view).unwrap();
    let diagnostics = compile_in(dir.path(), "gsd-band").unwrap_err();
    assert!(
        diagnostics[0].message.contains("leads outside"),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_nesting_bomb_in_a_view_is_refused_before_parsing() {
    let bomb = "Stack {".repeat(400) + &"}".repeat(400);
    let dir = plugin(
        "extension-bomb",
        &[("gsd-band", &gsd_files(MANIFEST, &bomb))],
    );
    let diagnostics = compile_in(dir.path(), "gsd-band").unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].file, "board.uze");
    assert!(diagnostics[0].message.contains("deeper than the 32"));
}

#[test]
fn the_exponential_slashdash_input_is_refused_within_the_limit() {
    let started = std::time::Instant::now();
    let diagnostics = refused(&"a /-{\n".repeat(22));
    assert!(started.elapsed() < PARSE_LIMIT);
    assert!(diagnostics[0].message.contains("/-"));
}

#[test]
fn one_refused_extension_leaves_the_others_alone() {
    let reserved = MANIFEST.replace("\"gsd-band\"", "\"code\"");
    let newer = MANIFEST
        .replace("protocol 1", "protocol 2")
        .replace("\"gsd-band\"", "\"later\"");
    let dir = plugin(
        "extension-discovery",
        &[
            ("gsd-band", &gsd_files(MANIFEST, BOARD)),
            ("code", &gsd_files(&reserved, BOARD)),
            ("later", &gsd_files(&newer, BOARD)),
        ],
    );
    std::fs::create_dir_all(dir.join(EXTENSIONS_DIR).join("no-manifest")).unwrap();
    let found = discover(dir.path());
    let ids: Vec<&str> = found.iter().map(|found| found.id.as_str()).collect();
    assert_eq!(ids, vec!["code", "gsd-band", "later"]);
    let results: Vec<_> = found
        .iter()
        .map(|found| compile(identity(&found.id), &found.dir))
        .collect();
    let reserved = results[0].as_ref().unwrap_err();
    assert!(reserved[0].message.contains("reserved"), "{reserved:#?}");
    assert!(results[1].is_ok(), "{:#?}", results[1]);
    let later = results[2].as_ref().unwrap_err();
    assert!(later[0].message.contains("protocol 2") && later[0].message.contains("protocol 1"));
}

#[test]
fn a_plugin_without_extensions_has_none() {
    let dir = TempDir::new("extension-none");
    assert_eq!(discover(dir.path()), vec![]);
}

#[test]
fn a_diagnostic_reads_as_file_line_column() {
    let diagnostic = the_one("stack {\n}");
    assert_eq!(
        diagnostic.to_string(),
        "board.uze:1:1: `stack` is not a catalog node: catalog nodes are PascalCase (write `Stack`)"
    );
}

const A_TEST: &str = r##"test "the next step is offered" {
    surface "panel"
    given "session" fixture="fixtures/session.json"
    given "agents" json=#"{"busy": true}"#
    size 46 40
    theme "light"
    locale "pt"
    now 1761570125000
    expect-text "Fases"
    click "Activity"
    press "down"
    press-button "▶ next"
    expect-action "next" command="/gsd 43" count=2
    expect-absent "Button" because="next"
    expect-no-text "Deploy"
    snapshot "panel"
}
"##;

#[test]
fn a_test_file_decodes_its_world_and_steps_in_order() {
    let file = super::decode_test("tests/panel.test.kdl", A_TEST).expect("decodes");
    let test = &file.tests[0];
    assert_eq!(test.name, "the next step is offered");
    assert_eq!(test.world.surface, "panel");
    assert_eq!(test.world.size, Some((46, 40)));
    assert_eq!(test.world.theme, Some(uze_contract::ThemeName::Light));
    assert_eq!(test.world.locale.as_deref(), Some("pt"));
    assert_eq!(test.world.now_ms, Some(1_761_570_125_000));
    assert_eq!(test.world.given.len(), 2);
    assert_eq!(
        test.world.given[1].data,
        uze_contract::GivenData::Json(serde_json::json!({"busy": true}))
    );
    use uze_contract::StepKind::*;
    let kinds: Vec<_> = test.steps.iter().map(|step| step.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            ExpectText("Fases".into()),
            Click("Activity".into()),
            Press("down".into()),
            PressButton("▶ next".into()),
            ExpectAction {
                action: "next".into(),
                args: vec![
                    ("command".into(), serde_json::json!("/gsd 43")),
                    ("count".into(), serde_json::json!(2)),
                ],
            },
            ExpectAbsent {
                node: uze_contract::NodeName::Button,
                because: "next".into(),
            },
            ExpectNoText("Deploy".into()),
            Snapshot("panel".into()),
        ]
    );
}

#[test]
fn now_is_milliseconds_since_the_epoch_said_once() {
    let refused = super::decode_test(
        "tests/a.test.kdl",
        "test \"a\" {\n    surface \"x\"\n    now \"today\"\n}\ntest \"b\" {\n    surface \"x\"\n    now 1\n    now 2\n}\n",
    )
    .expect_err("refused");
    let said: Vec<String> = refused.iter().map(ToString::to_string).collect();
    assert!(
        said.iter()
            .any(|line| line.contains("`now` takes the milliseconds since the epoch")),
        "{said:?}"
    );
    assert!(
        said.iter().any(|line| line.contains("`now` is said twice")),
        "{said:?}"
    );
}

#[test]
fn a_misspelled_step_gets_the_nearest_and_a_test_needs_a_surface() {
    let refused = super::decode_test(
        "tests/a.test.kdl",
        "test \"a\" {\n    surface \"x\"\n    expect-txt \"Phases\"\n}\ntest \"b\" {\n    expect-text \"y\"\n}\n",
    )
    .expect_err("refused");
    let said: Vec<String> = refused.iter().map(ToString::to_string).collect();
    assert!(
        said.iter().any(|line| line
            .starts_with("tests/a.test.kdl:3:5: a test has no step `expect-txt`")
            && line.contains("did you mean `expect-text`?")),
        "{said:#?}"
    );
    assert!(
        said.iter().any(|line| line.contains("opens no surface")),
        "{said:#?}"
    );
}

#[test]
fn a_test_file_is_bounded_like_a_view() {
    let deep = format!(
        "test \"a\" {{ surface \"x\"; {}{} }}",
        "Stack {".repeat(400),
        "}".repeat(400)
    );
    let refused = super::decode_test("tests/deep.test.kdl", &deep).expect_err("refused");
    assert!(
        refused.iter().any(|d| d.message.contains("nest")),
        "{refused:?}"
    );
}

#[test]
fn test_files_are_found_under_tests_by_their_suffix() {
    let root = std::env::temp_dir().join(format!("uze-test-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).unwrap();
    for name in ["b.test.kdl", "a.test.kdl", "notes.md", "c.uze"] {
        std::fs::write(root.join("tests").join(name), "").unwrap();
    }
    assert_eq!(
        super::test_files(&root),
        ["tests/a.test.kdl", "tests/b.test.kdl"]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_misspelled_name_is_answered_with_the_nearest_one() {
    let view = |source: &str| -> Vec<String> {
        let mut report = Report::new("board.uze", source);
        let document = bounded::parse(&mut report).expect("parses");
        view::compile(&document, "board.uze", &mut report);
        report
            .into_diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect()
    };
    let said = view("Tabel {\n}\n");
    assert!(
        said.iter()
            .any(|line| line.contains("`Tabel` is not a catalog node")
                && line.contains("did you mean `Table`?")),
        "{said:#?}"
    );
    let said = view("Text \"a\" rol=\"muted\"\n");
    assert!(
        said.iter()
            .any(|line| line.contains("did you mean `role=`?")),
        "{said:#?}"
    );
    let said = view("Text \"a\" role=\"mutd\"\n");
    assert!(
        said.iter()
            .any(|line| line.contains("did you mean `muted`?")),
        "{said:#?}"
    );
    let said = view("lookup \"tone\" { a \"success\"; }\nText \"a\" role=\"{tones:x}\"\n");
    assert!(
        said.iter()
            .any(|line| line.contains("did you mean `tone`?")),
        "{said:#?}"
    );
}

#[test]
fn a_template_reaching_a_cycle_it_is_not_on_is_refused_without_recursing() {
    let said = refused(
        r#"template "a" { Stack { use "b"; }; }
template "b" { Stack { use "c"; }; }
template "c" { Stack { use "b"; }; }
Stack { use "a"; }"#,
    );
    assert!(
        said.iter().any(|d| d.message.contains("`b` → `c` → `b`")),
        "{said:#?}"
    );
}

#[test]
fn a_block_on_a_leaf_whose_string_is_optional_is_said() {
    let said = the_one("Stack {\n    Rule {\n        Text \"hidden\"\n    }\n}");
    assert!(
        said.message.contains("`Rule` holds no other nodes"),
        "{said:?}"
    );
}

#[test]
fn a_part_node_placed_through_a_template_is_refused_like_one_written_there() {
    let said = the_one(
        r#"template "t" { Tab "x"; }
Row { use "t"; }"#,
    );
    assert!(
        said.message.contains("places a `Tab` in a `Row`"),
        "{said:?}"
    );
    let top = the_one(
        r#"template "t" { Mark; }
use "t""#,
    );
    assert!(top.message.contains("at the top of the view"), "{top:?}");
    assert!(
        view(
            r#"template "tabs" { Tab "a" value="a"; }
Tabs id="t" { use "tabs"; }"#
        )
        .is_ok()
    );
}

#[test]
fn a_lookups_locale_is_a_language_as_the_operators_is_read() {
    let said = the_one(
        r#"lookup "t" locale="pt-BR" { next "executar"; }
Text "{t::next}""#,
    );
    assert!(
        said.message.contains("`pt-BR` is not a language"),
        "{said:?}"
    );
    assert_eq!(said.suggestion.as_deref(), Some("write `locale=\"pt\"`"));
    assert!(view("lookup \"t\" locale=\"pt\" { next \"executar\"; }\nText \"{t::next}\"").is_ok());
}

#[test]
fn a_show_names_a_tab_by_its_label_and_a_template_leaves_the_choices_open() {
    assert!(
        view(
            r#"Tabs id="tab" { Tab "tasks"; Tab "Notes" value="notes"; }
Text "a" show="ui.tab=tasks""#
        )
        .is_ok()
    );
    assert!(
        view(
            r#"template "first" { Tab "Tasks" value="tasks"; }
Tabs id="tab" { use "first"; Tab "Notes" value="notes"; }
Text "a" show="ui.tab=tasks""#
        )
        .is_ok()
    );
    let refused = the_one(
        r#"Tabs id="tab" { Tab "tasks"; }
Text "a" show="ui.tab=task""#,
    );
    assert!(
        refused.message.contains("has no choice `task`"),
        "{refused:?}"
    );
}

/// What compiles from a load is what was read: a view swapped on disk
/// after the read changes nothing, and the same files compile alike from
/// either source.
#[test]
fn a_loaded_extension_compiles_from_its_bytes_alone() {
    let dir = plugin(
        "extension-loaded",
        &[("gsd-band", &gsd_files(MANIFEST, BOARD))],
    );
    let extension = dir.join(EXTENSIONS_DIR).join("gsd-band");
    let loaded = Loaded::read(&extension).expect("within the bounds");
    std::fs::write(extension.join("board.uze"), "Stack {").unwrap();
    std::fs::remove_file(extension.join("fixtures/board.json")).unwrap();

    let from_memory = compile_loaded(identity("gsd-band"), &loaded).expect("it compiles");
    assert!(
        compile_in(dir.path(), "gsd-band").is_err(),
        "the disk now holds a broken view and no fixture"
    );
    std::fs::write(extension.join("board.uze"), as_view(BOARD)).unwrap();
    std::fs::write(extension.join("fixtures/board.json"), "{}").unwrap();
    assert_eq!(Ok(from_memory), compile_in(dir.path(), "gsd-band"));
}

#[test]
fn a_loaded_extension_missing_a_file_says_so() {
    let mut files = std::collections::BTreeMap::new();
    files.insert(MANIFEST_FILE.to_owned(), MANIFEST.as_bytes().to_vec());
    files.insert("board.uze".to_owned(), as_view(BOARD).into_bytes());
    let errors = compile_loaded(identity("gsd-band"), &Loaded::from_files(files)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("fixtures/board.json")),
        "{errors:?}"
    );
}

#[test]
fn a_packaged_script_needs_its_interpreters_grant_and_must_exist() {
    let manifest = |wants: &str| {
        MANIFEST.replace(
            "source \"board\" { fixture \"fixtures/board.json\"; }",
            &format!("{wants}\nsource \"board\" {{ run \"./bin/meter\"; }}"),
        )
    };
    let compile_with = |wants: &str, meter: &str| {
        let manifest = manifest(wants);
        let mut files = gsd_files(&manifest, BOARD).to_vec();
        if !meter.is_empty() {
            files.push(("bin/meter", meter.to_owned()));
        }
        let dir = plugin("extension-program", &[("gsd-band", &files)]);
        compile_in(dir.path(), "gsd-band")
    };
    let refused = compile_with("", "#!/usr/bin/env python3\nprint(1)\n").unwrap_err();
    assert!(
        refused[0]
            .message
            .contains("is a script run by `python3`, which needs `exec:python3`"),
        "{refused:?}"
    );
    compile_with(
        "wants \"exec:python3\"",
        "#!/usr/bin/env python3\nprint(1)\n",
    )
    .expect("granted interpreter");
    compile_with("", "\x7fELF").expect("a program of its own needs no grant");
    let missing = compile_with("", "").unwrap_err();
    assert!(
        missing[0].message.contains("the program `bin/meter`"),
        "{missing:?}"
    );
}

#[test]
fn a_select_holds_one_of_its_choices_and_show_names_one() {
    assert!(
        view(
            r#"Select "Sort" id="sort" { Choice "Newest" value="new"; Choice "Oldest" value="old"; }
Text "a" show="ui.sort=old""#
        )
        .is_ok()
    );
    let refused = the_one(
        r#"Select id="sort" { Choice "Newest" value="new"; }
Text "a" show="ui.sort=old""#,
    );
    assert!(refused.message.contains("no choice `old`"), "{refused}");
    let misplaced = refused_one_of(r#"Choice "Newest""#);
    assert!(misplaced.contains("Choice"), "{misplaced}");
}

fn refused_one_of(source: &str) -> String {
    refused(source)
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_list_filters_by_a_field_the_view_declares() {
    assert!(
        view(
            r#"Field "Filter" id="query" placeholder="name"
List each="rows" filter="ui.query" { Text "{name}"; }"#
        )
        .is_ok()
    );
    let undeclared = the_one(r#"List each="rows" filter="ui.query" { Text "{name}"; }"#);
    assert!(undeclared.message.contains("never set"), "{undeclared}");
    let not_ui = the_one(
        r#"Field id="query"
List each="rows" filter="query" { Text "{name}"; }"#,
    );
    assert!(
        not_ui.message.contains("names the UI value it filters by"),
        "{not_ui}"
    );
}

#[test]
fn a_form_holds_fields_and_names_its_action() {
    let compiled = view(
        r#"Form action="comment" submit="Comment" item="{.}" {
    Field "Comment" id="comment"
    Select "Tone" id="tone" { Choice "Kind"; Choice "Blunt"; }
}"#,
    )
    .expect("a form compiles");
    let form = first_node(&compiled);
    assert_eq!(form.name, NodeName::Form);
    assert_eq!(form.children.len(), 2);
    let interpolated = the_one(r#"Form action="{name}" { Field id="c"; }"#);
    assert!(
        interpolated.message.contains("action"),
        "an action is identity, never data: {interpolated}"
    );
}

#[test]
fn an_icon_glyph_written_loose_is_refused_and_named_with_icon_instead() {
    let loose = the_one("Text \"\u{f017} {total}\"");
    assert!(loose.message.contains("U+F017"), "{loose:?}");
    assert!(loose.suggestion.unwrap().contains("Icon"));
    assert!(the_one("Text \"⏱ today\"").message.contains("U+23F1"));
    let table = the_one("lookup \"state\" {\n    done \"🚀\"\n}\nText \"{state:phase}\"");
    assert!(table.message.contains("glyph written loose"), "{table:?}");

    view("Row {\n    Icon \"clock\" role=\"info\"\n    Text \"▶ ● ━ ↑ {total}\"\n}")
        .expect("text marks stay text");
}

#[test]
fn an_icon_names_the_hosts_or_one_the_manifest_declares() {
    let manifest = |icons: &str| {
        format!(
            "protocol 1\nid \"board\"\nname \"Board\"\ndescription \"A board.\"\n{icons}\nsurface \"board\" slot=\"view\" view=\"board.uze\" source=\"board\"\nsource \"board\" {{ fixture \"fixtures/board.json\"; }}\n"
        )
    };
    let board = "Row {\n    Icon \"clock\"\n    Icon \"kennel\"\n    Icon \"{state}\"\n}";

    let undeclared = manifest("");
    let dir = plugin(
        "icon-undeclared",
        &[("board", &gsd_files(&undeclared, board))],
    );
    let diagnostics = compile_in(dir.path(), "board").expect_err("kennel is no icon");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert!(
        diagnostics[0].message.contains("there is no icon `kennel`"),
        "{diagnostics:#?}"
    );

    let declared = manifest(r#"icon "kennel" nerd="\u{f0a43}" unicode="⌂" ascii="K""#);
    let dir = plugin("icon-declared", &[("board", &gsd_files(&declared, board))]);
    compile_in(dir.path(), "board").expect("a declared icon and one the data names both compile");
}
