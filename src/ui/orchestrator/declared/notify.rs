//! What a declared extension's answers asked to tell the operator outside
//! the frame, sent from the frame loop (S6).
//!
//! The engine lets a notification through only under `notify` and within
//! the extension's rate, and counts what it held back in its notice. The
//! host takes what it let through only from an extension whose manifest
//! holds the grant, leads a desktop notification's title with the
//! extension's name, and sends it the way `uze_platform::notify` chooses: a
//! desktop tool on a thread of its own, or bytes for the terminal written
//! between frames. The sidebar's notice carries what the extension said and
//! nothing else: an id there took the row from the message.

use std::process::{Command, Stdio};

use super::*;
use uze_platform::notify::{BELL, Route, route};

/// One notification on its way out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui::orchestrator) struct Outgoing {
    /// The title, led by who asked, and the body; `None` for a cue alone.
    pub(in crate::ui::orchestrator) notification: Option<(String, String)>,
    /// Whether a cue sounds. Every cue is the terminal's bell.
    pub(in crate::ui::orchestrator) sound: bool,
}

impl WorkspaceModel {
    /// What the hosted extensions asked to send since the last frame. An
    /// extension that does not hold `notify` has what it asked dropped.
    pub(in crate::ui::orchestrator) fn take_notifications(&mut self) -> Vec<Outgoing> {
        let mut outgoing = Vec::new();
        let mut notice = None;
        for (owner, engine) in &mut self.remembered.declared.engines {
            let asked = engine.notifications();
            let Some(identity) = owner.declared().filter(|_| engine.may_notify()) else {
                continue;
            };
            let by = engine
                .declared()
                .map(|declared| declared.compiled().manifest.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| crate::ui::extension_view::tree::attribution(identity));
            for notify in asked {
                if let Some(said) = &notify.notification {
                    let headline = if said.title.is_empty() {
                        &by
                    } else {
                        &said.title
                    };
                    notice = Some((headline.clone(), said.body.clone()));
                }
                outgoing.push(Outgoing {
                    notification: notify
                        .notification
                        .map(|said| (attributed_title(&by, &said.title), said.body)),
                    sound: notify.sound.is_some(),
                });
            }
        }
        if notice.is_some() {
            self.extension_notice = notice;
            self.dirty = true;
        }
        outgoing
    }
}

/// `title`, led by who said it, as every third party's word is.
fn attributed_title(by: &str, title: &str) -> String {
    if title.is_empty() {
        by.to_owned()
    } else {
        format!("{by}: {title}")
    }
}

/// Sends `outgoing`: what is for the terminal through `emit`, between
/// frames, and a desktop tool through `spawn`. A cue is the bell, rung
/// once even when the notification already fell back to it.
pub(in crate::ui::orchestrator) fn send_notification(
    outgoing: &Outgoing,
    term_program: Option<&str>,
    emit: &mut dyn FnMut(&[u8]),
    spawn: &mut dyn FnMut(Command),
) {
    let mut belled = false;
    if let Some((title, body)) = &outgoing.notification {
        match route(title, body, term_program) {
            Route::Desktop(command) => spawn(command),
            Route::Terminal(bytes) => {
                belled = bytes == BELL;
                emit(&bytes);
            }
        }
    }
    if outgoing.sound && !belled {
        emit(BELL);
    }
}

/// Runs the desktop's notification tool on a thread of its own, never the
/// one that draws, with nothing of it reaching the terminal.
pub(in crate::ui::orchestrator) fn spawn_desktop(mut command: Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _ = std::thread::Builder::new()
        .name("uze-notify".to_owned())
        .spawn(move || {
            let _ = command.status();
        });
}
