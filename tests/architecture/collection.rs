//! Every path UZE sweeps has exactly one collector (design D7 of
//! `doctor-repair`): a disposable path added to the map with no owner to
//! decide what of it is still needed is how a dozen collection mechanisms
//! came to be scattered, and one claimed twice is two rules racing over
//! the same files.

use std::collections::BTreeMap;

use uze_application::{UzeApplication, UzeHome};

#[test]
fn every_disposable_root_has_one_collector() {
    uze::telemetry::register_collector();
    let root = uze_testkit::temp::scratch("architecture-collectors");
    let home = UzeHome::at(root.join("home"));
    let app = UzeApplication::from_env(home).expect("the application composes");

    let mut claimed: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for collector in app.collectors() {
        for path in collector.roots() {
            claimed.entry(path).or_default().push(collector.name());
        }
    }
    let uncollected: Vec<&str> = UzeHome::owned_paths()
        .iter()
        .filter(|owned| owned.disposable && !claimed.contains_key(owned.name))
        .map(|owned| owned.name)
        .collect();
    assert!(
        uncollected.is_empty(),
        "disposable paths no collector answers for: {uncollected:?}\n\n  \
         Fix: implement `Collectable` where the path's rule is known and \
         register it in `UzeApplication::collectors`."
    );
    let twice: Vec<_> = claimed
        .iter()
        .filter(|(_, collectors)| collectors.len() > 1)
        .collect();
    assert!(twice.is_empty(), "paths two collectors claim: {twice:?}");
    let not_disposable: Vec<&&str> = claimed
        .keys()
        .filter(|name| !UzeHome::owned_path(name).is_some_and(|owned| owned.disposable))
        .collect();
    assert!(
        not_disposable.is_empty(),
        "collectors claim paths the map does not mark disposable: {not_disposable:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}
