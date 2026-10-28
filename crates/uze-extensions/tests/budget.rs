//! D28's budget for binding and memory, held over the gallery
//! (`extension-platform` D28): binding a compiled view to its data
//! under 1 ms per surface, and under 1 MiB held per loaded extension.
//!
//! Its own test target, so its clock measures the binder and not the
//! hundreds of sibling tests a lib-test binary runs beside it. D28
//! measured 0.03 ms for a prototype binder in a release build; these run
//! unoptimized on whatever machine, so each surface's cost is the fastest
//! of many builds, which a loaded machine cannot raise. Unoptimized, the
//! gallery's heaviest view (`snake`) binds in about 0.4 ms on Linux and
//! 1.1 ms on CI's Windows runners, whose slower cores a fastest-of-many
//! cannot hide; so an optimized build is held to the target itself and an
//! unoptimized one to twice it, which still fails a binder that doubles.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use uze_contract::{GivenData, Identity, SourceKind};
use uze_extensions::contract::Extension;
use uze_extensions::declared::DeclaredExtension;
use uze_extensions::view::Size;

/// D28's targets; binding's, doubled where the binder runs unoptimized.
const BINDING_TARGET: Duration = Duration::from_millis(if cfg!(debug_assertions) { 2 } else { 1 });
const MEMORY_TARGET: usize = 1024 * 1024;

/// Builds measured per surface.
const BUILDS: usize = 41;

/// Every extension the gallery ships, by its directory.
fn gallery() -> Vec<PathBuf> {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/_fixtures/canonical");
    let mut found = Vec::new();
    for plugin in std::fs::read_dir(&canonical).unwrap().flatten() {
        let Ok(extensions) = std::fs::read_dir(plugin.path().join("extensions")) else {
            continue;
        };
        found.extend(extensions.flatten().map(|entry| entry.path()));
    }
    found.sort();
    assert!(found.len() >= 10, "the gallery is found: {found:?}");
    found
}

/// The extension in `dir`, compiled, with every source fed what its own
/// fixture or its tests give it: the data its views are drawn from.
fn loaded(dir: &Path) -> DeclaredExtension {
    let id = dir.file_name().unwrap().to_string_lossy().into_owned();
    let identity = Identity {
        marketplace: "gallery".to_owned(),
        plugin: id.clone(),
        id,
    };
    let compiled = uze_core::capability::extension::compile(identity, dir)
        .unwrap_or_else(|diagnostics| panic!("{}: {diagnostics:?}", dir.display()));
    let read = |file: &str| -> Option<serde_json::Value> {
        serde_json::from_slice(&std::fs::read(dir.join(file)).ok()?).ok()
    };
    let mut data = std::collections::BTreeMap::new();
    for source in &compiled.manifest.sources {
        if let SourceKind::Fixture(file) = &source.kind
            && let Some(value) = read(file)
        {
            data.insert(source.id.clone(), value);
        }
    }
    for file in uze_core::capability::extension::test_files(dir) {
        let Ok(tests) =
            uze_core::capability::extension::compile_test(dir, &file, &Default::default())
        else {
            continue;
        };
        for given in tests.tests.iter().flat_map(|test| &test.world.given) {
            let value = match &given.data {
                GivenData::Json(value) => Some(value.clone()),
                GivenData::Fixture(file) => read(file),
            };
            if let Some(value) = value {
                data.entry(given.source.clone()).or_insert(value);
            }
        }
    }
    let mut engine = DeclaredExtension::new(compiled, "en");
    for (source, value) in data {
        engine.set_data(source, Ok(value));
    }
    engine
}

/// The fastest of [`BUILDS`] fresh binds of `surface`: the space moves by
/// a column each time, which is a new build of the same data. The fastest
/// states what the binder can do, which is what a regression changes; a
/// loaded machine only ever adds to it.
fn best_build(engine: &DeclaredExtension, surface: &str) -> Duration {
    let mut took = Vec::with_capacity(BUILDS);
    for build in 0..BUILDS {
        let space = Size {
            width: 100 + (build % 2) as u16,
            height: 30,
        };
        let before = engine.builds();
        let started = Instant::now();
        let _ = engine.view(surface, space);
        let elapsed = started.elapsed();
        if engine.builds() > before {
            took.push(elapsed);
        }
    }
    took.into_iter().min().unwrap_or_default()
}

#[test]
fn binding_a_gallery_view_costs_under_a_millisecond_per_surface() {
    let mut measured = Vec::new();
    for dir in gallery() {
        let engine = loaded(&dir);
        for surface in &engine.compiled().manifest.surfaces {
            // A section binds against no space of its own, so a build of
            // it cannot be made fresh by moving one; its view is measured
            // wherever another slot draws it.
            if engine.data(&surface.source).is_none() || surface.slot == uze_contract::Slot::Section
            {
                continue;
            }
            let best = best_build(&engine, &surface.id);
            measured.push((
                format!("{}/{}", dir.file_name().unwrap().display(), surface.id),
                best,
            ));
        }
    }
    assert!(
        measured.len() >= 10,
        "the gallery's views bind: {measured:?}"
    );
    let over: Vec<_> = measured
        .iter()
        .filter(|(_, best)| *best > BINDING_TARGET)
        .collect();
    assert!(over.is_empty(), "over {BINDING_TARGET:?}: {over:?}");
}

#[test]
fn a_loaded_gallery_extension_holds_under_a_mebibyte() {
    for dir in gallery() {
        let engine = loaded(&dir);
        for surface in &engine.compiled().manifest.surfaces {
            let _ = engine.view(
                &surface.id,
                Size {
                    width: 120,
                    height: 40,
                },
            );
            let _ = engine.section(&surface.id, false);
        }
        let held = engine.approximate_bytes();
        assert!(
            held > 0 && held < MEMORY_TARGET,
            "{} holds {held} bytes",
            dir.display()
        );
    }
}
