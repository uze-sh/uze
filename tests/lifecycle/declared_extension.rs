//! The `gsd-band` reference extension from install to redraw, on real
//! files: a plugin from a linked marketplace carries it, the workspace's
//! services find it in the author's working tree, compile it behind their
//! cache, read its fixture, and the real engine binds the two into the
//! board. A save in the working tree is what the next compile draws, and a
//! broken save is said where it is.

use std::fs;
use std::path::{Path, PathBuf};

use uze_application::{InstalledExtension, UzeApplication, extension_stamp};
use uze_core::{UzeHome, trust::AlwaysTrust};
use uze_extensions::contract::Extension as _;
use uze_extensions::declared::DeclaredExtension;
use uze_extensions::view::{Content, Node, NodeAct, NodeKind, Role, Size, ViewHit};
use uze_testkit::temp::TestEnvironment;

const MARKET: &str = "studio";
const BOARD: &str = "board";
const SPACE: Size = Size {
    width: 100,
    height: 30,
};

/// The `gsd` fixture plugin as the one plugin of a marketplace repository,
/// registered and linked to its working tree, then installed.
fn installed_from_a_linked_marketplace(env: &TestEnvironment) -> (UzeApplication, PathBuf) {
    let market = env.root().join("market");
    let manifest = serde_json::json!({
        "name": MARKET,
        "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD band." }],
    });
    uze_testkit::marketplace::stage(
        &market,
        &manifest.to_string(),
        &[("gsd".to_owned(), uze_testkit::fixtures::canonical("gsd"))],
    );
    let app = UzeApplication::new(UzeHome::at(&env.uze_home), Vec::new());
    app.marketplace()
        .add(&market.display().to_string())
        .expect("the marketplace registers");
    app.marketplace()
        .link(MARKET, &market)
        .expect("the marketplace links to its working tree");
    app.marketplace()
        .install_plugin(&format!("gsd@{MARKET}"), &AlwaysTrust)
        .expect("the gsd plugin installs");
    (app, market)
}

fn the_band(app: &UzeApplication) -> InstalledExtension {
    the(app, "gsd-band")
}

/// The extension `id` of the installed `gsd` plugin, which carries two.
fn the(app: &UzeApplication, id: &str) -> InstalledExtension {
    let installed = app.extensions().installed();
    assert_eq!(installed.len(), 2, "the band and the panel: {installed:?}");
    installed
        .into_iter()
        .find(|extension| extension.identity.id == id)
        .unwrap_or_else(|| panic!("{id} is installed"))
}

/// The real engine, handed what the services compiled and read.
fn engine(app: &UzeApplication, installed: &InstalledExtension) -> DeclaredExtension {
    let load = app
        .extensions()
        .load(installed)
        .unwrap_or_else(|diagnostics| panic!("the band compiles: {diagnostics:#?}"));
    let mut engine = DeclaredExtension::new(load.compiled, "en");
    for (source, data) in load.fixtures {
        engine.set_data(source, data);
    }
    engine
}

fn board(engine: &DeclaredExtension) -> Node {
    match engine
        .view(BOARD, SPACE)
        .expect("the board is a view")
        .content
    {
        Content::Tree { root, .. } => root,
        other => panic!("not a tree: {other:?}"),
    }
}

fn nodes(root: &Node) -> Vec<&Node> {
    let mut all = vec![root];
    for child in &root.children {
        all.extend(nodes(child));
    }
    all
}

fn text(node: &Node) -> String {
    match &node.kind {
        NodeKind::Text { spans, .. } => spans.iter().map(|span| span.text.as_str()).collect(),
        other => panic!("not text: {other:?}"),
    }
}

fn texts(root: &Node) -> Vec<String> {
    nodes(root)
        .into_iter()
        .filter(|node| matches!(node.kind, NodeKind::Text { .. }))
        .map(text)
        .collect()
}

fn canonical(path: &Path) -> PathBuf {
    uze_platform::path::canonical(path).unwrap()
}

