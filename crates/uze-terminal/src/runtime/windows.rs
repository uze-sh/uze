//! What the runtime asks of Windows itself beyond what `uze-process`
//! answers: the stop event an old server is retired through.

use std::{ffi::OsStr, io, ptr};

use windows_sys::Win32::System::Threading::{
    CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};

pub(crate) use uze_process::windows::{
    Job as PaneJob, Owned, current_user_sid, image_of, terminate, wide,
};

/// The event a server waits on to stop: named after its endpoint, and a
/// shape no build ever changes, so any build can retire any other.
pub(crate) fn stop_event_name(endpoint: &std::path::Path) -> String {
    let leaf = endpoint
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("Local\\{leaf}-stop")
}

/// Blocks until the stop event for `name` is signalled.
pub(crate) fn wait_for_stop(name: &str) -> io::Result<()> {
    let wide_name = wide(OsStr::new(name));
    // SAFETY: a named manual-reset event, default security (the caller's
    // own session namespace).
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, wide_name.as_ptr()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let event = Owned(event);
    // SAFETY: valid event handle.
    unsafe { WaitForSingleObject(event.0, INFINITE) };
    Ok(())
}

/// Asks the server waiting on `name` to stop. `false` when no server of
/// that name is waiting.
pub(crate) fn signal_stop(name: &str) -> bool {
    let wide_name = wide(OsStr::new(name));
    // SAFETY: opens an existing event by name; null when there is none.
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide_name.as_ptr()) };
    if event.is_null() {
        return false;
    }
    let event = Owned(event);
    // SAFETY: valid event handle with modify access.
    unsafe { SetEvent(event.0) != 0 }
}
