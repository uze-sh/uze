//! Local terminal runtime used by UZE's workspace client.
//!
//! The server owns pseudoterminals and emulation state.  UI clients only
//! attach, render snapshots, and forward input; this keeps a pane alive when
//! a client leaves the workspace.

pub mod launch;
mod protocol;
mod runtime;
mod selection;
mod state;

pub use protocol::{
    CellAttributes, ClientEvent, ClientRequest, Cursor, Load, MouseMode, PROTOCOL_VERSION, Palette,
    PaneDamage, PaneSnapshot, RenderCell, Scheme, Seating, SelectionGesture, TerminalColor,
    USAGE_WINDOW, UsageReport,
};
pub use runtime::{
    Attached, Replaced, RuntimeError, Stream, Usage, answer_to_attach, attach, connect, host_pane,
    host_panes_with, open_space, put_first_on_pane_path, read_event, send_request, serve,
    server_key, socket_path, stop, stream_pair, usage,
};
pub use state::{
    NewSpace, Pane, PaneId, Session, Space, SpaceId, SpaceSeat, Tab, TabId, Workspace,
};