#[test]
fn the_gsd_band_is_found_in_the_working_tree_compiled_once_and_drawn_from_its_fixture() {
    let env = TestEnvironment::isolated();
    let (app, market) = installed_from_a_linked_marketplace(&env);

    let band = the_band(&app);
    assert_eq!(band.identity.to_string(), format!("gsd@{MARKET}/gsd-band"));
    assert!(band.linked, "a linked marketplace's extension is watched");
    assert_eq!(
        canonical(&band.dir),
        canonical(&market.join("plugins/gsd/extensions/gsd-band")),
        "read from the author's working tree, not the Store's copy"
    );

    // Compiled once: the second ask is served from the generated tier.
    let compiled = app.extensions().compile(&band).expect("the band compiles");
    assert_eq!(compiled.manifest.surfaces.len(), 2);
    let cache = UzeHome::at(&env.uze_home).compiled_extensions_dir();
    let cached: Vec<PathBuf> = fs::read_dir(&cache)
        .expect("the compile is cached")
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(cached.len(), 1, "{cached:?}");
    let mut marked: serde_json::Value =
        serde_json::from_slice(&fs::read(&cached[0]).unwrap()).unwrap();
    marked["manifest"]["name"] = "from the cache".into();
    fs::write(&cached[0], marked.to_string()).unwrap();
    assert_eq!(
        app.extensions().compile(&band).unwrap().manifest.name,
        "from the cache",
        "the same files are not parsed again"
    );
    fs::remove_file(&cached[0]).unwrap();

    let fixtures = app.extensions().load(&band).expect("it loads").fixtures;
    assert_eq!(
        compiled
            .manifest
            .source("state")
            .and_then(|state| state.script()),
        Some("state.uze")
    );
    assert!(
        !fixtures.contains_key("state"),
        "a script source has no data until scripts run"
    );
    let data = fixtures["milestone"].as_ref().expect("the fixture is read");
    assert_eq!(data["phases"].as_array().map(Vec::len), Some(6));

    let engine = engine(&app, &band);
    let root = board(&engine);
    let all = nodes(&root);

    let segments = all
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Progress { segments, .. } => Some(segments.clone()),
            _ => None,
        })
        .expect("a progress bar");
    assert_eq!(segments.len(), 6, "one segment a phase: {segments:?}");
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.weight)
            .collect::<Vec<_>>(),
        [4, 5, 6, 19, 4, 4]
    );
    assert_eq!(segments[3].role, Role::Warning, "the partial phase");
    assert_eq!(segments[3].filled, 890);

    let table = all
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Table { .. }))
        .expect("the phases table, on the first tab");
    assert_eq!(table.children.len(), 6, "a row a phase");
    let drawn = texts(&root);
    for phase in [
        "Fundação da loja",
        "Consulta de dados e diagnóstico",
        "Relatórios do lojista",
    ] {
        assert!(drawn.iter().any(|text| text == phase), "{phase}: {drawn:?}");
    }
    assert!(drawn.iter().any(|text| text == "65%"), "{drawn:?}");
    assert!(drawn.iter().any(|text| text == "3 of 6 done"), "{drawn:?}");
    assert!(
        all.iter().any(|node| matches!(
            &node.kind,
            NodeKind::Pips {
                done: 17,
                total: 19
            }
        )),
        "the phase in progress, plan by plan"
    );

    let tabs: Vec<(String, bool)> = all
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Tabs { tabs } => Some(
                tabs.iter()
                    .map(|tab| (tab.label.clone(), tab.active))
                    .collect(),
            ),
            _ => None,
        })
        .expect("the tabs");
    assert_eq!(
        tabs,
        [("phases".into(), true), ("activity".into(), false)],
        "a control's label is drawn lowercase"
    );
    assert!(
        all.iter().any(|node| matches!(
            &node.kind,
            NodeKind::Button { label, .. } if label == "run phase 12"
        )),
        "the next step, labelled in the operator's locale"
    );

    let section = engine
        .section("summary", false)
        .expect("the band's section");
    assert_eq!(section.title, "GSD");
    let section_tree = section.tree.expect("drawn from its own view");
    assert!(
        texts(&section_tree).contains(&"Consulta de dados e diagnóstico".to_owned()),
        "the sidebar shows the phase in progress: {:?}",
        texts(&section_tree)
    );
}

