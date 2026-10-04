//! The endpoint's transport: how a client reaches the server and how the
//! server accepts it, behind one shape on every platform (ADR-038).
//!
//! A Unix-domain socket on Linux and macOS; a named pipe on Windows. Callers
//! name [`Stream`] and [`Listener`] and never the platform's own type, so
//! the protocol, the handshake and the server's lifecycle are written once.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
pub use pipe::{PipeListener as Listener, PipeStream as Stream};

use std::{io, path::Path};

pub fn connect(endpoint: &Path) -> io::Result<Stream> {
    Stream::connect(endpoint)
}

pub fn bind(endpoint: &Path) -> io::Result<Listener> {
    Listener::bind(endpoint)
}

pub fn accept(listener: &Listener) -> io::Result<Stream> {
    #[cfg(unix)]
    {
        listener.accept().map(|(stream, _)| stream)
    }
    #[cfg(windows)]
    {
        listener.accept()
    }
}

#[cfg(windows)]
mod pipe {
    //! A named pipe that behaves like the socket it replaces.
    //!
    //! Invariants this module holds, each the answer to a way pipes fail
    //! where sockets do not:
    //!
    //! - **Every handle is overlapped.** A reader thread and a writer thread
    //!   share one connection; on a synchronous handle the I/O manager
    //!   serializes their calls, so a blocked read stalls every write.
    //! - **A cancelled operation is always completed before its buffer is
    //!   freed.** `CancelIoEx` only requests cancellation; the kernel may
    //!   still be writing into the buffer, and data that completed during
    //!   the cancel is real data.
    //! - **The pipe is the user's.** The first instance is created with
    //!   `FILE_FLAG_FIRST_PIPE_INSTANCE` (so a name another process created
    //!   first is refused, not shared), refuses remote clients, and grants
    //!   access to the current token's user alone. Clients connect at
    //!   identification level, so a server can never impersonate them.
    //! - **One instance is always pending.** A client arriving between two
    //!   accepts finds an instance to connect to instead of an error.

