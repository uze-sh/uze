//! Probe: what an LPAC AppContainer child inside a Job Object can reach.
//! `probe parent <work> <log>` sets up grants and runs scenarios;
//! `probe child <work> <port> <pipe> <parentpid>` reports each access.

use std::ffi::OsStr;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::Isolation::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::*;
use windows_sys::Win32::System::Threading::*;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain([0]).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("child") => child(&args[2], &args[3], &args[4], &args[5]),
        Some("parent") => parent(&args[2], &args[3]),
        _ => eprintln!("usage"),
    }
}

// ---------------------------------------------------------------- child

fn report(name: &str, result: Result<String, String>) {
    match result {
        Ok(ok) => println!("{name}: ALLOWED {ok}"),
        Err(e) => println!("{name}: denied {e}"),
    }
}

fn io<T>(r: std::io::Result<T>) -> Result<String, String> {
    r.map(|_| String::new()).map_err(|e| e.to_string())
}

fn child(work: &str, port: &str, pipe: &str, parent_pid: &str) {
    let w = |p: &str| format!("{work}\\{p}");
    report("read granted", io(std::fs::read(w("granted_read\\a.txt"))));
    report("read granted\\many (deny ACE for confined cap)", io(std::fs::read(w("granted_read\\many\\d0\\f0"))));
    report("list granted", io(std::fs::read_dir(w("granted_read"))));
    report("read denied", io(std::fs::read(w("denied\\secret.txt"))));
    report("list denied", io(std::fs::read_dir(w("denied"))));
    report("read work root listing", io(std::fs::read_dir(work)));
    report("stat denied", io(std::fs::metadata(w("denied\\secret.txt"))));
    report("write granted_write", io(std::fs::write(w("granted_write\\new.txt"), b"x")));
    report("mkdir granted_write", io(std::fs::create_dir(w("granted_write\\sub"))));
    report("write in granted_write\\sub", io(std::fs::write(w("granted_write\\sub\\f"), b"x")));
    report("delete granted_write file", io(std::fs::remove_file(w("granted_write\\pre.txt"))));
    report("write granted_read", io(std::fs::write(w("granted_read\\new.txt"), b"x")));
    report(
        "truncate granted_read",
        io(std::fs::OpenOptions::new().write(true).truncate(true).open(w("granted_read\\a.txt"))),
    );
    report("write denied", io(std::fs::write(w("denied\\x.txt"), b"x")));
    report("exec copy in bin", io(std::process::Command::new(w("bin\\probe.exe")).output()));
    report(
        "exec copy in granted_read (no X)",
        io(std::process::Command::new(w("granted_read\\probe.exe")).output()),
    );
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    report("list USERPROFILE", io(std::fs::read_dir(&profile)));
    report("read USERPROFILE\\NTUSER.DAT stat", io(std::fs::metadata(format!("{profile}\\NTUSER.DAT"))));
    let temp = std::env::var("TEMP").unwrap_or_default();
    report("write TEMP", io(std::fs::write(format!("{temp}\\probe-lpac.txt"), b"x")));
    report("read hosts", io(std::fs::read("C:\\Windows\\System32\\drivers\\etc\\hosts")));
    report("read win.ini", io(std::fs::read("C:\\Windows\\win.ini")));
    report("list Program Files", io(std::fs::read_dir("C:\\Program Files")));
    report("list C:\\", io(std::fs::read_dir("C:\\")));
    report("list C:\\Users", io(std::fs::read_dir("C:\\Users")));
    report("write C:\\Windows\\Temp", io(std::fs::write("C:\\Windows\\Temp\\probe.txt", b"x")));
    report("write C:\\", io(std::fs::write("C:\\probe-root.txt", b"x")));
    report(
        "run cmd /c echo",
        std::process::Command::new("C:\\Windows\\System32\\cmd.exe")
            .args(["/c", "echo", "hi"])
            .output()
            .map(|o| format!("status {:?} out {:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim()))
            .map_err(|e| e.to_string()),
    );
    report(
        "run powershell",
        std::process::Command::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
            .args(["-NoProfile", "-Command", "1+1"])
            .output()
            .map(|o| format!("status {:?} out {:?} err {:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim(), String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("")))
            .map_err(|e| e.to_string()),
    );
    for tool in ["C:\\Program Files\\Git\\cmd\\git.exe", "C:\\hostedtoolcache\\windows\\Python"] {
        if std::path::Path::new(tool).exists() {
            report(&format!("exists-check {tool}"), io(std::fs::metadata(tool)));
        }
    }
    if let Ok(git) = std::env::var("PROBE_GIT") {
        report(
            "run git --version",
            std::process::Command::new(&git)
                .arg("--version")
                .output()
                .map(|o| format!("status {:?} out {:?} err {:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim(), String::from_utf8_lossy(&o.stderr).trim()))
                .map_err(|e| e.to_string()),
        );
        report(
            "run git status in granted_read repo",
            std::process::Command::new(&git)
                .args(["-C", &w("granted_read"), "status", "--short"])
                .output()
                .map(|o| format!("status {:?} out {:?} err {:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim(), String::from_utf8_lossy(&o.stderr).trim()))
                .map_err(|e| e.to_string()),
        );
    }
    for var in ["PROBE_PYTHON", "PROBE_NODE"] {
        if let Ok(exe) = std::env::var(var) {
            let arg = if var == "PROBE_PYTHON" { "-c" } else { "-e" };
            let code = if var == "PROBE_PYTHON" { "print(1+1)" } else { "console.log(1+1)" };
            report(
                &format!("run {var}"),
                std::process::Command::new(&exe)
                    .args([arg, code])
                    .output()
                    .map(|o| format!("status {:?} out {:?} err {:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim(), String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")))
                    .map_err(|e| e.to_string()),
            );
        }
    }
    report("connect loopback", io(std::net::TcpStream::connect(format!("127.0.0.1:{port}"))));
    report(
        "connect internet 1.1.1.1:443",
        io(std::net::TcpStream::connect_timeout(&"1.1.1.1:443".parse().unwrap(), std::time::Duration::from_secs(5))),
    );
    report(
        "resolve dns",
        std::net::ToSocketAddrs::to_socket_addrs("example.com:443")
            .map(|a| format!("{:?}", a.collect::<Vec<_>>().first()))
            .map_err(|e| e.to_string()),
    );
    report(
        "bind+listen 0.0.0.0:0",
        std::net::TcpListener::bind("0.0.0.0:0")
            .map(|l| format!("{:?}", l.local_addr()))
            .map_err(|e| e.to_string()),
    );
    report("bind udp", io(std::net::UdpSocket::bind("0.0.0.0:0")));
    report(
        "udp dns query to 1.1.1.1:53",
        (|| -> std::io::Result<String> {
            let s = std::net::UdpSocket::bind("0.0.0.0:0")?;
            s.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
            let q: [u8; 29] = [0x12,0x34,1,0,0,1,0,0,0,0,0,0,7,b'e',b'x',b'a',b'm',b'p',b'l',b'e',3,b'c',b'o',b'm',0,0,1,0,1];
            s.send_to(&q, "1.1.1.1:53")?;
            let mut buf = [0u8; 512];
            let (n, _) = s.recv_from(&mut buf)?;
            Ok(format!("{n} bytes"))
        })().map_err(|e| e.to_string()),
    );
    let listen_port: u16 = port.parse::<u16>().unwrap() + 1;
    report(
        "listen and accept inbound (parent connects)",
        (|| -> std::io::Result<String> {
            let l = std::net::TcpListener::bind(("0.0.0.0", listen_port))?;
            l.set_nonblocking(true)?;
            let until = std::time::Instant::now() + std::time::Duration::from_secs(6);
            while std::time::Instant::now() < until {
                match l.accept() {
                    Ok((_, a)) => return Ok(format!("accepted from {a}")),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(e) => return Err(e),
                }
            }
            Err(std::io::Error::other("listening, but nothing reached it"))
        })().map_err(|e| e.to_string()),
    );
    println!("TEMP={}", std::env::var("TEMP").unwrap_or_default());
    println!("LOCALAPPDATA={}", std::env::var("LOCALAPPDATA").unwrap_or_default());
    report("open named pipe", io(std::fs::OpenOptions::new().read(true).write(true).open(pipe)));
    let pid: u32 = parent_pid.parse().unwrap();
    for (name, access) in [
        ("OpenProcess parent VM_READ", PROCESS_VM_READ),
        ("OpenProcess parent TERMINATE", PROCESS_TERMINATE),
        ("OpenProcess parent QUERY_LIMITED", PROCESS_QUERY_LIMITED_INFORMATION),
    ] {
        let h = unsafe { OpenProcess(access, 0, pid) };
        report(name, if h.is_null() { Err(format!("{}", std::io::Error::last_os_error())) } else { unsafe { CloseHandle(h) }; Ok(String::new()) });
    }
    {
        use windows_sys::Win32::System::Registry::*;
        let mut key = ptr::null_mut();
        let sub = wide("Software\\Microsoft");
        let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &mut key) };
        report("registry HKCU\\Software\\Microsoft", if r == 0 { Ok(String::new()) } else { Err(format!("error {r}")) });
        let sub = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, sub.as_ptr(), 0, KEY_READ, &mut key) };
        report("registry HKLM CurrentVersion", if r == 0 { Ok(String::new()) } else { Err(format!("error {r}")) });
        for k in ["Software\\UzeProbeSecret", "Environment", "Software", "Software\\Classes"] {
            let sub = wide(k);
            let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &mut key) };
            report(&format!("registry HKCU\\{k}"), if r == 0 { Ok(String::new()) } else { Err(format!("error {r}")) });
        }
        let sub = wide("Software\\UzeProbeWrite");
        let r = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, ptr::null(), 0, KEY_WRITE, ptr::null(), &mut key, ptr::null_mut()) };
        report("registry create HKCU\\Software\\UzeProbeWrite", if r == 0 { Ok(String::new()) } else { Err(format!("error {r}")) });
    }
    report("device \\\\.\\PhysicalDrive0", io(std::fs::File::open("\\\\.\\PhysicalDrive0")));
    report("device NUL write", io(std::fs::write("NUL", b"x")));
    report("device CONOUT$", io(std::fs::OpenOptions::new().write(true).open("CONOUT$")));
    // Memory last: the job's limit, if any, refuses the commit.
    let mut v: Vec<u8> = Vec::new();
    report(
        "commit 512 MiB",
        v.try_reserve_exact(512 << 20).map(|_| {
            v.resize(512 << 20, 1);
            "committed".to_string()
        }).map_err(|e| e.to_string()),
    );
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------- parent

