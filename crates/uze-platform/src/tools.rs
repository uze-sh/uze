//! The tools the operating system itself ships, found where it keeps them.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

/// `name` as the operating system ships it, never another program of the
/// same name earlier on `PATH`.
pub fn system(name: &str) -> Command {
    Command::new(system_program(name))
}

/// The program [`system`] runs for `name`, for a caller that describes a
/// process rather than spawning it.
///
/// Always a path inside [`system_directories`] (or `name` itself, when it
/// already is a path): a bare name would be looked up on `PATH`, where
/// anything that can prepend a directory chooses what runs. A tool the
/// system does not have resolves to where it would be, and fails to start.
pub fn system_program(name: &str) -> PathBuf {
    let named = Path::new(name);
    if named.is_absolute() {
        return named.to_path_buf();
    }
    let file = imp::file_name(name);
    system_directories()
        .iter()
        .map(|directory| directory.join(&file))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| imp::default_directory().join(&file))
}

/// Where the system keeps its own tools: `/usr/bin` and `/bin` (and the
/// system profile NixOS keeps instead); on Windows `System32`, Windows
/// PowerShell's directory and the OpenSSH client's. A `PATH` of only these
/// finds nothing a person installed.
pub fn system_directories() -> Vec<PathBuf> {
    imp::system_directories()
}

/// What every download UZE makes with `curl` passes first. `-q` must lead:
/// it is what keeps `curl` from reading a `.curlrc`, which can add a CA
/// bundle, a proxy or `--insecure` behind the command's back. The protocol
/// pins keep a redirect from leaving HTTPS.
pub const CURL_ARGUMENTS: [&str; 6] = [
    "-q",
    "--proto",
    "=https",
    "--proto-redir",
    "=https",
    "--tlsv1.2",
];

/// The system's `curl` with [`CURL_ARGUMENTS`] and without the variables
/// [`curl_reads`] names.
pub fn curl() -> Command {
    curl_without(std::env::vars_os().map(|(name, _)| name))
}

fn curl_without(present: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = system("curl");
    command.args(CURL_ARGUMENTS);
    for name in present.into_iter().filter(|name| curl_reads(name)) {
        command.env_remove(name);
    }
    command
}

/// The variables of this process [`curl`] removes, for a caller that
/// describes the process rather than spawning it.
pub fn curl_environment_to_remove() -> Vec<String> {
    std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| curl_reads(name))
        .filter_map(|name| name.into_string().ok())
        .collect()
}

/// Whether [`curl`] runs without `name`: the `CURL_*` settings and the
/// session-key log.
///
/// The proxy variables and the CA bundle (`SSL_CERT_FILE`, `SSL_CERT_DIR`,
/// `CURL_CA_BUNDLE`) are kept, because a machine behind a corporate proxy
/// that inspects TLS needs both to reach anything. That is safe because the
/// transport is not what an install trusts: a release is installed only
/// when its checksums carry the release key's signature, which no proxy or
/// CA can produce.
pub fn curl_reads(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_uppercase();
    let kept = name == "CURL_CA_BUNDLE";
    (name.starts_with("CURL_") && !kept) || name == "SSLKEYLOGFILE"
}

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;

    pub(super) fn file_name(name: &str) -> String {
        name.to_owned()
    }

    pub(super) fn default_directory() -> PathBuf {
        PathBuf::from("/usr/bin")
    }

    pub(super) fn system_directories() -> Vec<PathBuf> {
        ["/usr/bin", "/bin", "/run/current-system/sw/bin"]
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }
}

#[cfg(windows)]
mod imp {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt as _, path::PathBuf};

    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

    /// By path, with its extension: under Windows PowerShell `curl` is an
    /// alias for something else, and a `tar` earlier on `PATH` (Git's GNU
    /// tar) cannot read a zip.
    pub(super) fn file_name(name: &str) -> String {
        format!("{name}.exe")
    }

    pub(super) fn default_directory() -> PathBuf {
        system32()
    }

    pub(super) fn system_directories() -> Vec<PathBuf> {
        let system = system32();
        vec![
            system.join("WindowsPowerShell").join("v1.0"),
            system.clone(),
            system.join("OpenSSH"),
        ]
    }

    /// Asked of the kernel rather than read from `SystemRoot`, which is an
    /// environment variable and so whatever the parent process said it was.
    fn system32() -> PathBuf {
        let mut buffer = vec![0u16; 260];
        loop {
            let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            // SAFETY: `buffer` holds `capacity` UTF-16 units.
            let written = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), capacity) } as usize;
            if written == 0 {
                return PathBuf::from(r"C:\Windows\System32");
            }
            if written < buffer.len() {
                return PathBuf::from(OsString::from_wide(&buffer[..written]));
            }
            buffer.resize(written + 1, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_tool_is_named_by_its_path_never_looked_up_on_path() {
        for name in ["curl", "tar", "ssh-keygen", "no-such-tool-anywhere"] {
            let program = system_program(name);
            assert!(program.is_absolute(), "{name} resolved to {program:?}");
            assert!(
                system_directories()
                    .iter()
                    .any(|directory| program.parent() == Some(directory.as_path())),
                "{name} resolved outside the system's directories: {program:?}"
            );
        }
    }

    #[test]
    fn a_path_is_taken_as_given() {
        let absolute = system_directories()[0].join("reg.exe");
        assert_eq!(system_program(&absolute.to_string_lossy()), absolute);
    }

    #[test]
    fn curl_reads_no_configuration_file_and_speaks_only_https() {
        let command = curl_without(Vec::new());
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments.first().copied(), Some(OsStr::new("-q")));
        for pinned in ["--proto", "--proto-redir"] {
            let at = arguments.iter().position(|argument| *argument == pinned);
            assert_eq!(
                at.and_then(|at| arguments.get(at + 1)).copied(),
                Some(OsStr::new("=https")),
                "{pinned}"
            );
        }
        assert!(arguments.contains(&OsStr::new("--tlsv1.2")));
    }

    #[test]
    fn curl_runs_without_its_settings_but_keeps_the_proxy_and_ca() {
        let present = [
            "CURL_CA_BUNDLE",
            "CURL_HOME",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "SSLKEYLOGFILE",
            "https_proxy",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "no_proxy",
            "HOME",
            "PATH",
        ]
        .map(OsString::from);
        let command = curl_without(present);
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().to_ascii_uppercase())
            .collect();
        for name in ["CURL_HOME", "SSLKEYLOGFILE"] {
            assert!(removed.contains(&name.to_owned()), "{name}: {removed:?}");
        }
        for name in [
            "CURL_CA_BUNDLE",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "HOME",
            "PATH",
        ] {
            assert!(!removed.contains(&name.to_owned()), "{name}: {removed:?}");
        }
    }
}