    use std::{
        ffi::OsStr,
        io::{self, Read, Write},
        path::{Path, PathBuf},
        ptr,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA,
            ERROR_OPERATION_ABORTED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
            ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE,
            INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
            PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
            WriteFile,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
                GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
                WaitNamedPipeW,
            },
            Threading::{CreateEventW, INFINITE, WaitForSingleObject},
        },
    };

    use crate::runtime::windows::{current_user_sid, wide};

    const BUFFER: u32 = 64 * 1024;

    /// How long a client waits for a busy server to offer an instance
    /// before reporting the endpoint unreachable.
    const BUSY_WAIT: Duration = Duration::from_secs(2);

    struct Handle(HANDLE);

    // SAFETY: a pipe handle is a kernel object reference, valid from any
    // thread; every operation on it here carries its own OVERLAPPED.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle is owned by this value and closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Side {
        Client,
        Server,
    }

    struct Shared {
        handle: Handle,
        side: Side,
        shut: AtomicBool,
        timeouts: Mutex<(Option<Duration>, Option<Duration>)>,
    }

    /// One end of a connection. Clones share the connection, as a cloned
    /// socket does.
    pub struct PipeStream {
        shared: Arc<Shared>,
    }

    impl PipeStream {
        fn from_handle(handle: HANDLE, side: Side) -> Self {
            Self {
                shared: Arc::new(Shared {
                    handle: Handle(handle),
                    side,
                    shut: AtomicBool::new(false),
                    timeouts: Mutex::new((None, None)),
                }),
            }
        }

        pub fn connect(name: &Path) -> io::Result<Self> {
            let wide_name = wide(name.as_os_str());
            let deadline = Instant::now() + BUSY_WAIT;
            loop {
                // SAFETY: `wide_name` is NUL-terminated and outlives the
                // call; every other argument is a constant or null.
                let handle = unsafe {
                    CreateFileW(
                        wide_name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        0,
                        ptr::null(),
                        OPEN_EXISTING,
                        FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                        ptr::null_mut(),
                    )
                };
                if handle != INVALID_HANDLE_VALUE {
                    return Ok(Self::from_handle(handle, Side::Client));
                }
                // SAFETY: reads this thread's last error and nothing else.
                let error = unsafe { GetLastError() };
                if error != ERROR_PIPE_BUSY {
                    return Err(io::Error::from_raw_os_error(error as i32));
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "every instance of the terminal endpoint stayed busy",
                    ));
                }
                // SAFETY: as above; a false return only means "try again".
                unsafe { WaitNamedPipeW(wide_name.as_ptr(), remaining.as_millis() as u32) };
            }
        }

        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self {
                shared: Arc::clone(&self.shared),
            })
        }

        pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            self.shared
                .timeouts
                .lock()
                .expect("pipe timeouts poisoned")
                .0 = timeout;
            Ok(())
        }

        pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            self.shared
                .timeouts
                .lock()
                .expect("pipe timeouts poisoned")
                .1 = timeout;
            Ok(())
        }

        /// Ends the connection for every clone: a blocked read on another
        /// thread returns end-of-stream, as it does for a shut socket.
        pub fn shutdown(&self, _how: std::net::Shutdown) -> io::Result<()> {
            self.shared.shut.store(true, Ordering::SeqCst);
            // SAFETY: cancels every operation on the handle, from any
            // thread; each one completes its own OVERLAPPED before returning.
            unsafe { CancelIoEx(self.shared.handle.0, ptr::null()) };
            if self.shared.side == Side::Server {
                // SAFETY: valid server-side handle.
                unsafe { DisconnectNamedPipe(self.shared.handle.0) };
            }
            Ok(())
        }

        /// The process at the other end, as the kernel recorded it when the
        /// connection was made — not something the peer can claim.
        pub fn peer_pid(&self) -> Option<u32> {
            let mut pid = 0u32;
            // SAFETY: valid handle, and `pid` outlives the call.
            let ok = unsafe {
                match self.shared.side {
                    Side::Client => GetNamedPipeServerProcessId(self.shared.handle.0, &mut pid),
                    Side::Server => GetNamedPipeClientProcessId(self.shared.handle.0, &mut pid),
                }
            };
            (ok != 0 && pid != 0).then_some(pid)
        }

        fn transfer(&self, write: bool, buffer: *mut u8, len: u32) -> io::Result<usize> {
            if self.shared.shut.load(Ordering::SeqCst) {
                return if write {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(0)
                };
            }
            let timeout = {
                let timeouts = self.shared.timeouts.lock().expect("pipe timeouts poisoned");
                if write { timeouts.1 } else { timeouts.0 }
            };
            let event = Event::new()?;
            // SAFETY: zeroed OVERLAPPED is the documented initial state.
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            overlapped.hEvent = event.0;
            let mut done = 0u32;
            // SAFETY: `buffer` is valid for `len` bytes for the whole
            // operation, which is completed below before this returns.
            let started = unsafe {
                if write {
                    WriteFile(
                        self.shared.handle.0,
                        buffer,
                        len,
                        &mut done,
                        &mut overlapped,
                    )
                } else {
                    ReadFile(
                        self.shared.handle.0,
                        buffer,
                        len,
                        &mut done,
                        &mut overlapped,
                    )
                }
            };
            let mut timed_out = false;
            if started == 0 {
                // SAFETY: reads this thread's last error.
                let error = unsafe { GetLastError() };
                if error != ERROR_IO_PENDING {
                    return self.finished(write, error, false);
                }
                let millis = timeout.map_or(INFINITE, |timeout| {
                    u32::try_from(timeout.as_millis()).unwrap_or(INFINITE - 1)
                });
                // SAFETY: the event belongs to this operation.
                let waited = unsafe { WaitForSingleObject(event.0, millis) };
                if waited == WAIT_TIMEOUT {
                    timed_out = true;
                    // SAFETY: cancels this operation only.
                    unsafe { CancelIoEx(self.shared.handle.0, &overlapped) };
                } else if waited != WAIT_OBJECT_0 {
                    // Fall through: GetOverlappedResult below waits it out.
                }
                // SAFETY: waits for the operation to finish, whatever ended
                // it, so the buffer and OVERLAPPED are free to drop after.
                let completed =
                    unsafe { GetOverlappedResult(self.shared.handle.0, &overlapped, &mut done, 1) };
                if completed == 0 {
                    // SAFETY: reads this thread's last error.
                    let error = unsafe { GetLastError() };
                    return self.finished(write, error, timed_out);
                }
            }
            Ok(done as usize)
        }

        fn finished(&self, write: bool, error: u32, timed_out: bool) -> io::Result<usize> {
            match error {
                ERROR_OPERATION_ABORTED if timed_out => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the terminal endpoint did not answer in time",
                )),
                ERROR_OPERATION_ABORTED if self.shared.shut.load(Ordering::SeqCst) => {
                    if write {
                        Err(io::ErrorKind::BrokenPipe.into())
                    } else {
                        Ok(0)
                    }
                }
                ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA if !write => Ok(0),
                ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA => {
                    Err(io::ErrorKind::BrokenPipe.into())
                }
                other => Err(io::Error::from_raw_os_error(other as i32)),
            }
        }
    }

    impl Read for PipeStream {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            self.transfer(false, buffer.as_mut_ptr(), len)
        }
    }

    impl Write for PipeStream {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            self.transfer(true, buffer.as_ptr().cast_mut(), len)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct Event(HANDLE);

    impl Event {
        fn new() -> io::Result<Self> {
            // SAFETY: an unnamed manual-reset event with default security.
            let handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(handle))
        }
    }

    impl Drop for Event {
        fn drop(&mut self) {
            // SAFETY: owned, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The server's side: the pipe's name and the instance waiting for the
    /// next client.
    pub struct PipeListener {
        name: PathBuf,
        pending: Mutex<Option<HANDLE>>,
    }

    // SAFETY: the pending handle is only touched under the mutex.
    unsafe impl Send for PipeListener {}
    unsafe impl Sync for PipeListener {}

    impl PipeListener {
        pub fn bind(name: &Path) -> io::Result<Self> {
            let first = create_instance(name, true)?;
            Ok(Self {
                name: name.to_path_buf(),
                pending: Mutex::new(Some(first)),
            })
        }

        pub fn accept(&self) -> io::Result<PipeStream> {
            let handle = {
                let mut pending = self.pending.lock().expect("pipe listener poisoned");
                match pending.take() {
                    Some(handle) => handle,
                    None => create_instance(&self.name, false)?,
                }
            };
            let connected = wait_for_client(handle);
            // The next instance exists before this one is handed out, so a
            // client arriving now is never told the endpoint is gone.
            if let Ok(next) = create_instance(&self.name, false) {
                *self.pending.lock().expect("pipe listener poisoned") = Some(next);
            }
            match connected {
                Ok(()) => Ok(PipeStream::from_handle(handle, Side::Server)),
                Err(error) => {
                    // SAFETY: the instance never became a stream; close it.
                    unsafe { CloseHandle(handle) };
                    Err(error)
                }
            }
        }
    }

    impl Drop for PipeListener {
        fn drop(&mut self) {
            if let Some(handle) = self
                .pending
                .lock()
                .ok()
                .and_then(|mut pending| pending.take())
            {
                // SAFETY: owned, closed once.
                unsafe { CloseHandle(handle) };
            }
        }
    }

    fn wait_for_client(handle: HANDLE) -> io::Result<()> {
        let event = Event::new()?;
        // SAFETY: zeroed OVERLAPPED is the documented initial state.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event.0;
        // SAFETY: valid instance handle and an OVERLAPPED that outlives the
        // operation, which is completed below.
        if unsafe { ConnectNamedPipe(handle, &mut overlapped) } != 0 {
            return Ok(());
        }
        // SAFETY: reads this thread's last error.
        match unsafe { GetLastError() } {
            ERROR_PIPE_CONNECTED => Ok(()),
            ERROR_IO_PENDING => {
                let mut ignored = 0u32;
                // SAFETY: waits for this operation to complete.
                if unsafe { GetOverlappedResult(handle, &overlapped, &mut ignored, 1) } != 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            }
            error => Err(io::Error::from_raw_os_error(error as i32)),
        }
    }

    fn create_instance(name: &Path, first: bool) -> io::Result<HANDLE> {
        let descriptor = OwnerOnly::new()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let wide_name = wide(name.as_os_str());
        let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: `wide_name` and `attributes` (with the descriptor it
        // points at) outlive the call.
        let handle = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                &attributes,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            return Err(if first {
                io::Error::new(
                    error.kind(),
                    format!(
                        "the terminal endpoint {} is already held by another process ({error})",
                        name.display()
                    ),
                )
            } else {
                error
            });
        }
        Ok(handle)
    }

    /// A security descriptor granting the current token's user, and no one
    /// else, full access. The user SID and not the owner SID: an elevated
    /// token's owner is the Administrators group.
    struct OwnerOnly(PSECURITY_DESCRIPTOR);

    impl OwnerOnly {
        fn new() -> io::Result<Self> {
            let sid = current_user_sid()?;
            let sddl = wide(OsStr::new(&format!("D:P(A;;GA;;;{sid})")));
            let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
            // SAFETY: `sddl` is NUL-terminated; the descriptor is freed with
            // LocalFree in Drop, as the API requires.
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    impl Drop for OwnerOnly {
        fn drop(&mut self) {
            // SAFETY: allocated by the conversion above with LocalAlloc.
            unsafe { LocalFree(self.0) };
        }
    }
}
