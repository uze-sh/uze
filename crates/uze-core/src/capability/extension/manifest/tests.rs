use super::*;
use crate::capability::extension::Diagnostic;

pub(crate) const GSD: &str = r#"// extension.kdl
protocol 1
id "gsd-band"
name "GSD band"
description "O milestone GSD do checkout"
surface "board" slot="view" view="board.uze" source="board"
surface "summary" slot="section" view="board.uze" source="board" title="GSD"
source "board" { fixture "fixtures/board.json"; }
"#;

fn decode_text(source: &str) -> (Option<Decoded>, Vec<Diagnostic>) {
    let document: KdlDocument = source.parse().expect("valid KDL");
    let mut report = Report::new("extension.kdl", source);
    let decoded = decode(&document, "gsd-band", &mut report);
    (decoded, report.into_diagnostics())
}

fn refusals(source: &str) -> Vec<Diagnostic> {
    let (_, diagnostics) = decode_text(source);
    assert!(!diagnostics.is_empty(), "{source} was accepted");
    diagnostics
}

#[test]
fn the_gsd_band_manifest_decodes() {
    let (decoded, diagnostics) = decode_text(GSD);
    assert_eq!(diagnostics, vec![]);
    let manifest = decoded.unwrap().manifest;
    assert_eq!(manifest.protocol, 1);
    assert_eq!(manifest.id, "gsd-band");
    assert_eq!(manifest.name, "GSD band");
    assert_eq!(manifest.surfaces.len(), 2);
    let summary = manifest.surface("summary").unwrap();
    assert_eq!(summary.slot, Slot::Section);
    assert_eq!(summary.view, "board.uze");
    assert_eq!(summary.title.as_deref(), Some("GSD"));
    assert_eq!(manifest.surface("board").unwrap().title, None);
    assert_eq!(
        manifest.source("board").unwrap().fixture(),
        Some("fixtures/board.json")
    );
}

#[test]
fn an_unknown_protocol_names_both_versions_and_stops() {
    let (decoded, diagnostics) = decode_text(&GSD.replace("protocol 1", "protocol 2"));
    assert!(decoded.is_none());
    assert_eq!(diagnostics.len(), 1);
    let message = &diagnostics[0].message;
    assert!(
        message.contains("protocol 2") && message.contains("protocol 1"),
        "{message}"
    );
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 1));
}

#[test]
fn a_reserved_or_malformed_id_is_refused() {
    let reserved = refusals(&GSD.replace("\"gsd-band\"", "\"code\""));
    assert!(
        reserved[0]
            .message
            .contains("`code` is reserved by a built-in extension")
    );
    let malformed = refusals(&GSD.replace("\"gsd-band\"", "\"GSD-band\""));
    assert!(malformed[0].message.contains("is not an extension id"));
    assert_eq!(
        malformed[0].suggestion.as_deref(),
        Some("write `id \"gsd-band\"`")
    );
    let elsewhere = refusals(&GSD.replace("\"gsd-band\"", "\"other\""));
    assert!(elsewhere[0].message.contains("extensions/gsd-band/"));
}

#[test]
fn a_surface_is_checked_property_by_property() {
    let diagnostics = refusals(&GSD.replace(
        r#"surface "board" slot="view" view="board.uze" source="board""#,
        r#"surface "board" slot="bnad" view="../board.uze" source="board" colour="red""#,
    ));
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "`surface` has no property `colour`",
            "this version has no slot `bnad`",
            "`../board.uze` is not a path inside the extension's directory",
        ]
    );
    let unknown = refusals(&GSD.replace(r#"source="board" title"#, r#"source="feed" title"#));
    assert!(
        unknown[0].message.contains("`feed`, which is not declared"),
        "{}",
        unknown[0]
    );
    assert_eq!(
        unknown[0].suggestion.as_deref(),
        Some("declared sources: board")
    );
}

#[test]
fn a_missing_part_and_a_repeated_one_are_named() {
    let diagnostics = refusals(&GSD.replace("name \"GSD band\"\n", "id \"gsd-band\"\n"));
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.contains(&"`id` is declared twice"), "{messages:?}");
    assert!(messages.contains(&"declares no `name`"), "{messages:?}");
    let twice = refusals(&format!(
        "{GSD}source \"board\" {{ fixture \"b.json\"; }}\n"
    ));
    assert!(
        twice[0]
            .message
            .contains("source `board` is declared twice")
    );
}

#[test]
fn later_nodes_and_the_wrong_case_are_told_apart() {
    let case = refusals(&format!("{GSD}Surface \"x\"\n"));
    assert_eq!(
        case[0].suggestion.as_deref(),
        Some("manifest nodes are lowercase: write `surface`")
    );
    let typo = refusals(&format!("{GSD}wnats \"clipboard\"\n"));
    assert_eq!(typo[0].suggestion.as_deref(), Some("did you mean `wants`?"));
}

/// The manifest of `extension-manifest-and-scripts`, every part written.
const FULL: &str = r#"protocol 1
id "gsd-band"
name "GSD band"
description "O milestone GSD do checkout"
wants "read:checkout" "clipboard"
surface "board" slot="view" view="board.uze" source="board"
surface "log" slot="section" view="log.uze" source="log"
source "board" {
    script "board.uze"
    input phase="{phase}" dir=".planning"
    every "10s"
    refresh-on "paths.changed" ".planning" "docs/plan"
    refresh-on "checkout.changed"
}
source "log" { fixture "fixtures/log.json"; }
action "refresh" label="Refresh" on="board" key="ctrl-r" { refresh "board" "log"; }
action "archive" label="Archive" on="board" key="ctrl-d" confirm="Archive {name}?" {
    script "archive.uze"
}
action "copy" label="Copy hash" on="log" { clipboard "{hash}"; }
action "details" label="Details" on="log" { open "board"; }
"#;

