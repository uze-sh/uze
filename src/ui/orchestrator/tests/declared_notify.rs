//! A notification an answer asks for, through the real declared engine and
//! the frame loop's sending: attributed, sent once within the extension's
//! rate with the rest counted on its surface, and never sent for an
//! extension that does not hold `notify`.

use super::support::*;
use super::{
    EngineAnswer, ExtensionAnswer, HostedExtensions, LoadedExtension, Outgoing, Owner,
    WorkspaceModel, send_notification,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension};
use uze_extensions::contract::Checkout;
use uze_extensions::declared::NOTIFY_INTERVAL;
use uze_extensions::{Answer, DirEntry, Question, Unreadable};

const ROOT: &str = "/repo";

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "proving".to_owned(),
        plugin: "gpu".to_owned(),
        id: "gpu-meter".to_owned(),
    }
}

/// A section showing a source whose script answers, wanting `wants`.
fn compiled(wants: Vec<uze_contract::Grant>) -> CompiledExtension {
    let view = uze_contract::CompiledView {
        file: "meter.kdl".to_owned(),
        items: Vec::new(),
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    };
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "gpu-meter".to_owned(),
            name: "GPU meter".to_owned(),
            description: String::new(),
            surfaces: vec![uze_contract::SurfaceDecl {
                band: None,
                bar: None,
                id: "meter".to_owned(),
                slot: uze_contract::Slot::Section,
                view: "meter.kdl".to_owned(),
                source: "load".to_owned(),
                title: None,
                applies_when: None,
            }],
            wants,
            sources: vec![uze_contract::SourceDecl::new(
                "load",
                uze_contract::SourceKind::Script("load.rhai".to_owned()),
            )],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        views: BTreeMap::from([("meter.kdl".to_owned(), view)]),
    }
}

/// An agent in front and the extension hosted through the real engine.
fn hosting(compiled: CompiledExtension) -> WorkspaceModel {
    let mut model = agent_session_in(ROOT);
    model.remembered.declared = HostedExtensions {
        discovered_at: Some(Instant::now()),
        ..HostedExtensions::default()
    };
    let installed = InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/proving/plugins/gpu/extensions/gpu-meter"),
        linked: false,
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed.clone()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed,
            stamp: 1,
            compiled: Ok(compiled),
            data: BTreeMap::new(),
        }],
    }));
    model
}

/// A host whose every question its script answers with `envelope`.
struct Answering(uze_contract::Envelope);

impl uze_extensions::Host for Answering {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
        Err("no git here".to_owned())
    }
    fn read_file(&self, _: &Path) -> Result<String, Unreadable> {
        Err(Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        Err("read only".to_owned())
    }
    fn syntax_theme(&self) -> String {
        String::new()
    }
    fn answer(&self, _question: Question) -> Answer {
        Answer::Answered {
            envelope: self.0.clone(),
            trace: Vec::new(),
        }
    }
}

fn notifying() -> uze_contract::Envelope {
    uze_contract::Envelope {
        notify: Some(Box::new(uze_contract::Notification {
            title: "Hot".to_owned(),
            body: "load 92%".to_owned(),
        })),
        sound: Some(uze_contract::Sound::Attention),
        ..uze_contract::Envelope::data(serde_json::json!({ "load": 92 }))
    }
}

/// The source asked once more on the pass at `now`, its answer taken
/// the way the frame loop takes one.
fn answers(model: &mut WorkspaceModel, envelope: uze_contract::Envelope, now: Instant) {
    let owner = Owner::Declared(identity());
    let engine = model
        .remembered
        .declared
        .engines
        .get_mut(&owner)
        .expect("hosted");
    engine.reconsider();
    engine.focus(Some(&Checkout {
        root: PathBuf::from(ROOT),
        display: ROOT.to_owned(),
        target: None,
    }));
    model.pass_engines(now);
    let jobs = model
        .remembered
        .declared
        .engines
        .get_mut(&owner)
        .expect("hosted")
        .jobs();
    assert_eq!(jobs.len(), 1, "the section's source is asked");
    let host = Answering(envelope);
    for job in jobs {
        let answer = (job.run)(&host);
        model.absorb_engine_answer(EngineAnswer {
            extension: owner.clone(),
            answer,
        });
    }
}

