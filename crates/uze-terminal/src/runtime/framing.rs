//! The wire: length-prefixed frames, the handshake, and their limits.

use super::*;

pub fn send_request<W: Write>(writer: &mut W, value: &ClientRequest) -> Result<(), RuntimeError> {
    write_message(writer, value)
}
pub fn read_event<R: Read>(reader: &mut R) -> Result<Option<ClientEvent>, RuntimeError> {
    read_message(reader)
}
/// The largest frame either side of this wire will send or accept.
///
/// Both directions are bounded by the same number, so the two can never
/// disagree about what is sendable — and the number is chosen against the
/// largest thing this protocol legitimately carries: a full repaint of the
/// largest pane it allows, [`MAX_PANE_DIMENSION`] squared cells. A cap
/// below that would disconnect a client at the moment it resized, which is
/// why the two constants are tied rather than each picked on its own
/// (`a_full_repaint_of_the_largest_pane_fits_in_one_frame` holds them
/// together). Everything else on this wire is orders of magnitude smaller.
pub(super) const MAX_FRAME: u32 = 64 * 1024 * 1024;

/// How long a connection may stay silent before it has said who it is.
///
/// Until a client sends `Attach` it holds a reader thread, a writer thread
/// and whatever it has allocated, and nothing caps how many such
/// connections there are. A peer with nothing to say is dropped instead of
/// held forever; once attached, silence is ordinary — a person is reading.
pub(super) const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

/// The largest first frame a peer may send.
///
/// [`MAX_FRAME`] is sized for the largest *repaint* this wire carries, and
/// a repaint is a thing the server sends to a client it already knows. The
/// first frame is the opposite: nobody has vouched for the peer, and the
/// only two things it may legitimately say — `Attach`, naming a workspace
/// and a root, or `Stop` — are hundreds of bytes. Bounding it here rather
/// than at 64 MiB is the difference between a stranger reserving a path
/// and a stranger reserving memory.
pub(super) const MAX_HANDSHAKE_FRAME: u32 = 64 * 1024;

/// Bounds the whole handshake, rather than each read that makes it up.
///
/// `SO_RCVTIMEO` restarts on every successful read, so a peer dribbling one
/// byte just inside the timeout holds a reader thread, a writer thread and
/// whatever it has allocated for as long as it likes — which is precisely
/// what [`HANDSHAKE_DEADLINE`] exists to prevent. One deadline over the
/// whole exchange is what that actually takes. [`Handshake::attached`]
/// disarms it once the peer has said who it is.
pub(super) struct Handshake {
    pub(super) socket: Stream,
    pub(super) deadline: Option<Instant>,
}

impl Handshake {
    pub(super) fn new(socket: Stream, within: Duration) -> Self {
        Self {
            socket,
            deadline: Some(Instant::now() + within),
        }
    }

    pub(super) fn socket(&mut self) -> &mut Stream {
        &mut self.socket
    }

    /// Silence is a person reading from here on, not a peer holding
    /// threads it never intends to use.
    pub(super) fn attached(&mut self) {
        self.deadline = None;
        let _ = self.socket.set_read_timeout(None);
    }
}

impl Read for Handshake {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the peer never said who it is",
                ));
            }
            // Best-effort: a platform that will not take one leaves the
            // read blocking, which is where it was before.
            let _ = self.socket.set_read_timeout(Some(remaining));
        }
        self.socket.read(buffer)
    }
}

/// Length-prefixed bincode, not newline-delimited JSON: a `PaneSnapshot`
/// carries one `RenderCell` per grid cell, and JSON's per-field text
/// encoding of that (a `Snapshot`/`Damage` this size fires on every PTY
/// repaint — scrolling an agent's own transcript, not just resizes) was
/// measured spending hundreds of milliseconds in encode+decode alone on a
/// realistic multi-tab session, which is what made attaching a client
/// and scrolling inside a pane both feel slow. Framing can't be
/// newline-delimited any more since the payload is binary and may contain
/// a literal `0x0A` byte anywhere in it.
pub(super) fn write_message<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), RuntimeError> {
    // The length and the body as one write: two cost a second write, and a
    // pipe's a second overlapped operation, on every frame.
    let mut frame = vec![0u8; 4];
    bincode::serialize_into(&mut frame, value)
        .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
    let body = frame.len() - 4;
    let len = u32::try_from(body)
        .ok()
        .filter(|len| *len <= MAX_FRAME)
        .ok_or_else(|| oversized_frame(body as u64, MAX_FRAME))?;
    frame[..4].copy_from_slice(&len.to_le_bytes());
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}
pub(super) fn read_message<R: Read, T: DeserializeOwned>(
    reader: &mut R,
) -> Result<Option<T>, RuntimeError> {
    read_message_within(reader, MAX_FRAME)
}

/// The same read held to a smaller bound than the wire's own — see
/// [`MAX_HANDSHAKE_FRAME`].
pub(super) fn read_message_within<R: Read, T: DeserializeOwned>(
    reader: &mut R,
    limit: u32,
) -> Result<Option<T>, RuntimeError> {
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let len = u32::from_le_bytes(len_bytes);
    // Refused before it is allocated, not after. This prefix is the first
    // thing a peer says and the protocol version lives *inside* the frame
    // it describes, so nothing has vouched for the peer yet — and the
    // allocation is whatever the four bytes claim, up to 4 GiB.
    if len > limit {
        return Err(oversized_frame(u64::from(len), limit));
    }
    let mut buffer = vec![0u8; len as usize];
    reader.read_exact(&mut buffer)?;
    bincode::deserialize(&buffer)
        .map(Some)
        .map_err(|error| RuntimeError::Protocol(error.to_string()))
}

pub(super) fn oversized_frame(len: u64, limit: u32) -> RuntimeError {
    RuntimeError::Protocol(format!(
        "frame of {len} bytes exceeds the {limit}-byte limit"
    ))
}