fn decoded(source: &str) -> Manifest {
    let (decoded, diagnostics) = decode_text(source);
    assert_eq!(diagnostics, vec![], "{source}");
    decoded.unwrap().manifest
}

fn the_message(source: &str) -> String {
    let diagnostics = refusals(source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    diagnostics[0].message.clone()
}

/// The first refusal of `source`, whatever follows from it: a source that
/// does not decode is also one its surfaces read without it being declared.
fn first_message(source: &str) -> String {
    refusals(source)[0].message.clone()
}

#[test]
fn the_full_manifest_decodes() {
    use std::time::Duration;
    use uze_contract::{Does, EventName, Grant, SourceKind};

    let manifest = decoded(FULL);
    assert_eq!(manifest.wants, vec![Grant::ReadCheckout, Grant::Clipboard]);
    let board = manifest.source("board").unwrap();
    assert_eq!(board.kind, SourceKind::Script("board.uze".into()));
    assert_eq!(board.input["phase"], "{phase}");
    assert_eq!(board.input["dir"], ".planning");
    assert_eq!(board.every, Some(Duration::from_secs(10)));
    assert_eq!(
        board.refresh_on,
        vec![
            EventName::PathsChanged {
                paths: vec![".planning".into(), "docs/plan".into()]
            },
            EventName::CheckoutChanged,
        ]
    );
    assert_eq!(
        manifest.source("log").unwrap().fixture(),
        Some("fixtures/log.json")
    );
    let refresh = manifest.action("refresh").unwrap();
    assert_eq!(refresh.label, "Refresh");
    assert_eq!(refresh.on, "board");
    assert_eq!(refresh.key.as_ref().unwrap().to_string(), "ctrl-r");
    assert_eq!(refresh.confirm, None);
    assert_eq!(
        refresh.does,
        Does::Refresh(vec!["board".into(), "log".into()])
    );
    let archive = manifest.action("archive").unwrap();
    assert_eq!(archive.confirm.as_deref(), Some("Archive {name}?"));
    assert_eq!(archive.does, Does::Script("archive.uze".into()));
    assert_eq!(
        manifest.action("copy").unwrap().does,
        Does::Clipboard("{hash}".into())
    );
    assert_eq!(
        manifest.action("details").unwrap().does,
        Does::Open("board".into())
    );
}

#[test]
fn a_source_is_answered_by_exactly_one_thing() {
    let two = the_message(&GSD.replace(
        "{ fixture \"fixtures/board.json\"; }",
        "{ fixture \"fixtures/board.json\"; script \"board.uze\"; }",
    ));
    assert_eq!(
        two,
        "a source is answered by one of `script`, `run`, `fixture`, `events`, `agents` or `resident`"
    );
    let none =
        first_message(&GSD.replace("{ fixture \"fixtures/board.json\"; }", "{ every \"10s\"; }"));
    assert_eq!(none, "this source declares nothing to answer it");
    let outside =
        first_message(&GSD.replace("fixture \"fixtures/board.json\"", "script \"../board.uze\""));
    assert!(
        outside.contains("not a path inside the extension's directory"),
        "{outside}"
    );
    let not_a_component =
        first_message(&GSD.replace("fixture \"fixtures/board.json\"", "script \"board.js\""));
    assert_eq!(not_a_component, "`board.js` is not a `.uze` file");
    let loose =
        decode_text(&GSD.replace("fixture \"fixtures/board.json\"", "script \"board.rhai\"")).1;
    assert_eq!(
        loose[0].suggestion.as_deref(),
        Some(
            "views and scripts live in components: move it into `board.uze` between \
             `<script>` and `</script>`"
        )
    );
}

/// GSD with its fixture replaced by `answer`, wanting `wants`, plus `extra`
/// top-level nodes.
fn running(answer: &str, wants: &str, extra: &str) -> String {
    GSD.replace(
        "source \"board\" { fixture \"fixtures/board.json\"; }",
        &format!("{wants}\nsource \"board\" {{\n    {answer}\n}}\n{extra}"),
    )
}

#[test]
fn a_program_is_an_argv_with_an_optional_lower_time_limit() {
    let manifest = decoded(&running(
        r#"run "./bin/meter" "--json" timeout="5s""#,
        "",
        "",
    ));
    let SourceKind::Run(program) = &manifest.source("board").unwrap().kind else {
        panic!("not a program");
    };
    assert_eq!(program.argv, ["./bin/meter", "--json"]);
    assert_eq!(program.timeout, Some(std::time::Duration::from_secs(5)));
    assert!(
        first_message(&running(r#"run "./bin/meter" timeout="1m""#, "", ""))
            .contains("a program runs for at most 10s")
    );
}

#[test]
fn a_shell_string_or_a_path_out_of_the_extension_is_refused() {
    assert!(
        first_message(&running(r#"run "openspec list --json | jq .""#, "", ""))
            .contains("a program is an argv array")
    );
    for outside in ["../bin/tool", "./../bin/tool", "/usr/bin/tool"] {
        assert!(
            first_message(&running(&format!("run \"{outside}\""), "", ""))
                .contains("is not a path inside the extension's directory"),
            "{outside}"
        );
    }
}

#[test]
fn a_bare_program_needs_its_exec_grant_and_uze_is_never_granted() {
    let source = r#"run "nvidia-smi" "--query-gpu=name""#;
    assert_eq!(
        first_message(&running(source, "", "")),
        "the source `board` runs `nvidia-smi`, which needs `exec:nvidia-smi`"
    );
    let manifest = decoded(&running(source, r#"wants "exec:nvidia-smi""#, ""));
    assert_eq!(manifest.programs().collect::<Vec<_>>(), ["nvidia-smi"]);
    assert!(
        first_message(&running(
            r#"run "uze" "install""#,
            r#"wants "exec:uze""#,
            ""
        ))
        .contains("`exec:uze` is never granted")
    );
}

#[test]
fn a_substituted_value_follows_a_double_dash() {
    let before = refusals(&running(r#"run "./bin/archive" "{name}""#, "", ""));
    assert!(before[0].message.contains("before `--`"), "{before:?}");
    decoded(&running(r#"run "./bin/archive" "--" "{name}""#, "", ""));
    decoded(&running(r#"run "./bin/archive" "--name={name}""#, "", ""));
}

#[test]
fn a_program_source_is_asked_at_most_every_thirty_seconds() {
    let every = |every: &str| {
        running(
            &format!("run \"./bin/meter\"\n    every \"{every}\""),
            "",
            "",
        )
    };
    assert!(first_message(&every("10s")).contains("`every` is at least 30s for a program"));
    decoded(&every("30s"));
}

#[test]
fn lines_name_their_fields_and_only_a_program_has_an_output() {
    let manifest = decoded(&running(
        "run \"./bin/meter\"\n    output \"lines\" separator=\",\"\n    fields \"name\" \"load\"\n    store",
        "",
        "",
    ));
    let source = manifest.source("board").unwrap();
    assert_eq!(
        source.output,
        uze_contract::Output::Lines {
            fields: vec!["name".into(), "load".into()],
            separator: ','
        }
    );
    assert!(source.with_store);
    assert!(
        first_message(&running(
            "run \"./bin/meter\"\n    output \"lines\"",
            "",
            ""
        ))
        .contains("names the fields of a line")
    );
    assert!(
        first_message(&running(
            "script \"board.uze\"\n    output \"json\"",
            "",
            ""
        ))
        .contains("this source runs no program")
    );
    assert_eq!(
        first_message(&running("run \"./bin/meter\"\n    output \"csv\"", "", "")),
        "there is no output `csv`"
    );
    let composed = decoded(&running(
        "script \"board.uze\"\n    output \"view\"",
        "",
        "",
    ));
    assert_eq!(
        composed.source("board").unwrap().output,
        uze_contract::Output::View
    );
}

#[test]
fn an_exec_names_what_a_wanted_program_reads() {
    let manifest = decoded(&running(
        r#"run "nvidia-smi""#,
        r#"wants "exec:nvidia-smi""#,
        r#"exec "nvidia-smi" { reads "/dev/nvidiactl" "/dev/dxg"; }"#,
    ));
    assert_eq!(
        manifest.reads_of("nvidia-smi"),
        ["/dev/nvidiactl", "/dev/dxg"]
    );
    assert!(
        first_message(&running(
            r#"script "board.uze""#,
            "",
            r#"exec "gh" { reads "/dev/null"; }"#
        ))
        .contains("names a program the manifest does not want")
    );
    // Judged as the POSIX paths a confinement reads, whatever the host.
    for refused in ["dev/x", r"C:\\Windows", "/dev/../etc"] {
        assert!(
            first_message(&running(
                r#"run "nvidia-smi""#,
                r#"wants "exec:nvidia-smi""#,
                &format!(r#"exec "nvidia-smi" {{ reads "{refused}"; }}"#),
            ))
            .contains("is not an absolute path"),
            "{refused}"
        );
    }
}

#[test]
fn an_action_runs_a_program() {
    let manifest = decoded(&GSD.replace(
        "source \"board\"",
        "action \"go\" label=\"Go\" on=\"board\" { run \"./bin/go\" \"--\" \"{name}\"; }\nsource \"board\"",
    ));
    let uze_contract::Does::Run(program) = &manifest.action("go").unwrap().does else {
        panic!("not a program");
    };
    assert_eq!(program.argv, ["./bin/go", "--", "{name}"]);
}

#[test]
fn every_is_a_duration_of_at_least_five_seconds() {
    let with = |every: &str| {
        GSD.replace(
            "{ fixture \"fixtures/board.json\"; }",
            &format!("{{ fixture \"fixtures/board.json\"; every \"{every}\"; }}"),
        )
    };
    for fine in ["5s", "5000ms", "1m", "2h"] {
        decoded(&with(fine));
    }
    assert!(the_message(&with("4s")).contains("`every` is at least 5s"));
    assert!(the_message(&with("500ms")).contains("`every` is at least 5s"));
    assert_eq!(the_message(&with("soon")), "`soon` is not a duration");
    assert_eq!(the_message(&with("10 s")), "`10 s` is not a duration");
}

#[test]
fn an_event_is_one_this_version_watches() {
    let with = |event: &str| {
        GSD.replace(
            "{ fixture \"fixtures/board.json\"; }",
            &format!("{{ fixture \"fixtures/board.json\"; refresh-on {event}; }}"),
        )
    };
    assert_eq!(
        the_message(&with("\"paths.changed\"")),
        "`paths.changed` names the paths it watches"
    );
    assert_eq!(
        the_message(&with("\"paths.changed\" \"../secrets\"")),
        "`../secrets` is not a path inside the checkout"
    );
    assert_eq!(
        the_message(&with("\"agent.finished\"")),
        "there is no event `agent.finished`"
    );
    let typo = refusals(&with("\"path.changed\" \"x\""));
    assert_eq!(
        typo[0].suggestion.as_deref(),
        Some("did you mean `paths.changed`?")
    );
    assert!(
        the_message(&with(
            "\"checkout.changed\"; refresh-on \"checkout.changed\""
        ))
        .contains("refreshed on `checkout.changed` twice")
    );
}

#[test]
fn an_input_is_text_with_references() {
    let with = |input: &str| {
        GSD.replace(
            "{ fixture \"fixtures/board.json\"; }",
            &format!("{{ fixture \"fixtures/board.json\"; input {input}; }}"),
        )
    };
    assert_eq!(the_message(&with("limit=10")), "an input is text, not 10");
    assert!(the_message(&with("phase=\"{a + b}\"")).contains("is not a reference"));
    assert_eq!(
        the_message(&with("\"phase\"")),
        "`input` takes properties, not arguments"
    );
}

#[test]
fn only_the_grants_this_version_enforces_are_accepted() {
    let wanting = |grants: &str| {
        GSD.replace(
            "source \"board\"",
            &format!("wants {grants}\nsource \"board\""),
        )
    };
    decoded(&wanting(
        "\"read:checkout\" \"clipboard\" \"open-path\" \"store\" \"store:global\" \"exec:gh\"",
    ));
    assert_eq!(
        the_message(&wanting("\"exec:../gh\"")),
        "`../gh` is not a program's name"
    );
    assert_eq!(
        the_message(&wanting("\"exec\"")),
        "`exec` names the program it grants"
    );
    decoded(&wanting("\"net:api.github.com\""));
    for (grant, message) in [
        ("net", "`net` names the host it grants"),
        ("net:https://x.com", "`https://x.com` is not a host name"),
        ("net:*.github.com", "`*.github.com` is not a host name"),
        (
            "secret",
            "`secret` is not a grant: a secret is a preference the operator sets",
        ),
        (
            "edit:checkout",
            "`edit:checkout` is not offered: no extension writes the checkout yet",
        ),
    ] {
        assert_eq!(the_message(&wanting(&format!("\"{grant}\""))), message);
    }
    assert_eq!(
        the_message(&wanting("\"write:checkout\"")),
        "`write:checkout` does not exist: no extension writes the checkout"
    );
    let typo = refusals(&wanting("\"clipbaord\""));
    assert_eq!(typo[0].message, "there is no grant `clipbaord`");
    assert_eq!(
        typo[0].suggestion.as_deref(),
        Some("did you mean `clipboard`?")
    );
    assert_eq!(
        the_message(&wanting("\"clipboard\" \"clipboard\"")),
        "`clipboard` is wanted twice"
    );
}

#[test]
fn a_confirmed_action_needs_a_modifier_on_its_key() {
    let action = |key: &str| {
        format!(
            "{GSD}action \"archive\" label=\"Archive\" on=\"board\" key=\"{key}\" confirm=\"Archive {{name}}?\" {{ refresh \"board\"; }}\n"
        )
    };
    decoded(&action("ctrl-d"));
    let bare = refusals(&action("d"));
    assert_eq!(bare.len(), 1, "{bare:?}");
    assert!(bare[0].message.contains("needs a modifier"), "{}", bare[0]);
    assert_eq!(
        bare[0].suggestion.as_deref(),
        Some("write `key=\"ctrl-d\"`")
    );
    decoded(&format!(
        "{GSD}action \"next\" label=\"Next\" on=\"board\" key=\"n\" {{ refresh \"board\"; }}\n"
    ));
    assert_eq!(
        the_message(&format!(
            "{GSD}action \"next\" label=\"Next\" on=\"board\" key=\"ctrl+n\" {{ refresh \"board\"; }}\n"
        )),
        "`ctrl+n` is not a key"
    );
}

#[test]
fn an_action_that_reaches_an_agent_needs_a_modifier_on_its_key() {
    let with = |wants: &str, does: &str, key: &str| {
        GSD.replace(
            "source \"board\"",
            &format!("wants \"{wants}\"\nsource \"board\""),
        ) + &format!("action \"go\" label=\"Go\" on=\"board\" key=\"{key}\" {{ {does}; }}\n")
    };
    for (wants, does) in [
        ("agent:prompt", r#"agent-prompt "Continue" target="self""#),
        ("agent:restart", r#"agent-restart target="self""#),
        ("agent:observe", r#"focus-agent target="self""#),
    ] {
        decoded(&with(wants, does, "ctrl-g"));
        let bare = refusals(&with(wants, does, "g"));
        assert_eq!(bare.len(), 1, "{does}: {bare:?}");
        assert!(
            bare[0]
                .message
                .contains("reaches an agent, so its key needs a modifier"),
            "{}",
            bare[0]
        );
        assert_eq!(
            bare[0].suggestion.as_deref(),
            Some("write `key=\"ctrl-g\"`")
        );
    }
}

#[test]
fn an_action_names_only_what_is_declared() {
    let action = |on: &str, does: &str| {
        format!("{GSD}action \"go\" label=\"Go\" on=\"{on}\" {{ {does}; }}\n")
    };
    assert_eq!(
        the_message(&action("bord", "refresh \"board\"")),
        "the action `go` is offered on the surface `bord`, which is not declared"
    );
    let refresh = refusals(&action("board", "refresh \"board\" \"feed\""));
    assert_eq!(
        refresh[0].message,
        "the action `go` refreshes the source `feed`, which is not declared"
    );
    assert_eq!(
        refresh[0].suggestion.as_deref(),
        Some("declared sources: board")
    );
    assert_eq!(
        the_message(&action("board", "open \"panel\"")),
        "the action `go` opens the surface `panel`, which is not declared"
    );
    assert_eq!(
        the_message(&action("board", "clipboard \"{hash}\"")),
        "the action `go` needs `clipboard`, which the manifest does not want"
    );
    assert_eq!(
        the_message(&action("board", "open-path \"{path}\"")),
        "the action `go` needs `open-path`, which the manifest does not want"
    );
    assert_eq!(
        the_message(&format!("{GSD}action \"go\" label=\"Go\" on=\"board\"\n")),
        "this action declares nothing to do"
    );
    assert_eq!(
        the_message(&action("board", "refresh \"board\"; open \"board\"")),
        "an action does one thing"
    );
    let twice = refusals(&format!(
        "{}{}",
        action("board", "refresh \"board\""),
        "action \"go\" label=\"Go\" on=\"board\" { refresh \"board\"; }\n"
    ));
    assert!(twice[0].message.contains("action `go` is declared twice"));
}

#[test]
fn an_action_is_checked_property_by_property() {
    let diagnostics = refusals(&format!(
        "{GSD}action \"go\" label=\"Go\" on=\"board\" colour=\"red\" {{ refresh \"board\"; }}\naction \"stop\" on=\"board\" {{ refresh \"board\"; }}\n"
    ));
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "`action` has no property `colour`",
            "`action` declares no `label=`",
        ]
    );
}

#[test]
fn an_id_is_at_most_thirty_two_characters_and_timeline_is_reserved() {
    let long = "a".repeat(33);
    let refused = refusals(&GSD.replace("\"gsd-band\"", &format!("\"{long}\"")));
    assert!(
        refused[0].message.contains("at most 32 long"),
        "{}",
        refused[0]
    );
    let timeline = refusals(&GSD.replace("\"gsd-band\"", "\"timeline\""));
    assert!(timeline[0].message.contains("`timeline` is reserved"));
}

#[test]
fn a_band_says_its_rows_and_place_and_nothing_else_does() {
    let with_surface = |surface: &str| {
        GSD.replace(
            r#"surface "summary" slot="section" view="board.uze" source="board" title="GSD""#,
            surface,
        )
    };
    let manifest = decoded(&with_surface(
        r#"surface "summary" slot="band" view="board.uze" source="board" rows=2 place="bottom""#,
    ));
    let band = manifest.surface("summary").unwrap().band.unwrap();
    assert_eq!(
        (band.rows, band.place),
        (2, uze_contract::BandPlace::Bottom)
    );
    assert!(
        first_message(&with_surface(
            r#"surface "summary" slot="band" view="board.uze" source="board""#
        ))
        .contains("a band names how many rows it takes")
    );
    assert!(
        first_message(&with_surface(
            r#"surface "summary" slot="band" view="board.uze" source="board" rows=9"#
        ))
        .contains("a band takes 1 to 6 rows")
    );
    assert!(
        first_message(&with_surface(
            r#"surface "summary" slot="section" view="board.uze" source="board" rows=2"#
        ))
        .contains("only a band takes `rows` and `place`")
    );
    for slot in ["status", "overlay"] {
        let manifest = decoded(&with_surface(&format!(
            r#"surface "summary" slot="{slot}" view="board.uze" source="board""#
        )));
        assert_eq!(manifest.surface("summary").unwrap().slot.name(), slot);
    }
}

#[test]
fn preferences_are_typed_with_a_default_they_may_hold() {
    let manifest = decoded(&format!(
        "{GSD}preferences {{\n    number \"warn-at\" label=\"Warn at\" default=70 min=1 max=100\n    toggle \"fahrenheit\" default=#false\n    choice \"unit\" \"MiB\" \"GiB\" default=\"MiB\"\n    text \"title\" default=\"GPU\"\n}}\n"
    ));
    let names: Vec<&str> = manifest
        .preferences
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["warn-at", "fahrenheit", "unit", "title"]);
    let warn = manifest.preference("warn-at").unwrap();
    assert_eq!(warn.label, "Warn at");
    assert_eq!(
        warn.admit(Some(&serde_json::json!(50))),
        serde_json::json!(50)
    );
    assert_eq!(
        warn.admit(Some(&serde_json::json!(500))),
        serde_json::json!(70)
    );
    assert_eq!(
        warn.admit(Some(&serde_json::json!("50"))),
        serde_json::json!(70)
    );
    let unit = manifest.preference("unit").unwrap();
    assert_eq!(
        unit.admit(Some(&serde_json::json!("TiB"))),
        serde_json::json!("MiB")
    );
    assert!(
        first_message(&format!(
            "{GSD}preferences {{ choice \"unit\" \"MiB\" default=\"TiB\"; }}\n"
        ))
        .contains("is not a value it may hold")
    );
    assert!(
        first_message(&format!("{GSD}preferences {{ toggle \"on\"; }}\n"))
            .contains("says its `default`")
    );
}

#[test]
fn a_secret_has_no_default_and_is_kept_only_for_a_host() {
    let with = |wants: &str, secret: &str| {
        GSD.replace(
            "source \"board\"",
            &format!("{wants}preferences {{ {secret} }}\nsource \"board\""),
        )
    };
    let manifest = decoded(&with(
        "wants \"net:api.github.com\"\n",
        "secret \"token\" label=\"GitHub token\";",
    ));
    let token = manifest.preference("token").unwrap();
    assert!(token.is_secret());
    assert_eq!(token.default, serde_json::Value::Null);
    assert!(token.holds(&serde_json::json!("ghp_x")));
    for refused in [
        serde_json::json!(""),
        serde_json::json!("a\nb"),
        serde_json::json!(1),
    ] {
        assert!(!token.holds(&refused), "{refused}");
    }
    assert_eq!(manifest.hosts().collect::<Vec<_>>(), ["api.github.com"]);
    assert!(
        the_message(&with("", "secret \"token\";")).contains("the secret `token` is never used")
    );
    assert!(
        first_message(&with(
            "wants \"net:api.github.com\"\n",
            "secret \"token\" default=\"x\";"
        ))
        .contains("default")
    );
}

#[test]
fn a_source_may_be_the_events_it_saw_delivered_each() {
    let manifest = decoded(&GSD.replace(
        "source \"board\" { fixture \"fixtures/board.json\"; }",
        "source \"board\" {\n    fixture \"fixtures/board.json\"\n    refresh-on \"paths.changed\" \".planning\"\n    deliver \"each\"\n    preferences\n}\nsource \"log\" { events 50; }",
    ));
    let board = manifest.source("board").unwrap();
    assert_eq!(board.deliver, uze_contract::Deliver::Each);
    assert!(board.with_preferences);
    assert_eq!(manifest.source("log").unwrap().kind, SourceKind::Events(50));
    assert!(
        first_message(&GSD.replace(
            "source \"board\" { fixture \"fixtures/board.json\"; }",
            "source \"board\" { fixture \"fixtures/board.json\"; deliver \"each\"; }",
        ))
        .contains("no event refreshes it")
    );
    assert!(
        first_message(&GSD.replace(
            "source \"board\" { fixture \"fixtures/board.json\"; }",
            "source \"board\" { events 1000; }",
        ))
        .contains("`events` keeps 1 to 200")
    );
    decoded(&GSD.replace("source \"board\"", "wants \"notify\"\nsource \"board\""));
}

#[test]
fn agent_events_actions_and_the_agents_source_need_their_grants() {
    let with = |wants: &str, rest: &str| {
        GSD.replace(
            "source \"board\"",
            &format!("{wants}\n{rest}\nsource \"board\""),
        )
    };
    let observing = r#"source "crew" { agents; refresh-on "agent.turn.finished"; }"#;
    assert!(first_message(&with("", observing)).contains("which needs `agent:observe`"));
    let manifest = decoded(&with(r#"wants "agent:observe""#, observing));
    let crew = manifest.source("crew").unwrap();
    assert_eq!(crew.kind, SourceKind::Agents);
    assert_eq!(
        crew.refresh_on,
        [uze_contract::EventName::Agent(
            uze_contract::AgentEvent::TurnFinished
        )]
    );

    let prompt = r#"action "next" label="Run phase {number}" on="board" { agent-prompt skill="gsd:execute-phase" "{number}" target="self"; }"#;
    assert!(first_message(&with("", prompt)).contains("needs `agent:prompt`"));
    let manifest = decoded(&with(r#"wants "agent:prompt""#, prompt));
    let uze_contract::Does::AgentPrompt {
        prompt,
        target,
        auto,
    } = &manifest.action("next").unwrap().does
    else {
        panic!("a prompt");
    };
    assert_eq!(
        prompt,
        &uze_contract::PromptText::Skill {
            skill: "gsd:execute-phase".into(),
            args: vec!["{number}".into()]
        }
    );
    assert_eq!(target, &uze_contract::AgentTarget::Own);
    assert!(auto.is_none());

    let unattended = r#"action "nudge" label="Nudge" on="board" { agent-prompt "Continue with {number}" target="self" when="agent.turn.finished"; }"#;
    assert!(
        first_message(&with(r#"wants "agent:prompt:auto""#, unattended))
            .contains("a prompt sent unattended is fixed")
    );
    let fixed = unattended.replace("Continue with {number}", "Continue");
    let manifest = decoded(&with(r#"wants "agent:prompt:auto""#, &fixed));
    assert!(matches!(
        manifest.action("nudge").unwrap().does,
        uze_contract::Does::AgentPrompt { auto: Some(_), .. }
    ));
    assert!(
        first_message(&with(
            r#"wants "agent:observe""#,
            r#"action "go" label="Go" on="board" { focus-agent target="agent-3"; }"#
        ))
        .contains("is not an agent")
    );
}

#[test]
fn a_sprite_and_a_resident_source_are_declared_and_checked() {
    let with = |rest: &str| GSD.replace("source \"board\"", &format!("{rest}\nsource \"board\""));
    let sprite = r#"sprite "pet" sheet="pet.png" frame="32x32" columns=4 fallback="idle" {
    state "idle" frames="0-3" fps=4
    state "working" frames="4,5,6,7" fps=12 loop=#false
}"#;
    let manifest = decoded(&with(sprite));
    let pet = manifest.sprite("pet").expect("declared");
    assert_eq!(pet.frame, (32, 32));
    assert_eq!(pet.states[1].frames, [4, 5, 6, 7]);
    assert!(!pet.states[1].looped);
    assert!(
        first_message(&with(
            &sprite.replace(r#"fallback="idle""#, r#"fallback="asleep""#)
        ))
        .contains("the fallback `asleep` is none of its states")
    );
    assert!(first_message(&with(&sprite.replace("32x32", "big"))).contains("is not a frame size"));
    assert!(
        first_message(&with(&sprite.replace("fps=12", "fps=60")))
            .contains("`fps` is a whole number from 1 to 30")
    );
    assert!(
        first_message(&with(&sprite.replace("pet.png", "../pet.png")))
            .contains("is not a path inside the extension's directory")
    );

    let resident = r#"source "game" { resident "game.uze"; }"#;
    assert!(first_message(&with(resident)).contains("which needs `resident`"));
    let manifest = decoded(&with(&format!("wants \"resident\"\n{resident}")));
    assert_eq!(
        manifest.source("game").unwrap().kind,
        SourceKind::Resident("game.uze".to_owned())
    );
}

#[test]
fn applies_when_names_a_path_inside_the_checkout() {
    let marked = GSD
        .replace(
            r#"surface "board" slot="view" view="board.uze" source="board""#,
            r#"surface "board" slot="view" view="board.uze" source="board" applies-when=".planning""#,
        )
        .replace(
            r#"source "board" { fixture "fixtures/board.json"; }"#,
            r#"source "board" { fixture "fixtures/board.json"; applies-when ".planning/STATE.md"; }"#,
        );
    let (decoded, diagnostics) = decode_text(&marked);
    assert_eq!(diagnostics, vec![]);
    let manifest = decoded.unwrap().manifest;
    assert_eq!(
        manifest.surface("board").unwrap().applies_when.as_deref(),
        Some(".planning")
    );
    assert_eq!(manifest.surface("summary").unwrap().applies_when, None);
    assert_eq!(
        manifest.source("board").unwrap().applies_when.as_deref(),
        Some(".planning/STATE.md")
    );
    for outside in ["../secrets", "/etc", ""] {
        let refused = refusals(&GSD.replace(
            r#"source="board" title="GSD""#,
            &format!(r#"source="board" title="GSD" applies-when="{outside}""#),
        ));
        assert!(
            refused[0].message.contains("not a path inside a checkout"),
            "{outside}: {refused:?}"
        );
    }
}

#[test]
fn an_action_reads_declared_ui_and_form_values_and_runs_then_prompts() {
    let manifest = format!(
        r#"{GSD}wants "agent:prompt"
action "review" label="Review" on="board" {{
    run "./bin/next.sh"
    with "ui.sort" "form.*"
    then {{ agent-prompt "Review {{answer.file}}" target="self"; }}
}}
"#
    );
    let (decoded, diagnostics) = decode_text(&manifest);
    assert_eq!(diagnostics, vec![]);
    let action = decoded.unwrap().manifest.actions.remove(0);
    assert_eq!(action.with, vec!["ui.sort".to_owned(), "form.*".to_owned()]);
    assert!(action.reads_ui("sort") && action.reads_form("comment"));
    assert_eq!(
        action.then,
        Some(uze_contract::Then {
            prompt: uze_contract::PromptText::Text("Review {answer.file}".into()),
            target: uze_contract::AgentTarget::Own,
        })
    );
}

#[test]
fn a_second_step_is_a_gesture_prompt_after_a_run_under_its_grant() {
    let action = |does: &str, then: &str| {
        format!(
            "{GSD}action \"review\" label=\"Review\" on=\"board\" {{\n    {does}\n    then {{ {then} }}\n}}\n"
        )
    };
    let after_refresh = refusals(&format!(
        "wants \"agent:prompt\"\n{}",
        action(
            "refresh \"board\"",
            "agent-prompt \"x {answer.a}\" target=\"self\";"
        )
    ));
    assert!(
        after_refresh[0]
            .message
            .contains("only an action that runs something"),
        "{after_refresh:?}"
    );
    let unattended = refusals(&format!(
        "wants \"agent:prompt\"\n{}",
        action(
            "script \"next.uze\"",
            "agent-prompt \"x\" target=\"self\" when=\"agent.turn.finished\";"
        )
    ));
    assert!(
        unattended[0].message.contains("never sent unattended"),
        "{unattended:?}"
    );
    let ungranted = refusals(&action(
        "script \"next.uze\"",
        "agent-prompt \"Review {answer.file}\" target=\"self\";",
    ));
    assert!(
        ungranted[0].message.contains("needs `agent:prompt`"),
        "{ungranted:?}"
    );
    let unread = refusals(&format!(
        "{GSD}action \"a\" label=\"A\" on=\"board\" {{ refresh \"board\"; with \"store.x\"; }}\n"
    ));
    assert!(
        unread[0].message.contains("not a value an action reads"),
        "{unread:?}"
    );
}

#[test]
fn a_skill_is_named_as_a_plugin_names_it_and_never_as_a_harness_command() {
    let with = |skill: &str| {
        GSD.replace(
            "source \"board\"",
            &format!(
                "wants \"agent:prompt\"\naction \"go\" label=\"Go\" on=\"board\" {{ agent-prompt skill=\"{skill}\" target=\"self\"; }}\nsource \"board\""
            ),
        )
    };
    for builtin in [
        "clear",
        "compact",
        "/clear",
        "gsd:",
        ":clear",
        "gsd:run phase",
        "gsd:x/y",
        "a:b:c",
    ] {
        assert!(
            first_message(&with(builtin)).contains("is not a skill"),
            "`{builtin}` is refused"
        );
    }
    decoded(&with("gsd:execute-phase"));
    decoded(&with("my_plugin:Run-2"));
}

#[test]
fn an_icon_is_declared_in_every_glyph_set_and_never_shadows_the_hosts() {
    let with = |rest: &str| GSD.replace("source \"board\"", &format!("{rest}\nsource \"board\""));
    let kennel = r#"icon "kennel" nerd="\u{f0a43}" unicode="⌂" ascii="K""#;
    let manifest = decoded(&with(kennel));
    assert_eq!(manifest.icons[0].name, "kennel");
    assert_eq!(manifest.icons[0].ascii, "K");

    assert!(
        first_message(&with(&kennel.replace("kennel", "clock")))
            .contains("one of the host's icons")
    );
    assert!(first_message(&with(&kennel.replace(r#" ascii="K""#, ""))).contains("ascii"));
    assert!(
        first_message(&with(&kennel.replace(r#"nerd="\u{f0a43}""#, r#"nerd="⌂""#)))
            .contains("`nerd` is one character of a Nerd Font's private use area")
    );
    assert!(
        first_message(&with(&kennel.replace(r#"unicode="⌂""#, r#"unicode="🐕""#)))
            .contains("`unicode` is a character drawn as text, not an emoji")
    );
    assert!(
        first_message(&with(&kennel.replace(r#"ascii="K""#, r#"ascii="KEN""#)))
            .contains("`ascii` is one or two printable ASCII characters")
    );
    assert!(
        first_message(&with(&format!("{kennel}\n{kennel}")))
            .contains("the icon `kennel` is declared twice")
    );
}

#[test]
fn an_mcp_server_is_named_with_its_plugin_and_time_and_used_by_a_grant() {
    let with = |rest: &str| GSD.replace("source \"board\"", &format!("{rest}\nsource \"board\""));
    let granted = r#"wants "mcp:rtc/rtc_timesheet""#;
    let node = r#"mcp "rtc" plugin="std@cardinal" timeout="5m""#;
    let manifest = decoded(&with(&format!("{granted}\n{node}")));
    assert_eq!(
        manifest.mcp,
        [uze_contract::McpDecl {
            server: "rtc".into(),
            plugin: Some("std@cardinal".into()),
            timeout: Some(std::time::Duration::from_secs(300)),
        }]
    );
    assert!(manifest.wants(&uze_contract::Grant::Mcp {
        server: "rtc".into(),
        tool: "rtc_timesheet".into(),
    }));
    decoded(&with(granted));

    assert!(
        first_message(&with(node)).contains("names a server no tool of which the manifest wants")
    );
    assert!(
        first_message(&with(&format!("{granted}\n{}", node.replace("5m", "11m"))))
            .contains("is not a call's time")
    );
    assert!(
        first_message(&with(&format!(
            "{granted}\n{}",
            node.replace("std@cardinal", "std")
        )))
        .contains("is not a plugin as `<plugin>@<marketplace>`")
    );
    assert!(
        first_message(&with(&format!("{granted}\n{node}\n{node}")))
            .contains("`mcp \"rtc\"` is declared twice")
    );
}

#[test]
fn a_call_names_a_function_and_never_enters_a_resident() {
    let with = |rest: &str| GSD.replace("source \"board\"", &format!("{rest}\nsource \"board\""));
    let manifest = decoded(&with(
        r#"action "prev" label="Prev" on="board" { call "prev"; }"#,
    ));
    assert_eq!(
        manifest.action("prev").unwrap().does,
        uze_contract::Does::Call {
            function: "prev".into(),
            file: None
        }
    );
    assert_eq!(
        manifest.script_of(manifest.action("prev").unwrap()),
        Some(("board.uze", "prev")),
        "without `in`, the component the surface draws"
    );
    let shared = decoded(&with(
        r#"action "prev" label="Prev" on="board" { call "prev" in="sheet.uze"; }"#,
    ));
    assert_eq!(
        shared.script_of(shared.action("prev").unwrap()),
        Some(("sheet.uze", "prev"))
    );

    assert!(
        first_message(&with(
            r#"action "a" label="A" on="board" { call "answer"; }"#
        ))
        .contains("`answer` is an entry the host calls")
    );
    assert!(
        first_message(&with(r#"action "a" label="A" on="board" { call "go!"; }"#))
            .contains("`go!` is not a function's name")
    );
    assert!(
        first_message(&with(
            r#"action "a" label="A" on="board" { call "prev" in="sheet.rhai"; }"#
        ))
        .contains("`sheet.rhai` is not a `.uze` file")
    );
    let resident = with(
        "wants \"resident\"\nsource \"game\" { resident \"board.uze\"; }\naction \"turn\" label=\"Turn\" on=\"board\" { call \"turn\"; }",
    );
    assert!(
        first_message(&resident).contains("whose script runs as a resident"),
        "{}",
        first_message(&resident)
    );
}
