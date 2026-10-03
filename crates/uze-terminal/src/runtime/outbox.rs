//! A client as the server holds it: its selection and the bounded outbox its events wait in.

use super::*;

/// What one attached client is looking at. The session itself carries the
/// server's defaults; a client's own selection overlays them in the
/// `Session` it receives, so two terminals attached to the one server can
/// look at two different agents.
#[derive(Clone, Debug, Default)]
pub(super) struct Selection {
    pub(super) space: Option<SpaceId>,
    pub(super) tabs: BTreeMap<SpaceId, TabId>,
}

pub(super) struct Client {
    pub(super) id: u64,
    pub(super) events: Arc<Outbox>,
    pub(super) selection: Selection,
}

/// How many events a client may have waiting on its socket before it is
/// treated as stale. A frame is at most one pane's repaint, so this bounds
/// what a client that stopped reading can make the server hold.
pub(super) const OUTBOX_CAPACITY: usize = 256;

/// The events waiting for one client's socket.
///
/// Bounded, because a client that stops reading — a suspended `uze`, a
/// stalled socket — would otherwise have every repaint of every pane
/// queued for it for as long as it stays attached. What overflows is not
/// kept: the client is marked stale and, once its queue has drained, is
/// sent the whole workspace again (see [`Server::resync_stale_clients`]).
/// Nothing is lost by dropping repaints — each one carries absolute cells,
/// and the resync supersedes all of them.
pub(super) struct Outbox {
    pub(super) sender: mpsc::SyncSender<ClientEvent>,
    pub(super) backlog: Arc<Backlog>,
}

/// How far behind one client is — the only part of its [`Outbox`] the
/// writer thread is given.
///
/// It is split out because the sender must not be: a writer holding an
/// `Outbox` holds a sender to the very channel it is blocked on, so
/// `recv` could never report the client gone and the thread outlived
/// the connection by the life of the server. Every probe of the endpoint
/// then cost a thread and a descriptor permanently, and a machine that
/// had run for a day could no longer `fork`.
pub(super) struct Backlog {
    pub(super) pending: std::sync::atomic::AtomicUsize,
    pub(super) stale: std::sync::atomic::AtomicBool,
}

impl Backlog {
    pub(super) fn delivered(&self) {
        self.pending
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Outbox {
    pub(super) fn new() -> (Self, mpsc::Receiver<ClientEvent>) {
        let (sender, receiver) = mpsc::sync_channel(OUTBOX_CAPACITY);
        let outbox = Self {
            sender,
            backlog: Arc::new(Backlog {
                pending: std::sync::atomic::AtomicUsize::new(0),
                stale: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        (outbox, receiver)
    }

    /// A handle for the writer thread serving this client.
    pub(super) fn backlog(&self) -> Arc<Backlog> {
        Arc::clone(&self.backlog)
    }

    /// Queues a broadcast without ever waiting, and answers whether the
    /// client is still there. A stale client is sent nothing until it is
    /// resynchronized.
    pub(super) fn offer(&self, event: ClientEvent) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        if self.backlog.stale.load(Relaxed) {
            return true;
        }
        match self.sender.try_send(event) {
            Ok(()) => {
                self.backlog.pending.fetch_add(1, Relaxed);
                true
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.backlog.stale.store(true, Relaxed);
                true
            }
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        }
    }

    /// Queues an answer to this client's own request. It may wait: only
    /// the thread serving this client is held, and an answer is not
    /// something a resync could stand in for.
    pub(super) fn reply(&self, event: ClientEvent) {
        if self.sender.send(event).is_ok() {
            self.backlog
                .pending
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Whether this client missed broadcasts, caught up or not.
    pub(super) fn is_stale(&self) -> bool {
        self.backlog
            .stale
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether this client missed broadcasts and has since caught up with
    /// everything it was sent, so a resync would reach it.
    pub(super) fn awaits_resync(&self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        self.backlog.stale.load(Relaxed) && self.backlog.pending.load(Relaxed) == 0
    }

    /// Sends the whole workspace to a stale client, and answers whether the
    /// client is still there. A resync that overflows again leaves the
    /// client stale; the next one starts from a fresh `Snapshot`.
    pub(super) fn resync(&self, session: Session, repaints: &[PaneDamage]) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        self.backlog.stale.store(false, Relaxed);
        let events = std::iter::once(ClientEvent::Snapshot { session })
            .chain(repaints.iter().cloned().map(ClientEvent::Damage));
        for event in events {
            if !self.offer(event) {
                return false;
            }
            if self.backlog.stale.load(Relaxed) {
                break;
            }
        }
        true
    }
}