struct Log(std::fs::File);
impl Log {
    fn line(&mut self, s: &str) {
        let _ = writeln!(self.0, "{s}");
        println!("{s}");
    }
}

fn sid_string(sid: PSID) -> String {
    let mut s = ptr::null_mut();
    unsafe { ConvertSidToStringSidW(sid, &mut s) };
    let len = (0..).take_while(|&i| unsafe { *s.add(i) } != 0).count();
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(s, len) })
}

fn capability(name: &str) -> PSID {
    let n = wide(name);
    let (mut groups, mut ng, mut caps, mut nc) = (ptr::null_mut(), 0u32, ptr::null_mut(), 0u32);
    let ok = unsafe { DeriveCapabilitySidsFromName(n.as_ptr(), &mut groups, &mut ng, &mut caps, &mut nc) };
    assert!(ok != 0 && nc >= 1, "DeriveCapabilitySidsFromName {}", std::io::Error::last_os_error());
    unsafe { *caps }
}

fn grant(path: &str, sid: PSID, rights: u32, log: &mut Log) {
    ace(path, SE_FILE_OBJECT, GRANT_ACCESS, sid, rights, log)
}

fn ace(path: &str, kind: SE_OBJECT_TYPE, mode: ACCESS_MODE, sid: PSID, rights: u32, log: &mut Log) {
    let p = wide(path);
    let mut old: *mut ACL = ptr::null_mut();
    let mut sd = ptr::null_mut();
    let r = unsafe { GetNamedSecurityInfoW(p.as_ptr(), kind, DACL_SECURITY_INFORMATION, ptr::null_mut(), ptr::null_mut(), &mut old, ptr::null_mut(), &mut sd) };
    assert_eq!(r, 0, "GetNamedSecurityInfo {path}");
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: rights,
        grfAccessMode: mode,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            ptstrName: sid as _,
        },
    };
    let mut new: *mut ACL = ptr::null_mut();
    let r = unsafe { SetEntriesInAclW(1, &ea, old, &mut new) };
    assert_eq!(r, 0, "SetEntriesInAcl");
    let t = std::time::Instant::now();
    let r = unsafe { SetNamedSecurityInfoW(p.as_ptr() as _, kind, DACL_SECURITY_INFORMATION, ptr::null_mut(), ptr::null_mut(), new, ptr::null()) };
    log.line(&format!("grant {path} rights {rights:#x} -> {r} in {:?}", t.elapsed()));
}