#[test]
fn a_tab_switches_what_is_drawn_and_a_save_in_the_working_tree_is_what_compiles_next() {
    let env = TestEnvironment::isolated();
    let (app, market) = installed_from_a_linked_marketplace(&env);
    let band = the_band(&app);
    let mut engine = engine(&app, &band);
    let data_before = engine.data("milestone").cloned();

    let tabs = nodes(&board(&engine))
        .into_iter()
        .find(|node| matches!(node.kind, NodeKind::Tabs { .. }))
        .expect("the tabs")
        .id;
    engine.hit(
        BOARD,
        ViewHit::Node {
            node: tabs,
            act: NodeAct::Tab(1),
        },
        SPACE,
    );
    let root = board(&engine);
    assert!(
        nodes(&root)
            .iter()
            .all(|node| !matches!(node.kind, NodeKind::Table { .. })),
        "the phases give way to the activity"
    );
    assert!(
        nodes(&root).iter().any(|node| matches!(
            &node.kind,
            NodeKind::Log { lines } if lines.iter().any(|line| line.text == "Turn finished in 4m12s")
        )),
        "the activity is drawn"
    );
    assert_eq!(
        engine.data("milestone").cloned(),
        data_before,
        "the data is untouched"
    );
    assert!(engine.recorded().is_empty(), "nothing ran");

    // The author saves a change to the view in the working tree.
    let view = band.dir.join("board.uze");
    let before = extension_stamp(&band.dir);
    let source = fs::read_to_string(&view).unwrap();
    let edited = source.replace(r#"Text "{percent}%""#, r#"Text "{percent}% done""#);
    assert_ne!(edited, source, "the edit lands");
    fs::write(&view, &edited).unwrap();
    assert_ne!(extension_stamp(&band.dir), before, "the save is seen");
    assert_eq!(
        canonical(&the_band(&app).dir),
        canonical(&market.join("plugins/gsd/extensions/gsd-band")),
        "without a reinstall"
    );

    let recompiled = app.extensions().compile(&band).expect("the edit compiles");
    engine.replace(recompiled);
    let root = board(&engine);
    assert!(
        texts(&root).contains(&"65% done".to_owned()),
        "{:?}",
        texts(&root)
    );
    assert_eq!(
        engine
            .ui(BOARD)
            .and_then(|ui| ui.get("tab"))
            .map(String::as_str),
        Some("log"),
        "the reload keeps the tab"
    );

    // A broken save is said at its file, line and column.
    let broken = edited.replace("        Table \"#\"", "        Tabel \"#\"");
    fs::write(&view, &broken).unwrap();
    let diagnostics = app
        .extensions()
        .compile(&band)
        .expect_err("an unknown node is refused");
    let line = broken
        .lines()
        .position(|line| line.contains("Tabel"))
        .unwrap()
        + 1;
    let named = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("Tabel"))
        .unwrap_or_else(|| panic!("{diagnostics:#?}"));
    assert!(named.file.ends_with("board.uze"), "{named:?}");
    assert_eq!((named.line, named.column), (line, 9), "{named:?}");
    assert!(
        named.to_string().contains(&format!("board.uze:{line}:9:")),
        "{named}"
    );
}

#[test]
fn the_panel_is_a_drawer_whose_story_is_told_whole() {
    let env = TestEnvironment::isolated();
    let (app, _market) = installed_from_a_linked_marketplace(&env);
    let panel = the(&app, "gsd-panel");
    let load = app.extensions().load(&panel).expect("the panel compiles");
    let compiled = load.compiled;
    let surface = compiled.manifest.surface("panel").expect("its one surface");
    assert_eq!(surface.slot, uze_contract::Slot::Drawer);

    let mut engine = DeclaredExtension::new(compiled.clone(), "en");
    for (source, data) in load.fixtures {
        engine.set_data(source, data);
    }
    let view = engine
        .view("panel", SPACE)
        .expect("a drawer answers a view");
    let Content::Tree { root, .. } = view.content else {
        panic!("a drawer is a tree");
    };
    let drawn = texts(&root);
    for wanted in ["Phases", "Blockers", "Stopped at", "+71 more in STATE.md"] {
        assert!(
            drawn.iter().any(|text| text == wanted),
            "{wanted}: {drawn:?}"
        );
    }
    assert!(
        nodes(&root).iter().any(|node| matches!(
            &node.kind,
            NodeKind::Button { label, .. } if label == "▶ /gsd-discuss-phase 43"
        )),
        "the next step"
    );
    let stopped = drawn
        .iter()
        .find(|text| text.starts_with("Fases 43-45"))
        .expect("where the session stopped");
    assert!(
        stopped.ends_with("Falta: commit + validação live em staging."),
        "the paragraph is whole: {stopped}"
    );
}