/// What the frame loop writes to the terminal for what was asked since
/// the last frame, in a terminal that draws OSC 9, and the desktop tools
/// it would start.
fn sent(model: &mut WorkspaceModel) -> (Vec<Vec<u8>>, usize) {
    let mut written = Vec::new();
    let mut started = 0;
    for outgoing in model.take_notifications() {
        send_notification(
            &outgoing,
            Some("WezTerm"),
            &mut |bytes| written.push(bytes.to_vec()),
            &mut |_| started += 1,
        );
    }
    (written, started)
}

fn notice(model: &WorkspaceModel) -> Option<String> {
    let engine = model
        .remembered
        .declared
        .engine(&Owner::Declared(identity()))
        .expect("hosted");
    let caption = engine.section("meter", false)?.caption;
    (!caption.text.is_empty()).then_some(caption.text)
}

#[test]
fn a_notification_is_attributed_sent_once_and_the_next_within_ten_seconds_counted() {
    let mut model = hosting(compiled(vec![uze_contract::Grant::Notify]));
    let start = Instant::now();
    answers(&mut model, notifying(), start);
    let outgoing = model.take_notifications();
    assert_eq!(
        outgoing,
        [Outgoing {
            notification: Some((
                "gpu-meter · gpu@proving: Hot".to_owned(),
                "load 92%".to_owned(),
            )),
            sound: true,
        }],
        "led by the extension's attribution"
    );
    let mut written = Vec::new();
    let mut started = 0;
    send_notification(
        &outgoing[0],
        Some("WezTerm"),
        &mut |bytes| written.push(bytes.to_vec()),
        &mut |_| started += 1,
    );
    // A desktop session takes the notification; the cue is the bell
    // either way.
    let osc9 = b"\x1b]9;gpu-meter \xc2\xb7 gpu@proving: Hot: load 92%\x07".to_vec();
    match started {
        0 => assert_eq!(written, [osc9, b"\x07".to_vec()]),
        _ => assert_eq!(written, [b"\x07".to_vec()]),
    }
    assert!(model.take_notifications().is_empty(), "taken once");

    answers(&mut model, notifying(), start + NOTIFY_INTERVAL / 2);
    assert_eq!(sent(&mut model), (Vec::new(), 0), "within ten seconds");
    assert_eq!(
        notice(&model).as_deref(),
        Some("1 notification dropped: at most 1 every 10 s and 30 an hour")
    );

    answers(&mut model, notifying(), start + NOTIFY_INTERVAL * 2);
    let (written, started) = sent(&mut model);
    assert_eq!(started + written.len(), 2, "rested, it goes out again");
    assert_eq!(notice(&model), None);
}

#[test]
fn an_extension_without_notify_sends_nothing() {
    let mut model = hosting(compiled(Vec::new()));
    answers(&mut model, notifying(), Instant::now());
    assert_eq!(sent(&mut model), (Vec::new(), 0));
    assert_eq!(
        notice(&model).as_deref(),
        Some("not notified: needs the notify grant")
    );
}

/// A cue alone is the bell, rung once.
#[test]
fn a_sound_alone_rings_the_bell() {
    let mut model = hosting(compiled(vec![uze_contract::Grant::Notify]));
    let cue = uze_contract::Envelope {
        sound: Some(uze_contract::Sound::Done),
        ..uze_contract::Envelope::data(serde_json::json!({}))
    };
    answers(&mut model, cue, Instant::now());
    assert_eq!(sent(&mut model), (vec![b"\x07".to_vec()], 0));
}