fn parent(work: &str, log_path: &str) {
    let mut log = Log(std::fs::File::create(log_path).unwrap());
    let w = |p: &str| format!("{work}\\{p}");
    let _ = std::fs::remove_dir_all(work);
    for d in ["granted_read", "granted_write", "denied", "bin"] {
        std::fs::create_dir_all(w(d)).unwrap();
    }
    std::fs::write(w("granted_read\\a.txt"), b"hello").unwrap();
    std::fs::write(w("granted_write\\pre.txt"), b"pre").unwrap();
    std::fs::write(w("denied\\secret.txt"), b"secret").unwrap();
    let me = std::env::current_exe().unwrap();
    std::fs::copy(&me, w("bin\\probe.exe")).unwrap();
    std::fs::copy(&me, w("granted_read\\probe.exe")).unwrap();
    if let Ok(git) = std::env::var("PROBE_GIT") {
        let _ = std::process::Command::new(git).args(["init", "-q", &w("granted_read")]).status();
    }
    // a many-files tree, to time an inherited grant's propagation
    let many = w("granted_read\\many");
    for i in 0..50 {
        let d = format!("{many}\\d{i}");
        std::fs::create_dir_all(&d).unwrap();
        for j in 0..100 {
            std::fs::write(format!("{d}\\f{j}"), b"x").unwrap();
        }
    }

    let read_cap = capability("uze.probe.read");
    let write_cap = capability("uze.probe.write");
    let exec_cap = capability("uze.probe.exec");
    let deny_cap = capability("uze.probe.confined");
    let _ = deny_cap;
    log.line(&format!("caps read {} write {} exec {}", sid_string(read_cap), sid_string(write_cap), sid_string(exec_cap)));
    // read: list + read data + attributes, no execute
    let read = FILE_GENERIC_READ;
    let read_exec = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
    let write = FILE_GENERIC_READ | FILE_GENERIC_WRITE | 0x10000 /*DELETE*/ | FILE_DELETE_CHILD;
    grant(&w("granted_read"), read_cap, read, &mut log);
    grant(&w("granted_write"), write_cap, write, &mut log);
    grant(&w("bin"), exec_cap, read_exec, &mut log);
    for dir in std::env::var("PROBE_GRANT_EXEC").unwrap_or_default().split(';').filter(|d| !d.is_empty()) {
        grant(dir, exec_cap, read_exec, &mut log);
    }

    // container SID without a registered profile
    let mut container: PSID = ptr::null_mut();
    let name = wide("uze.probe");
    let hr = if std::env::var("PROBE_PROFILE").is_ok() {
        let display = wide("uze probe");
        let hr = unsafe { CreateAppContainerProfile(name.as_ptr(), display.as_ptr(), display.as_ptr(), ptr::null(), 0, &mut container) };
        log.line(&format!("create profile -> {hr:#x}"));
        if hr != 0 { unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut container) } } else { hr }
    } else {
        unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut container) }
    };
    log.line(&format!("derive container -> {hr:#x} {}", sid_string(container)));

    {
        use windows_sys::Win32::System::Registry::*;
        let mut key = ptr::null_mut();
        let sub = wide("Software\\UzeProbeSecret");
        let r = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, ptr::null(), 0, KEY_WRITE, ptr::null(), &mut key, ptr::null_mut()) };
        log.line(&format!("created HKCU secret key -> {r}"));
    }
    for i in 0..3 {
        let n = wide(&format!("uze.probe.run{i}"));
        let t = std::time::Instant::now();
        let mut sid = ptr::null_mut();
        let hr = unsafe { CreateAppContainerProfile(n.as_ptr(), n.as_ptr(), n.as_ptr(), ptr::null(), 0, &mut sid) };
        let created = t.elapsed();
        let t = std::time::Instant::now();
        let hd = unsafe { DeleteAppContainerProfile(n.as_ptr()) };
        log.line(&format!("profile create {hr:#x} in {created:?}, delete {hd:#x} in {:?}", t.elapsed()));
    }
    ace("CURRENT_USER\\Environment", SE_REGISTRY_KEY, DENY_ACCESS, container, 0x20019 /*KEY_READ*/, &mut log);
    ace(&w("granted_read\\many"), SE_FILE_OBJECT, DENY_ACCESS, container, FILE_GENERIC_READ, &mut log);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let pipe_name = format!("\\\\.\\pipe\\uze-probe-{}", std::process::id());
    let pn = wide(&pipe_name);
    let pipe = unsafe { CreateNamedPipeW(pn.as_ptr(), PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE, 255, 4096, 4096, 0, ptr::null()) };
    log.line(&format!("pipe {pipe_name} {:?}", pipe != INVALID_HANDLE_VALUE));

    let internet_client: PSID = {
        // S-1-15-3-1 internetClient
        let mut sid = ptr::null_mut();
        let s = wide("S-1-15-3-1");
        unsafe { ConvertStringSidToSidW(s.as_ptr(), &mut sid) };
        sid
    };

    let only: Option<String> = std::env::var("PROBE_ONLY").ok();
    let scenarios: Vec<(&str, bool, bool, Vec<PSID>, u64)> = vec![
        ("unconfined", false, false, vec![], 0),
        ("ac no-net", true, false, vec![read_cap, write_cap, exec_cap], 256 << 20),
        ("ac deny", true, false, vec![read_cap, write_cap, exec_cap, deny_cap], 256 << 20),
        ("lpac no-net", true, true, vec![read_cap, write_cap, exec_cap], 256 << 20),
        ("lpac net", true, true, vec![read_cap, write_cap, exec_cap, internet_client], 256 << 20),
        ("lpac no caps", true, true, vec![exec_cap], 0),
        ("lpac+caps no-net", true, true, [read_cap, write_cap, exec_cap].into_iter().chain(["registryRead", "lpacCom", "lpacAppExperience", "lpacInstrumentation", "lpacCryptoServices", "lpacIdentityServices", "lpacPnPNotifications", "lpacServicesManagement", "lpacSessionManagement", "lpacEnterprisePolicyChangeNotifications"].map(capability)).collect(), 256 << 20),
        ("lpac+caps net", true, true, [read_cap, write_cap, exec_cap, internet_client].into_iter().chain(["registryRead", "lpacCom", "lpacAppExperience", "lpacInstrumentation", "lpacCryptoServices", "lpacIdentityServices", "lpacPnPNotifications", "lpacServicesManagement", "lpacSessionManagement", "lpacEnterprisePolicyChangeNotifications"].map(capability)).collect(), 256 << 20),
        ("lpac custom", true, true, [read_cap, write_cap, exec_cap].into_iter().chain(std::env::var("PROBE_NET").ok().map(|_| internet_client)).chain(std::env::var("PROBE_EXTRA").unwrap_or_default().split(',').filter(|c| !c.is_empty()).map(capability)).collect(), 256 << 20),
        ("ac net", true, false, vec![read_cap, write_cap, exec_cap, internet_client], 256 << 20),
    ];
    for (label, contained, lpac, caps, memory) in scenarios {
        if only.as_deref().is_some_and(|o| !label.contains(o)) { continue; }
        log.line(&format!("\n===== {label} (memory {memory}) ====="));
        let connector = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while std::time::Instant::now() < until {
                for host in ["127.0.0.1"] {
                    if std::net::TcpStream::connect_timeout(&format!("{host}:{}", port + 1).parse().unwrap(), std::time::Duration::from_millis(200)).is_ok() {
                        seen.push(host);
                        return seen;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            seen
        });
        let out = spawn(&w("bin\\probe.exe"), &format!("\"{}\" child \"{work}\" {port} {pipe_name} {}", w("bin\\probe.exe"), std::process::id()), contained.then_some(container), lpac, &caps, memory, &mut log);
        log.line(&out);
        log.line(&format!("parent's inbound connect reached the child via {:?}", connector.join().unwrap()));
    }
    log.line("listener accepted?");
    log.line(&format!("{:?}", listener.accept().map(|(_, a)| a)));

    // propagation timing for a second grant across 5k files
    let t = std::time::Instant::now();
    let other = capability("uze.probe.other");
    grant(&w("granted_read"), other, read, &mut log);
    log.line(&format!("re-grant over 5000 files took {:?}", t.elapsed()));
    log.line("DONE");
}

fn spawn(app: &str, cmdline: &str, container: Option<PSID>, use_lpac: bool, caps: &[PSID], memory: u64, log: &mut Log) -> String {
    unsafe {
        let sa = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: ptr::null_mut(), bInheritHandle: 1 };
        let (mut rd, mut wr) = (ptr::null_mut(), ptr::null_mut());
        assert!(CreatePipe(&mut rd, &mut wr, &sa, 1 << 20) != 0);
        SetHandleInformation(rd, HANDLE_FLAG_INHERIT, 0);

        let mut attrs: Vec<SID_AND_ATTRIBUTES> = caps.iter().map(|&s| SID_AND_ATTRIBUTES { Sid: s, Attributes: 4 }).collect();
        let mut sc = SECURITY_CAPABILITIES { AppContainerSid: container.unwrap_or(ptr::null_mut()), Capabilities: attrs.as_mut_ptr(), CapabilityCount: attrs.len() as u32, Reserved: 0 };
        let lpac: u32 = 1; // PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT
        let handles = [wr];
        let mut size = 0usize;
        InitializeProcThreadAttributeList(ptr::null_mut(), 3, 0, &mut size);
        let mut buf = vec![0u8; size];
        let list = buf.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        assert!(InitializeProcThreadAttributeList(list, 3, 0, &mut size) != 0);
        assert!(UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize, handles.as_ptr().cast(), size_of_val(&handles), ptr::null_mut(), ptr::null()) != 0);
        if container.is_some() {
            assert!(UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize, (&sc as *const SECURITY_CAPABILITIES).cast(), size_of::<SECURITY_CAPABILITIES>(), ptr::null_mut(), ptr::null()) != 0);
            if use_lpac { assert!(UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY as usize, (&lpac as *const u32).cast(), 4, ptr::null_mut(), ptr::null()) != 0); }
        }
        let _ = &mut sc;
        let mut si: STARTUPINFOEXW = std::mem::zeroed();
        si.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        si.StartupInfo.hStdOutput = wr;
        si.StartupInfo.hStdError = wr;
        si.lpAttributeList = list;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let a = wide(app);
        let mut c = wide(cmdline);
        let ok = CreateProcessW(a.as_ptr(), c.as_mut_ptr(), ptr::null(), ptr::null(), 1, EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW, ptr::null(), ptr::null(), &si.StartupInfo, &mut pi);
        if ok == 0 {
            return format!("CreateProcess failed: {}", std::io::Error::last_os_error());
        }
        let job = CreateJobObjectW(ptr::null(), ptr::null());
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | if memory > 0 { JOB_OBJECT_LIMIT_PROCESS_MEMORY } else { 0 };
        limits.ProcessMemoryLimit = memory as usize;
        let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(), size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        let assigned = AssignProcessToJobObject(job, pi.hProcess);
        log.line(&format!("job set {set} assigned {assigned}"));
        ResumeThread(pi.hThread);
        CloseHandle(wr);
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut n = 0u32;
            if ReadFile(rd, chunk.as_mut_ptr(), chunk.len() as u32, &mut n, ptr::null_mut()) == 0 || n == 0 {
                break;
            }
            out.extend_from_slice(&chunk[..n as usize]);
        }
        WaitForSingleObject(pi.hProcess, 60_000);
        let mut code = 0u32;
        GetExitCodeProcess(pi.hProcess, &mut code);
        DeleteProcThreadAttributeList(list);
        let _ = std::io::stdout().flush();
        format!("{}exit {code:#x}", String::from_utf8_lossy(&out))
    }
}
